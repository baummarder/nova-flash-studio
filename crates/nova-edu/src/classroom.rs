//! Classes, groups and members (M20.2).
//!
//! Members are identified by stable UUIDs and carry only a display name or
//! pseudonym — no accounts, no personal data beyond what the teacher
//! chooses to record.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EduError, EduResult};
use crate::store::{EDU_SCHEMA_VERSION, read_or, write_json};
use crate::workspace::{Workspace, now_unix};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Member {
    pub id: Uuid,
    pub display_name: String,
    pub pseudonym: Option<String>,
    /// Group this member belongs to (None = unassigned).
    pub group_id: Option<Uuid>,
    /// archived members are hidden from active lists but kept for history.
    pub archived: bool,
}

impl Default for Member {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            display_name: String::new(),
            pseudonym: None,
            group_id: None,
            archived: false,
        }
    }
}

impl Member {
    pub fn author_name(&self) -> &str {
        self.pseudonym
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .unwrap_or(&self.display_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub description: String,
}

impl Default for Group {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            description: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Classroom {
    pub schema: u32,
    pub id: Uuid,
    pub name: String,
    pub subject: String,
    pub term: String,
    pub groups: Vec<Group>,
    pub members: Vec<Member>,
    /// Archived classes are kept for history but excluded from active lists.
    pub archived: bool,
    pub created_unix: u64,
    /// Forward-compatible extras.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for Classroom {
    fn default() -> Self {
        Self {
            schema: EDU_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: String::new(),
            subject: String::new(),
            term: String::new(),
            groups: Vec::new(),
            members: Vec::new(),
            archived: false,
            created_unix: 0,
            extra: serde_json::Map::new(),
        }
    }
}

// ------------------------------------------------------------------ store

fn load_classroom(workspace: &Workspace, id: Uuid) -> EduResult<Classroom> {
    let mut class: Classroom = read_or(
        &workspace.classroom_path(id),
        Classroom {
            id,
            ..Classroom::default()
        },
    )?;
    if class.schema == 0 {
        class.schema = EDU_SCHEMA_VERSION;
    }
    Ok(class)
}

fn save_classroom(workspace: &Workspace, class: &Classroom) -> EduResult<()> {
    class.validate()?;
    write_json(&workspace.classroom_path(class.id), class)
}

/// Loads every classroom in the workspace.
pub fn list_classrooms(workspace: &Workspace) -> EduResult<Vec<Classroom>> {
    let mut out = Vec::new();
    let dir = workspace.classrooms_dir();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(id) = Uuid::parse_str(name) else {
            continue;
        };
        if let Ok(class) = load_classroom(workspace, id) {
            out.push(class);
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

pub fn active_classrooms(workspace: &Workspace) -> EduResult<Vec<Classroom>> {
    Ok(list_classrooms(workspace)?
        .into_iter()
        .filter(|c| !c.archived)
        .collect())
}

pub fn get_classroom(workspace: &Workspace, id: Uuid) -> EduResult<Classroom> {
    let path = workspace.classroom_path(id);
    if !path.exists() {
        return Err(EduError::NotFound(format!("Klasse {id} nicht gefunden")));
    }
    load_classroom(workspace, id)
}

pub fn create_classroom(workspace: &Workspace, name: &str) -> EduResult<Classroom> {
    let name = name.trim();
    if name.is_empty() {
        return Err(EduError::Invalid("Klassenname darf nicht leer sein".into()));
    }
    if list_classrooms(workspace)?
        .iter()
        .any(|c| c.name == name && !c.archived)
    {
        return Err(EduError::Conflict(format!(
            "Klasse '{name}' existiert bereits"
        )));
    }
    let class = Classroom {
        name: name.to_string(),
        created_unix: now_unix(),
        ..Classroom::default()
    };
    class.validate()?;
    save_classroom(workspace, &class)?;
    Ok(class)
}

pub fn rename_classroom(
    workspace: &Workspace,
    id: Uuid,
    name: &str,
    subject: &str,
    term: &str,
) -> EduResult<Classroom> {
    let mut class = get_classroom(workspace, id)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(EduError::Invalid("Klassenname darf nicht leer sein".into()));
    }
    class.name = name.to_string();
    class.subject = subject.trim().to_string();
    class.term = term.trim().to_string();
    save_classroom(workspace, &class)?;
    Ok(class)
}

/// Archives (never deletes) a classroom. Submissions/feedback stay on disk.
pub fn archive_classroom(workspace: &Workspace, id: Uuid) -> EduResult<Classroom> {
    let mut class = get_classroom(workspace, id)?;
    class.archived = true;
    save_classroom(workspace, &class)?;
    Ok(class)
}

/// Permanently deletes a classroom and its member list. Assignment,
/// submission and feedback sidecars under inbox/feedback are left intact
/// (they are referenced by id and remain exportable); the class record
/// itself is removed.
pub fn delete_classroom(workspace: &Workspace, id: Uuid) -> EduResult<()> {
    crate::store::remove_if_exists(&workspace.classroom_path(id))
}

// ------------------------------------------------------------------ groups

pub fn add_group(workspace: &Workspace, class_id: Uuid, name: &str) -> EduResult<Group> {
    let mut class = get_classroom(workspace, class_id)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(EduError::Invalid("Gruppenname darf nicht leer sein".into()));
    }
    if class.groups.iter().any(|g| g.name == name) {
        return Err(EduError::Conflict(format!(
            "Gruppe '{name}' existiert bereits"
        )));
    }
    let group = Group {
        name: name.to_string(),
        ..Group::default()
    };
    class.groups.push(group.clone());
    save_classroom(workspace, &class)?;
    Ok(group)
}

pub fn rename_group(
    workspace: &Workspace,
    class_id: Uuid,
    group_id: Uuid,
    name: &str,
) -> EduResult<()> {
    let mut class = get_classroom(workspace, class_id)?;
    let group = class
        .groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or_else(|| EduError::NotFound("Gruppe nicht gefunden".to_string()))?;
    group.name = name.trim().to_string();
    save_classroom(workspace, &class)
}

/// Removes a group; members keep their records but lose the assignment.
pub fn remove_group(workspace: &Workspace, class_id: Uuid, group_id: Uuid) -> EduResult<()> {
    let mut class = get_classroom(workspace, class_id)?;
    class.groups.retain(|g| g.id != group_id);
    for member in class.members.iter_mut() {
        if member.group_id == Some(group_id) {
            member.group_id = None;
        }
    }
    save_classroom(workspace, &class)
}

// ----------------------------------------------------------------- members

pub fn add_member(
    workspace: &Workspace,
    class_id: Uuid,
    display_name: &str,
    pseudonym: Option<&str>,
) -> EduResult<Member> {
    let mut class = get_classroom(workspace, class_id)?;
    let name = display_name.trim();
    if name.is_empty() && pseudonym.map(str::trim).unwrap_or("").is_empty() {
        return Err(EduError::Invalid(
            "Mitglied braucht Anzeigename oder Pseudonym".into(),
        ));
    }
    let member = Member {
        display_name: name.to_string(),
        pseudonym: pseudonym
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string),
        ..Member::default()
    };
    class.members.push(member.clone());
    save_classroom(workspace, &class)?;
    Ok(member)
}

pub fn move_member_to_group(
    workspace: &Workspace,
    class_id: Uuid,
    member_id: Uuid,
    group_id: Option<Uuid>,
) -> EduResult<()> {
    let mut class = get_classroom(workspace, class_id)?;
    if let Some(gid) = group_id
        && !class.groups.iter().any(|g| g.id == gid)
    {
        return Err(EduError::NotFound("Zielgruppe nicht gefunden".into()));
    }
    let member = class
        .members
        .iter_mut()
        .find(|m| m.id == member_id)
        .ok_or_else(|| EduError::NotFound("Mitglied nicht gefunden".to_string()))?;
    member.group_id = group_id;
    save_classroom(workspace, &class)
}

pub fn rename_member(
    workspace: &Workspace,
    class_id: Uuid,
    member_id: Uuid,
    display_name: &str,
    pseudonym: Option<&str>,
) -> EduResult<()> {
    let mut class = get_classroom(workspace, class_id)?;
    let member = class
        .members
        .iter_mut()
        .find(|m| m.id == member_id)
        .ok_or_else(|| EduError::NotFound("Mitglied nicht gefunden".to_string()))?;
    member.display_name = display_name.trim().to_string();
    member.pseudonym = pseudonym
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string);
    save_classroom(workspace, &class)
}

/// Removes a member record. Their submissions/feedback stay on disk (they
/// are keyed by member id) but the roster entry disappears.
pub fn remove_member(workspace: &Workspace, class_id: Uuid, member_id: Uuid) -> EduResult<()> {
    let mut class = get_classroom(workspace, class_id)?;
    class.members.retain(|m| m.id != member_id);
    save_classroom(workspace, &class)
}

pub fn find_member(class: &Classroom, member_id: Uuid) -> Option<&Member> {
    class.members.iter().find(|m| m.id == member_id)
}

/// Members grouped by group for UI display: (group_name_or_none, members).
pub fn members_by_group(class: &Classroom) -> BTreeMap<Option<String>, Vec<&Member>> {
    let mut map: BTreeMap<Option<String>, Vec<&Member>> = BTreeMap::new();
    for member in class.members.iter().filter(|m| !m.archived) {
        let group_name = member
            .group_id
            .and_then(|gid| class.groups.iter().find(|g| g.id == gid))
            .map(|g| g.name.clone());
        map.entry(group_name).or_default().push(member);
    }
    map
}

impl Classroom {
    pub fn validate(&self) -> EduResult<()> {
        if self.name.trim().is_empty() {
            return Err(EduError::Invalid("Klassenname darf nicht leer sein".into()));
        }
        let mut member_ids = std::collections::HashSet::new();
        for member in &self.members {
            if !member_ids.insert(member.id) {
                return Err(EduError::Invalid("Doppelte Mitglieder-ID".into()));
            }
            if let Some(gid) = member.group_id
                && !self.groups.iter().any(|g| g.id == gid)
            {
                return Err(EduError::Invalid(format!(
                    "Mitglied '{}' referenziert unbekannte Gruppe",
                    member.author_name()
                )));
            }
        }
        let mut group_ids = std::collections::HashSet::new();
        for group in &self.groups {
            if !group_ids.insert(group.id) {
                return Err(EduError::Invalid("Doppelte Gruppen-ID".into()));
            }
            if group.name.trim().is_empty() {
                return Err(EduError::Invalid("Gruppenname darf nicht leer sein".into()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::school::SchoolConfig;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova-edu-cls-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(&dir, &SchoolConfig::default()).unwrap();
        (ws, dir)
    }

    #[test]
    fn class_crud_roundtrip() {
        let (ws, dir) = ws("crud");
        let class = create_classroom(&ws, "  Informatik 11  ").unwrap();
        assert_eq!(class.name, "Informatik 11");
        let renamed = rename_classroom(&ws, class.id, "Info 11a", "IT", "2026").unwrap();
        assert_eq!(renamed.subject, "IT");
        let loaded = get_classroom(&ws, class.id).unwrap();
        assert_eq!(loaded.name, "Info 11a");
        archive_classroom(&ws, class.id).unwrap();
        assert!(active_classrooms(&ws).unwrap().is_empty());
        assert_eq!(list_classrooms(&ws).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_class_rejected() {
        let (ws, dir) = ws("dup");
        create_classroom(&ws, "A").unwrap();
        assert!(create_classroom(&ws, "A").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn groups_and_members() {
        let (ws, dir) = ws("grp");
        let class = create_classroom(&ws, "K").unwrap();
        let g1 = add_group(&ws, class.id, "Gruppe A").unwrap();
        let m = add_member(&ws, class.id, "Anna", Some("A.A.")).unwrap();
        move_member_to_group(&ws, class.id, m.id, Some(g1.id)).unwrap();
        let class = get_classroom(&ws, class.id).unwrap();
        assert_eq!(class.members[0].group_id, Some(g1.id));
        assert_eq!(class.members[0].author_name(), "A.A.");
        let grouped = members_by_group(&class);
        assert_eq!(grouped.len(), 1);
        // Moving to a nonexistent group fails.
        assert!(move_member_to_group(&ws, class.id, m.id, Some(Uuid::new_v4())).is_err());
        remove_group(&ws, class.id, g1.id).unwrap();
        let class = get_classroom(&ws, class.id).unwrap();
        assert_eq!(class.members[0].group_id, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_member_keeps_sidecars_possible() {
        let (ws, dir) = ws("rm");
        let class = create_classroom(&ws, "K").unwrap();
        let m = add_member(&ws, class.id, "B", None).unwrap();
        remove_member(&ws, class.id, m.id).unwrap();
        let class = get_classroom(&ws, class.id).unwrap();
        assert!(class.members.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
