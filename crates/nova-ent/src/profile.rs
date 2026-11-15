//! Local enterprise profile + organization/team model (M21.1/M21.2).
//!
//! IMPORTANT: everything here is **local workflow data**. There is no
//! authentication, no password, no session. The OS filesystem ACLs are
//! the real security boundary; roles decide what the UI offers and what
//! the domain API accepts *for the current local operator*.

use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::EntResult;
use crate::store::{ENT_SCHEMA_VERSION, read_or, write_json};

/// Enterprise workflow roles. Ordered roughly by privilege for display;
/// the permission matrix lives in `permissions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnterpriseRole {
    Owner,
    Admin,
    ProjectManager,
    Developer,
    Designer,
    Reviewer,
    Viewer,
}

impl EnterpriseRole {
    pub const ALL: &[EnterpriseRole] = &[
        Self::Owner,
        Self::Admin,
        Self::ProjectManager,
        Self::Developer,
        Self::Designer,
        Self::Reviewer,
        Self::Viewer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Owner => "Organisation Owner",
            Self::Admin => "Administrator",
            Self::ProjectManager => "Projektmanager",
            Self::Developer => "Developer",
            Self::Designer => "Designer",
            Self::Reviewer => "Reviewer",
            Self::Viewer => "Betrachter",
        }
    }

    /// Parses a serde string form back into the role.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|r| {
            serde_json::to_value(r)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .as_deref()
                == Some(s)
        })
    }

    /// Rank for "at least" comparisons (higher = more privileged).
    pub fn rank(self) -> u8 {
        match self {
            Self::Owner => 70,
            Self::Admin => 60,
            Self::ProjectManager => 50,
            Self::Developer => 40,
            Self::Designer => 40,
            Self::Reviewer => 30,
            Self::Viewer => 10,
        }
    }
}

/// Actions the permission matrix can grant. These are *workflow* actions —
/// see module docs for the security caveat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Open/read project files and workspace data.
    ReadProjects,
    /// Create/edit project registrations and open projects.
    EditProjects,
    /// Create snapshots / restore versions.
    ManageVersions,
    /// Manage the shared asset library.
    ManageAssets,
    /// Create and decide reviews.
    ManageReviews,
    /// Create/edit build profiles and run builds.
    ManageBuilds,
    /// Manage team members and roles.
    ManageTeam,
    /// Edit organization settings and integrations.
    ManageOrg,
    /// View audit log and admin overview.
    ViewAdmin,
}

impl Action {
    pub const ALL: &[Action] = &[
        Self::ReadProjects,
        Self::EditProjects,
        Self::ManageVersions,
        Self::ManageAssets,
        Self::ManageReviews,
        Self::ManageBuilds,
        Self::ManageTeam,
        Self::ManageOrg,
        Self::ViewAdmin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ReadProjects => "Projekte lesen",
            Self::EditProjects => "Projekte bearbeiten",
            Self::ManageVersions => "Versionen verwalten",
            Self::ManageAssets => "Asset-Bibliothek verwalten",
            Self::ManageReviews => "Reviews verwalten",
            Self::ManageBuilds => "Builds verwalten",
            Self::ManageTeam => "Team verwalten",
            Self::ManageOrg => "Organisation verwalten",
            Self::ViewAdmin => "Administration ansehen",
        }
    }
}

/// Default permission matrix: role → allowed actions. This is a *policy*
/// for the local operator's workflow, not an authentication boundary.
pub fn default_permissions(role: EnterpriseRole) -> Vec<Action> {
    use Action::*;
    use EnterpriseRole::*;
    let all: &[Action] = Action::ALL;
    match role {
        Owner | Admin => all.to_vec(),
        ProjectManager => vec![
            ReadProjects,
            EditProjects,
            ManageVersions,
            ManageAssets,
            ManageReviews,
            ManageBuilds,
            ViewAdmin,
        ],
        Developer => vec![
            ReadProjects,
            EditProjects,
            ManageVersions,
            ManageAssets,
            ManageBuilds,
        ],
        Designer => vec![ReadProjects, EditProjects, ManageAssets],
        Reviewer => vec![ReadProjects, ManageReviews],
        Viewer => vec![ReadProjects],
    }
}

/// Whether `role` may perform `action` under the default matrix.
/// Per-project overrides live on `ProjectGrant` and are evaluated by
/// `effective_permissions`.
pub fn can(role: EnterpriseRole, action: Action) -> bool {
    default_permissions(role).contains(&action)
}

/// Local profile of the machine operator (config dir, not workspace).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EntProfile {
    pub schema: u32,
    pub id: Uuid,
    pub display_name: String,
    /// Chosen workflow role for this workstation.
    pub role: EnterpriseRole,
    /// Optional link to a team member record (display convenience only).
    pub member_id: Option<Uuid>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for EntProfile {
    fn default() -> Self {
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            display_name: String::new(),
            role: EnterpriseRole::Viewer,
            member_id: None,
            extra: serde_json::Map::new(),
        }
    }
}

impl EntProfile {
    fn profile_path(config_dir: &Path) -> std::path::PathBuf {
        config_dir.join("ent-profile.json")
    }

    pub fn load(config_dir: &Path) -> EntResult<Self> {
        let mut p: Self = read_or(&Self::profile_path(config_dir), Self::default())?;
        if p.schema == 0 {
            p.schema = ENT_SCHEMA_VERSION;
        }
        Ok(p)
    }

    pub fn save(&self, config_dir: &Path) -> EntResult<()> {
        write_json(&Self::profile_path(config_dir), self)
    }

    pub fn author_name(&self) -> &str {
        if self.display_name.trim().is_empty() {
            "Unbekannt"
        } else {
            self.display_name.trim()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_labels_and_ranks() {
        assert_eq!(EnterpriseRole::Owner.label(), "Organisation Owner");
        assert!(EnterpriseRole::Owner.rank() > EnterpriseRole::Viewer.rank());
        assert_eq!(
            EnterpriseRole::parse("developer"),
            Some(EnterpriseRole::Developer)
        );
        assert!(EnterpriseRole::parse("nope").is_none());
    }

    #[test]
    fn permission_matrix() {
        assert!(can(EnterpriseRole::Owner, Action::ManageOrg));
        assert!(can(EnterpriseRole::Developer, Action::ManageBuilds));
        assert!(!can(EnterpriseRole::Developer, Action::ManageTeam));
        assert!(can(EnterpriseRole::Reviewer, Action::ManageReviews));
        assert!(!can(EnterpriseRole::Reviewer, Action::EditProjects));
        assert_eq!(
            default_permissions(EnterpriseRole::Viewer),
            vec![Action::ReadProjects]
        );
    }

    #[test]
    fn profile_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nova-ent-prof-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = EntProfile {
            display_name: "Ada".into(),
            role: EnterpriseRole::Admin,
            ..Default::default()
        };
        p.save(&dir).unwrap();
        let loaded = EntProfile::load(&dir).unwrap();
        assert_eq!(loaded.display_name, "Ada");
        assert_eq!(loaded.role, EnterpriseRole::Admin);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
