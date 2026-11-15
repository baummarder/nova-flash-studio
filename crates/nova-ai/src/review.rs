//! Isolated review workspace for agent-proposed file changes (M25.4–M25.6).
//!
//! Proposed changes never touch the original files directly. Each change is
//! staged into a separate directory together with a snapshot of the original
//! bytes. The user reviews per-file diffs, accepts or rejects each change,
//! and only the accepted subset is applied — with a freshness check that
//! blocks files modified since staging and a rollback for partial failures.

use std::fs;
use std::path::{Path, PathBuf};

use crate::access::GuardedWorkspace;
use crate::diff::{DiffPreview, diff_text};
use crate::error::{AiError, AiResult};
use crate::files::{AppliedChange, ChangeKind, ChangePreview, ProposedChange};

/// A single staged change awaiting the user's decision.
#[derive(Debug, Clone)]
pub struct ReviewItem {
    pub path: String,
    pub kind: ChangeKind,
    pub reason: String,
    /// Staged new content (file inside the staging dir). `None` for deletes.
    pub staged: Option<PathBuf>,
    /// Snapshot of the original bytes at staging time (`None` = did not exist).
    pub original_snapshot: Option<Vec<u8>>,
    /// UTF-8 view of the original for diffing (`None` for binary/missing).
    pub old_text: Option<String>,
    pub diff: DiffPreview,
    /// User decision; defaults: writes on, deletes off.
    pub accepted: bool,
    /// Set when the original changed between staging and apply.
    pub conflicted: bool,
}

impl ReviewItem {
    pub fn summary(&self) -> String {
        format!(
            "{} · {} · {}",
            self.kind.label(),
            self.path,
            self.diff.summary()
        )
    }

    /// Full new content for preview (reads the staged file).
    pub fn staged_content(&self) -> Option<String> {
        self.staged
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
    }
}

#[derive(Debug)]
pub struct ReviewSession {
    project_root: PathBuf,
    staging_dir: PathBuf,
    items: Vec<ReviewItem>,
}

/// Result of [`ReviewSession::apply`].
#[derive(Debug, Default)]
pub struct ApplyReport {
    pub applied: Vec<AppliedChange>,
    /// Paths skipped because the original changed after staging.
    pub conflicts: Vec<String>,
    /// Paths skipped because write permission was revoked meanwhile.
    pub denied: Vec<String>,
    /// True when every accepted item was applied without error.
    pub complete: bool,
}

impl ReviewSession {
    /// Create a staging area for `project_root` under `staging_root`.
    /// The directory name is derived from the canonical root so different
    /// projects never share staged content.
    pub fn new(project_root: &Path, staging_root: &Path) -> AiResult<Self> {
        let root = project_root.canonicalize().map_err(AiError::io)?;
        let key: String = root
            .to_string_lossy()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let staging_dir = staging_root.join(format!("review-{key}"));
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir).map_err(AiError::io)?;
        }
        fs::create_dir_all(&staging_dir).map_err(AiError::io)?;
        Ok(Self {
            project_root: root,
            staging_dir,
            items: Vec::new(),
        })
    }

    pub fn staging_dir(&self) -> &Path {
        &self.staging_dir
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn items(&self) -> &[ReviewItem] {
        &self.items
    }

    pub fn items_mut(&mut self) -> &mut Vec<ReviewItem> {
        &mut self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn accepted_count(&self) -> usize {
        self.items.iter().filter(|item| item.accepted).count()
    }

    /// Stage one proposed change: validate write access, snapshot the
    /// original, write the proposed content into the isolated staging dir
    /// and compute the review diff. Nothing touches the project files.
    pub fn stage(&mut self, guarded: &GuardedWorkspace, change: &ProposedChange) -> AiResult<()> {
        let normalized = change.path.trim().replace('\\', "/");
        guarded.ensure_write(&normalized)?;

        // Resolve through the workspace for traversal/symlink safety.
        let absolute = guarded.workspace().resolve(&normalized)?;
        let existed = absolute.exists();
        let kind = change.kind(existed);

        let original_snapshot = if existed {
            Some(fs::read(&absolute).map_err(AiError::io)?)
        } else {
            None
        };
        let old_text = original_snapshot
            .as_ref()
            .and_then(|bytes| String::from_utf8(bytes.clone()).ok());

        // Stage the new content under a path-safe name inside staging_dir.
        let safe_name: String = normalized
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let staged_path = self.staging_dir.join(&safe_name);
        let staged = if kind == ChangeKind::Delete {
            None
        } else {
            if let Some(parent) = staged_path.parent() {
                fs::create_dir_all(parent).map_err(AiError::io)?;
            }
            fs::write(&staged_path, change.content.as_bytes()).map_err(AiError::io)?;
            Some(staged_path)
        };

        let new_text = if kind == ChangeKind::Delete {
            None
        } else {
            Some(change.content.clone())
        };
        let diff = diff_text(
            old_text.as_deref().unwrap_or(""),
            new_text.as_deref().unwrap_or(""),
        );

        self.items.push(ReviewItem {
            path: normalized,
            kind,
            reason: change.reason.clone(),
            staged,
            original_snapshot,
            old_text,
            diff,
            // Deletes require an explicit extra opt-in at apply time.
            accepted: kind != ChangeKind::Delete,
            conflicted: false,
        });
        Ok(())
    }

    /// Build a [`ChangePreview`] for UI display without touching originals.
    pub fn preview_of(&self, index: usize) -> Option<ChangePreview> {
        let item = self.items.get(index)?;
        Some(ChangePreview {
            path: item.path.clone(),
            kind: item.kind,
            exists: item.original_snapshot.is_some(),
            old_content: item.old_text.clone(),
            new_content: item.staged_content(),
            diff: item.diff.clone(),
            reason: item.reason.clone(),
        })
    }

    /// Apply all accepted items. For each file the current original bytes are
    /// compared against the staging-time snapshot; mismatches are reported as
    /// conflicts and skipped. Writes go through the guarded workspace, which
    /// re-checks grants. On a mid-apply failure, already-applied files are
    /// rolled back via their recorded previous content.
    pub fn apply(&mut self, guarded: &GuardedWorkspace) -> AiResult<ApplyReport> {
        let mut report = ApplyReport::default();
        let mut staged_applied: Vec<AppliedChange> = Vec::new();

        for item in &mut self.items {
            if !item.accepted {
                continue;
            }
            let absolute = match guarded.workspace().resolve(&item.path) {
                Ok(path) => path,
                Err(error) => {
                    report.denied.push(format!("{}: {error}", item.path));
                    continue;
                }
            };
            // Freshness check: original must still match the snapshot.
            let current = if absolute.exists() {
                fs::read(&absolute).map_err(AiError::io)?
            } else {
                Vec::new()
            };
            let snapshot = item.original_snapshot.clone().unwrap_or_default();
            if current != snapshot {
                item.conflicted = true;
                report.conflicts.push(item.path.clone());
                continue;
            }
            // Write permission re-check immediately before touching disk.
            if let Err(error) = guarded.ensure_write(&item.path) {
                report.denied.push(format!("{}: {error}", item.path));
                continue;
            }

            let change = ProposedChange {
                path: item.path.clone(),
                content: item.staged_content().unwrap_or_default(),
                reason: item.reason.clone(),
                delete: item.kind == ChangeKind::Delete,
                kind: Some(item.kind),
            };
            match guarded.workspace().apply(&change) {
                Ok(applied) => staged_applied.push(applied),
                Err(error) => {
                    // Roll back everything applied so far in this session.
                    for applied_change in staged_applied.iter().rev() {
                        let _ = guarded.workspace().undo(applied_change);
                    }
                    return Err(AiError::Unsupported(format!(
                        "Anwendung bei '{}' fehlgeschlagen ({error}); alle vorherigen Dateien wurden zurückgesetzt.",
                        item.path
                    )));
                }
            }
        }

        report.applied = staged_applied;
        report.complete = report.conflicts.is_empty() && report.denied.is_empty();
        Ok(report)
    }

    /// Drop the staging area (call after apply or discard).
    pub fn cleanup(self) {
        let _ = fs::remove_dir_all(&self.staging_dir);
    }
}

/// Convenience: stage a batch of proposed changes, skipping ones that fail
/// validation. Returns the session plus per-change errors.
pub fn stage_changes(
    project_root: &Path,
    staging_root: &Path,
    guarded: &GuardedWorkspace,
    changes: &[ProposedChange],
) -> AiResult<(ReviewSession, Vec<String>)> {
    let mut session = ReviewSession::new(project_root, staging_root)?;
    let mut errors = Vec::new();
    for change in changes {
        if let Err(error) = session.stage(guarded, change) {
            errors.push(format!("{}: {error}", change.path));
        }
    }
    Ok((session, errors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{AccessLevel, AccessPolicy, Grant};
    use crate::files::Workspace;

    fn fixture(name: &str) -> (PathBuf, PathBuf, GuardedWorkspace) {
        let root =
            std::env::temp_dir().join(format!("nova-ai-review-{name}-{}", std::process::id()));
        let staging = std::env::temp_dir().join(format!(
            "nova-ai-review-staging-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(root.join("src/main.as"), "class Main {\n  int x;\n}\n").unwrap();
        fs::write(root.join("src/util.as"), "class Util {}\n").unwrap();
        let canonical = root.canonicalize().unwrap();
        let workspace = Workspace::new(&canonical).unwrap();
        let guarded = GuardedWorkspace::new(
            workspace,
            AccessPolicy {
                read_only: false,
                grants: vec![Grant {
                    path: canonical.clone(),
                    level: AccessLevel::ReadWrite,
                }],
            },
        );
        (canonical, staging, guarded)
    }

    #[test]
    fn staging_does_not_touch_originals_and_diff_is_correct() {
        let (root, staging, guarded) = fixture("stage");
        let mut session = ReviewSession::new(&root, &staging).unwrap();
        session
            .stage(
                &guarded,
                &ProposedChange::modify("src/main.as", "class Main {\n  int x;\n  int y;\n}\n"),
            )
            .unwrap();
        session
            .stage(
                &guarded,
                &ProposedChange::create("src/new.as", "class New {}\n"),
            )
            .unwrap();

        assert_eq!(session.items().len(), 2);
        assert!(session.items()[0].diff.added >= 1);
        // Originals untouched.
        assert_eq!(
            fs::read_to_string(root.join("src/main.as")).unwrap(),
            "class Main {\n  int x;\n}\n"
        );
        assert!(!root.join("src/new.as").exists());
        session.cleanup();
    }

    #[test]
    fn apply_writes_accepted_files_and_skips_rejected() {
        let (root, staging, guarded) = fixture("apply");
        let mut session = ReviewSession::new(&root, &staging).unwrap();
        session
            .stage(
                &guarded,
                &ProposedChange::modify("src/main.as", "class Main {\n  int x = 1;\n}\n"),
            )
            .unwrap();
        session
            .stage(
                &guarded,
                &ProposedChange::modify("src/util.as", "class Util { int z; }\n"),
            )
            .unwrap();
        // Reject the second change.
        session.items_mut()[1].accepted = false;

        let report = session.apply(&guarded).unwrap();
        assert_eq!(report.applied.len(), 1);
        assert!(report.complete);
        assert_eq!(
            fs::read_to_string(root.join("src/main.as")).unwrap(),
            "class Main {\n  int x = 1;\n}\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("src/util.as")).unwrap(),
            "class Util {}\n"
        );
        session.cleanup();
    }

    #[test]
    fn conflict_blocks_apply_when_original_changed() {
        let (root, staging, guarded) = fixture("conflict");
        let mut session = ReviewSession::new(&root, &staging).unwrap();
        session
            .stage(
                &guarded,
                &ProposedChange::modify("src/main.as", "agent version\n"),
            )
            .unwrap();
        // User edits the file after staging.
        fs::write(root.join("src/main.as"), "user version\n").unwrap();

        let report = session.apply(&guarded).unwrap();
        assert_eq!(report.conflicts, vec!["src/main.as".to_string()]);
        assert!(report.applied.is_empty());
        assert_eq!(
            fs::read_to_string(root.join("src/main.as")).unwrap(),
            "user version\n"
        );
        assert!(session.items()[0].conflicted);
        session.cleanup();
    }

    #[test]
    fn write_grant_revoked_before_apply_is_denied() {
        let (root, staging, guarded) = fixture("revoke");
        let mut session = ReviewSession::new(&root, &staging).unwrap();
        session
            .stage(
                &guarded,
                &ProposedChange::modify("src/main.as", "agent version\n"),
            )
            .unwrap();
        // Policy without write access → apply must deny, not write.
        let read_only = GuardedWorkspace::new(
            guarded.workspace().clone(),
            AccessPolicy {
                read_only: true,
                grants: vec![Grant {
                    path: root.clone(),
                    level: AccessLevel::ReadWrite,
                }],
            },
        );
        let report = session.apply(&read_only).unwrap();
        assert_eq!(report.denied.len(), 1);
        assert!(report.applied.is_empty());
        assert_eq!(
            fs::read_to_string(root.join("src/main.as")).unwrap(),
            "class Main {\n  int x;\n}\n"
        );
        session.cleanup();
    }

    #[test]
    fn delete_requires_explicit_acceptance() {
        let (root, staging, guarded) = fixture("delete");
        let mut session = ReviewSession::new(&root, &staging).unwrap();
        session
            .stage(&guarded, &ProposedChange::delete("src/util.as"))
            .unwrap();
        assert!(!session.items()[0].accepted);
        let report = session.apply(&guarded).unwrap();
        assert!(report.applied.is_empty());
        assert!(root.join("src/util.as").exists());

        session.items_mut()[0].accepted = true;
        let report = session.apply(&guarded).unwrap();
        assert_eq!(report.applied.len(), 1);
        assert!(!root.join("src/util.as").exists());
        session.cleanup();
    }

    #[test]
    fn sensitive_paths_cannot_be_staged() {
        let (root, staging, guarded) = fixture("sensitive");
        fs::write(root.join(".env"), "SECRET=1\n").unwrap();
        let mut session = ReviewSession::new(&root, &staging).unwrap();
        let result = session.stage(&guarded, &ProposedChange::modify(".env", "x\n"));
        assert!(result.is_err());
        session.cleanup();
    }
}
