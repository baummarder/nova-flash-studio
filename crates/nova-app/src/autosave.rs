//! Crash-safe autosave files and recovery checks.
//!
//! Autosaves are written next to named projects (`<name>.nova.autosave`) and in
//! the user's temporary directory for unsaved projects. A recovery file records
//! the project it belongs to and the project revision, so a stale autosave can
//! never overwrite a newer project that the user opened or saved in the
//! meantime.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nova_core::Project;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    /// The project file this autosave belongs to, or `None` for untitled work.
    pub source: Option<PathBuf>,
    /// Unix timestamp when the autosave was written.
    pub saved_at_unix: u64,
    /// Full project snapshot. Its `metadata.modified_unix` records the revision
    /// at the moment of the autosave.
    pub project: Project,
}

impl Recovery {
    /// True when the snapshot is at least as new as the project loaded from
    /// disk. Equal revisions are accepted because the autosave may contain the
    /// final edit that was never saved.
    pub fn is_newer_than(&self, loaded: &Project) -> bool {
        self.project.metadata.modified_unix >= loaded.metadata.modified_unix
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Directory used for untitled projects and recovery indexes.
pub fn recovery_dir() -> PathBuf {
    std::env::temp_dir().join("nova-flash-studio")
}

/// The autosave file for a project. Named projects keep it next to the project
/// so that recovery also works after a reboot; untitled projects use the
/// temporary recovery directory.
pub fn autosave_path(project_path: Option<&Path>) -> PathBuf {
    match project_path {
        Some(path) => {
            let mut name = path.as_os_str().to_os_string();
            name.push(".autosave");
            PathBuf::from(name)
        }
        None => recovery_dir().join("untitled.nova.autosave"),
    }
}

/// Writes a recovery snapshot atomically (temporary file then rename).
pub fn write(project: &Project, project_path: Option<&Path>) -> Result<PathBuf, String> {
    let path = autosave_path(project_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let recovery = Recovery {
        source: project_path.map(Path::to_path_buf),
        saved_at_unix: now_unix(),
        project: project.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&recovery).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("autosave.tmp");
    {
        let mut file = fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Reads a recovery snapshot. Corrupt files are ignored rather than crashing
/// the editor.
pub fn read(path: &Path) -> Option<Recovery> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Returns the recovery snapshot for a project if one exists and is not older
/// than the project currently loaded from disk.
pub fn recover_for(project_path: Option<&Path>, loaded: &Project) -> Option<(PathBuf, Recovery)> {
    let path = autosave_path(project_path);
    let recovery = read(&path)?;
    if recovery.source.as_deref() != project_path {
        return None;
    }
    recovery.is_newer_than(loaded).then_some((path, recovery))
}

pub fn discard(project_path: Option<&Path>) {
    let _ = fs::remove_file(autosave_path(project_path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_core::{Metadata, Project};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nova-autosave-{tag}-{stamp}.nova"))
    }

    fn project(name: &str, revision: u64) -> Project {
        let mut project = Project::new_empty(name);
        project.metadata = Metadata::new(name);
        project.metadata.modified_unix = revision;
        project
    }

    #[test]
    fn writes_and_reads_named_recovery() {
        let path = temp_path("named");
        let project = project("Autosave", 100);
        let autosave = write(&project, Some(&path)).expect("write");
        let recovery = read(&autosave).expect("read");
        assert_eq!(recovery.source.as_deref(), Some(path.as_path()));
        assert_eq!(recovery.project.metadata.name, "Autosave");
        discard(Some(&path));
        assert!(!autosave.exists());
    }

    #[test]
    fn newer_revisions_are_recoverable_and_older_ones_are_not() {
        let loaded = project("Projekt", 20);
        let mut newer = project("Projekt", 21);
        let recovery = Recovery {
            source: None,
            saved_at_unix: 0,
            project: newer.clone(),
        };
        assert!(recovery.is_newer_than(&loaded));

        newer.metadata.modified_unix = 19;
        let stale = Recovery {
            source: None,
            saved_at_unix: 0,
            project: newer,
        };
        assert!(!stale.is_newer_than(&loaded));
    }

    #[test]
    fn untitled_autosave_lives_in_recovery_directory() {
        let path = autosave_path(None);
        assert_eq!(path, recovery_dir().join("untitled.nova.autosave"));
        let named = autosave_path(Some(Path::new("C:/tmp/example.nova")));
        assert!(named.to_string_lossy().ends_with("example.nova.autosave"));
    }
}
