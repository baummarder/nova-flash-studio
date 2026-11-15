//! Progress tracking (M20.6): derived strictly from recorded workflow
//! events — submissions, feedback, rubric marks. No telemetry, no
//! keystroke or timing metrics.

use std::collections::BTreeMap;

use serde::Serialize;
use uuid::Uuid;

use crate::assignment::Assignment;
use crate::classroom::{Classroom, find_member};
use crate::error::EduResult;
use crate::feedback::FeedbackBundle;
use crate::submission::{Submission, SubmissionState};
use crate::workspace::Workspace;

/// Per-student rollup for one assignment.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEntry {
    pub member_id: Uuid,
    pub member_name: String,
    pub group_name: Option<String>,
    pub submission_state: Option<SubmissionState>,
    pub revision: u32,
    pub submitted_unix: Option<u64>,
    pub late: bool,
    pub feedback_comments: usize,
    pub unacknowledged: usize,
    pub rubric_marks: BTreeMap<String, String>,
    /// Derived status — see `derive_status`.
    pub status: ProgressStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStatus {
    NotStarted,
    InProgress,
    Submitted,
    Reviewed,
    Mastered,
}

impl ProgressStatus {
    pub fn label(self) -> &'static str {
        match self {
            ProgressStatus::NotStarted => "Nicht begonnen",
            ProgressStatus::InProgress => "In Arbeit",
            ProgressStatus::Submitted => "Abgegeben",
            ProgressStatus::Reviewed => "Gesehen",
            ProgressStatus::Mastered => "Bewertet",
        }
    }
}

/// Derives the coarse status from recorded state only.
pub fn derive_status(sub: Option<&Submission>, bundle: Option<&FeedbackBundle>) -> ProgressStatus {
    match sub.map(|s| s.state) {
        Some(SubmissionState::Graded) => ProgressStatus::Mastered,
        Some(SubmissionState::Reviewed) => ProgressStatus::Reviewed,
        Some(SubmissionState::Submitted) | Some(SubmissionState::Returned) => {
            ProgressStatus::Submitted
        }
        Some(SubmissionState::Draft) => ProgressStatus::InProgress,
        None => {
            // Draft state isn't persisted as a submission; presence of
            // feedback without submission still means work exists.
            if bundle.map(|b| !b.comments.is_empty()).unwrap_or(false) {
                ProgressStatus::InProgress
            } else {
                ProgressStatus::NotStarted
            }
        }
    }
}

/// Builds the full progress table for an assignment: one row per member.
pub fn assignment_progress(
    workspace: &Workspace,
    class: &Classroom,
    assignment: &Assignment,
) -> EduResult<Vec<ProgressEntry>> {
    let submissions = crate::submission::latest_per_member(workspace, assignment.id)?;
    let bundles = crate::feedback::list_feedback(workspace, assignment.id)?;

    let mut rows = Vec::new();
    for member in class.members.iter().filter(|m| !m.archived) {
        let sub = submissions.iter().find(|s| s.member_id == member.id);
        let bundle = bundles
            .iter()
            .find(|b| b.member_id == member.id && b.class_id == class.id);
        let status = derive_status(sub, bundle);
        rows.push(ProgressEntry {
            member_id: member.id,
            member_name: member.author_name().to_string(),
            group_name: member.group_id.and_then(|gid| {
                class
                    .groups
                    .iter()
                    .find(|g| g.id == gid)
                    .map(|g| g.name.clone())
            }),
            submission_state: sub.map(|s| s.state),
            revision: sub.map(|s| s.revision).unwrap_or(0),
            submitted_unix: sub.map(|s| s.submitted_unix),
            late: sub.map(|s| s.late).unwrap_or(false),
            feedback_comments: bundle.map(|b| b.comments.len()).unwrap_or(0),
            unacknowledged: bundle
                .map(|b| {
                    b.comments
                        .iter()
                        .filter(|c| !c.acknowledged && !c.resolved)
                        .count()
                })
                .unwrap_or(0),
            rubric_marks: bundle.map(|b| b.rubric_marks.clone()).unwrap_or_default(),
            status,
        });
    }
    rows.sort_by(|a, b| {
        a.member_name
            .to_lowercase()
            .cmp(&b.member_name.to_lowercase())
    });
    Ok(rows)
}

/// Class-level overview: per-assignment status counts.
#[derive(Debug, Clone, Serialize)]
pub struct ClassOverview {
    pub class_id: Uuid,
    pub class_name: String,
    pub members: usize,
    pub assignments: Vec<AssignmentSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssignmentSummary {
    pub assignment_id: Uuid,
    pub title: String,
    pub due_unix: Option<u64>,
    pub submitted: usize,
    pub reviewed: usize,
    pub total_members: usize,
}

pub fn class_overview(workspace: &Workspace, class: &Classroom) -> EduResult<ClassOverview> {
    let assignments = crate::assignment::active_assignments(workspace)?;
    let mut summaries = Vec::new();
    for asg in &assignments {
        // Only assignments targeting this class (or untargeted = all).
        if !asg.targets.class_ids.is_empty() && !asg.targets.class_ids.contains(&class.id) {
            continue;
        }
        let rows = assignment_progress(workspace, class, asg)?;
        summaries.push(AssignmentSummary {
            assignment_id: asg.id,
            title: asg.title.clone(),
            due_unix: asg.due_unix,
            submitted: rows.iter().filter(|r| r.submission_state.is_some()).count(),
            reviewed: rows
                .iter()
                .filter(|r| {
                    matches!(
                        r.status,
                        ProgressStatus::Reviewed | ProgressStatus::Mastered
                    )
                })
                .count(),
            total_members: rows.len(),
        });
    }
    Ok(ClassOverview {
        class_id: class.id,
        class_name: class.name.clone(),
        members: class.members.iter().filter(|m| !m.archived).count(),
        assignments: summaries,
    })
}

/// Group-level aggregation of one assignment.
pub fn group_progress(
    workspace: &Workspace,
    class: &Classroom,
    assignment: &Assignment,
) -> EduResult<BTreeMap<String, Vec<ProgressEntry>>> {
    let rows = assignment_progress(workspace, class, assignment)?;
    let mut map: BTreeMap<String, Vec<ProgressEntry>> = BTreeMap::new();
    for row in rows {
        map.entry(
            row.group_name
                .clone()
                .unwrap_or_else(|| "Ohne Gruppe".into()),
        )
        .or_default()
        .push(row);
    }
    Ok(map)
}

/// Individual student overview across all assignments of a class.
pub fn student_overview(
    workspace: &Workspace,
    class: &Classroom,
    member_id: Uuid,
) -> EduResult<Option<Vec<ProgressEntry>>> {
    let member = find_member(class, member_id);
    if member.is_none() {
        return Ok(None);
    }
    let assignments = crate::assignment::active_assignments(workspace)?;
    let mut rows = Vec::new();
    for asg in &assignments {
        if !asg.targets.class_ids.is_empty() && !asg.targets.class_ids.contains(&class.id) {
            continue;
        }
        let all = assignment_progress(workspace, class, asg)?;
        if let Some(row) = all.into_iter().find(|r| r.member_id == member_id) {
            rows.push(row);
        }
    }
    Ok(Some(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assignment::Assignment;
    use crate::classroom::{add_member, create_classroom};
    use crate::school::SchoolConfig;
    use crate::submission::{Submission, record_submission};

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("nova-edu-prog-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(&dir, &SchoolConfig::default()).unwrap();
        (ws, dir)
    }

    #[test]
    fn status_derivation() {
        assert_eq!(derive_status(None, None), ProgressStatus::NotStarted);
        let sub = Submission {
            state: SubmissionState::Submitted,
            ..Default::default()
        };
        assert_eq!(derive_status(Some(&sub), None), ProgressStatus::Submitted);
        let graded = Submission {
            state: SubmissionState::Graded,
            ..Default::default()
        };
        assert_eq!(derive_status(Some(&graded), None), ProgressStatus::Mastered);
    }

    #[test]
    fn progress_rows_per_member() {
        let (ws, dir) = ws("rows");
        let class = create_classroom(&ws, "K").unwrap();
        let m1 = add_member(&ws, class.id, "Anna", None).unwrap();
        let m2 = add_member(&ws, class.id, "Ben", None).unwrap();
        let asg = crate::assignment::save_new_assignment(
            &ws,
            Assignment {
                title: "A".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let project = nova_core::ProjectTemplate::Empty.create("p");
        let bytes = project.to_json().unwrap().into_bytes();
        record_submission(
            &ws,
            Submission {
                assignment_id: asg.id,
                class_id: class.id,
                member_id: m1.id,
                ..Default::default()
            },
            &[("project/p.nova".to_string(), bytes)],
        )
        .unwrap();

        let class = crate::classroom::get_classroom(&ws, class.id).unwrap();
        let rows = assignment_progress(&ws, &class, &asg).unwrap();
        assert_eq!(rows.len(), 2);
        let anna = rows.iter().find(|r| r.member_id == m1.id).unwrap();
        let ben = rows.iter().find(|r| r.member_id == m2.id).unwrap();
        assert_eq!(anna.status, ProgressStatus::Submitted);
        assert_eq!(ben.status, ProgressStatus::NotStarted);

        let overview = class_overview(&ws, &class).unwrap();
        assert_eq!(overview.assignments.len(), 1);
        assert_eq!(overview.assignments[0].submitted, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
