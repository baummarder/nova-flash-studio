//! Feedback and anchored comments (M20.5).
//!
//! Feedback lives in sidecar JSON files — never inside the `.nova`
//! project. Anchors reference stable UUIDs (object > layer+frame >
//! scene+label); unresolvable anchors are marked `orphaned` instead of
//! being silently re-attached.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EduError, EduResult};
use crate::store::{EDU_SCHEMA_VERSION, read_or, write_json};
use crate::submission::SubmissionState;
use crate::workspace::{Workspace, now_unix};

/// Anchor into a `.nova` project. Resolution order: object id, then
/// layer+frame, then scene+label. All fields optional except scene.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProjectAnchor {
    pub scene_id: Option<Uuid>,
    pub layer_id: Option<Uuid>,
    pub object_id: Option<Uuid>,
    pub frame: Option<usize>,
    /// Human-readable fallback when ids no longer resolve.
    pub label: Option<String>,
}

impl ProjectAnchor {
    pub fn is_empty(&self) -> bool {
        self.scene_id.is_none()
            && self.layer_id.is_none()
            && self.object_id.is_none()
            && self.frame.is_none()
            && self.label.is_none()
    }

    /// Best human-readable description for UI.
    pub fn describe(&self) -> String {
        if let Some(label) = &self.label
            && !label.trim().is_empty()
        {
            return label.clone();
        }
        let mut parts = Vec::new();
        if let Some(id) = self.object_id {
            parts.push(format!("Objekt {}", short_id(id)));
        }
        if let Some(id) = self.layer_id {
            parts.push(format!("Ebene {}", short_id(id)));
        }
        if let Some(f) = self.frame {
            parts.push(format!("Frame {}", f + 1));
        }
        if parts.is_empty() {
            "Allgemein".to_string()
        } else {
            parts.join(" · ")
        }
    }
}

fn short_id(id: Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentCategory {
    Praise,
    Issue,
    Question,
    Requirement,
}

impl CommentCategory {
    pub fn label(self) -> &'static str {
        match self {
            CommentCategory::Praise => "Lob",
            CommentCategory::Issue => "Problem",
            CommentCategory::Question => "Frage",
            CommentCategory::Requirement => "Anforderung",
        }
    }
}

/// Who wrote the comment — local display info only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorRef {
    pub display_name: String,
    /// "teacher" | "student" | "assistant"
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Comment {
    pub id: Uuid,
    pub author: AuthorRef,
    pub created_unix: u64,
    pub anchor: Option<ProjectAnchor>,
    pub category: CommentCategory,
    pub text: String,
    pub resolved: bool,
    /// Set when the anchor could not be resolved in the latest project —
    /// the comment is kept but visibly marked.
    pub orphaned: bool,
    /// Student acknowledgement (feedback read receipt).
    pub acknowledged: bool,
}

impl Default for Comment {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            author: AuthorRef {
                display_name: String::new(),
                role: "teacher".into(),
            },
            created_unix: 0,
            anchor: None,
            category: CommentCategory::Issue,
            text: String::new(),
            resolved: false,
            orphaned: false,
            acknowledged: false,
        }
    }
}

/// Feedback bundle for one member's work on one assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedbackBundle {
    pub schema: u32,
    pub class_id: Uuid,
    pub assignment_id: Uuid,
    pub member_id: Uuid,
    /// Optional overall grade/assessment note.
    pub summary: Option<String>,
    pub rubric_marks: BTreeMap<String, String>,
    pub comments: Vec<Comment>,
    pub updated_unix: u64,
}

use std::collections::BTreeMap;

impl Default for FeedbackBundle {
    fn default() -> Self {
        Self {
            schema: EDU_SCHEMA_VERSION,
            class_id: Uuid::nil(),
            assignment_id: Uuid::nil(),
            member_id: Uuid::nil(),
            summary: None,
            rubric_marks: BTreeMap::new(),
            comments: Vec::new(),
            updated_unix: 0,
        }
    }
}

impl FeedbackBundle {
    pub fn feedback_path(&self, workspace: &Workspace) -> PathBuf {
        workspace.feedback_path(self.class_id, self.assignment_id, self.member_id)
    }

    pub fn validate(&self) -> EduResult<()> {
        for c in &self.comments {
            if c.text.trim().is_empty() {
                return Err(EduError::Invalid("Kommentar darf nicht leer sein".into()));
            }
        }
        Ok(())
    }
}

fn load_bundle(
    workspace: &Workspace,
    class_id: Uuid,
    assignment_id: Uuid,
    member_id: Uuid,
) -> EduResult<FeedbackBundle> {
    let path = workspace.feedback_path(class_id, assignment_id, member_id);
    let mut b: FeedbackBundle = read_or(
        &path,
        FeedbackBundle {
            class_id,
            assignment_id,
            member_id,
            ..FeedbackBundle::default()
        },
    )?;
    if b.schema == 0 {
        b.schema = EDU_SCHEMA_VERSION;
    }
    Ok(b)
}

/// Persists a feedback bundle directly (used by package import).
pub fn save_feedback_bundle(workspace: &Workspace, b: &FeedbackBundle) -> EduResult<()> {
    save_bundle(workspace, b)
}

fn save_bundle(workspace: &Workspace, b: &FeedbackBundle) -> EduResult<()> {
    b.validate()?;
    write_json(&b.feedback_path(workspace), b)
}

pub fn get_feedback(
    workspace: &Workspace,
    class_id: Uuid,
    assignment_id: Uuid,
    member_id: Uuid,
) -> EduResult<FeedbackBundle> {
    load_bundle(workspace, class_id, assignment_id, member_id)
}

/// All feedback bundles for an assignment (across members/classes).
pub fn list_feedback(workspace: &Workspace, assignment_id: Uuid) -> EduResult<Vec<FeedbackBundle>> {
    let mut out = Vec::new();
    let dir = workspace.root.join("feedback");
    if !dir.exists() {
        return Ok(out);
    }
    for class_entry in std::fs::read_dir(&dir)? {
        let class_dir = class_entry?.path();
        if !class_dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&class_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // filename: "<assignment>__<member>.json" ('__' separates the
            // two UUIDs unambiguously — UUIDs themselves contain '-').
            let Some((a_part, _)) = name.split_once("__") else {
                continue;
            };
            if Uuid::parse_str(a_part).ok() != Some(assignment_id) {
                continue;
            }
            if let Ok(b) = read_or::<FeedbackBundle>(&path, FeedbackBundle::default())
                && b.schema != 0
            {
                out.push(b);
            }
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub fn add_comment(
    workspace: &Workspace,
    class_id: Uuid,
    assignment_id: Uuid,
    member_id: Uuid,
    author: AuthorRef,
    anchor: Option<ProjectAnchor>,
    category: CommentCategory,
    text: &str,
) -> EduResult<Comment> {
    let mut bundle = load_bundle(workspace, class_id, assignment_id, member_id)?;
    let comment = Comment {
        author,
        anchor,
        category,
        text: text.trim().to_string(),
        created_unix: now_unix(),
        ..Comment::default()
    };
    bundle.comments.push(comment.clone());
    bundle.updated_unix = now_unix();
    save_bundle(workspace, &bundle)?;
    Ok(comment)
}

pub fn resolve_comment(
    workspace: &Workspace,
    class_id: Uuid,
    assignment_id: Uuid,
    member_id: Uuid,
    comment_id: Uuid,
) -> EduResult<()> {
    let mut bundle = load_bundle(workspace, class_id, assignment_id, member_id)?;
    let c = bundle
        .comments
        .iter_mut()
        .find(|c| c.id == comment_id)
        .ok_or_else(|| EduError::NotFound("Kommentar nicht gefunden".into()))?;
    c.resolved = true;
    bundle.updated_unix = now_unix();
    save_bundle(workspace, &bundle)
}

/// Student acknowledges a comment (read receipt).
pub fn acknowledge_comment(
    workspace: &Workspace,
    class_id: Uuid,
    assignment_id: Uuid,
    member_id: Uuid,
    comment_id: Uuid,
) -> EduResult<()> {
    let mut bundle = load_bundle(workspace, class_id, assignment_id, member_id)?;
    let c = bundle
        .comments
        .iter_mut()
        .find(|c| c.id == comment_id)
        .ok_or_else(|| EduError::NotFound("Kommentar nicht gefunden".into()))?;
    c.acknowledged = true;
    bundle.updated_unix = now_unix();
    save_bundle(workspace, &bundle)
}

/// Set summary/rubric marks.
pub fn set_feedback_summary(
    workspace: &Workspace,
    class_id: Uuid,
    assignment_id: Uuid,
    member_id: Uuid,
    summary: Option<String>,
    rubric_marks: BTreeMap<String, String>,
) -> EduResult<FeedbackBundle> {
    let mut bundle = load_bundle(workspace, class_id, assignment_id, member_id)?;
    bundle.summary = summary;
    bundle.rubric_marks = rubric_marks;
    bundle.updated_unix = now_unix();
    save_bundle(workspace, &bundle)?;
    Ok(bundle)
}

/// Checks whether an anchor resolves against a loaded `Project`.
/// Used by the app when rendering feedback overlays: object id first,
/// then layer+frame, then scene label. Returns true when resolved.
pub fn anchor_resolves(project: &nova_core::Project, anchor: &ProjectAnchor) -> bool {
    if anchor.is_empty() {
        return true; // general comment always "resolves"
    }
    // Try object id across all scenes.
    if let Some(oid) = anchor.object_id {
        for scene in project.scenes.iter() {
            for layer in &scene.layers {
                for frame in &layer.frames {
                    if frame.objects.iter().any(|o| o.id == oid) {
                        return true;
                    }
                }
            }
        }
        // Also check symbol timelines.
        for symbol in &project.symbols {
            for layer in &symbol.timeline.layers {
                for frame in &layer.frames {
                    if frame.objects.iter().any(|o| o.id == oid) {
                        return true;
                    }
                }
            }
        }
        return false;
    }
    // Layer+frame or scene fallback: check scene existence.
    if let Some(sid) = anchor.scene_id {
        return project.scenes.iter().any(|s| s.id == sid)
            || project.symbols.iter().any(|s| s.id == sid);
    }
    // Only a label: treat as resolvable (human-readable fallback).
    anchor
        .label
        .as_ref()
        .map(|l| !l.trim().is_empty())
        .unwrap_or(false)
}

/// Marks `orphaned` on all comments of a bundle given a resolution check.
pub fn refresh_orphans(
    workspace: &Workspace,
    class_id: Uuid,
    assignment_id: Uuid,
    member_id: Uuid,
    project: &nova_core::Project,
) -> EduResult<usize> {
    let mut bundle = load_bundle(workspace, class_id, assignment_id, member_id)?;
    let mut changed = 0;
    for c in bundle.comments.iter_mut() {
        let orphaned = match &c.anchor {
            Some(a) => !anchor_resolves(project, a),
            None => false,
        };
        if orphaned != c.orphaned {
            c.orphaned = orphaned;
            changed += 1;
        }
    }
    if changed > 0 {
        bundle.updated_unix = now_unix();
        save_bundle(workspace, &bundle)?;
    }
    Ok(changed)
}

/// Submission state label passthrough for progress aggregation.
pub fn state_label(state: SubmissionState) -> &'static str {
    state.label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::school::SchoolConfig;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova-edu-fb-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(&dir, &SchoolConfig::default()).unwrap();
        (ws, dir)
    }

    #[test]
    fn comment_roundtrip_and_resolve() {
        let (ws, dir) = ws("c");
        let ids = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let author = AuthorRef {
            display_name: "Lehrkraft".into(),
            role: "teacher".into(),
        };
        let c = add_comment(
            &ws,
            ids.0,
            ids.1,
            ids.2,
            author,
            Some(ProjectAnchor {
                object_id: Some(Uuid::new_v4()),
                label: Some("Kugel".into()),
                ..Default::default()
            }),
            CommentCategory::Issue,
            "Bitte Farbe anpassen",
        )
        .unwrap();

        let bundle = get_feedback(&ws, ids.0, ids.1, ids.2).unwrap();
        assert_eq!(bundle.comments.len(), 1);
        assert_eq!(bundle.comments[0].text, "Bitte Farbe anpassen");

        resolve_comment(&ws, ids.0, ids.1, ids.2, c.id).unwrap();
        acknowledge_comment(&ws, ids.0, ids.1, ids.2, c.id).unwrap();
        let bundle = get_feedback(&ws, ids.0, ids.1, ids.2).unwrap();
        assert!(bundle.comments[0].resolved);
        assert!(bundle.comments[0].acknowledged);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn orphan_detection() {
        let project = nova_core::Project::demo();
        let real_obj = project.scenes[0]
            .layers
            .iter()
            .flat_map(|l| l.frames.iter())
            .flat_map(|f| f.objects.iter())
            .map(|o| o.id)
            .next()
            .unwrap();

        let good = ProjectAnchor {
            object_id: Some(real_obj),
            ..Default::default()
        };
        let bad = ProjectAnchor {
            object_id: Some(Uuid::new_v4()),
            ..Default::default()
        };
        let general = ProjectAnchor::default();
        assert!(anchor_resolves(&project, &good));
        assert!(!anchor_resolves(&project, &bad));
        assert!(anchor_resolves(&project, &general));
        let _ = project;
    }

    #[test]
    fn list_feedback_filters_assignment() {
        let (ws, dir) = ws("list");
        let (c, a1, m) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let a2 = Uuid::new_v4();
        add_comment(
            &ws,
            c,
            a1,
            m,
            AuthorRef {
                display_name: "x".into(),
                role: "teacher".into(),
            },
            None,
            CommentCategory::Praise,
            "gut",
        )
        .unwrap();
        add_comment(
            &ws,
            c,
            a2,
            m,
            AuthorRef {
                display_name: "x".into(),
                role: "teacher".into(),
            },
            None,
            CommentCategory::Praise,
            "gut",
        )
        .unwrap();
        assert_eq!(list_feedback(&ws, a1).unwrap().len(), 1);
        assert_eq!(list_feedback(&ws, a2).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
