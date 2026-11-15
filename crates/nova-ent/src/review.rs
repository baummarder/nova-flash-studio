//! Review & approval workflows (M21.5).
//!
//! Reviews attach to a project registration. Comments may carry an anchor
//! (object > layer+frame > scene/label) resolved against a loaded
//! `nova_core::Project`; unresolvable anchors are marked `orphaned`, never
//! silently dropped. All transitions append to an in-record history.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EntError, EntResult};
use crate::store::{ENT_SCHEMA_VERSION, now_unix, read_or, write_json};
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Draft,
    PendingReview,
    InReview,
    ChangesRequested,
    Approved,
    Rejected,
}

impl ReviewStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Entwurf",
            Self::PendingReview => "Wartet auf Review",
            Self::InReview => "In PrÃ¼fung",
            Self::ChangesRequested => "Ã„nderungen angefordert",
            Self::Approved => "Freigegeben",
            Self::Rejected => "Abgelehnt",
        }
    }

    /// Legal transitions. Rejected can be resubmitted â†’ PendingReview;
    /// ChangesRequested â†’ PendingReview on resubmission.
    pub fn can_transition_to(self, next: ReviewStatus) -> bool {
        use ReviewStatus::*;
        matches!(
            (self, next),
            (Draft, PendingReview)
                | (PendingReview, InReview)
                | (PendingReview, Rejected)
                | (InReview, Approved)
                | (InReview, ChangesRequested)
                | (InReview, Rejected)
                | (ChangesRequested, PendingReview)
                | (Rejected, PendingReview)
        )
    }
}

/// Anchor into the reviewed project â€” same semantics as nova-edu.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewAnchor {
    Object { object_id: Uuid },
    LayerFrame { layer_id: Uuid, frame: u32 },
    SceneLabel { scene: String, label: String },
    Scene { scene_id: Uuid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentCategory {
    Praise,
    Problem,
    Question,
    Requirement,
}

impl CommentCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Praise => "Lob",
            Self::Problem => "Problem",
            Self::Question => "Frage",
            Self::Requirement => "Anforderung",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewComment {
    pub id: Uuid,
    /// Display-name label of the commenter (policy identity, not auth).
    pub author: String,
    pub category: CommentCategory,
    pub body: String,
    pub anchor: Option<ReviewAnchor>,
    /// Set when the anchor could not be resolved against the project.
    pub orphaned: bool,
    pub created_unix: u64,
    pub resolved: bool,
}

impl Default for ReviewComment {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            author: String::new(),
            category: CommentCategory::Problem,
            body: String::new(),
            anchor: None,
            orphaned: false,
            created_unix: now_unix(),
            resolved: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewEvent {
    pub at_unix: u64,
    pub actor: String,
    pub from: ReviewStatus,
    pub to: ReviewStatus,
    pub note: String,
}

/// A review request for one project registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Review {
    pub schema: u32,
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: String,
    /// Requested reviewer member id (workflow assignment).
    pub reviewer: Option<Uuid>,
    pub status: ReviewStatus,
    pub comments: Vec<ReviewComment>,
    /// Free-text decision reason (required for reject/changes).
    pub decision_note: String,
    pub created_unix: u64,
    pub updated_unix: u64,
    pub history: Vec<ReviewEvent>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for Review {
    fn default() -> Self {
        let now = now_unix();
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            title: String::new(),
            description: String::new(),
            reviewer: None,
            status: ReviewStatus::Draft,
            comments: Vec::new(),
            decision_note: String::new(),
            created_unix: now,
            updated_unix: now,
            history: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

fn load(workspace: &Workspace, id: Uuid) -> EntResult<Review> {
    let mut r: Review = read_or(&workspace.review_path(id), Review::default())?;
    if r.schema == 0 {
        r.schema = ENT_SCHEMA_VERSION;
    }
    Ok(r)
}

fn save(workspace: &Workspace, r: &Review) -> EntResult<()> {
    write_json(&workspace.review_path(r.id), r)
}

pub fn list_reviews(workspace: &Workspace) -> EntResult<Vec<Review>> {
    let mut out = Vec::new();
    let dir = workspace.reviews_dir();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(r) = read_or::<Review>(&path, Review::default())
            && r.schema != 0
        {
            out.push(r);
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.updated_unix));
    Ok(out)
}

pub fn reviews_for_project(workspace: &Workspace, project_id: Uuid) -> EntResult<Vec<Review>> {
    Ok(list_reviews(workspace)?
        .into_iter()
        .filter(|r| r.project_id == project_id)
        .collect())
}

pub fn get_review(workspace: &Workspace, id: Uuid) -> EntResult<Review> {
    let path = workspace.review_path(id);
    if !path.exists() {
        return Err(EntError::NotFound(format!("Review {id} nicht gefunden")));
    }
    load(workspace, id)
}

pub fn create_review(
    workspace: &Workspace,
    project_id: Uuid,
    title: &str,
    description: &str,
    reviewer: Option<Uuid>,
    actor: &str,
) -> EntResult<Review> {
    if title.trim().is_empty() {
        return Err(EntError::Invalid(
            "Review-Titel darf nicht leer sein".into(),
        ));
    }
    // Project registration must exist.
    crate::team::get_project(workspace, project_id)?;
    let mut r = Review {
        project_id,
        title: title.trim().to_string(),
        description: description.to_string(),
        reviewer,
        ..Review::default()
    };
    r.schema = ENT_SCHEMA_VERSION;
    r.history.push(ReviewEvent {
        at_unix: now_unix(),
        actor: actor.to_string(),
        from: ReviewStatus::Draft,
        to: ReviewStatus::Draft,
        note: "Review angelegt".into(),
    });
    save(workspace, &r)?;
    Ok(r)
}

/// Transitions status with history entry. Reject/ChangesRequested require
/// a non-empty decision note.
pub fn transition(
    workspace: &Workspace,
    id: Uuid,
    to: ReviewStatus,
    actor: &str,
    note: &str,
) -> EntResult<Review> {
    let mut r = get_review(workspace, id)?;
    if !r.status.can_transition_to(to) {
        return Err(EntError::Invalid(format!(
            "UngÃ¼ltiger Review-Ãœbergang {} â†’ {}",
            r.status.label(),
            to.label()
        )));
    }
    if matches!(to, ReviewStatus::Rejected | ReviewStatus::ChangesRequested)
        && note.trim().is_empty()
    {
        return Err(EntError::Invalid(
            "Ablehnung/RÃ¼ckgabe erfordert eine BegrÃ¼ndung".into(),
        ));
    }
    let from = r.status;
    r.status = to;
    if !note.trim().is_empty() {
        r.decision_note = note.trim().to_string();
    }
    r.updated_unix = now_unix();
    r.history.push(ReviewEvent {
        at_unix: now_unix(),
        actor: actor.to_string(),
        from,
        to,
        note: note.to_string(),
    });
    save(workspace, &r)?;
    Ok(r)
}

/// Adds a comment. When an anchor is supplied, `resolve_anchor` marks it
/// orphaned if it cannot be found in the project.
pub fn add_comment(
    workspace: &Workspace,
    id: Uuid,
    author: &str,
    category: CommentCategory,
    body: &str,
    anchor: Option<ReviewAnchor>,
    orphaned: bool,
) -> EntResult<ReviewComment> {
    let mut r = get_review(workspace, id)?;
    if body.trim().is_empty() {
        return Err(EntError::Invalid("Kommentar darf nicht leer sein".into()));
    }
    let comment = ReviewComment {
        author: author.to_string(),
        category,
        body: body.trim().to_string(),
        anchor,
        orphaned,
        ..ReviewComment::default()
    };
    r.comments.push(comment.clone());
    r.updated_unix = now_unix();
    save(workspace, &r)?;
    Ok(comment)
}

/// Marks a comment resolved/unresolved.
pub fn set_comment_resolved(
    workspace: &Workspace,
    id: Uuid,
    comment_id: Uuid,
    resolved: bool,
) -> EntResult<()> {
    let mut r = get_review(workspace, id)?;
    let c = r
        .comments
        .iter_mut()
        .find(|c| c.id == comment_id)
        .ok_or_else(|| EntError::NotFound("Kommentar nicht gefunden".into()))?;
    c.resolved = resolved;
    r.updated_unix = now_unix();
    save(workspace, &r)
}

/// Resolves an anchor against a loaded project. Returns true when found.
pub fn anchor_resolves(anchor: &ReviewAnchor, project: &nova_core::Project) -> bool {
    match anchor {
        ReviewAnchor::Object { object_id } => project.scenes.iter().any(|s| {
            s.layers.iter().any(|l| {
                l.frames
                    .iter()
                    .any(|f| f.objects.iter().any(|o| o.id == *object_id))
            })
        }),
        ReviewAnchor::LayerFrame { layer_id, .. } => project
            .scenes
            .iter()
            .any(|s| s.layers.iter().any(|l| l.id == *layer_id)),
        ReviewAnchor::SceneLabel { scene, label } => project.scenes.iter().any(|s| {
            s.name == *scene
                && s.layers.iter().any(|l| {
                    l.frames
                        .iter()
                        .any(|f| f.label.as_deref() == Some(label.as_str()))
                })
        }),
        ReviewAnchor::Scene { scene_id } => project.scenes.iter().any(|s| s.id == *scene_id),
    }
}

/// Convenience: add a comment anchored into a project, auto-marking orphans.
pub fn add_anchored_comment(
    workspace: &Workspace,
    id: Uuid,
    author: &str,
    category: CommentCategory,
    body: &str,
    anchor: Option<ReviewAnchor>,
    project: Option<&nova_core::Project>,
) -> EntResult<ReviewComment> {
    let orphaned = match (&anchor, project) {
        (Some(a), Some(p)) => !anchor_resolves(a, p),
        (Some(_), None) => true,
        (None, _) => false,
    };
    add_comment(workspace, id, author, category, body, anchor, orphaned)
}

/// Deletes a review and its comments (history is part of the record).
pub fn delete_review(workspace: &Workspace, id: Uuid) -> EntResult<()> {
    crate::store::remove_if_exists(&workspace.review_path(id))
}

/// Review counts per status for dashboards.
pub fn status_counts(
    workspace: &Workspace,
) -> EntResult<std::collections::BTreeMap<ReviewStatus, usize>> {
    let mut map = std::collections::BTreeMap::new();
    for r in list_reviews(workspace)? {
        *map.entry(r.status).or_insert(0) += 1;
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::register_project;
    use crate::workspace::Organization;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova-ent-rev-{}-{}", tag, std::process::id()));
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

    fn reg_project(ws: &Workspace, dir: &std::path::Path) -> crate::team::ProjectRegistration {
        let f = dir.join("p.nova");
        nova_core::ProjectTemplate::Empty
            .create("P")
            .save(&f)
            .unwrap();
        register_project(ws, &f, "P", None).unwrap()
    }

    #[test]
    fn lifecycle() {
        let (ws, dir) = ws("life");
        let reg = reg_project(&ws, &dir);
        let r = create_review(&ws, reg.id, "Review 1", "Bitte prÃ¼fen", None, "PM").unwrap();
        assert_eq!(r.status, ReviewStatus::Draft);

        // Illegal: Draft â†’ Approved.
        assert!(transition(&ws, r.id, ReviewStatus::Approved, "PM", "").is_err());
        transition(&ws, r.id, ReviewStatus::PendingReview, "PM", "").unwrap();
        transition(&ws, r.id, ReviewStatus::InReview, "Rev", "").unwrap();
        // Reject without note fails.
        assert!(transition(&ws, r.id, ReviewStatus::Rejected, "Rev", "").is_err());
        let r = transition(
            &ws,
            r.id,
            ReviewStatus::ChangesRequested,
            "Rev",
            "Bitte Screenshots ergÃ¤nzen",
        )
        .unwrap();
        assert_eq!(r.status, ReviewStatus::ChangesRequested);
        assert_eq!(r.history.len(), 4);
        // Resubmit.
        let r = transition(
            &ws,
            r.id,
            ReviewStatus::PendingReview,
            "PM",
            "Neu eingereicht",
        )
        .unwrap();
        transition(&ws, r.id, ReviewStatus::InReview, "Rev", "").unwrap();
        let r = transition(&ws, r.id, ReviewStatus::Approved, "Rev", "OK").unwrap();
        assert_eq!(r.status, ReviewStatus::Approved);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn comments_and_anchors() {
        let (ws, dir) = ws("cmt");
        let reg = reg_project(&ws, &dir);
        let r = create_review(&ws, reg.id, "R", "", None, "PM").unwrap();
        let project = nova_core::Project::load(&reg.abs_path()).unwrap();
        let scene = project.scenes[0].clone();
        let layer = scene.layers[0].clone();

        // Valid anchor.
        let c1 = add_anchored_comment(
            &ws,
            r.id,
            "Rev",
            CommentCategory::Problem,
            "Layer falsch",
            Some(ReviewAnchor::LayerFrame {
                layer_id: layer.id,
                frame: 0,
            }),
            Some(&project),
        )
        .unwrap();
        assert!(!c1.orphaned);

        // Orphaned anchor.
        let c2 = add_anchored_comment(
            &ws,
            r.id,
            "Rev",
            CommentCategory::Question,
            "Wo ist X?",
            Some(ReviewAnchor::Object {
                object_id: Uuid::new_v4(),
            }),
            Some(&project),
        )
        .unwrap();
        assert!(c2.orphaned);

        // SceneLabel anchor resolves.
        let mut proj2 = project.clone();
        proj2.scenes[0].layers[0].frames[0].label = Some("intro".into());
        let c3 = add_anchored_comment(
            &ws,
            r.id,
            "Rev",
            CommentCategory::Requirement,
            "Label prÃ¼fen",
            Some(ReviewAnchor::SceneLabel {
                scene: scene.name.clone(),
                label: "intro".into(),
            }),
            Some(&proj2),
        )
        .unwrap();
        assert!(!c3.orphaned);

        set_comment_resolved(&ws, r.id, c1.id, true).unwrap();
        let r = get_review(&ws, r.id).unwrap();
        assert!(r.comments.iter().find(|c| c.id == c1.id).unwrap().resolved);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
