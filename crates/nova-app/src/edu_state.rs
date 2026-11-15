//! Classroom-mode UI state (M20.7). Kept separate from `AppState`/`Project`
//! so classroom data never leaks into `.nova` files, undo history or
//! autosave snapshots.

use std::path::PathBuf;

use nova_edu::profile::{EduRole, LocalProfile};
use nova_edu::school::{EduPrefs, SchoolConfig};
use nova_edu::workspace::Workspace;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EduTab {
    Overview,
    Classes,
    Assignments,
    Submissions,
    Feedback,
    Progress,
    Templates,
}

impl EduTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Übersicht",
            Self::Classes => "Klassen",
            Self::Assignments => "Aufgaben",
            Self::Submissions => "Abgaben",
            Self::Feedback => "Feedback",
            Self::Progress => "Fortschritt",
            Self::Templates => "Vorlagen",
        }
    }

    pub const ALL: &[EduTab] = &[
        Self::Overview,
        Self::Classes,
        Self::Assignments,
        Self::Submissions,
        Self::Feedback,
        Self::Progress,
        Self::Templates,
    ];
}

#[derive(Debug)]
pub struct EduUi {
    pub workspace: Option<Workspace>,
    pub profile: LocalProfile,
    pub prefs: EduPrefs,
    pub school: SchoolConfig,
    pub tab: EduTab,
    /// Currently selected class in the Classes tab.
    pub selected_class: Option<Uuid>,
    /// Currently selected assignment (context for submissions/feedback).
    pub selected_assignment: Option<Uuid>,
    /// Selected member inside a class detail view.
    pub selected_member: Option<Uuid>,
    /// Submission currently open in read-only review (project path).
    pub review_project: Option<PathBuf>,
    pub status: String,
    pub error: Option<String>,
    /// Simple text inputs for dialogs (class name, member name, …).
    pub input_class_name: String,
    pub input_member_name: String,
    pub input_pseudonym: String,
    pub input_group_name: String,
    pub input_assignment_title: String,
    pub input_assignment_instructions: String,
    /// Assignment draft starter path (workspace-relative).
    pub draft_starter: String,
}

impl Default for EduUi {
    fn default() -> Self {
        Self {
            workspace: None,
            profile: LocalProfile::default(),
            prefs: EduPrefs::default(),
            school: SchoolConfig::default(),
            tab: EduTab::Overview,
            selected_class: None,
            selected_assignment: None,
            selected_member: None,
            review_project: None,
            status: String::new(),
            error: None,
            input_class_name: String::new(),
            input_member_name: String::new(),
            input_pseudonym: String::new(),
            input_group_name: String::new(),
            input_assignment_title: String::new(),
            input_assignment_instructions: String::new(),
            draft_starter: String::new(),
        }
    }
}

impl EduUi {
    /// Loads local profile/prefs from the edition config dir.
    pub fn load_local(config_dir: &std::path::Path) -> Self {
        let mut ui = Self::default();
        match LocalProfile::load(config_dir) {
            Ok(p) => ui.profile = p,
            Err(e) => ui.error = Some(format!("Profil: {e}")),
        }
        match EduPrefs::load(config_dir) {
            Ok(p) => {
                ui.prefs = p;
                if ui.prefs.language.is_empty() {
                    ui.prefs.language = "de".into();
                }
            }
            Err(e) => ui.error = Some(format!("Einstellungen: {e}")),
        }
        ui
    }

    pub fn save_local(&self, config_dir: &std::path::Path) {
        let _ = self.profile.save(config_dir);
        let _ = self.prefs.save(config_dir);
    }

    pub fn role(&self) -> EduRole {
        self.profile.role
    }

    pub fn is_teacher(&self) -> bool {
        matches!(self.profile.role, EduRole::Teacher | EduRole::Assistant)
    }

    pub fn open_workspace(&mut self, root: PathBuf) -> Result<(), String> {
        let ws = Workspace::open(&root).map_err(|e| e.to_string())?;
        self.workspace = Some(ws);
        self.reload_school();
        self.status = format!("Workspace geöffnet: {}", root.display());
        self.error = None;
        Ok(())
    }

    pub fn create_workspace(&mut self, root: PathBuf, school_name: &str) -> Result<(), String> {
        let school = SchoolConfig {
            school: school_name.to_string(),
            ..Default::default()
        };
        let ws = Workspace::create(&root, &school).map_err(|e| e.to_string())?;
        self.workspace = Some(ws);
        self.reload_school();
        self.status = format!("Workspace erstellt: {}", root.display());
        self.error = None;
        Ok(())
    }

    pub fn create_sample(&mut self) -> Result<(), String> {
        let Some(ws) = &self.workspace else {
            return Err("Kein Workspace geöffnet.".into());
        };
        nova_edu::template::create_sample_workspace(ws).map_err(|e| e.to_string())?;
        self.reload_school();
        self.status = "Beispiel-Workspace erstellt.".into();
        self.error = None;
        Ok(())
    }

    pub fn reload_school(&mut self) {
        if let Some(ws) = &self.workspace {
            self.school = SchoolConfig::load(ws).unwrap_or_default();
        }
    }

    pub fn set_error(&mut self, e: impl Into<String>) {
        self.error = Some(e.into());
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }
}
