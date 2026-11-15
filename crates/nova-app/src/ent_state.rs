//! Enterprise-mode UI state (M21.9). Kept separate from `AppState`/`Project`
//! so enterprise data never leaks into `.nova` files, undo history or
//! autosave snapshots.

use std::path::PathBuf;

use nova_ent::profile::{EntProfile, EnterpriseRole};
use nova_ent::workspace::Workspace;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntTab {
    Overview,
    Projects,
    Team,
    Assets,
    Versions,
    Reviews,
    Builds,
    Admin,
    Integrations,
}

impl EntTab {
    pub fn label(self) -> String {
        crate::i18n::t(match self {
            Self::Overview => "ent.tab.overview",
            Self::Projects => "ent.tab.projects",
            Self::Team => "ent.tab.team",
            Self::Assets => "ent.tab.assets",
            Self::Versions => "ent.tab.versions",
            Self::Reviews => "ent.tab.reviews",
            Self::Builds => "ent.tab.builds",
            Self::Admin => "ent.tab.admin",
            Self::Integrations => "ent.tab.integrations",
        })
    }

    pub const ALL: &[EntTab] = &[
        Self::Overview,
        Self::Projects,
        Self::Team,
        Self::Assets,
        Self::Versions,
        Self::Reviews,
        Self::Builds,
        Self::Admin,
        Self::Integrations,
    ];
}

#[derive(Debug)]
pub struct EntUi {
    pub workspace: Option<Workspace>,
    pub profile: EntProfile,
    pub tab: EntTab,
    pub status: String,
    pub error: Option<String>,

    // Selections
    pub selected_project: Option<Uuid>,
    pub selected_member: Option<Uuid>,
    pub selected_asset: Option<Uuid>,
    pub selected_review: Option<Uuid>,
    pub selected_version: Option<Uuid>,
    pub selected_profile: Option<Uuid>,
    pub selected_integration: Option<Uuid>,

    // Inputs (dialogs/forms)
    pub input_org_name: String,
    pub input_member_name: String,
    pub input_member_role: EnterpriseRole,
    pub input_asset_name: String,
    pub input_asset_folder: String,
    pub input_asset_tags: String,
    pub input_version_message: String,
    pub input_review_title: String,
    pub input_review_desc: String,
    pub input_review_comment: String,
    pub input_review_category: nova_ent::review::CommentCategory,
    pub input_profile_name: String,
    pub input_integration_name: String,
    pub input_integration_kind: nova_ent::integrations::IntegrationKind,
    pub input_integration_endpoint: String,
    pub input_integration_model: String,
    pub input_integration_key_env: String,
    pub input_package_dir: PathBuf,
}

impl Default for EntUi {
    fn default() -> Self {
        Self {
            workspace: None,
            profile: EntProfile::default(),
            tab: EntTab::Overview,
            status: String::new(),
            error: None,
            selected_project: None,
            selected_member: None,
            selected_asset: None,
            selected_review: None,
            selected_version: None,
            selected_profile: None,
            selected_integration: None,
            input_org_name: String::new(),
            input_member_name: String::new(),
            input_member_role: EnterpriseRole::Developer,
            input_asset_name: String::new(),
            input_asset_folder: String::new(),
            input_asset_tags: String::new(),
            input_version_message: String::new(),
            input_review_title: String::new(),
            input_review_desc: String::new(),
            input_review_comment: String::new(),
            input_review_category: nova_ent::review::CommentCategory::Problem,
            input_profile_name: String::new(),
            input_integration_name: String::new(),
            input_integration_kind: nova_ent::integrations::IntegrationKind::Ollama,
            input_integration_endpoint: String::new(),
            input_integration_model: String::new(),
            input_integration_key_env: String::new(),
            input_package_dir: std::env::temp_dir(),
        }
    }
}

impl EntUi {
    /// Loads local profile from the edition config dir.
    pub fn load_local(config_dir: &std::path::Path) -> Self {
        let mut ui = Self::default();
        match EntProfile::load(config_dir) {
            Ok(p) => ui.profile = p,
            Err(e) => ui.error = Some(format!("Profil: {e}")),
        }
        ui
    }

    pub fn save_local(&self, config_dir: &std::path::Path) {
        let _ = self.profile.save(config_dir);
    }

    pub fn open_workspace(&mut self, root: PathBuf) -> Result<(), String> {
        let ws = Workspace::open(&root).map_err(|e| e.to_string())?;
        self.workspace = Some(ws);
        self.status = format!("Enterprise-Workspace geöffnet: {}", root.display());
        self.error = None;
        Ok(())
    }

    pub fn create_workspace(&mut self, root: PathBuf, org_name: &str) -> Result<(), String> {
        let org = nova_ent::workspace::Organization {
            name: org_name.to_string(),
            ..Default::default()
        };
        let ws = Workspace::create(&root, &org).map_err(|e| e.to_string())?;
        self.workspace = Some(ws);
        self.status = format!("Enterprise-Workspace erstellt: {}", root.display());
        self.error = None;
        Ok(())
    }

    pub fn set_error(&mut self, e: impl Into<String>) {
        self.error = Some(e.into());
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Workspace handle or a user-facing error string.
    pub fn ws(&self) -> Result<&Workspace, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "Kein Enterprise-Workspace geöffnet.".to_string())
    }

    /// Actor label used for audit/version/review attribution.
    pub fn actor(&self) -> String {
        self.profile.author_name().to_string()
    }
}
