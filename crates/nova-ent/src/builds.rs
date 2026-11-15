//! Build profiles, build history and export validation (M21.7).
//!
//! Profiles are named, versioned configurations that drive the *existing*
//! `nova-build`/`nova-formats` pipelines â€” no parallel exporter. History
//! records are append-only JSONL with output path + sha256.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assets::sha256_hex;
use crate::error::{EntError, EntResult};
use crate::store::{ENT_SCHEMA_VERSION, append_jsonl, now_unix, read_jsonl, read_or, write_json};
use crate::workspace::Workspace;

/// Export targets supported by the existing pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildTarget {
    /// Binary SWF via `nova_build::build_swf`.
    Swf,
    /// XFL interchange via `nova_formats`.
    Xfl,
}

impl BuildTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Swf => "SWF",
            Self::Xfl => "XFL",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Swf => "swf",
            Self::Xfl => "xfl",
        }
    }
}

/// Named, reusable build configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildProfile {
    pub schema: u32,
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub targets: Vec<BuildTarget>,
    /// Compress SWF output (deflate) when the pipeline supports it.
    pub compress: bool,
    /// Include game/scene metadata exports where applicable.
    pub include_all_scenes: bool,
    /// Optional notes rendered into the build report.
    pub notes: String,
    pub created_unix: u64,
    pub updated_unix: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for BuildProfile {
    fn default() -> Self {
        let now = now_unix();
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: String::new(),
            description: String::new(),
            targets: vec![BuildTarget::Swf],
            compress: true,
            include_all_scenes: true,
            notes: String::new(),
            created_unix: now,
            updated_unix: now,
            extra: serde_json::Map::new(),
        }
    }
}

impl BuildProfile {
    pub fn validate(&self) -> EntResult<()> {
        if self.name.trim().is_empty() {
            return Err(EntError::Invalid("Profilname darf nicht leer sein".into()));
        }
        if self.targets.is_empty() {
            return Err(EntError::Invalid("Mindestens ein Build-Ziel nÃ¶tig".into()));
        }
        Ok(())
    }
}

/// Whole profile collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileSet {
    pub schema: u32,
    pub profiles: Vec<BuildProfile>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for ProfileSet {
    fn default() -> Self {
        Self {
            schema: ENT_SCHEMA_VERSION,
            profiles: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

fn load_profiles(workspace: &Workspace) -> EntResult<ProfileSet> {
    let mut set: ProfileSet = read_or(&workspace.profiles_path(), ProfileSet::default())?;
    if set.schema == 0 {
        set.schema = ENT_SCHEMA_VERSION;
    }
    Ok(set)
}

fn save_profiles(workspace: &Workspace, set: &ProfileSet) -> EntResult<()> {
    write_json(&workspace.profiles_path(), set)
}

pub fn list_profiles(workspace: &Workspace) -> EntResult<Vec<BuildProfile>> {
    Ok(load_profiles(workspace)?.profiles)
}

pub fn get_profile(workspace: &Workspace, id: Uuid) -> EntResult<BuildProfile> {
    load_profiles(workspace)?
        .profiles
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| EntError::NotFound("Build-Profil nicht gefunden".into()))
}

pub fn save_profile(workspace: &Workspace, profile: &BuildProfile) -> EntResult<BuildProfile> {
    let mut set = load_profiles(workspace)?;
    profile.validate()?;
    let mut p = profile.clone();
    p.schema = ENT_SCHEMA_VERSION;
    p.updated_unix = now_unix();
    if let Some(existing) = set.profiles.iter_mut().find(|x| x.id == p.id) {
        *existing = p.clone();
    } else {
        set.profiles.push(p.clone());
    }
    save_profiles(workspace, &set)?;
    Ok(p)
}

pub fn delete_profile(workspace: &Workspace, id: Uuid) -> EntResult<()> {
    let mut set = load_profiles(workspace)?;
    set.profiles.retain(|p| p.id != id);
    save_profiles(workspace, &set)
}

/// One executed build record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildRecord {
    pub schema: u32,
    pub id: Uuid,
    pub profile_id: Uuid,
    pub profile_name: String,
    pub project_id: Uuid,
    pub project_path: String,
    /// Workspace-relative or absolute output path of the artifact.
    pub output: String,
    pub target: BuildTarget,
    pub sha256: String,
    pub size_bytes: u64,
    pub warnings: usize,
    pub errors: usize,
    pub duration_ms: u64,
    pub actor: String,
    pub created_unix: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for BuildRecord {
    fn default() -> Self {
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            profile_id: Uuid::nil(),
            profile_name: String::new(),
            project_id: Uuid::nil(),
            project_path: String::new(),
            output: String::new(),
            target: BuildTarget::Swf,
            sha256: String::new(),
            size_bytes: 0,
            warnings: 0,
            errors: 0,
            duration_ms: 0,
            actor: String::new(),
            created_unix: now_unix(),
            extra: serde_json::Map::new(),
        }
    }
}

/// Result of actually running a build (filled by the app layer which owns
/// the project handle; the domain layer records + validates).
pub struct BuildOutcome {
    pub output_path: PathBuf,
    pub bytes: Vec<u8>,
    pub warnings: usize,
    pub errors: usize,
    pub duration_ms: u64,
}

/// Executes a build for one target via the existing pipelines.
/// `project` must already be loaded; XFL export goes through
/// `nova_formats` when available for the chosen target.
pub fn run_build(
    workspace: &Workspace,
    profile: &BuildProfile,
    project: &nova_core::Project,
    project_id: Uuid,
    project_path: &str,
    actor: &str,
) -> EntResult<Vec<BuildRecord>> {
    let mut records = Vec::new();
    for target in &profile.targets {
        let started = std::time::Instant::now();
        let out_dir = workspace
            .exports_dir()
            .join(format!("{}-{}", project_id, now_unix()));
        std::fs::create_dir_all(&out_dir)?;
        let (bytes, warnings, errors) = match target {
            BuildTarget::Swf => {
                let out = nova_build::build_swf(project)
                    .map_err(|e| EntError::Invalid(format!("SWF-Build fehlgeschlagen: {e}")))?;
                (out.bytes, out.warnings.len(), 0usize)
            }
            BuildTarget::Xfl => {
                // XFL compatibility export via nova-formats (experimental,
                // documented subset â€” same pipeline the UI uses).
                let result = nova_formats::export_xfl(project)
                    .map_err(|e| EntError::Invalid(format!("XFL-Export fehlgeschlagen: {e}")))?;
                (result.bytes, result.warnings.len(), 0)
            }
        };
        let duration_ms = started.elapsed().as_millis() as u64;
        let sha = sha256_hex(&bytes);
        let filename = format!(
            "{}-{}.{}",
            sanitize_project_name(&project.metadata.name),
            &sha[..8],
            target.extension()
        );
        let output = out_dir.join(filename);
        std::fs::write(&output, &bytes)?;

        let record = BuildRecord {
            profile_id: profile.id,
            profile_name: profile.name.clone(),
            project_id,
            project_path: project_path.to_string(),
            output: output.to_string_lossy().into_owned(),
            target: *target,
            sha256: sha,
            size_bytes: bytes.len() as u64,
            warnings,
            errors,
            duration_ms,
            actor: actor.to_string(),
            ..BuildRecord::default()
        };
        append_jsonl(&workspace.build_history_path(), &record)?;
        records.push(record);
    }
    Ok(records)
}

/// Storage stats helper kept for parity with other modules.
#[allow(dead_code)]
fn count_files(dir: &std::path::Path) -> usize {
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                n += count_files(&p);
            } else {
                n += 1;
            }
        }
    }
    n
}

fn sanitize_project_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let t = cleaned.trim_matches('-');
    if t.is_empty() {
        "project".into()
    } else {
        t.chars().take(40).collect()
    }
}

pub fn build_history(workspace: &Workspace) -> EntResult<Vec<BuildRecord>> {
    let mut v: Vec<BuildRecord> = read_jsonl(&workspace.build_history_path())?;
    v.sort_by_key(|a| std::cmp::Reverse(a.created_unix));
    Ok(v)
}

/// Verifies a recorded artifact still exists and matches its sha256.
pub fn verify_build_record(record: &BuildRecord) -> EntResult<bool> {
    let bytes = std::fs::read(&record.output)?;
    Ok(sha256_hex(&bytes) == record.sha256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::register_project;
    use crate::workspace::Organization;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("nova-ent-build-{}-{}", tag, std::process::id()));
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

    #[test]
    fn profiles_crud() {
        let (ws, dir) = ws("prof");
        let p = save_profile(
            &ws,
            &BuildProfile {
                name: "Release".into(),
                targets: vec![BuildTarget::Swf],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(list_profiles(&ws).unwrap().len(), 1);
        assert!(
            save_profile(
                &ws,
                &BuildProfile {
                    name: "  ".into(),
                    ..Default::default()
                }
            )
            .is_err()
        );
        delete_profile(&ws, p.id).unwrap();
        assert!(list_profiles(&ws).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_and_history() {
        let (ws, dir) = ws("run");
        let f = dir.join("g.nova");
        let project = nova_core::ProjectTemplate::Empty.create("Build Test");
        project.save(&f).unwrap();
        let reg = register_project(&ws, &f, "G", None).unwrap();
        let profile = save_profile(
            &ws,
            &BuildProfile {
                name: "SWF".into(),
                targets: vec![BuildTarget::Swf],
                ..Default::default()
            },
        )
        .unwrap();
        let loaded = nova_core::Project::load(&f).unwrap();
        let records = run_build(&ws, &profile, &loaded, reg.id, &reg.path, "Dev").unwrap();
        assert_eq!(records.len(), 1);
        let rec = &records[0];
        assert!(rec.sha256.len() == 64);
        assert!(std::path::Path::new(&rec.output).exists());
        assert!(verify_build_record(rec).unwrap());
        assert_eq!(build_history(&ws).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
