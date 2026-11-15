//! Team & role management (M21.2) + project registry (M21.1).
//!
//! Team members and project registrations live in the workspace as JSON.
//! Roles produce a *policy* permission set — see `profile` module docs
//! for the explicit non-authentication caveat.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EntError, EntResult};
use crate::profile::{Action, EnterpriseRole, can, default_permissions};
use crate::store::{ENT_SCHEMA_VERSION, now_unix, read_or, write_json};
use crate::workspace::Workspace;

// ------------------------------------------------------------------ team

/// A team member record. `external_ref` is an optional opaque id for a
/// future directory/SSO integration — never interpreted locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamMember {
    pub schema: u32,
    pub id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    /// Organization-level role.
    pub role: EnterpriseRole,
    /// Per-project role overrides: project registration id → role.
    pub project_roles: BTreeMap<Uuid, EnterpriseRole>,
    pub active: bool,
    pub created_unix: u64,
    pub updated_unix: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for TeamMember {
    fn default() -> Self {
        let now = now_unix();
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            display_name: String::new(),
            email: None,
            role: EnterpriseRole::Viewer,
            project_roles: BTreeMap::new(),
            active: true,
            created_unix: now,
            updated_unix: now,
            extra: serde_json::Map::new(),
        }
    }
}

/// Whole team file (`team.json` at workspace root).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Team {
    pub schema: u32,
    pub members: Vec<TeamMember>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for Team {
    fn default() -> Self {
        Self {
            schema: ENT_SCHEMA_VERSION,
            members: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

fn load_team(workspace: &Workspace) -> EntResult<Team> {
    let mut team: Team = read_or(&workspace.team_path(), Team::default())?;
    if team.schema == 0 {
        team.schema = ENT_SCHEMA_VERSION;
    }
    Ok(team)
}

fn save_team(workspace: &Workspace, team: &Team) -> EntResult<()> {
    write_json(&workspace.team_path(), team)
}

pub fn list_members(workspace: &Workspace) -> EntResult<Vec<TeamMember>> {
    Ok(load_team(workspace)?.members)
}

pub fn get_member(workspace: &Workspace, id: Uuid) -> EntResult<TeamMember> {
    load_team(workspace)?
        .members
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| EntError::NotFound(format!("Teammitglied {id} nicht gefunden")))
}

pub fn add_member(
    workspace: &Workspace,
    display_name: &str,
    role: EnterpriseRole,
) -> EntResult<TeamMember> {
    let name = display_name.trim();
    if name.is_empty() {
        return Err(EntError::Invalid("Anzeigename darf nicht leer sein".into()));
    }
    let mut team = load_team(workspace)?;
    if team
        .members
        .iter()
        .any(|m| m.display_name.eq_ignore_ascii_case(name) && m.active)
    {
        return Err(EntError::Conflict(format!(
            "Mitglied '{name}' existiert bereits"
        )));
    }
    let mut member = TeamMember {
        display_name: name.to_string(),
        role,
        ..TeamMember::default()
    };
    member.schema = ENT_SCHEMA_VERSION;
    team.members.push(member.clone());
    save_team(workspace, &team)?;
    Ok(member)
}

/// Applies an edit closure to a member and persists.
pub fn update_member(
    workspace: &Workspace,
    id: Uuid,
    edit: impl FnOnce(&mut TeamMember),
) -> EntResult<TeamMember> {
    let mut team = load_team(workspace)?;
    let member = team
        .members
        .iter_mut()
        .find(|m| m.id == id)
        .ok_or_else(|| EntError::NotFound("Teammitglied nicht gefunden".into()))?;
    edit(member);
    member.updated_unix = now_unix();
    member.schema = ENT_SCHEMA_VERSION;
    let result = member.clone();
    save_team(workspace, &team)?;
    Ok(result)
}

pub fn remove_member(workspace: &Workspace, id: Uuid) -> EntResult<()> {
    let mut team = load_team(workspace)?;
    team.members.retain(|m| m.id != id);
    save_team(workspace, &team)
}

/// Sets or clears a per-project role override for a member.
pub fn set_project_role(
    workspace: &Workspace,
    member_id: Uuid,
    project_id: Uuid,
    role: Option<EnterpriseRole>,
) -> EntResult<()> {
    update_member(workspace, member_id, |m| match role {
        Some(r) => {
            m.project_roles.insert(project_id, r);
        }
        None => {
            m.project_roles.remove(&project_id);
        }
    })?;
    Ok(())
}

/// Effective actions for a member on a project: project override wins over
/// the org-level role. Still *policy*, not authentication.
pub fn member_permissions(
    workspace: &Workspace,
    member_id: Uuid,
    project_id: Option<Uuid>,
) -> EntResult<Vec<Action>> {
    let member = get_member(workspace, member_id)?;
    let role = project_id
        .and_then(|pid| member.project_roles.get(&pid).copied())
        .unwrap_or(member.role);
    Ok(default_permissions(role))
}

/// Whether a member may perform `action` on a project (policy check).
pub fn member_can(
    workspace: &Workspace,
    member_id: Uuid,
    project_id: Option<Uuid>,
    action: Action,
) -> EntResult<bool> {
    let member = get_member(workspace, member_id)?;
    let role = project_id
        .and_then(|pid| member.project_roles.get(&pid).copied())
        .unwrap_or(member.role);
    Ok(can(role, action))
}

// ------------------------------------------------------------ registry

/// Registration of a `.nova` project inside the workspace. The file itself
/// is never copied or moved; `path` is the absolute location.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectRegistration {
    pub schema: u32,
    pub id: Uuid,
    /// Absolute path of the `.nova` file on this machine.
    pub path: String,
    /// Display name (may differ from the project metadata name).
    pub name: String,
    /// sha256 of the file at registration time (informational baseline).
    pub registered_sha256: Option<String>,
    /// Owning team member (workflow assignment, not ownership enforcement).
    pub owner_member: Option<Uuid>,
    pub archived: bool,
    pub created_unix: u64,
    pub updated_unix: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for ProjectRegistration {
    fn default() -> Self {
        let now = now_unix();
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            path: String::new(),
            name: String::new(),
            registered_sha256: None,
            owner_member: None,
            archived: false,
            created_unix: now,
            updated_unix: now,
            extra: serde_json::Map::new(),
        }
    }
}

impl ProjectRegistration {
    pub fn abs_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.path)
    }

    pub fn validate(&self) -> EntResult<()> {
        if self.path.trim().is_empty() {
            return Err(EntError::Invalid("Projektpfad darf nicht leer sein".into()));
        }
        Ok(())
    }
}

pub fn list_projects(workspace: &Workspace) -> EntResult<Vec<ProjectRegistration>> {
    let mut out = Vec::new();
    let dir = workspace.projects_dir();
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
        if let Ok(reg) = read_or::<ProjectRegistration>(&path, ProjectRegistration::default()) {
            if reg.schema != 0 {
                out.push(reg);
            }
            let _ = id;
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

pub fn active_projects(workspace: &Workspace) -> EntResult<Vec<ProjectRegistration>> {
    Ok(list_projects(workspace)?
        .into_iter()
        .filter(|p| !p.archived)
        .collect())
}

pub fn get_project(workspace: &Workspace, id: Uuid) -> EntResult<ProjectRegistration> {
    let path = workspace.project_reg_path(id);
    if !path.exists() {
        return Err(EntError::NotFound(format!(
            "Projekt {id} nicht registriert"
        )));
    }
    let mut reg: ProjectRegistration = read_or(&path, ProjectRegistration::default())?;
    if reg.schema == 0 {
        reg.schema = ENT_SCHEMA_VERSION;
    }
    Ok(reg)
}

/// Registers an existing `.nova` file. Computes sha256 baseline and loads
/// the project to sanity-check it is a valid `.nova` v4 document.
pub fn register_project(
    workspace: &Workspace,
    path: &std::path::Path,
    name: &str,
    owner: Option<Uuid>,
) -> EntResult<ProjectRegistration> {
    let abs = std::fs::canonicalize(path)?;
    if !abs.is_file() {
        return Err(EntError::Invalid(format!("Keine Datei: {}", abs.display())));
    }
    if abs.extension().and_then(|e| e.to_str()) != Some("nova") {
        return Err(EntError::Invalid(
            "Nur .nova-Dateien können registriert werden".into(),
        ));
    }
    // Validate the project loads.
    let project = nova_core::Project::load(&abs)?;
    let bytes = std::fs::read(&abs)?;
    let sha = crate::assets::sha256_hex(&bytes);

    let mut reg = ProjectRegistration {
        path: abs.to_string_lossy().into_owned(),
        name: if name.trim().is_empty() {
            project.metadata.name.clone()
        } else {
            name.trim().to_string()
        },
        registered_sha256: Some(sha),
        owner_member: owner,
        ..ProjectRegistration::default()
    };
    reg.schema = ENT_SCHEMA_VERSION;
    reg.validate()?;
    write_json(&workspace.project_reg_path(reg.id), &reg)?;
    Ok(reg)
}

pub fn update_project(
    workspace: &Workspace,
    id: Uuid,
    edit: impl FnOnce(&mut ProjectRegistration),
) -> EntResult<ProjectRegistration> {
    let mut reg = get_project(workspace, id)?;
    edit(&mut reg);
    reg.updated_unix = now_unix();
    reg.validate()?;
    write_json(&workspace.project_reg_path(id), &reg)?;
    Ok(reg)
}

pub fn archive_project(workspace: &Workspace, id: Uuid) -> EntResult<ProjectRegistration> {
    update_project(workspace, id, |r| r.archived = true)
}

pub fn delete_project(workspace: &Workspace, id: Uuid) -> EntResult<()> {
    crate::store::remove_if_exists(&workspace.project_reg_path(id))
}

/// Current sha256 of the registered file (None when missing/unreadable).
pub fn current_sha256(reg: &ProjectRegistration) -> Option<String> {
    std::fs::read(reg.abs_path())
        .ok()
        .map(|b| crate::assets::sha256_hex(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Organization;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("nova-ent-team-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(
            &dir,
            &Organization {
                name: "Org".into(),
                ..Default::default()
            },
        )
        .unwrap();
        (ws, dir)
    }

    fn nova_file(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        let project = nova_core::ProjectTemplate::Empty.create("Test");
        project.save(&p).unwrap();
        p
    }

    #[test]
    fn team_crud_and_roles() {
        let (ws, dir) = ws("team");
        let dev = add_member(&ws, "Dev Eins", EnterpriseRole::Developer).unwrap();
        let rev = add_member(&ws, "Rev Eins", EnterpriseRole::Reviewer).unwrap();
        assert_eq!(list_members(&ws).unwrap().len(), 2);
        assert!(add_member(&ws, "dev eins", EnterpriseRole::Viewer).is_err());

        // Per-project override: dev becomes reviewer on one project.
        let project = Uuid::new_v4();
        set_project_role(&ws, dev.id, project, Some(EnterpriseRole::Reviewer)).unwrap();
        assert!(member_can(&ws, dev.id, Some(project), Action::ManageReviews).unwrap());
        assert!(!member_can(&ws, dev.id, Some(project), Action::EditProjects).unwrap());
        // Without project context the org role applies.
        assert!(member_can(&ws, dev.id, None, Action::EditProjects).unwrap());
        assert!(member_can(&ws, rev.id, Some(project), Action::ManageReviews).unwrap());

        update_member(&ws, rev.id, |m| m.role = EnterpriseRole::Admin).unwrap();
        assert!(member_can(&ws, rev.id, None, Action::ManageOrg).unwrap());
        remove_member(&ws, rev.id).unwrap();
        assert_eq!(list_members(&ws).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_registration() {
        let (ws, dir) = ws("reg");
        let f = nova_file(&dir, "game.nova");
        let reg = register_project(&ws, &f, "Mein Spiel", None).unwrap();
        assert_eq!(reg.name, "Mein Spiel");
        assert!(reg.registered_sha256.is_some());
        assert!(reg.abs_path().exists());
        let list = list_projects(&ws).unwrap();
        assert_eq!(list.len(), 1);
        assert!(current_sha256(&reg).is_some());

        // Non-nova rejected.
        let bad = dir.join("x.txt");
        std::fs::write(&bad, b"no").unwrap();
        assert!(register_project(&ws, &bad, "x", None).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
