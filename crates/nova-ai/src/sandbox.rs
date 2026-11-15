//! Isolated build/test sandbox for agent tasks (M27.1/M27.2).
//!
//! A [`Sandbox`] is a private copy of the granted project *plus* the staged
//! changes from a [`ReviewSession`]. Agent build/test commands run inside it
//! so the real project is never touched. The M25 `ReviewSession` remains the
//! authoritative staging/apply instance — the sandbox is derived state only.
//!
//! Security posture (honest limits documented in docs):
//! - no admin rights required; commands run as the current user;
//! - copy skips symlinks/junctions, `.git`, `target`, `node_modules`,
//!   `dist`, `build` and every path matched by the M25 sensitive denylist;
//! - a size cap aborts oversized copies;
//! - commands still go through [`crate::runner::CommandRunner`]'s argv
//!   allowlist — no shell;
//! - **not** implemented: OS-level network isolation, job-object process
//!   trees, memory quotas. Cargo may reach crates.io for dependencies.
//!   This is a *project-copy sandbox*, not a hardened VM.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::access::is_sensitive;
use crate::error::{AiError, AiResult};
use crate::review::ReviewSession;

/// Directories never copied into a sandbox.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".idea",
    ".vs",
    ".nova-ai",
];

/// Default sandbox size cap (copied payload, staged files excluded).
pub const DEFAULT_MAX_SANDBOX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// One isolated sandbox bound to a task id.
pub struct Sandbox {
    /// Task id this sandbox belongs to.
    pub task_id: String,
    /// Root directory of the sandboxed project copy.
    pub root: PathBuf,
    /// Workspace root (parent of `root`), removed on cleanup.
    workspace: PathBuf,
    /// Paths whose sandbox content currently reflects staged overrides.
    state: std::collections::HashSet<String>,
    /// Fingerprint of the staged review state at last sync.
    pub staged_fingerprint: Option<String>,
    /// Bytes copied (payload only).
    pub copied_bytes: u64,
    /// Files skipped (sensitive/symlink/oversize).
    pub skipped: Vec<String>,
    /// Canonical project root — needed to restore originals on unstage.
    original_root: Option<PathBuf>,
    created_at: Instant,
}

impl Sandbox {
    /// Create a sandbox under `base_dir` (e.g. config/ai-sandboxes).
    /// Copies the project (grant-filtered reads are *not* re-applied here —
    /// the copy uses the same `is_sensitive` denylist as M25) and materializes
    /// every staged change from `review`.
    pub fn create(
        task_id: &str,
        project_root: &Path,
        review: &ReviewSession,
        base_dir: &Path,
        max_bytes: u64,
    ) -> AiResult<Self> {
        let workspace = base_dir.join(sanitize(task_id));
        if workspace.exists() {
            fs::remove_dir_all(&workspace).map_err(AiError::io)?;
        }
        let root = workspace.join("project");
        fs::create_dir_all(&root).map_err(AiError::io)?;

        let mut sandbox = Self {
            task_id: task_id.to_string(),
            root: root.clone(),
            workspace: workspace.clone(),
            state: std::collections::HashSet::new(),
            staged_fingerprint: None,
            copied_bytes: 0,
            skipped: Vec::new(),
            original_root: project_root.canonicalize().ok(),
            created_at: Instant::now(),
        };
        sandbox.copy_project(project_root, max_bytes)?;
        sandbox.apply_review(review)?;
        sandbox.staged_fingerprint = Some(review.fingerprint());
        Ok(sandbox)
    }

    /// Re-materialize staged changes after the agent revised the review.
    /// Only files whose desired state changed are touched; `target/` and
    /// other build outputs inside the sandbox are preserved.
    pub fn sync(&mut self, review: &ReviewSession) -> AiResult<()> {
        self.apply_review(review)?;
        self.staged_fingerprint = Some(review.fingerprint());
        Ok(())
    }

    /// Whether the sandbox contains a Rust workspace (Cargo.toml).
    pub fn has_cargo_project(&self) -> bool {
        self.root.join("Cargo.toml").is_file()
    }

    /// Absolute path inside the sandbox for a project-relative path.
    /// Rejects traversal — same rules as the agent workspace.
    pub fn resolve(&self, rel: &str) -> AiResult<PathBuf> {
        let normalized = rel.trim().replace('\\', "/");
        if normalized.is_empty()
            || normalized.starts_with('/')
            || normalized.contains("..")
            || normalized.contains(':')
        {
            return Err(AiError::Unsupported(format!(
                "Ungültiger Sandbox-Pfad '{rel}'"
            )));
        }
        Ok(self.root.join(&normalized))
    }

    /// Remove the whole sandbox workspace. Idempotent; also runs on Drop.
    pub fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.workspace);
    }

    /// Total size of the sandbox directory (for limit reporting).
    pub fn size_bytes(&self) -> u64 {
        dir_size(&self.workspace).unwrap_or(0)
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }

    // ------------------------------------------------ internals

    fn copy_project(&mut self, project_root: &Path, max_bytes: u64) -> AiResult<()> {
        let mut stack = vec![project_root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir).map_err(AiError::io)?;
            for entry in entries {
                let entry = entry.map_err(AiError::io)?;
                let path = entry.path();
                let rel = path
                    .strip_prefix(project_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let file_type = entry.file_type().map_err(AiError::io)?;
                if file_type.is_symlink() {
                    self.skipped.push(format!("{rel} (Symlink)"));
                    continue;
                }
                if file_type.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if SKIP_DIRS.contains(&name.as_str()) || is_sensitive(&rel) {
                        self.skipped.push(format!("{rel}/ (übersprungen)"));
                        continue;
                    }
                    stack.push(path);
                    continue;
                }
                if is_sensitive(&rel) {
                    self.skipped.push(format!("{rel} (sensibel)"));
                    continue;
                }
                let size = entry.metadata().map_err(AiError::io)?.len();
                if self.copied_bytes + size > max_bytes {
                    return Err(AiError::Unsupported(format!(
                        "Sandbox-Größenlimit überschritten ({max_bytes} Bytes)"
                    )));
                }
                let dest = self.root.join(&rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(AiError::io)?;
                }
                fs::copy(&path, &dest).map_err(AiError::io)?;
                self.copied_bytes += size;
                // original copy — not a staged override
            }
        }
        Ok(())
    }

    /// Materialize the current staged state of every review item (content-
    /// compared, so revised staging syncs correctly) and revert paths that
    /// are no longer staged.
    fn apply_review(&mut self, review: &ReviewSession) -> AiResult<()> {
        let mut current: std::collections::HashSet<String> = std::collections::HashSet::new();
        for item in review.items() {
            current.insert(item.path.clone());
            self.materialize(item)?;
        }
        // Paths previously materialized but no longer staged → restore the
        // original project state inside the sandbox.
        let stale: Vec<String> = self
            .state
            .iter()
            .filter(|path| !current.contains(*path))
            .cloned()
            .collect();
        for path in stale {
            self.restore_original(&path)?;
            self.state.remove(&path);
        }
        self.state = current;
        Ok(())
    }

    /// Ensure the sandbox file matches the staged content exactly; writes
    /// only when bytes differ (cheap for the few staged files per task).
    fn materialize(&self, item: &crate::review::ReviewItem) -> AiResult<()> {
        let dest = self.resolve(&item.path)?;
        match item.kind {
            crate::files::ChangeKind::Delete => {
                if dest.exists() {
                    fs::remove_file(&dest).map_err(AiError::io)?;
                }
            }
            _ => {
                let content = item.staged_content().ok_or_else(|| {
                    AiError::Unsupported(format!("{}: kein gestagter Inhalt", item.path))
                })?;
                let up_to_date = fs::read(&dest)
                    .map(|bytes| bytes == content.as_bytes())
                    .unwrap_or(false);
                if !up_to_date {
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent).map_err(AiError::io)?;
                    }
                    fs::write(&dest, content).map_err(AiError::io)?;
                }
            }
        }
        Ok(())
    }

    /// Copy the original project file back into the sandbox (or remove the
    /// sandbox file when the path never existed originally).
    fn restore_original(&self, rel: &str) -> AiResult<()> {
        let Some(original_root) = &self.original_root else {
            // Fallback: remove the file (treat as never-existing).
            let dest = self.resolve(rel)?;
            if dest.exists() {
                fs::remove_file(&dest).map_err(AiError::io)?;
            }
            return Ok(());
        };
        let src = original_root.join(rel);
        let dest = self.resolve(rel)?;
        if src.is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(AiError::io)?;
            }
            fs::copy(&src, &dest).map_err(AiError::io)?;
        } else if dest.exists() {
            fs::remove_file(&dest).map_err(AiError::io)?;
        }
        Ok(())
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn sanitize(task_id: &str) -> String {
    task_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_dir() {
                total += dir_size(&entry.path())?;
            } else {
                total += meta.len();
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{AccessLevel, AccessStore, GuardedWorkspace};
    use crate::files::{ProposedChange, Workspace};

    fn fixture(name: &str) -> (PathBuf, PathBuf, GuardedWorkspace, ReviewSession) {
        let root = std::env::temp_dir().join(format!("nova-sandbox-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.as"), "class Main {}\n").unwrap();
        fs::write(root.join("src/old.as"), "delete me\n").unwrap();
        fs::write(root.join(".env"), "SECRET=1\n").unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/big.bin"), "x".repeat(1024)).unwrap();
        let config =
            std::env::temp_dir().join(format!("nova-sandbox-cfg-{name}-{}", std::process::id()));
        let mut store = AccessStore::fallback();
        store.grant(&root, &root, AccessLevel::ReadWrite).unwrap();
        let guarded =
            GuardedWorkspace::new(Workspace::new(&root).unwrap(), store.policy_for(&root));
        let staging =
            std::env::temp_dir().join(format!("nova-sandbox-stage-{name}-{}", std::process::id()));
        let mut review = ReviewSession::new(&root, &staging).unwrap();
        review
            .stage(
                &guarded,
                &ProposedChange::create("src/new.as", "class New {}\n"),
            )
            .unwrap();
        review
            .stage(
                &guarded,
                &ProposedChange::modify("src/main.as", "class Main { int x; }\n"),
            )
            .unwrap();
        review
            .stage(&guarded, &ProposedChange::delete("src/old.as"))
            .unwrap();
        (root, config, guarded, review)
    }

    #[test]
    fn sandbox_contains_project_plus_staged_state() {
        let (root, base, _guarded, review) = fixture("basic");
        let sandbox =
            Sandbox::create("task-1", &root, &review, &base, DEFAULT_MAX_SANDBOX_BYTES).unwrap();
        // Original preserved.
        assert_eq!(
            fs::read_to_string(root.join("src/main.as")).unwrap(),
            "class Main {}\n"
        );
        // Staged changes materialized.
        assert_eq!(
            fs::read_to_string(sandbox.root.join("src/new.as")).unwrap(),
            "class New {}\n"
        );
        assert_eq!(
            fs::read_to_string(sandbox.root.join("src/main.as")).unwrap(),
            "class Main { int x; }\n"
        );
        assert!(!sandbox.root.join("src/old.as").exists());
        // Sensitive + build dirs excluded.
        assert!(!sandbox.root.join(".env").exists());
        assert!(!sandbox.root.join("target").exists());
        assert!(sandbox.skipped.iter().any(|s| s.contains(".env")));
        sandbox.cleanup();
        assert!(!sandbox.workspace.exists());
    }

    #[test]
    fn sandboxes_are_independent_per_task() {
        let (root, base, _g, review) = fixture("multi");
        let s1 =
            Sandbox::create("task-a", &root, &review, &base, DEFAULT_MAX_SANDBOX_BYTES).unwrap();
        let s2 =
            Sandbox::create("task-b", &root, &review, &base, DEFAULT_MAX_SANDBOX_BYTES).unwrap();
        assert_ne!(s1.workspace, s2.workspace);
        fs::write(s1.root.join("src/main.as"), "sandbox-only\n").unwrap();
        assert_ne!(
            fs::read_to_string(s2.root.join("src/main.as")).unwrap(),
            "sandbox-only\n"
        );
        s1.cleanup();
        s2.cleanup();
    }

    #[test]
    fn sync_updates_only_changed_staged_files() {
        let (root, base, guarded, mut review) = fixture("sync");
        let mut sandbox = Sandbox::create(
            "task-sync",
            &root,
            &review,
            &base,
            DEFAULT_MAX_SANDBOX_BYTES,
        )
        .unwrap();
        // Build artifact inside sandbox must survive syncs.
        fs::create_dir_all(sandbox.root.join("target")).unwrap();
        fs::write(sandbox.root.join("target/out.bin"), "keep").unwrap();
        // Agent revises the staged content.
        review
            .stage_or_update(
                &guarded,
                &ProposedChange::modify("src/main.as", "class Main { int y; }\n"),
            )
            .unwrap();
        sandbox.sync(&review).unwrap();
        assert_eq!(
            fs::read_to_string(sandbox.root.join("src/main.as")).unwrap(),
            "class Main { int y; }\n"
        );
        assert_eq!(
            fs::read_to_string(sandbox.root.join("target/out.bin")).unwrap(),
            "keep"
        );
        sandbox.cleanup();
    }

    #[test]
    fn resolve_rejects_traversal() {
        let (root, base, _g, review) = fixture("trav");
        let sandbox =
            Sandbox::create("t", &root, &review, &base, DEFAULT_MAX_SANDBOX_BYTES).unwrap();
        assert!(sandbox.resolve("../evil").is_err());
        assert!(sandbox.resolve("src/../../evil").is_err());
        assert!(sandbox.resolve("C:/Windows/x").is_err());
        sandbox.cleanup();
    }

    #[test]
    fn size_cap_aborts_copy() {
        let (root, base, _g, review) = fixture("cap");
        let result = Sandbox::create("cap", &root, &review, &base, 10);
        assert!(result.is_err());
    }

    #[test]
    fn drop_cleans_up() {
        let (root, base, _g, review) = fixture("drop");
        let workspace;
        {
            let sandbox =
                Sandbox::create("drop", &root, &review, &base, DEFAULT_MAX_SANDBOX_BYTES).unwrap();
            workspace = sandbox.workspace.clone();
            assert!(workspace.exists());
        }
        assert!(!workspace.exists());
    }
}
