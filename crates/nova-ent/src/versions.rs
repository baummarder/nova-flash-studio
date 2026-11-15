//! Project versioning (M21.4): snapshots, history, diff summary, restore.
//!
//! Snapshots store a full byte copy of the `.nova` file under
//! `versions/<project-id>/` plus metadata. Restore writes the snapshot
//! back **only after** taking a safety snapshot of the current state, so
//! restores are themselves undoable. The original file is never touched
//! by anything except an explicit `restore` call.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assets::sha256_hex;
use crate::error::{EntError, EntResult};
use crate::store::{ENT_SCHEMA_VERSION, now_unix, read_or, write_json};
use crate::team::ProjectRegistration;
use crate::workspace::Workspace;

/// Human/machine-readable change summary between two project states.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChangeSummary {
    pub scenes_added: Vec<String>,
    pub scenes_removed: Vec<String>,
    pub scenes_renamed: Vec<(String, String)>,
    pub assets_added: Vec<String>,
    pub assets_removed: Vec<String>,
    pub layers_added: Vec<String>,
    pub layers_removed: Vec<String>,
    pub objects_added: usize,
    pub objects_removed: usize,
    /// Free-form note lines for details that don't fit the buckets.
    pub notes: Vec<String>,
}

impl ChangeSummary {
    pub fn is_empty(&self) -> bool {
        self.scenes_added.is_empty()
            && self.scenes_removed.is_empty()
            && self.scenes_renamed.is_empty()
            && self.assets_added.is_empty()
            && self.assets_removed.is_empty()
            && self.layers_added.is_empty()
            && self.layers_removed.is_empty()
            && self.objects_added == 0
            && self.objects_removed == 0
            && self.notes.is_empty()
    }

    /// One-line summary for lists.
    pub fn headline(&self) -> String {
        if self.is_empty() {
            return "Keine strukturellen Änderungen".into();
        }
        let mut parts = Vec::new();
        if !self.scenes_added.is_empty() {
            parts.push(format!("+{} Szenen", self.scenes_added.len()));
        }
        if !self.scenes_removed.is_empty() {
            parts.push(format!("-{} Szenen", self.scenes_removed.len()));
        }
        if !self.assets_added.is_empty() {
            parts.push(format!("+{} Assets", self.assets_added.len()));
        }
        if !self.assets_removed.is_empty() {
            parts.push(format!("-{} Assets", self.assets_removed.len()));
        }
        if self.objects_added > 0 || self.objects_removed > 0 {
            parts.push(format!(
                "Objekte +{}/-{}",
                self.objects_added, self.objects_removed
            ));
        }
        if parts.is_empty() {
            "Details in Versionseintrag".into()
        } else {
            parts.join(", ")
        }
    }
}

/// One snapshot record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionEntry {
    pub schema: u32,
    pub id: Uuid,
    /// Project registration this version belongs to.
    pub project_id: Uuid,
    /// File name inside `versions/<project>/` (sha256-based + counter).
    pub file: String,
    /// sha256 of the snapshot bytes (also the integrity anchor).
    pub sha256: String,
    pub size_bytes: u64,
    /// Author label (local profile display name — not authentication).
    pub author: String,
    pub message: String,
    /// Parent version id (previous head at snapshot time).
    pub parent: Option<Uuid>,
    /// Diff summary vs parent (None for root snapshots).
    pub changes: Option<ChangeSummary>,
    pub created_unix: u64,
    /// True when this entry was created automatically by a restore.
    pub safety: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for VersionEntry {
    fn default() -> Self {
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            file: String::new(),
            sha256: String::new(),
            size_bytes: 0,
            author: String::new(),
            message: String::new(),
            parent: None,
            changes: None,
            created_unix: now_unix(),
            safety: false,
            extra: serde_json::Map::new(),
        }
    }
}

fn versions_index_path(workspace: &Workspace, project_id: Uuid) -> PathBuf {
    workspace.version_dir(project_id).join("index.json")
}

fn load_index(workspace: &Workspace, project_id: Uuid) -> EntResult<Vec<VersionEntry>> {
    let mut entries: Vec<VersionEntry> =
        read_or(&versions_index_path(workspace, project_id), Vec::new())?;
    for e in &mut entries {
        if e.schema == 0 {
            e.schema = ENT_SCHEMA_VERSION;
        }
    }
    entries.sort_by_key(|e| e.created_unix);
    Ok(entries)
}

fn save_index(workspace: &Workspace, project_id: Uuid, entries: &[VersionEntry]) -> EntResult<()> {
    write_json(
        &versions_index_path(workspace, project_id),
        &entries.to_vec(),
    )
}

/// Snapshot file path for an entry.
pub fn snapshot_path(workspace: &Workspace, entry: &VersionEntry) -> PathBuf {
    workspace.version_dir(entry.project_id).join(&entry.file)
}

/// Loads + parses a project file into `nova_core::Project`.
fn parse_project(bytes: &[u8]) -> EntResult<nova_core::Project> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| EntError::Invalid("Projekt ist kein gültiges UTF-8".into()))?;
    Ok(nova_core::Project::from_json(text)?)
}

/// Extracts comparable identity sets from a loaded project.
struct ProjectShape {
    scene_names: Vec<String>,
    asset_names: Vec<String>,
    layer_names: Vec<String>,
    object_count: usize,
}

fn shape_of(project: &nova_core::Project) -> ProjectShape {
    let mut scene_names: Vec<String> = project.scenes.iter().map(|s| s.name.clone()).collect();
    scene_names.sort();
    let mut asset_names: Vec<String> = project.assets.iter().map(|a| a.name.clone()).collect();
    asset_names.sort();
    asset_names.dedup();
    let mut layer_names: Vec<String> = Vec::new();
    let mut object_count = 0usize;
    for scene in &project.scenes {
        for layer in &scene.layers {
            layer_names.push(format!("{}:{}", scene.name, layer.name));
            for frame in &layer.frames {
                object_count += frame.objects.len();
            }
        }
    }
    layer_names.sort();
    layer_names.dedup();
    ProjectShape {
        scene_names,
        asset_names,
        layer_names,
        object_count,
    }
}

fn diff_shapes(old: &ProjectShape, new: &ProjectShape) -> ChangeSummary {
    let scenes_added: Vec<String> = new
        .scene_names
        .iter()
        .filter(|n| !old.scene_names.contains(n))
        .cloned()
        .collect();
    let scenes_removed: Vec<String> = old
        .scene_names
        .iter()
        .filter(|n| !new.scene_names.contains(n))
        .cloned()
        .collect();
    let assets_added: Vec<String> = new
        .asset_names
        .iter()
        .filter(|n| !old.asset_names.contains(n))
        .cloned()
        .collect();
    let assets_removed: Vec<String> = old
        .asset_names
        .iter()
        .filter(|n| !new.asset_names.contains(n))
        .cloned()
        .collect();
    let layers_added: Vec<String> = new
        .layer_names
        .iter()
        .filter(|n| !old.layer_names.contains(n))
        .cloned()
        .collect();
    let layers_removed: Vec<String> = old
        .layer_names
        .iter()
        .filter(|n| !new.layer_names.contains(n))
        .cloned()
        .collect();
    ChangeSummary {
        scenes_added,
        scenes_removed,
        scenes_renamed: Vec::new(),
        assets_added,
        assets_removed,
        layers_added,
        layers_removed,
        objects_added: new.object_count.saturating_sub(old.object_count),
        objects_removed: old.object_count.saturating_sub(new.object_count),
        notes: Vec::new(),
    }
}

fn compute_changes(
    workspace: &Workspace,
    _project_id: Uuid,
    new_bytes: &[u8],
    parent: Option<&VersionEntry>,
) -> Option<ChangeSummary> {
    // Compare against the parent snapshot when available; otherwise try
    // the currently registered file BEFORE overwrite? Caller passes the
    // parent entry; for root snapshots return None.
    let parent = parent?;
    let parent_path = snapshot_path(workspace, parent);
    let old_bytes = std::fs::read(&parent_path).ok()?;
    let old_project = parse_project(&old_bytes).ok()?;
    let new_project = parse_project(new_bytes).ok()?;
    Some(diff_shapes(
        &shape_of(&old_project),
        &shape_of(&new_project),
    ))
}

/// Creates a snapshot of the registered project's current file state.
pub fn create_snapshot(
    workspace: &Workspace,
    reg: &ProjectRegistration,
    author: &str,
    message: &str,
    safety: bool,
) -> EntResult<VersionEntry> {
    let src = reg.abs_path();
    let bytes = std::fs::read(&src).map_err(|e| {
        EntError::Invalid(format!(
            "Projektdatei '{}' nicht lesbar: {e}",
            src.display()
        ))
    })?;
    let sha = sha256_hex(&bytes);
    let dir = workspace.version_dir(reg.id);
    std::fs::create_dir_all(&dir)?;

    let mut entries = load_index(workspace, reg.id)?;
    let parent = entries.last().cloned();

    let file = format!("snap-{}-{}.nova", now_unix(), &sha[..12]);
    let snap_path = dir.join(&file);
    // Avoid clobbering an identical existing snapshot (non-safety only —
    // safety snapshots must always be recorded for a truthful restore trail).
    if !safety && snap_path.exists() {
        let existing_sha = sha256_hex(&std::fs::read(&snap_path)?);
        if existing_sha == sha {
            // Return a pointer to the existing identical snapshot instead
            // of duplicating bytes — but keep index consistent.
            if let Some(existing) = entries.iter().find(|e| e.sha256 == sha && !e.safety) {
                return Ok(existing.clone());
            }
        }
    }
    std::fs::write(&snap_path, &bytes)?;

    let changes = if safety {
        None
    } else {
        compute_changes(workspace, reg.id, &bytes, parent.as_ref())
    };

    let entry = VersionEntry {
        project_id: reg.id,
        file,
        sha256: sha,
        size_bytes: bytes.len() as u64,
        author: author.to_string(),
        message: message.to_string(),
        parent: parent.map(|p| p.id),
        changes,
        safety,
        ..VersionEntry::default()
    };
    entries.push(entry.clone());
    save_index(workspace, reg.id, &entries)?;
    Ok(entry)
}

pub fn list_versions(workspace: &Workspace, project_id: Uuid) -> EntResult<Vec<VersionEntry>> {
    // Newest first for UI.
    let mut v = load_index(workspace, project_id)?;
    v.sort_by_key(|a| std::cmp::Reverse(a.created_unix));
    Ok(v)
}

pub fn get_version(workspace: &Workspace, project_id: Uuid, id: Uuid) -> EntResult<VersionEntry> {
    load_index(workspace, project_id)?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| EntError::NotFound("Version nicht gefunden".into()))
}

/// Verifies snapshot integrity (sha256 of stored bytes).
pub fn verify_version(workspace: &Workspace, entry: &VersionEntry) -> EntResult<bool> {
    let path = snapshot_path(workspace, entry);
    let bytes = std::fs::read(&path)?;
    Ok(sha256_hex(&bytes) == entry.sha256)
}

/// Restores a snapshot: first takes a safety snapshot of the CURRENT file,
/// then copies snapshot bytes over the project file. Returns (restored
/// entry, safety entry).
pub fn restore_version(
    workspace: &Workspace,
    reg: &ProjectRegistration,
    version_id: Uuid,
    author: &str,
) -> EntResult<(VersionEntry, Option<VersionEntry>)> {
    let entry = get_version(workspace, reg.id, version_id)?;
    if !verify_version(workspace, &entry)? {
        return Err(EntError::Integrity(format!(
            "Snapshot {} ist beschädigt (sha256-Mismatch)",
            entry.id
        )));
    }
    // Safety snapshot of current state (skip when file unreadable —
    // restore should still be possible for a missing file? No: refuse,
    // restoring over nothing loses the "before" state implicitly anyway).
    let safety = create_snapshot(
        workspace,
        reg,
        author,
        "Sicherheits-Snapshot vor Wiederherstellung",
        true,
    )?;
    let snap_bytes = std::fs::read(snapshot_path(workspace, &entry))?;
    let target = reg.abs_path();
    // Validate the snapshot is a real project before overwriting.
    parse_project(&snap_bytes)?;
    // Write via temp+rename in the target directory.
    let tmp = target.with_extension("nova.restore-tmp");
    std::fs::write(&tmp, &snap_bytes)?;
    let _ = std::fs::remove_file(&target);
    std::fs::rename(&tmp, &target)?;
    Ok((entry, Some(safety)))
}

/// Deletes a version entry and its bytes (safety entries included).
pub fn delete_version(workspace: &Workspace, project_id: Uuid, id: Uuid) -> EntResult<()> {
    let mut entries = load_index(workspace, project_id)?;
    let Some(pos) = entries.iter().position(|e| e.id == id) else {
        return Err(EntError::NotFound("Version nicht gefunden".into()));
    };
    let removed = entries.remove(pos);
    save_index(workspace, project_id, &entries)?;
    crate::store::remove_if_exists(&snapshot_path(workspace, &removed))?;
    Ok(())
}

/// Compares two arbitrary byte states (e.g. current file vs snapshot).
pub fn compare_bytes(a: &[u8], b: &[u8]) -> Option<ChangeSummary> {
    let pa = parse_project(a).ok()?;
    let pb = parse_project(b).ok()?;
    Some(diff_shapes(&shape_of(&pa), &shape_of(&pb)))
}

/// Summary of the current file vs a given snapshot.
pub fn compare_with_current(
    workspace: &Workspace,
    reg: &ProjectRegistration,
    version_id: Uuid,
) -> EntResult<ChangeSummary> {
    let entry = get_version(workspace, reg.id, version_id)?;
    let snap = std::fs::read(snapshot_path(workspace, &entry))?;
    let current = std::fs::read(reg.abs_path())?;
    // Diff direction: snapshot → current ("what changed since this version").
    compare_bytes(&snap, &current).ok_or_else(|| {
        EntError::Invalid("Projekt oder Snapshot lässt sich nicht als .nova parsen".into())
    })
}

/// Loads the project inside a snapshot for preview purposes.
pub fn load_snapshot_project(
    workspace: &Workspace,
    entry: &VersionEntry,
) -> EntResult<nova_core::Project> {
    let bytes = std::fs::read(snapshot_path(workspace, entry))?;
    parse_project(&bytes)
}

/// Storage stats for admin overview.
pub fn storage_stats(workspace: &Workspace) -> EntResult<(usize, u64)> {
    let mut count = 0usize;
    let mut bytes = 0u64;
    let dir = workspace.versions_dir();
    if dir.exists() {
        for entry in walkdir(&dir) {
            if let Ok(meta) = std::fs::metadata(&entry)
                && meta.is_file()
            {
                count += 1;
                bytes += meta.len();
            }
        }
    }
    Ok((count, bytes))
}

fn walkdir(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walkdir(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::register_project;
    use crate::workspace::Organization;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova-ent-ver-{}-{}", tag, std::process::id()));
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

    fn project_with_scene(name: &str) -> nova_core::Project {
        let mut p = nova_core::ProjectTemplate::Empty.create(name);
        // Default template already has a scene; rename it for clarity.
        if let Some(s) = p.scenes.first_mut() {
            s.name = format!("Szene {name}");
        }
        p
    }

    #[test]
    fn snapshot_history_restore() {
        let (ws, dir) = ws("hist");
        let f = dir.join("game.nova");
        project_with_scene("Eins").save(&f).unwrap();
        let reg = register_project(&ws, &f, "Game", None).unwrap();

        let v1 = create_snapshot(&ws, &reg, "Ada", "Basis", false).unwrap();
        assert!(verify_version(&ws, &v1).unwrap());

        // Modify project, snapshot again → diff should show changes.
        let mut p2 = nova_core::Project::load(&f).unwrap();
        p2.scenes
            .push(nova_core::Scene::new("Szene Zwei", 800.0, 600.0));
        p2.save(&f).unwrap();
        let v2 = create_snapshot(&ws, &reg, "Ada", "Zweite Szene", false).unwrap();
        let changes = v2.changes.as_ref().unwrap();
        assert_eq!(changes.scenes_added, vec!["Szene Zwei".to_string()]);
        assert_eq!(v2.parent, Some(v1.id));

        // Restore v1 → safety snapshot taken, file reverts.
        let (restored, safety) = restore_version(&ws, &reg, v1.id, "Ada").unwrap();
        assert_eq!(restored.id, v1.id);
        assert!(safety.is_some());
        let after = nova_core::Project::load(&f).unwrap();
        assert_eq!(after.scenes.len(), 1);

        // History intact: v1, v2, safety.
        assert_eq!(list_versions(&ws, reg.id).unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tampered_snapshot_rejected() {
        let (ws, dir) = ws("tamper");
        let f = dir.join("g.nova");
        project_with_scene("X").save(&f).unwrap();
        let reg = register_project(&ws, &f, "G", None).unwrap();
        let v = create_snapshot(&ws, &reg, "A", "", false).unwrap();
        std::fs::write(snapshot_path(&ws, &v), b"garbage").unwrap();
        assert!(restore_version(&ws, &reg, v.id, "A").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compare_current_vs_version() {
        let (ws, dir) = ws("cmp");
        let f = dir.join("g.nova");
        project_with_scene("Alt").save(&f).unwrap();
        let reg = register_project(&ws, &f, "G", None).unwrap();
        let v = create_snapshot(&ws, &reg, "A", "", false).unwrap();
        let mut p = nova_core::Project::load(&f).unwrap();
        p.assets.push(nova_core::Asset::new(
            "Neu",
            nova_core::AssetKind::Bitmap,
            "neu.png",
        ));
        p.save(&f).unwrap();
        let diff = compare_with_current(&ws, &reg, v.id).unwrap();
        assert_eq!(diff.assets_added, vec!["Neu".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
