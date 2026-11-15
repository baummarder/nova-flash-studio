//! Audit log + administration overview (M21.6).
//!
//! The audit log is append-only JSONL inside the workspace. Entries record
//! *who did what* using local display-name labels â€” again, policy
//! attribution, not authentication.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::EntResult;
use crate::store::{append_jsonl, now_unix, read_jsonl};
use crate::workspace::Workspace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub at_unix: u64,
    /// Display-name label of the actor.
    pub actor: String,
    /// Category, e.g. "project", "version", "review", "asset", "build",
    /// "team", "org", "integration", "package".
    pub category: String,
    /// Machine action id, e.g. "project.register", "version.restore".
    pub action: String,
    /// Human-readable detail.
    pub detail: String,
    /// Optional entity id (project/review/asset UUID).
    pub entity: Option<Uuid>,
}

impl AuditEntry {
    pub fn new(actor: &str, category: &str, action: &str, detail: impl Into<String>) -> Self {
        Self {
            at_unix: now_unix(),
            actor: actor.to_string(),
            category: category.to_string(),
            action: action.to_string(),
            detail: detail.into(),
            entity: None,
        }
    }

    pub fn with_entity(mut self, id: Uuid) -> Self {
        self.entity = Some(id);
        self
    }
}

/// Appends an audit entry. Never fails silently â€” errors propagate so
/// callers can decide whether the mutation should be rolled back.
pub fn log(workspace: &Workspace, entry: &AuditEntry) -> EntResult<()> {
    append_jsonl(&workspace.audit_path(), entry)
}

/// Convenience wrapper returning the entry for chaining.
pub fn record(
    workspace: &Workspace,
    actor: &str,
    category: &str,
    action: &str,
    detail: impl Into<String>,
) -> EntResult<AuditEntry> {
    let entry = AuditEntry::new(actor, category, action, detail);
    log(workspace, &entry)?;
    Ok(entry)
}

pub fn audit_log(workspace: &Workspace) -> EntResult<Vec<AuditEntry>> {
    let mut v: Vec<AuditEntry> = read_jsonl(&workspace.audit_path())?;
    v.sort_by_key(|a| std::cmp::Reverse(a.at_unix));
    Ok(v)
}

pub fn audit_log_for(workspace: &Workspace, entity: Uuid) -> EntResult<Vec<AuditEntry>> {
    Ok(audit_log(workspace)?
        .into_iter()
        .filter(|e| e.entity == Some(entity))
        .collect())
}

/// Aggregated administration overview for the dashboard.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AdminOverview {
    pub org_name: String,
    pub members: usize,
    pub active_members: usize,
    pub projects: usize,
    pub archived_projects: usize,
    pub assets: usize,
    pub asset_folders: usize,
    pub reviews_open: usize,
    pub reviews_approved: usize,
    pub build_profiles: usize,
    pub builds_total: usize,
    pub versions_total: usize,
    pub versions_bytes: u64,
    pub integrations_enabled: usize,
    pub audit_entries: usize,
    /// Asset library integrity problems (empty = clean).
    pub integrity_problems: Vec<String>,
}

pub fn overview(workspace: &Workspace) -> EntResult<AdminOverview> {
    let org = workspace.org()?;
    let members = crate::team::list_members(workspace)?;
    let projects = crate::team::list_projects(workspace)?;
    let assets = crate::assets::list_assets(workspace)?;
    let reviews = crate::review::list_reviews(workspace)?;
    let profiles = crate::builds::list_profiles(workspace)?;
    let history = crate::builds::build_history(workspace)?;
    let (versions_total, versions_bytes) = crate::versions::storage_stats(workspace)?;
    let integrations = crate::integrations::list_integrations(workspace)?;
    let audit = audit_log(workspace)?;
    let integrity = crate::assets::verify_library(workspace)?;

    Ok(AdminOverview {
        org_name: org.name,
        members: members.len(),
        active_members: members.iter().filter(|m| m.active).count(),
        projects: projects.iter().filter(|p| !p.archived).count(),
        archived_projects: projects.iter().filter(|p| p.archived).count(),
        assets: assets.len(),
        asset_folders: crate::assets::list_folders(workspace)?.len(),
        reviews_open: reviews
            .iter()
            .filter(|r| {
                !matches!(
                    r.status,
                    crate::review::ReviewStatus::Approved | crate::review::ReviewStatus::Rejected
                )
            })
            .count(),
        reviews_approved: reviews
            .iter()
            .filter(|r| r.status == crate::review::ReviewStatus::Approved)
            .count(),
        build_profiles: profiles.len(),
        builds_total: history.len(),
        versions_total,
        versions_bytes,
        integrations_enabled: integrations.iter().filter(|i| i.enabled).count(),
        audit_entries: audit.len(),
        integrity_problems: integrity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Organization;

    #[test]
    fn audit_append_and_overview() {
        let dir = std::env::temp_dir().join(format!("nova-ent-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(
            &dir,
            &Organization {
                name: "Audit Org".into(),
                ..Default::default()
            },
        )
        .unwrap();
        record(&ws, "Ada", "project", "project.register", "game.nova").unwrap();
        let entity = Uuid::new_v4();
        let e = AuditEntry::new("Ben", "version", "version.restore", "snap-1").with_entity(entity);
        log(&ws, &e).unwrap();

        let log = audit_log(&ws).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(audit_log_for(&ws, entity).unwrap().len(), 1);

        let ov = overview(&ws).unwrap();
        assert_eq!(ov.org_name, "Audit Org");
        assert_eq!(ov.audit_entries, 2);
        assert!(ov.integrity_problems.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
