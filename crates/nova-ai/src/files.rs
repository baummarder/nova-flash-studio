//! Safe, project-root-confined file operations for agent changes.
//!
//! The workspace is deliberately independent of egui and of the project model.
//! The UI previews every change, the user accepts it explicitly, and only then
//! is anything written. Writes use a same-directory temporary file and a
//! replace step; the previous content is returned for the editor's undo stack.

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::diff::{DiffPreview, diff_text};
use crate::error::{AiError, AiResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Create,
    Modify,
    Delete,
}

impl ChangeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Create => "Neu",
            Self::Modify => "Ändern",
            Self::Delete => "Löschen",
        }
    }
}

/// One file operation proposed by the agent or by a local editor action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedChange {
    pub path: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub delete: bool,
    /// Explicit operation when the caller knows it. `None` means infer from the
    /// file's existence, which is what an agent reply normally does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ChangeKind>,
}

impl ProposedChange {
    pub fn create(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            reason: String::new(),
            delete: false,
            kind: Some(ChangeKind::Create),
        }
    }

    pub fn modify(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            reason: String::new(),
            delete: false,
            kind: Some(ChangeKind::Modify),
        }
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: String::new(),
            reason: String::new(),
            delete: true,
            kind: Some(ChangeKind::Delete),
        }
    }

    pub fn inferred(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            reason: String::new(),
            delete: false,
            kind: None,
        }
    }

    pub fn kind(&self, existed: bool) -> ChangeKind {
        if self.delete {
            ChangeKind::Delete
        } else if let Some(kind) = self.kind {
            kind
        } else if existed {
            ChangeKind::Modify
        } else {
            ChangeKind::Create
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePreview {
    pub path: String,
    pub kind: ChangeKind,
    pub exists: bool,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub diff: DiffPreview,
    pub reason: String,
}

impl ChangePreview {
    pub fn summary(&self) -> String {
        format!(
            "{} · {} · {}",
            self.kind.label(),
            self.path,
            self.diff.summary()
        )
    }
}

#[derive(Debug, Clone)]
pub struct AppliedChange {
    pub path: String,
    pub previous: Option<String>,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn new(root: impl Into<PathBuf>) -> AiResult<Self> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(AiError::io)?;
        let root = root.canonicalize().map_err(AiError::io)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a project-relative path and reject absolute paths, drive
    /// prefixes, parent traversal and symlink escapes.
    pub fn resolve(&self, relative: &str) -> AiResult<PathBuf> {
        let normalized = relative.trim().replace('\\', "/");
        if normalized.is_empty() {
            return Err(AiError::UnsafePath("Leerer Dateipfad".into()));
        }
        if normalized.starts_with('/') || normalized.starts_with("//") {
            return Err(AiError::UnsafePath(relative.to_string()));
        }
        let path = Path::new(&normalized);
        for component in path.components() {
            match component {
                Component::Normal(_) => {}
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(AiError::UnsafePath(relative.to_string()));
                }
            }
        }
        let candidate = self.root.join(path);
        if candidate.exists() {
            let canonical = candidate.canonicalize().map_err(AiError::io)?;
            if !canonical.starts_with(&self.root) {
                return Err(AiError::UnsafePath(relative.to_string()));
            }
            return Ok(canonical);
        }
        // For a new file, canonicalize the nearest existing ancestor. This
        // catches a symlinked directory that points outside the workspace.
        let mut ancestor = candidate.parent();
        while let Some(parent) = ancestor {
            if parent.exists() {
                let canonical = parent.canonicalize().map_err(AiError::io)?;
                if !canonical.starts_with(&self.root) {
                    return Err(AiError::UnsafePath(relative.to_string()));
                }
                break;
            }
            ancestor = parent.parent();
        }
        Ok(candidate)
    }

    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    pub fn preview(&self, change: &ProposedChange) -> AiResult<ChangePreview> {
        let path = self.resolve(&change.path)?;
        let exists = path.exists();
        if exists && path.is_dir() {
            return Err(AiError::Unsupported(format!(
                "'{}' ist ein Verzeichnis.",
                change.path
            )));
        }
        let old_content = if exists {
            Some(fs::read_to_string(&path).map_err(AiError::io)?)
        } else {
            None
        };
        let kind = change.kind(exists);
        match kind {
            ChangeKind::Create if exists => {
                return Err(AiError::WouldOverwrite(path));
            }
            ChangeKind::Modify if !exists => {
                return Err(AiError::MissingFile(path));
            }
            ChangeKind::Delete if !exists => {
                return Err(AiError::MissingFile(path));
            }
            _ => {}
        }
        let new_content = if kind == ChangeKind::Delete {
            None
        } else {
            Some(change.content.clone())
        };
        let diff = diff_text(
            old_content.as_deref().unwrap_or(""),
            new_content.as_deref().unwrap_or(""),
        );
        Ok(ChangePreview {
            path: change.path.clone(),
            kind,
            exists,
            old_content,
            new_content,
            diff,
            reason: change.reason.clone(),
        })
    }

    pub fn apply(&self, change: &ProposedChange) -> AiResult<AppliedChange> {
        let preview = self.preview(change)?;
        let path = self.resolve(&change.path)?;
        let previous = preview.old_content.clone();
        match preview.kind {
            ChangeKind::Create | ChangeKind::Modify => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(AiError::io)?;
                }
                atomic_write(&path, preview.new_content.as_deref().unwrap_or(""))?;
            }
            ChangeKind::Delete => {
                fs::remove_file(&path).map_err(AiError::io)?;
            }
        }
        Ok(AppliedChange {
            path: change.path.clone(),
            previous,
            kind: preview.kind,
        })
    }

    pub fn undo(&self, applied: &AppliedChange) -> AiResult<()> {
        let path = self.resolve(&applied.path)?;
        match (&applied.previous, applied.kind) {
            (Some(previous), _) => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(AiError::io)?;
                }
                atomic_write(&path, previous)
            }
            (None, ChangeKind::Create) => {
                if path.exists() {
                    fs::remove_file(path).map_err(AiError::io)?;
                }
                Ok(())
            }
            (None, _) => Err(AiError::MissingFile(path)),
        }
    }

    pub fn read(&self, relative: &str) -> AiResult<String> {
        let path = self.resolve(relative)?;
        fs::read_to_string(path).map_err(AiError::io)
    }

    /// Reads arbitrary asset bytes through the same workspace confinement as
    /// source files. Used by asset import project operations.
    pub fn read_bytes(&self, relative: &str) -> AiResult<Vec<u8>> {
        let path = self.resolve(relative)?;
        fs::read(path).map_err(AiError::io)
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.resolve(relative)
            .map(|path| path.exists())
            .unwrap_or(false)
    }

    /// List editable source-like files for the explorer. Traversal is bounded
    /// and symlinks are skipped.
    pub fn list_source_files(&self) -> Vec<String> {
        let mut files = Vec::new();
        self.walk(&self.root, &mut files, 0);
        files.sort();
        files.truncate(300);
        files
    }

    fn walk(&self, dir: &Path, files: &mut Vec<String>, depth: usize) {
        if depth > 4 || files.len() >= 300 {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || matches!(name.as_str(), "target" | "node_modules" | "dist")
            {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                self.walk(&path, files, depth + 1);
            } else if file_type.is_file() {
                let extension = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    extension.as_str(),
                    "as" | "mxml" | "xml" | "json" | "txt" | "md"
                ) {
                    files.push(self.relative(&path));
                }
            }
        }
    }
}

fn atomic_write(path: &Path, content: &str) -> AiResult<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temporary = path.with_file_name(format!(".{file_name}.nova-ai.tmp"));
    fs::write(&temporary, content.as_bytes()).map_err(AiError::io)?;
    if path.exists() {
        fs::remove_file(path).map_err(AiError::io)?;
    }
    fs::rename(&temporary, path).map_err(AiError::io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace(name: &str) -> Workspace {
        let root =
            std::env::temp_dir().join(format!("nova-ai-files-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        Workspace::new(&root).unwrap()
    }

    #[test]
    fn rejects_parent_traversal_and_absolute_paths() {
        let workspace = temp_workspace("traversal");
        assert!(workspace.resolve("../evil.as").is_err());
        assert!(workspace.resolve("scripts/../../evil.as").is_err());
        assert!(workspace.resolve("C:/Windows/evil.as").is_err());
        assert!(workspace.resolve("/etc/passwd").is_err());
        assert!(workspace.resolve("scripts/Main.as").is_ok());
        let _ = fs::remove_dir_all(workspace.root());
    }

    #[test]
    fn preview_refuses_to_overwrite_and_apply_is_reversible() {
        let workspace = temp_workspace("preview");
        let create = ProposedChange::create("scripts/Main.as", "class Main {}\n");
        let preview = workspace.preview(&create).unwrap();
        assert_eq!(preview.kind, ChangeKind::Create);
        assert!(!preview.exists);
        let applied = workspace.apply(&create).unwrap();
        assert_eq!(
            workspace.read("scripts/Main.as").unwrap(),
            "class Main {}\n"
        );
        assert!(workspace.preview(&create).is_err());

        let modify = ProposedChange::modify("scripts/Main.as", "class Main { int x; }\n");
        let preview = workspace.preview(&modify).unwrap();
        assert_eq!(preview.kind, ChangeKind::Modify);
        assert!(preview.diff.added > 0);
        workspace.apply(&modify).unwrap();
        workspace.undo(&applied).unwrap();
        assert!(!workspace.exists("scripts/Main.as"));

        let _ = fs::remove_dir_all(workspace.root());
    }

    #[test]
    fn delete_requires_confirmation_and_can_be_undone() {
        let workspace = temp_workspace("delete");
        workspace
            .apply(&ProposedChange::create("scripts/Old.as", "old\n"))
            .unwrap();
        let delete = ProposedChange::delete("scripts/Old.as");
        let preview = workspace.preview(&delete).unwrap();
        assert_eq!(preview.kind, ChangeKind::Delete);
        let applied = workspace.apply(&delete).unwrap();
        assert!(!workspace.exists("scripts/Old.as"));
        workspace.undo(&applied).unwrap();
        assert_eq!(workspace.read("scripts/Old.as").unwrap(), "old\n");
        let _ = fs::remove_dir_all(workspace.root());
    }

    #[test]
    fn source_listing_is_bounded_and_ignores_build_directories() {
        let workspace = temp_workspace("listing");
        workspace
            .apply(&ProposedChange::create(
                "scripts/Main.as",
                "class Main {}\n",
            ))
            .unwrap();
        fs::create_dir_all(workspace.root().join("target")).unwrap();
        fs::write(workspace.root().join("target/ignored.as"), "x").unwrap();
        let files = workspace.list_source_files();
        assert!(files.contains(&"scripts/Main.as".to_string()));
        assert!(!files.iter().any(|file| file.starts_with("target/")));
        let _ = fs::remove_dir_all(workspace.root());
    }
}
