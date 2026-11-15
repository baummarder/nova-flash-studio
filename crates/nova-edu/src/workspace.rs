//! Workspace layout and lifecycle.
//!
//! A workspace is a plain directory (school share, USB stick, local folder)
//! that contains `school.json` and the classroom data subdirectories. There
//! is no server and no lock service; concurrent access is the user's
//! responsibility (documented in EDU_GUIDE).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EduError, EduResult, safe_join};
use crate::school::SchoolConfig;
use crate::store::{EDU_SCHEMA_VERSION, read_or, write_json};

/// On-disk marker file identifying an edu workspace root.
pub const WORKSPACE_MARKER: &str = "school.json";

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    schema: u32,
    id: Uuid,
    created_unix: u64,
}

const META_FILE: &str = "workspace.json";

impl Workspace {
    /// Opens an existing workspace (must contain `school.json`).
    pub fn open(root: impl AsRef<Path>) -> EduResult<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.join(WORKSPACE_MARKER).exists() {
            return Err(EduError::NotFound(format!(
                "Kein Edu-Workspace unter '{}' (school.json fehlt)",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    /// Creates a new workspace at `root` (must not already be one).
    pub fn create(root: impl AsRef<Path>, school: &SchoolConfig) -> EduResult<Self> {
        let root = root.as_ref().to_path_buf();
        if root.join(WORKSPACE_MARKER).exists() {
            return Err(EduError::Conflict(format!(
                "'{}' ist bereits ein Edu-Workspace",
                root.display()
            )));
        }
        let ws = Self { root };
        ws.ensure_layout()?;
        write_json(
            &ws.meta_path(),
            &WorkspaceMeta {
                schema: EDU_SCHEMA_VERSION,
                id: Uuid::new_v4(),
                created_unix: now_unix(),
            },
        )?;
        school.save(&ws)?;
        Ok(ws)
    }

    pub fn ensure_layout(&self) -> EduResult<()> {
        for dir in [
            "classrooms",
            "assignments",
            "inbox",
            "feedback",
            "reports",
            "templates",
            "tutorials",
        ] {
            std::fs::create_dir_all(self.root.join(dir))?;
        }
        Ok(())
    }

    pub fn meta_path(&self) -> PathBuf {
        self.root.join(META_FILE)
    }

    pub fn meta(&self) -> EduResult<WorkspaceMeta> {
        read_or(
            &self.meta_path(),
            WorkspaceMeta {
                schema: EDU_SCHEMA_VERSION,
                id: Uuid::nil(),
                created_unix: 0,
            },
        )
    }

    // ------------------------------------------------------------- paths

    /// Resolves a workspace-relative path with traversal protection.
    pub fn path(&self, rel: &str) -> EduResult<PathBuf> {
        safe_join(&self.root, rel)
    }

    pub fn school_path(&self) -> PathBuf {
        self.root.join(WORKSPACE_MARKER)
    }

    pub fn classroom_path(&self, id: Uuid) -> PathBuf {
        self.root.join("classrooms").join(format!("{id}.json"))
    }

    pub fn classrooms_dir(&self) -> PathBuf {
        self.root.join("classrooms")
    }

    pub fn assignment_path(&self, id: Uuid) -> PathBuf {
        self.root.join("assignments").join(format!("{id}.json"))
    }

    pub fn assignments_dir(&self) -> PathBuf {
        self.root.join("assignments")
    }

    pub fn submission_dir(&self, class_id: Uuid, assignment_id: Uuid) -> PathBuf {
        self.root
            .join("inbox")
            .join(class_id.to_string())
            .join(assignment_id.to_string())
    }

    pub fn feedback_path(&self, class_id: Uuid, assignment_id: Uuid, member_id: Uuid) -> PathBuf {
        self.root
            .join("feedback")
            .join(class_id.to_string())
            .join(format!("{assignment_id}__{member_id}.json"))
    }

    pub fn reports_dir(&self) -> PathBuf {
        self.root.join("reports")
    }

    /// Recently used workspaces live in the *local* edu config dir, not in
    /// the workspace itself.
    pub fn recent_path(config_root: &Path) -> PathBuf {
        config_root.join("recent.json")
    }

    pub fn record_recent(&self, config_root: &Path) -> EduResult<()> {
        let path = Self::recent_path(config_root);
        let mut list: Vec<String> = crate::store::read_json(&path)?.unwrap_or_default();
        let entry = self.root.to_string_lossy().into_owned();
        list.retain(|p| p != &entry);
        list.insert(0, entry);
        list.truncate(10);
        write_json(&path, &list)
    }
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Local edu configuration directory inside the edition config dir.
pub fn local_config_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("edu")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::school::SchoolConfig;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nova-edu-ws-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn create_open_roundtrip() {
        let dir = tempdir("create");
        let ws = Workspace::create(&dir, &SchoolConfig::default()).unwrap();
        assert!(ws.school_path().exists());
        let reopened = Workspace::open(&dir).unwrap();
        assert_eq!(reopened.meta().unwrap().id, ws.meta().unwrap().id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_rejects_non_workspace() {
        let dir = tempdir("open");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(Workspace::open(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_rejects_traversal() {
        let dir = tempdir("path");
        let ws = Workspace::create(&dir, &SchoolConfig::default()).unwrap();
        assert!(ws.path("../escape").is_err());
        assert!(ws.path("classrooms/x.json").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_list_is_bounded() {
        let dir = tempdir("recent");
        let cfg = dir.join("cfg");
        let ws = Workspace::create(dir.join("ws"), &SchoolConfig::default()).unwrap();
        for _ in 0..15 {
            ws.record_recent(&cfg).unwrap();
        }
        let list: Vec<String> = crate::store::read_json(&Workspace::recent_path(&cfg))
            .unwrap()
            .unwrap();
        assert!(list.len() <= 10);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
