//! Submissions (M20.4): student work + teacher inbox.
//!
//! A submission is a snapshot record pointing at project files inside the
//! workspace `inbox/` tree. Files are copied in — the student's working
//! `.nova` is never moved or overwritten.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assignment::ExportKind;
use crate::error::{EduError, EduResult, safe_join};
use crate::store::{EDU_SCHEMA_VERSION, read_or, write_json};
use crate::workspace::{Workspace, now_unix};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionState {
    /// Student still working (autosave only, not yet submitted).
    Draft,
    Submitted,
    /// Teacher returned it for rework.
    Returned,
    Reviewed,
    Graded,
}

impl SubmissionState {
    pub fn label(self) -> &'static str {
        match self {
            SubmissionState::Draft => "Entwurf",
            SubmissionState::Submitted => "Abgegeben",
            SubmissionState::Returned => "Zurückgegeben",
            SubmissionState::Reviewed => "Gesehen",
            SubmissionState::Graded => "Bewertet",
        }
    }

    /// Terminal-ish states that still allow a new revision.
    pub fn allows_resubmit(self) -> bool {
        matches!(
            self,
            SubmissionState::Returned | SubmissionState::Reviewed | SubmissionState::Graded
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Submission {
    pub schema: u32,
    pub id: Uuid,
    pub assignment_id: Uuid,
    pub class_id: Uuid,
    pub member_id: Uuid,
    pub revision: u32,
    pub submitted_unix: u64,
    /// Workspace-relative path of the submitted `.nova` inside inbox/.
    pub project: String,
    /// Relative path → sha256 hex for every packaged file.
    pub checksums: BTreeMap<String, String>,
    pub note: Option<String>,
    pub state: SubmissionState,
    /// True when submitted after the deadline (only possible if allowed).
    pub late: bool,
    /// Optional exported artifacts (swf/xfl) packaged alongside.
    pub exports: Vec<ExportedFile>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedFile {
    pub kind: ExportKind,
    pub path: String,
}

impl Default for Submission {
    fn default() -> Self {
        Self {
            schema: EDU_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            assignment_id: Uuid::nil(),
            class_id: Uuid::nil(),
            member_id: Uuid::nil(),
            revision: 0,
            submitted_unix: 0,
            project: String::new(),
            checksums: BTreeMap::new(),
            note: None,
            state: SubmissionState::Draft,
            late: false,
            exports: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Submission {
    pub fn submission_dir(&self, workspace: &Workspace) -> PathBuf {
        workspace.submission_dir(self.class_id, self.assignment_id)
    }

    pub fn meta_path(&self, workspace: &Workspace) -> PathBuf {
        self.submission_dir(workspace)
            .join(format!("{}-r{}.json", self.member_id, self.revision))
    }

    pub fn project_abs(&self, workspace: &Workspace) -> EduResult<PathBuf> {
        safe_join(&workspace.root, &self.project)
    }

    pub fn validate(&self) -> EduResult<()> {
        if self.project.is_empty() {
            return Err(EduError::Invalid("Submission braucht Projektpfad".into()));
        }
        if self.project.contains("..") || std::path::Path::new(&self.project).is_absolute() {
            return Err(EduError::Invalid(
                "Projektpfad muss relativ und ohne .. sein".into(),
            ));
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ store

fn submission_meta_path(workspace: &Workspace, sub: &Submission) -> PathBuf {
    sub.submission_dir(workspace)
        .join(format!("sub-{}.json", sub.id))
}

fn save_submission(workspace: &Workspace, sub: &Submission) -> EduResult<()> {
    sub.validate()?;
    let dir = sub.submission_dir(workspace);
    std::fs::create_dir_all(&dir)?;
    write_json(&submission_meta_path(workspace, sub), sub)
}

/// All submissions for one assignment across classes.
pub fn list_submissions(workspace: &Workspace, assignment_id: Uuid) -> EduResult<Vec<Submission>> {
    let mut out = Vec::new();
    let inbox = workspace.root.join("inbox");
    if !inbox.exists() {
        return Ok(out);
    }
    for class_entry in std::fs::read_dir(&inbox)? {
        let class_dir = class_entry?.path();
        if !class_dir.is_dir() {
            continue;
        }
        for asg_dir in std::fs::read_dir(&class_dir)? {
            let dir = asg_dir?.path();
            if dir.file_name().and_then(|n| n.to_str()) != Some(&assignment_id.to_string()) {
                continue;
            }
            for entry in std::fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(sub) = read_or::<Submission>(&path, Submission::default()) {
                    if sub.schema != 0 {
                        out.push(sub);
                    }
                }
            }
        }
    }
    out.sort_by_key(|s| s.submitted_unix);
    Ok(out)
}

/// Latest revision per member for an assignment.
pub fn latest_per_member(workspace: &Workspace, assignment_id: Uuid) -> EduResult<Vec<Submission>> {
    let mut latest: BTreeMap<Uuid, Submission> = BTreeMap::new();
    for sub in list_submissions(workspace, assignment_id)? {
        match latest.get(&sub.member_id) {
            Some(existing) if existing.revision >= sub.revision => {}
            _ => {
                latest.insert(sub.member_id, sub);
            }
        }
    }
    Ok(latest.into_values().collect())
}

pub fn get_submission(workspace: &Workspace, id: Uuid) -> EduResult<Submission> {
    // Scan is acceptable at classroom scale (hundreds of submissions).
    for class_entry in std::fs::read_dir(workspace.root.join("inbox"))? {
        let class_dir = class_entry?.path();
        if !class_dir.is_dir() {
            continue;
        }
        for asg_entry in std::fs::read_dir(&class_dir)? {
            let asg_dir = asg_entry?.path();
            if !asg_dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&asg_dir)? {
                let path = entry?.path();
                if path.file_stem().and_then(|s| s.to_str()) == Some(&format!("sub-{id}")) {
                    let sub: Submission = crate::store::read_or(&path, Submission::default())?;
                    return Ok(sub);
                }
            }
        }
    }
    Err(EduError::NotFound(format!("Abgabe {id} nicht gefunden")))
}

/// Records a submission. `files` are (workspace-relative destination, bytes)
/// already validated by the caller; the `.nova` project must be first.
/// Never overwrites: each revision gets a new id and new files.
pub fn record_submission(
    workspace: &Workspace,
    mut sub: Submission,
    files: &[(String, Vec<u8>)],
) -> EduResult<Submission> {
    sub.schema = EDU_SCHEMA_VERSION;
    sub.submitted_unix = now_unix();
    sub.state = SubmissionState::Submitted;

    // Determine next revision for this member+assignment.
    let existing = list_submissions(workspace, sub.assignment_id)?
        .into_iter()
        .filter(|s| s.member_id == sub.member_id)
        .map(|s| s.revision)
        .max()
        .unwrap_or(0);
    sub.revision = existing + 1;

    let dir = sub.submission_dir(workspace).join(sub.id.to_string());
    std::fs::create_dir_all(&dir)?;

    // Write payload files under inbox/<class>/<asg>/<sub-id>/files/.
    let payload_rel = format!(
        "inbox/{}/{}/{}/files",
        sub.class_id, sub.assignment_id, sub.id
    );
    let mut checksums = BTreeMap::new();
    for (rel_dest, bytes) in files {
        // rel_dest is like "project/arbeit.nova" or "exports/film.swf"
        let safe = safe_join(&dir.join("files"), rel_dest)?;
        if let Some(parent) = safe.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&safe, bytes)?;
        let rel_key = format!("{payload_rel}/{rel_dest}");
        checksums.insert(
            rel_key.clone(),
            sha256_hex(std::fs::read(&safe)?.as_slice()),
        );
        // Record project path as the canonical .nova.
        if rel_dest.starts_with("project/") && sub.project.is_empty() {
            sub.project = rel_key;
        }
    }
    sub.checksums = checksums;
    sub.validate()?;
    save_submission(workspace, &sub)?;
    Ok(sub)
}

/// Marks a submission reviewed/graded/returned.
pub fn set_submission_state(
    workspace: &Workspace,
    id: Uuid,
    state: SubmissionState,
) -> EduResult<Submission> {
    let mut sub = get_submission(workspace, id)?;
    sub.state = state;
    save_submission(workspace, &sub)?;
    Ok(sub)
}

/// Deletes a submission and its payload directory.
pub fn delete_submission(workspace: &Workspace, id: Uuid) -> EduResult<()> {
    let sub = get_submission(workspace, id)?;
    crate::store::remove_if_exists(&submission_meta_path(workspace, &sub))?;
    // Files live under inbox/<class>/<asg>/<sub-id>/files/; remove them via
    // the recorded checksums, then drop the whole per-submission directory.
    for key in sub.checksums.keys() {
        if let Ok(abs) = safe_join(&workspace.root, key) {
            crate::store::remove_if_exists(&abs)?;
        }
    }
    // Remove the per-submission payload directory.
    let _ = crate::store::remove_dir_if_exists(&sub.submission_dir(workspace).join(id.to_string()));
    Ok(())
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Late flag helper: compares submission time to assignment deadline.
pub fn is_late(submitted_unix: u64, due_unix: Option<u64>) -> bool {
    due_unix.map(|due| submitted_unix > due).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assignment::Assignment;
    use crate::school::SchoolConfig;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova-edu-sub-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(&dir, &SchoolConfig::default()).unwrap();
        (ws, dir)
    }

    fn nova_bytes(tag: &str) -> Vec<u8> {
        let project = nova_core::ProjectTemplate::Empty.create(tag);
        project.to_json().unwrap().into_bytes()
    }

    #[test]
    fn submission_lifecycle() {
        let (ws, dir) = ws("life");
        let class = crate::classroom::create_classroom(&ws, "K").unwrap();
        let member = crate::classroom::add_member(&ws, class.id, "Schüler", None).unwrap();
        let asg = crate::assignment::save_new_assignment(
            &ws,
            Assignment {
                title: "A".into(),
                ..Default::default()
            },
        )
        .unwrap();

        let files = vec![
            ("project/arbeit.nova".to_string(), nova_bytes("s1")),
            ("exports/film.swf".to_string(), b"fake-swf".to_vec()),
        ];
        let sub = record_submission(
            &ws,
            Submission {
                assignment_id: asg.id,
                class_id: class.id,
                member_id: member.id,
                ..Default::default()
            },
            &files,
        )
        .unwrap();

        assert_eq!(sub.revision, 1);
        assert_eq!(sub.state, SubmissionState::Submitted);
        assert!(sub.project.contains("project/arbeit.nova"));
        assert_eq!(sub.checksums.len(), 2);
        assert!(sub.project_abs(&ws).unwrap().exists());

        // Resubmission bumps revision, never overwrites.
        let sub2 = record_submission(
            &ws,
            Submission {
                assignment_id: asg.id,
                class_id: class.id,
                member_id: member.id,
                ..Default::default()
            },
            &[("project/arbeit2.nova".to_string(), nova_bytes("s2"))],
        )
        .unwrap();
        assert_eq!(sub2.revision, 2);
        assert_ne!(sub.id, sub2.id);

        let listed = list_submissions(&ws, asg.id).unwrap();
        assert_eq!(listed.len(), 2);
        let latest = latest_per_member(&ws, asg.id).unwrap();
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].revision, 2);

        set_submission_state(&ws, sub2.id, SubmissionState::Reviewed).unwrap();
        let fetched = get_submission(&ws, sub2.id).unwrap();
        assert_eq!(fetched.state, SubmissionState::Reviewed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn submission_requires_project() {
        let (ws, dir) = ws("noproject");
        let err = record_submission(
            &ws,
            Submission {
                assignment_id: Uuid::new_v4(),
                class_id: Uuid::new_v4(),
                member_id: Uuid::new_v4(),
                ..Default::default()
            },
            &[("exports/x.swf".to_string(), vec![1])],
        )
        .unwrap_err();
        assert!(matches!(err, EduError::Invalid(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn late_detection() {
        assert!(is_late(2000, Some(1000)));
        assert!(!is_late(500, Some(1000)));
        assert!(!is_late(500, None));
    }
}
