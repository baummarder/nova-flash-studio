//! Project access grants for the AI agent (M25).
//!
//! The agent never gets implicit filesystem access. Every read or write must
//! be covered by an explicit [`Grant`] stored per project in the edition's
//! AI config directory (`ai-access.json`). Grants are keyed by the canonical
//! project root, so permissions can never leak into another project.
//!
//! On top of grants, [`is_sensitive`] hard-denies credentials, keys and
//! other secret material — even inside a read-write grant. The deny list is
//! intentionally conservative and is surfaced in the UI.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AiError, AiResult};
use crate::files::Workspace;

pub const ACCESS_FILE: &str = "ai-access.json";
pub const ACCESS_VERSION: u32 = 1;

/// What the agent may do inside a granted path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    /// Read files and browse the tree; no change proposals may be applied.
    Read,
    /// Read plus prepare/apply changes (still user-confirmed per file).
    ReadWrite,
}

impl AccessLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Read => "Lesen",
            Self::ReadWrite => "Lesen + Ändern",
        }
    }
}

/// One explicit permission: `path` is a canonical file or directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    /// Absolute canonical path (file or directory).
    pub path: PathBuf,
    pub level: AccessLevel,
}

/// Effective permission set for one project.
#[derive(Debug, Clone, Default)]
pub struct AccessPolicy {
    /// Project-wide read-only cap: downgrades every grant to `Read`.
    pub read_only: bool,
    pub grants: Vec<Grant>,
}

impl AccessPolicy {
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }

    /// Effective level for an absolute path, or `None` when no grant covers it.
    /// The most specific (deepest) matching grant wins.
    pub fn level_for(&self, absolute: &Path) -> Option<AccessLevel> {
        let mut best: Option<(usize, AccessLevel)> = None;
        for grant in &self.grants {
            // Component-wise prefix: a directory grant covers descendants,
            // a file grant covers exactly itself (`starts_with` is
            // component-aware, so "b.txt" never matches "b.txt2").
            if !absolute.starts_with(&grant.path) {
                continue;
            }
            let depth = grant.path.components().count();
            let level = if self.read_only {
                AccessLevel::Read
            } else {
                grant.level
            };
            if best.is_none_or(|(d, _)| depth > d) {
                best = Some((depth, level));
            }
        }
        best.map(|(_, level)| level)
    }

    pub fn allows_read(&self, absolute: &Path) -> bool {
        matches!(
            self.level_for(absolute),
            Some(AccessLevel::Read | AccessLevel::ReadWrite)
        )
    }

    pub fn allows_write(&self, absolute: &Path) -> bool {
        matches!(self.level_for(absolute), Some(AccessLevel::ReadWrite))
    }
}

/// Sensitive path detection — applied *after* grant checks, so even a
/// read-write grant cannot reach these. Matching is on the
/// project-relative path with `/` separators, case-insensitive.
pub fn is_sensitive(relative: &str) -> bool {
    let normalized = relative.trim().replace('\\', "/").to_lowercase();
    if normalized.is_empty() {
        return true;
    }
    let parts: Vec<&str> = normalized.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        let name = *part;
        let is_dir_component = index + 1 < parts.len();
        // Credential / VCS directories anywhere in the tree.
        if is_dir_component && matches!(name, ".ssh" | ".aws" | ".gnupg" | ".git") {
            return true;
        }
        // Credential / key file names.
        if name == ".env"
            || name.starts_with(".env.")
            || name == "credentials"
            || name == ".netrc"
            || name == ".git-credentials"
            || name == "id_rsa"
            || name.starts_with("id_rsa.")
            || name == "id_ed25519"
            || name.starts_with("id_ed25519.")
            || name.ends_with(".pem")
            || name.ends_with(".key")
            || name.ends_with(".p12")
            || name.ends_with(".pfx")
            || name.ends_with(".keystore")
            || name.ends_with(".kdbx")
        {
            return true;
        }
    }
    false
}

/// Human-readable explanation of the exclusion rules (UI display).
pub const SENSITIVE_RULES: &[&str] = &[
    ".env / .env.* — Umgebungsvariablen und Secrets",
    "*.pem, *.key, *.p12, *.pfx, *.keystore, *.kdbx — Schlüssel und Zertifikate",
    "id_rsa*, id_ed25519* — SSH-Private-Keys",
    "credentials, .netrc, .git-credentials — Zugangsdaten",
    ".ssh/, .aws/, .gnupg/ — Credential-Verzeichnisse",
    ".git/ — Versionskontroll-Interna",
];

// ------------------------------------------------------------------ store

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGrant {
    path: String,
    level: AccessLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectAccess {
    /// Canonical project root this entry belongs to.
    root: String,
    #[serde(default)]
    read_only: bool,
    #[serde(default)]
    grants: Vec<StoredGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessData {
    version: u32,
    #[serde(default)]
    projects: BTreeMap<String, ProjectAccess>,
}

/// Persistent per-project grant storage in the edition config dir.
/// The file is shared with provider settings but keyed by project root,
/// so grants can never migrate to a different project silently.
pub struct AccessStore {
    file: PathBuf,
    data: AccessData,
}

impl AccessStore {
    pub fn load(config_dir: &Path) -> AiResult<Self> {
        fs::create_dir_all(config_dir).map_err(AiError::io)?;
        let file = config_dir.join(ACCESS_FILE);
        let data = if file.exists() {
            let text = fs::read_to_string(&file).map_err(AiError::io)?;
            match serde_json::from_str::<AccessData>(&text) {
                Ok(data) if data.version == ACCESS_VERSION => data,
                _ => AccessData {
                    version: ACCESS_VERSION,
                    projects: BTreeMap::new(),
                },
            }
        } else {
            AccessData {
                version: ACCESS_VERSION,
                projects: BTreeMap::new(),
            }
        };
        Ok(Self { file, data })
    }

    /// In-memory fallback when the config dir cannot be created/written.
    /// Grants still work for the session but are not persisted.
    pub fn fallback() -> Self {
        Self {
            file: std::env::temp_dir().join("nova-ai-access-fallback.json"),
            data: AccessData {
                version: ACCESS_VERSION,
                projects: BTreeMap::new(),
            },
        }
    }

    pub fn save(&self) -> AiResult<()> {
        let bytes = serde_json::to_vec_pretty(&self.data).map_err(|e| {
            AiError::Unsupported(format!(
                "Zugriffsrechte konnten nicht serialisiert werden: {e}"
            ))
        })?;
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, bytes).map_err(AiError::io)?;
        fs::rename(&tmp, &self.file).map_err(AiError::io)
    }

    /// Canonical project-root key used to isolate grants per project.
    pub fn project_key(project_root: &Path) -> AiResult<String> {
        Ok(project_root
            .canonicalize()
            .map_err(AiError::io)?
            .to_string_lossy()
            .to_string())
    }

    /// Effective policy for `project_root` (empty when never granted).
    pub fn policy_for(&self, project_root: &Path) -> AccessPolicy {
        let Ok(key) = Self::project_key(project_root) else {
            return AccessPolicy::default();
        };
        let Some(entry) = self.data.projects.get(&key) else {
            return AccessPolicy::default();
        };
        let grants = entry
            .grants
            .iter()
            .filter_map(|stored| {
                let path = PathBuf::from(&stored.path);
                // Grants pointing outside the current project root are dead.
                let root = PathBuf::from(&entry.root);
                if !path.starts_with(&root) {
                    return None;
                }
                Some(Grant {
                    path,
                    level: stored.level,
                })
            })
            .collect();
        AccessPolicy {
            read_only: entry.read_only,
            grants,
        }
    }

    /// Stored grants for display: (project-relative display path,
    /// stored absolute path for revoke, level).
    pub fn grants_for(&self, project_root: &Path) -> Vec<(String, String, AccessLevel)> {
        let Ok(key) = Self::project_key(project_root) else {
            return Vec::new();
        };
        let Some(entry) = self.data.projects.get(&key) else {
            return Vec::new();
        };
        let root = PathBuf::from(&entry.root);
        entry
            .grants
            .iter()
            .map(|stored| {
                let path = PathBuf::from(&stored.path);
                let display = path
                    .strip_prefix(&root)
                    .map(|rel| rel.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| stored.path.clone());
                (display, stored.path.clone(), stored.level)
            })
            .collect()
    }

    pub fn read_only(&self, project_root: &Path) -> bool {
        Self::project_key(project_root)
            .ok()
            .and_then(|key| self.data.projects.get(&key).map(|entry| entry.read_only))
            .unwrap_or(false)
    }

    /// Grant `path` (must live inside `project_root`) at `level`.
    /// Returns the stored display path.
    pub fn grant(
        &mut self,
        project_root: &Path,
        path: &Path,
        level: AccessLevel,
    ) -> AiResult<String> {
        let key = Self::project_key(project_root)?;
        let root = project_root.canonicalize().map_err(AiError::io)?;
        let canonical = if path.exists() {
            path.canonicalize().map_err(AiError::io)?
        } else {
            return Err(AiError::UnsafePath(format!(
                "Freigabe-Pfad existiert nicht: {}",
                path.display()
            )));
        };
        if !canonical.starts_with(&root) {
            return Err(AiError::UnsafePath(format!(
                "Freigabe liegt außerhalb des Projektordners: {}",
                path.display()
            )));
        }
        let entry = self
            .data
            .projects
            .entry(key)
            .or_insert_with(|| ProjectAccess {
                root: root.to_string_lossy().to_string(),
                read_only: false,
                grants: Vec::new(),
            });
        let stored_path = canonical.to_string_lossy().to_string();
        entry.grants.retain(|grant| grant.path != stored_path);
        entry.grants.push(StoredGrant {
            path: stored_path.clone(),
            level,
        });
        entry.grants.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(stored_path)
    }

    /// Revoke the grant covering `display_or_abs` (matched by stored path).
    pub fn revoke(&mut self, project_root: &Path, stored_path: &str) -> bool {
        let Ok(key) = Self::project_key(project_root) else {
            return false;
        };
        let Some(entry) = self.data.projects.get_mut(&key) else {
            return false;
        };
        let before = entry.grants.len();
        entry.grants.retain(|grant| grant.path != stored_path);
        entry.grants.len() != before
    }

    /// Revoke every grant for the project (full revocation).
    pub fn revoke_all(&mut self, project_root: &Path) -> bool {
        let Ok(key) = Self::project_key(project_root) else {
            return false;
        };
        self.data.projects.remove(&key).is_some()
    }

    pub fn set_read_only(&mut self, project_root: &Path, read_only: bool) -> AiResult<()> {
        let key = Self::project_key(project_root)?;
        let root = project_root.canonicalize().map_err(AiError::io)?;
        let entry = self
            .data
            .projects
            .entry(key)
            .or_insert_with(|| ProjectAccess {
                root: root.to_string_lossy().to_string(),
                read_only: false,
                grants: Vec::new(),
            });
        entry.read_only = read_only;
        Ok(())
    }
}

// ------------------------------------------------------- guarded workspace

/// A [`Workspace`] plus the project's [`AccessPolicy`]. Every operation the
/// agent can reach goes through this type — there is no second,
/// uncontrolled filesystem interface.
#[derive(Debug, Clone)]
pub struct GuardedWorkspace {
    inner: Workspace,
    policy: AccessPolicy,
    root: PathBuf,
}

impl GuardedWorkspace {
    pub fn new(workspace: Workspace, policy: AccessPolicy) -> Self {
        let root = workspace.root().to_path_buf();
        Self {
            inner: workspace,
            policy,
            root,
        }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.inner
    }

    pub fn policy(&self) -> &AccessPolicy {
        &self.policy
    }

    /// Resolve `relative` and enforce grant + sensitive-file rules.
    fn check(&self, relative: &str, write: bool) -> AiResult<PathBuf> {
        let normalized = relative.trim().replace('\\', "/");
        if is_sensitive(&normalized) {
            return Err(AiError::UnsafePath(format!(
                "Sensible Datei ist für den Agenten gesperrt: {normalized}"
            )));
        }
        let absolute = self.inner.resolve(&normalized)?;
        let allowed = if write {
            self.policy.allows_write(&absolute)
        } else {
            self.policy.allows_read(&absolute)
        };
        if !allowed {
            return Err(AiError::UnsafePath(format!(
                "Keine {}-Freigabe für: {normalized}",
                if write { "Schreib" } else { "Lese" }
            )));
        }
        Ok(absolute)
    }

    /// Whether the agent may read `relative` (grant + sensitive rules).
    pub fn can_read(&self, relative: &str) -> bool {
        self.check(relative, false).is_ok()
    }

    /// Whether the agent may propose/apply changes to `relative`.
    pub fn can_write(&self, relative: &str) -> bool {
        self.check(relative, true).is_ok()
    }

    pub fn read(&self, relative: &str) -> AiResult<String> {
        let path = self.check(relative, false)?;
        fs::read_to_string(path).map_err(AiError::io)
    }

    pub fn read_bytes(&self, relative: &str) -> AiResult<Vec<u8>> {
        let path = self.check(relative, false)?;
        fs::read(path).map_err(AiError::io)
    }

    /// Source-like files the agent may actually read, grant-filtered.
    pub fn list_readable_files(&self) -> Vec<String> {
        self.inner
            .list_source_files()
            .into_iter()
            .filter(|file| self.can_read(file))
            .collect()
    }

    /// Re-check that write access still holds immediately before applying.
    pub fn ensure_write(&self, relative: &str) -> AiResult<()> {
        self.check(relative, true).map(|_| ())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nova-ai-access-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn grants_are_scoped_to_their_project() {
        let config = temp_dir("cfg");
        let project_a = temp_dir("proj-a");
        let project_b = temp_dir("proj-b");
        fs::create_dir_all(project_a.join("src")).unwrap();
        fs::write(project_a.join("src/main.as"), "class A {}").unwrap();
        fs::create_dir_all(project_b.join("src")).unwrap();
        fs::write(project_b.join("src/main.as"), "class B {}").unwrap();

        let mut store = AccessStore::load(&config).unwrap();
        store
            .grant(&project_a, &project_a.join("src"), AccessLevel::Read)
            .unwrap();
        store.save().unwrap();

        // Reload from disk: grant must persist and stay project-scoped.
        let store = AccessStore::load(&config).unwrap();
        let policy_a = store.policy_for(&project_a);
        let policy_b = store.policy_for(&project_b);
        assert!(policy_a.allows_read(&project_a.join("src/main.as").canonicalize().unwrap()));
        assert!(!policy_a.allows_write(&project_a.join("src/main.as").canonicalize().unwrap()));
        assert!(policy_b.grants.is_empty());
        assert!(!policy_b.allows_read(&project_b.join("src/main.as").canonicalize().unwrap()));

        let _ = fs::remove_dir_all(&config);
        let _ = fs::remove_dir_all(&project_a);
        let _ = fs::remove_dir_all(&project_b);
    }

    #[test]
    fn read_only_mode_caps_everything_to_read() {
        let mut policy = AccessPolicy {
            read_only: true,
            grants: vec![Grant {
                path: PathBuf::from("/tmp/x"),
                level: AccessLevel::ReadWrite,
            }],
        };
        assert_eq!(
            policy.level_for(Path::new("/tmp/x/f.as")),
            Some(AccessLevel::Read)
        );
        policy.read_only = false;
        assert_eq!(
            policy.level_for(Path::new("/tmp/x/f.as")),
            Some(AccessLevel::ReadWrite)
        );
    }

    #[test]
    fn sensitive_files_are_always_denied() {
        for path in [
            ".env",
            "config/.env.local",
            "certs/server.pem",
            "keys/id_rsa",
            "auth/credentials",
            ".ssh/config",
            ".git/config",
            "store/app.keystore",
        ] {
            assert!(is_sensitive(path), "{path} must be sensitive");
        }
        for path in [
            "scripts/Main.as",
            "assets/logo.png",
            "docs/readme.md",
            "src/env_utils.as",
        ] {
            assert!(!is_sensitive(path), "{path} must not be sensitive");
        }
    }

    #[test]
    fn guarded_workspace_blocks_ungranted_and_sensitive_reads() {
        let root = temp_dir("guarded");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/open.as"), "class Open {}").unwrap();
        fs::write(root.join(".env"), "SECRET=1").unwrap();
        let workspace = Workspace::new(&root).unwrap();

        // No grants at all → everything denied.
        let guarded = GuardedWorkspace::new(workspace.clone(), AccessPolicy::default());
        assert!(!guarded.can_read("src/open.as"));
        assert!(guarded.read("src/open.as").is_err());

        // Grant only src/ → .env still denied by the sensitive filter even
        // with a read-write grant on the root.
        let guarded = GuardedWorkspace::new(
            workspace,
            AccessPolicy {
                read_only: false,
                grants: vec![Grant {
                    path: root.canonicalize().unwrap(),
                    level: AccessLevel::ReadWrite,
                }],
            },
        );
        assert!(guarded.can_read("src/open.as"));
        assert!(!guarded.can_read(".env"));
        assert!(guarded.read(".env").is_err());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn grant_outside_project_is_rejected() {
        let config = temp_dir("cfg2");
        let project = temp_dir("proj");
        let outside = temp_dir("outside");
        let mut store = AccessStore::load(&config).unwrap();
        assert!(store.grant(&project, &outside, AccessLevel::Read).is_err());
        let _ = fs::remove_dir_all(&config);
        let _ = fs::remove_dir_all(&project);
        let _ = fs::remove_dir_all(&outside);
    }
}
