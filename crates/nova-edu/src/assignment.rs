//! Assignments (M20.3): teacher-side definition + distribution metadata.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EduError, EduResult, safe_join};
use crate::store::{EDU_SCHEMA_VERSION, read_or, write_json};
use crate::workspace::{Workspace, now_unix};

/// Where the starter `.nova` comes from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StarterRef {
    /// No starter; students start from an empty/new project.
    None,
    /// A `.nova` file inside the workspace (relative path).
    Workspace { path: String },
    /// A starter bundled in a teaching template package.
    Template { id: String, path: String },
}

impl Default for StarterRef {
    fn default() -> Self {
        Self::None
    }
}

/// Which exports students must produce for the assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    NovaProject,
    Swf,
    Xfl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricCriterion {
    pub id: String,
    pub label: String,
    /// Ordered levels, lowest → highest (e.g. "offen", "teilweise", "erfüllt").
    pub levels: Vec<String>,
}

impl RubricCriterion {
    pub fn new(label: &str, levels: &[&str]) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            label: label.to_string(),
            levels: levels.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Target classes/groups for distribution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AssignmentTargets {
    /// `None`/empty = whole workspace-visible classes chosen by the teacher
    /// at distribution time; populated = explicit class ids.
    #[serde(default)]
    pub class_ids: Vec<Uuid>,
    /// Optional group restriction within those classes.
    #[serde(default)]
    pub group_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Assignment {
    pub schema: u32,
    pub id: Uuid,
    pub title: String,
    /// Markdown instructions shown to students.
    pub instructions_md: String,
    pub starter: StarterRef,
    pub rubric: Vec<RubricCriterion>,
    /// Unix timestamp; None = no deadline.
    pub due_unix: Option<u64>,
    /// Accept late submissions (marked, not rejected).
    pub allow_late: bool,
    pub required_exports: Vec<ExportKind>,
    /// Optional tutorial file inside a package (relative path).
    pub tutorial: Option<String>,
    pub targets: AssignmentTargets,
    pub archived: bool,
    pub created_unix: u64,
    pub updated_unix: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for Assignment {
    fn default() -> Self {
        let now = now_unix();
        Self {
            schema: EDU_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            title: String::new(),
            instructions_md: String::new(),
            starter: StarterRef::None,
            rubric: Vec::new(),
            due_unix: None,
            allow_late: true,
            required_exports: vec![ExportKind::NovaProject],
            tutorial: None,
            targets: AssignmentTargets::default(),
            archived: false,
            created_unix: now,
            updated_unix: now,
            extra: serde_json::Map::new(),
        }
    }
}

impl Assignment {
    pub fn is_open(&self, now: u64) -> bool {
        !self.archived && self.due_unix.map(|due| now <= due).unwrap_or(true)
    }

    pub fn days_until_due(&self, now: u64) -> Option<i64> {
        self.due_unix.map(|due| (due as i64 - now as i64) / 86_400)
    }

    pub fn validate(&self) -> EduResult<()> {
        if self.title.trim().is_empty() {
            return Err(EduError::Invalid(
                "Aufgabentitel darf nicht leer sein".into(),
            ));
        }
        if let StarterRef::Workspace { path } = &self.starter {
            // Path safety is enforced again at package time; here we only
            // require it to look relative.
            if path.contains("..") || std::path::Path::new(path).is_absolute() {
                return Err(EduError::Invalid(
                    "Starter-Pfad muss relativ und ohne .. sein".into(),
                ));
            }
        }
        for criterion in &self.rubric {
            if criterion.label.trim().is_empty() || criterion.levels.len() < 2 {
                return Err(EduError::Invalid(format!(
                    "Rubrik-Kriterium '{}' braucht label und >=2 Stufen",
                    criterion.label
                )));
            }
        }
        if let Some(t) = &self.tutorial {
            if t.contains("..") || std::path::Path::new(t).is_absolute() {
                return Err(EduError::Invalid(
                    "Tutorial-Pfad muss relativ und ohne .. sein".into(),
                ));
            }
        }
        Ok(())
    }
}

fn load_assignment(workspace: &Workspace, id: Uuid) -> EduResult<Assignment> {
    let mut a: Assignment = read_or(
        &workspace.assignment_path(id),
        Assignment {
            id,
            ..Assignment::default()
        },
    )?;
    if a.schema == 0 {
        a.schema = EDU_SCHEMA_VERSION;
    }
    Ok(a)
}

fn save_assignment(workspace: &Workspace, a: &Assignment) -> EduResult<()> {
    a.validate()?;
    write_json(&workspace.assignment_path(a.id), a)
}

pub fn list_assignments(workspace: &Workspace) -> EduResult<Vec<Assignment>> {
    let mut out = Vec::new();
    let dir = workspace.assignments_dir();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(id) = Uuid::parse_str(name) else {
            continue;
        };
        if let Ok(a) = load_assignment(workspace, id) {
            out.push(a);
        }
    }
    out.sort_by(|a, b| b.created_unix.cmp(&a.created_unix));
    Ok(out)
}

pub fn active_assignments(workspace: &Workspace) -> EduResult<Vec<Assignment>> {
    Ok(list_assignments(workspace)?
        .into_iter()
        .filter(|a| !a.archived)
        .collect())
}

pub fn get_assignment(workspace: &Workspace, id: Uuid) -> EduResult<Assignment> {
    if !workspace.assignment_path(id).exists() {
        return Err(EduError::NotFound(format!("Aufgabe {id} nicht gefunden")));
    }
    load_assignment(workspace, id)
}

pub fn save_new_assignment(workspace: &Workspace, mut a: Assignment) -> EduResult<Assignment> {
    a.schema = EDU_SCHEMA_VERSION;
    a.updated_unix = now_unix();
    save_assignment(workspace, &a)?;
    Ok(a)
}

pub fn update_assignment(
    workspace: &Workspace,
    id: Uuid,
    edit: impl FnOnce(&mut Assignment),
) -> EduResult<Assignment> {
    let mut a = get_assignment(workspace, id)?;
    edit(&mut a);
    a.updated_unix = now_unix();
    save_assignment(workspace, &a)?;
    Ok(a)
}

pub fn archive_assignment(workspace: &Workspace, id: Uuid) -> EduResult<Assignment> {
    update_assignment(workspace, id, |a| a.archived = true)
}

pub fn delete_assignment(workspace: &Workspace, id: Uuid) -> EduResult<()> {
    crate::store::remove_if_exists(&workspace.assignment_path(id))
}

/// Resolves the starter `.nova` path for packaging. Returns the absolute
/// source path plus the workspace-relative name to embed.
pub fn starter_source(
    workspace: &Workspace,
    a: &Assignment,
) -> EduResult<Option<(PathBuf, String)>> {
    match &a.starter {
        StarterRef::None => Ok(None),
        StarterRef::Workspace { path } => {
            let abs = safe_join(&workspace.root, path)?;
            if !abs.exists() {
                return Err(EduError::NotFound(format!(
                    "Starter-Datei '{}' fehlt",
                    path
                )));
            }
            let name = abs
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "starter.nova".into());
            Ok(Some((abs, format!("starter/{name}"))))
        }
        StarterRef::Template { path, .. } => {
            let abs = safe_join(&workspace.root.join("templates"), path)?;
            if !abs.exists() {
                return Err(EduError::NotFound(format!(
                    "Template-Starter '{}' fehlt",
                    path
                )));
            }
            let name = abs
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "starter.nova".into());
            Ok(Some((abs, format!("starter/{name}"))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::school::SchoolConfig;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova-edu-asg-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(&dir, &SchoolConfig::default()).unwrap();
        (ws, dir)
    }

    #[test]
    fn assignment_lifecycle() {
        let (ws, dir) = ws("life");
        let a = save_new_assignment(
            &ws,
            Assignment {
                title: "Erste Animation".into(),
                instructions_md: "# Aufgabe\nMach was.".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(active_assignments(&ws).unwrap().len(), 1);
        let edited = update_assignment(&ws, a.id, |a| a.title = "Zweite".into()).unwrap();
        assert_eq!(edited.title, "Zweite");
        archive_assignment(&ws, a.id).unwrap();
        assert!(active_assignments(&ws).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validation() {
        let bad = Assignment {
            title: "  ".into(),
            ..Default::default()
        };
        assert!(bad.validate().is_err());
        let bad2 = Assignment {
            title: "x".into(),
            starter: StarterRef::Workspace {
                path: "../evil.nova".into(),
            },
            ..Default::default()
        };
        assert!(bad2.validate().is_err());
    }

    #[test]
    fn due_date_logic() {
        let a = Assignment {
            due_unix: Some(1000),
            ..Default::default()
        };
        assert!(a.is_open(500));
        assert!(!a.is_open(1500));
        assert_eq!(a.days_until_due(1000), Some(0));
    }
}
