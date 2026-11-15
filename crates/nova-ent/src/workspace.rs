//! Enterprise workspace (M21.1): directory layout, org marker, metadata.
//!
//! An enterprise workspace is a plain directory containing `org.json`
//! plus `projects/`, `assets/`, `versions/`, `reviews/`, `builds/`,
//! `audit/` subdirectories. Project *files* stay where they are — the
//! workspace only records registrations (path + id + checksum).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EntError, EntResult, safe_join};
use crate::store::{ENT_SCHEMA_VERSION, now_unix, read_or, write_json};

/// On-disk marker identifying an enterprise workspace root.
pub const WORKSPACE_MARKER: &str = "org.json";

/// Organization-level metadata stored at the workspace root.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Organization {
    pub schema: u32,
    pub id: Uuid,
    pub name: String,
    /// Free-form department/team label.
    pub department: String,
    pub created_unix: u64,
    pub updated_unix: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for Organization {
    fn default() -> Self {
        let now = now_unix();
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: String::new(),
            department: String::new(),
            created_unix: now,
            updated_unix: now,
            extra: serde_json::Map::new(),
        }
    }
}

impl Organization {
    pub fn validate(&self) -> EntResult<()> {
        if self.name.trim().is_empty() {
            return Err(EntError::Invalid(
                "Organisationsname darf nicht leer sein".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

impl Workspace {
    /// Opens an existing workspace (marker file must exist).
    pub fn open(root: impl Into<PathBuf>) -> EntResult<Self> {
        let root = root.into();
        if !root.join(WORKSPACE_MARKER).exists() {
            return Err(EntError::NotFound(format!(
                "Kein Enterprise-Workspace in '{}' (org.json fehlt)",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    /// Creates a new workspace directory with org metadata.
    pub fn create(root: impl Into<PathBuf>, org: &Organization) -> EntResult<Self> {
        let root = root.into();
        if root.join(WORKSPACE_MARKER).exists() {
            return Err(EntError::Conflict(format!(
                "Workspace existiert bereits: {}",
                root.display()
            )));
        }
        std::fs::create_dir_all(&root)?;
        let ws = Self { root };
        ws.ensure_layout()?;
        let mut org = org.clone();
        org.schema = ENT_SCHEMA_VERSION;
        org.validate()?;
        ws.save_org(&org)?;
        Ok(ws)
    }

    /// Whether `path` looks like an enterprise workspace.
    pub fn is_workspace(path: &Path) -> bool {
        path.join(WORKSPACE_MARKER).exists()
    }

    pub fn ensure_layout(&self) -> EntResult<()> {
        for dir in [
            "projects",
            "assets/blobs",
            "versions",
            "reviews",
            "builds/history",
            "audit",
            "exports",
            "integrations",
        ] {
            std::fs::create_dir_all(self.root.join(dir))?;
        }
        Ok(())
    }

    // ------------------------------------------------------------- org

    pub fn org_path(&self) -> PathBuf {
        self.root.join(WORKSPACE_MARKER)
    }

    pub fn org(&self) -> EntResult<Organization> {
        let mut org: Organization = read_or(&self.org_path(), Organization::default())?;
        if org.schema == 0 {
            org.schema = ENT_SCHEMA_VERSION;
        }
        Ok(org)
    }

    pub fn save_org(&self, org: &Organization) -> EntResult<()> {
        org.validate()?;
        let mut org = org.clone();
        org.schema = ENT_SCHEMA_VERSION;
        org.updated_unix = now_unix();
        write_json(&self.org_path(), &org)
    }

    // ------------------------------------------------------------- paths

    pub fn projects_dir(&self) -> PathBuf {
        self.root.join("projects")
    }
    pub fn project_reg_path(&self, id: Uuid) -> PathBuf {
        self.projects_dir().join(format!("{id}.json"))
    }
    pub fn assets_index_path(&self) -> PathBuf {
        self.root.join("assets").join("index.json")
    }
    pub fn asset_blob_path(&self, sha256: &str) -> PathBuf {
        self.root
            .join("assets")
            .join("blobs")
            .join(format!("{sha256}.bin"))
    }
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn version_dir(&self, project_id: Uuid) -> PathBuf {
        self.versions_dir().join(project_id.to_string())
    }
    pub fn reviews_dir(&self) -> PathBuf {
        self.root.join("reviews")
    }
    pub fn review_path(&self, id: Uuid) -> PathBuf {
        self.reviews_dir().join(format!("{id}.json"))
    }
    pub fn builds_dir(&self) -> PathBuf {
        self.root.join("builds")
    }
    pub fn profiles_path(&self) -> PathBuf {
        self.builds_dir().join("profiles.json")
    }
    pub fn build_history_path(&self) -> PathBuf {
        self.builds_dir().join("history.jsonl")
    }
    pub fn audit_path(&self) -> PathBuf {
        self.root.join("audit").join("audit.jsonl")
    }
    pub fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }
    pub fn integrations_path(&self) -> PathBuf {
        self.root.join("integrations").join("config.json")
    }
    pub fn team_path(&self) -> PathBuf {
        self.root.join("team.json")
    }

    /// Workspace-relative path for a file, or error on escape.
    pub fn rel_path(&self, abs: &Path) -> EntResult<String> {
        abs.strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .map_err(|_| EntError::Path {
                path: abs.display().to_string(),
                reason: "liegt außerhalb des Workspace".into(),
            })
    }

    pub fn abs_path(&self, rel: &str) -> EntResult<PathBuf> {
        safe_join(&self.root, rel)
    }
}

/// Workspace-level preferences kept in `meta.json` (separate from org.json
/// so org metadata can be exported/shared without local UI state).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspacePrefs {
    pub schema: u32,
    /// Last selected project registration id.
    pub last_project: Option<Uuid>,
    /// Last selected tab in the enterprise view.
    pub last_tab: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for WorkspacePrefs {
    fn default() -> Self {
        Self {
            schema: ENT_SCHEMA_VERSION,
            last_project: None,
            last_tab: "overview".into(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Workspace {
    pub fn prefs_path(&self) -> PathBuf {
        self.root.join("meta.json")
    }

    pub fn prefs(&self) -> EntResult<WorkspacePrefs> {
        let mut p: WorkspacePrefs = read_or(&self.prefs_path(), WorkspacePrefs::default())?;
        if p.schema == 0 {
            p.schema = ENT_SCHEMA_VERSION;
        }
        Ok(p)
    }

    pub fn save_prefs(&self, prefs: &WorkspacePrefs) -> EntResult<()> {
        write_json(&self.prefs_path(), prefs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nova-ent-ws-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn create_open_roundtrip() {
        let dir = tempdir("rt");
        let org = Organization {
            name: "Muster GmbH".into(),
            department: "Medien".into(),
            ..Default::default()
        };
        let ws = Workspace::create(&dir, &org).unwrap();
        assert!(Workspace::is_workspace(&dir));
        let ws2 = Workspace::open(&dir).unwrap();
        assert_eq!(ws2.org().unwrap().name, "Muster GmbH");
        assert!(ws.projects_dir().exists());
        assert!(ws.audit_path().parent().unwrap().exists());
        // Re-create must conflict.
        assert!(Workspace::create(&dir, &org).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn org_validation() {
        let bad = Organization {
            name: "  ".into(),
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn safe_paths() {
        let dir = tempdir("paths");
        let ws = Workspace::create(&dir, &Organization::default()).unwrap_err();
        let _ = ws;
        // open fails without org name anyway; test safe_join directly
        assert!(safe_join(std::path::Path::new("/tmp/x"), "../evil").is_err());
        assert!(safe_join(std::path::Path::new("/tmp/x"), "ok/file.json").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
