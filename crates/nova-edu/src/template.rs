//! Teaching template catalog + sample classroom (M20.7/M20.8).
//!
//! Templates are plain `.nova` files plus metadata; the catalog lives in
//! the workspace (`templates/catalog.json`) or falls back to bundled
//! samples generated at runtime. Nothing here requires a network.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::EduResult;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateEntry {
    pub id: String,
    pub title: String,
    pub description: String,
    /// `.nova` file relative to the catalog dir.
    pub project: String,
    /// Difficulty/level hint shown in UI.
    pub level: String,
    /// Estimated minutes (informational only).
    pub minutes: u32,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCatalog {
    pub schema: u32,
    #[serde(default)]
    pub templates: Vec<TemplateEntry>,
}

impl Default for TemplateCatalog {
    fn default() -> Self {
        Self {
            schema: crate::store::EDU_SCHEMA_VERSION,
            templates: Vec::new(),
        }
    }
}

impl TemplateCatalog {
    pub fn load(workspace: &Workspace) -> EduResult<Self> {
        let path = workspace.root.join("templates").join("catalog.json");
        crate::store::read_or(&path, Self::default())
    }

    pub fn save(&self, workspace: &Workspace) -> EduResult<()> {
        crate::store::write_json(&workspace.root.join("templates").join("catalog.json"), self)
    }
}

/// Built-in tutorial topics (Markdown text served by the in-app viewer).
pub fn builtin_tutorials() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "erste-form",
            "Erste Form zeichnen",
            "# Erste Form\n\n1. Wähle das Rechteck-Werkzeug.\n2. Klicke auf die Bühne und ziehe.\n3. Wechsle zur Timeline und setze ein Schlüsselbild.\n4. Exportiere probehalber als SWF.\n",
        ),
        (
            " tween",
            "Ein einfacher Tween",
            "# Tween\n\n1. Setze auf Frame 1 ein Schlüsselbild.\n2. Gehe zu Frame 12, verschiebe das Objekt, setze wieder ein Schlüsselbild.\n3. Wähle den Tween dazwischen.\n",
        ),
        (
            "abgabe",
            "Abgeben und Feedback",
            "# Abgeben\n\n1. Speichere dein Projekt als `.nova`.\n2. Exportiere es als `.novasubmission`-Paket.\n3. Die Lehrkraft öffnet es read-only und gibt Feedback.\n",
        ),
    ]
}

/// Creates a demo workspace populated with a sample class, assignment and
/// tutorial stubs — used by onboarding and tests.
pub fn create_sample_workspace(workspace: &Workspace) -> EduResult<()> {
    workspace.ensure_layout()?;

    // Sample class with two groups and three members.
    let class = crate::classroom::create_classroom(workspace, "Beispielkurs Grafik")?;
    let g1 = crate::classroom::add_group(workspace, class.id, "Gruppe A")?;
    let _g2 = crate::classroom::add_group(workspace, class.id, "Gruppe B")?;
    for (name, group) in [
        ("Alex Beispiel", Some(g1.id)),
        ("Mia Muster", None),
        ("Leo Probe", None),
    ] {
        let m = crate::classroom::add_member(workspace, class.id, name, None)?;
        if let Some(gid) = group {
            crate::classroom::move_member_to_group(workspace, class.id, m.id, Some(gid))?;
        }
    }

    // Sample assignment with an empty-project starter.
    let starter_path = workspace
        .root
        .join("templates")
        .join("starter-erste-form.nova");
    std::fs::create_dir_all(starter_path.parent().unwrap())?;
    let project = nova_core::ProjectTemplate::Empty.create("Erste Form");
    project
        .save(&starter_path)
        .map_err(|e| crate::error::EduError::Invalid(format!("Starter-Projekt: {e}")))?;

    crate::assignment::save_new_assignment(
        workspace,
        crate::assignment::Assignment {
            title: "Aufgabe 1: Erste Form".into(),
            instructions_md:
                "# Aufgabe 1\n\nZeichne ein Rechteck und animiere es über 12 Frames.\n".into(),
            starter: crate::assignment::StarterRef::Workspace {
                path: "templates/starter-erste-form.nova".into(),
            },
            due_unix: Some(crate::workspace::now_unix() + 7 * 86_400),
            targets: crate::assignment::AssignmentTargets {
                class_ids: vec![class.id],
                ..Default::default()
            },
            ..Default::default()
        },
    )?;

    // Tutorial stubs.
    for (id, title, body) in builtin_tutorials() {
        let path = workspace.root.join("tutorials").join(format!("{id}.md"));
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, format!("# {title}\n\n{body}"))?;
    }

    // School config with safe defaults.
    let school = crate::school::SchoolConfig {
        school: "Beispielschule".into(),
        ..Default::default()
    };
    school.save(workspace)?;

    let _ = Uuid::nil();
    Ok(())
}

/// Whether a directory already looks like a populated edu workspace.
pub fn is_populated(workspace: &Workspace) -> bool {
    workspace.classrooms_dir().exists()
        && std::fs::read_dir(workspace.classrooms_dir())
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::school::SchoolConfig;

    #[test]
    fn sample_workspace_populates() {
        let dir = std::env::temp_dir().join(format!("nova-edu-sample-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(&dir, &SchoolConfig::default()).unwrap();
        create_sample_workspace(&ws).unwrap();
        assert!(is_populated(&ws));
        let classes = crate::classroom::list_classrooms(&ws).unwrap();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].members.len(), 3);
        let asgs = crate::assignment::list_assignments(&ws).unwrap();
        assert_eq!(asgs.len(), 1);
        let _ = Uuid::nil();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
