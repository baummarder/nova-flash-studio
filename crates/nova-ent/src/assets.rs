//! Shared asset library (M21.3): folders, categories, tags, versions,
//! SHA-256 integrity, import/export.
//!
//! The library is a *sidecar* store: blobs live under `assets/blobs/`
//! content-addressed by sha256; metadata in `assets/index.json`. It does
//! not replace `Project.assets` — linking into a project uses the normal
//! asset import path.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EntError, EntResult, safe_join, sanitize_filename};
use crate::store::{ENT_SCHEMA_VERSION, now_unix, read_or, write_json};
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Svg,
    Png,
    Jpeg,
    Gif,
    Audio,
    Video,
    NovaResource,
    Symbol,
    Component,
    Other,
}

impl AssetKind {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "svg" => Self::Svg,
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "gif" => Self::Gif,
            "mp3" | "wav" | "ogg" | "flac" | "m4a" => Self::Audio,
            "mp4" | "webm" | "mov" | "avi" => Self::Video,
            "nova" => Self::NovaResource,
            _ => Self::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Svg => "SVG",
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::NovaResource => ".nova-Ressource",
            Self::Symbol => "Symbol",
            Self::Component => "Komponente",
            Self::Other => "Sonstiges",
        }
    }
}

/// One version of an asset's bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetVersion {
    pub id: Uuid,
    /// sha256 of the blob bytes.
    pub sha256: String,
    /// Workspace-relative blob path (usually assets/blobs/<sha>.bin).
    pub blob: String,
    pub size_bytes: u64,
    pub created_unix: u64,
    pub note: Option<String>,
}

/// Library entry metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryAsset {
    pub schema: u32,
    pub id: Uuid,
    pub name: String,
    /// Folder/category path inside the library, e.g. "marketing/logos".
    pub folder: String,
    pub kind: AssetKind,
    pub tags: Vec<String>,
    /// Original file name when imported.
    pub origin_name: Option<String>,
    /// Optional preview: workspace-relative path of a generated thumbnail.
    pub preview: Option<String>,
    /// Version history, newest last.
    pub versions: Vec<AssetVersion>,
    pub created_unix: u64,
    pub updated_unix: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for LibraryAsset {
    fn default() -> Self {
        let now = now_unix();
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: String::new(),
            folder: String::new(),
            kind: AssetKind::Other,
            tags: Vec::new(),
            origin_name: None,
            preview: None,
            versions: Vec::new(),
            created_unix: now,
            updated_unix: now,
            extra: serde_json::Map::new(),
        }
    }
}

impl LibraryAsset {
    pub fn current_version(&self) -> Option<&AssetVersion> {
        self.versions.last()
    }

    pub fn validate(&self) -> EntResult<()> {
        if self.name.trim().is_empty() {
            return Err(EntError::Invalid("Asset-Name darf nicht leer sein".into()));
        }
        if self.versions.is_empty() {
            return Err(EntError::Invalid(
                "Asset braucht mindestens eine Version".into(),
            ));
        }
        Ok(())
    }
}

/// Whole library index.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetIndex {
    pub schema: u32,
    pub assets: Vec<LibraryAsset>,
    /// Folder/category paths that were explicitly created.
    pub folders: Vec<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for AssetIndex {
    fn default() -> Self {
        Self {
            schema: ENT_SCHEMA_VERSION,
            assets: Vec::new(),
            folders: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

fn load_index(workspace: &Workspace) -> EntResult<AssetIndex> {
    let mut idx: AssetIndex = read_or(&workspace.assets_index_path(), AssetIndex::default())?;
    if idx.schema == 0 {
        idx.schema = ENT_SCHEMA_VERSION;
    }
    Ok(idx)
}

fn save_index(workspace: &Workspace, idx: &AssetIndex) -> EntResult<()> {
    write_json(&workspace.assets_index_path(), idx)
}

pub fn list_assets(workspace: &Workspace) -> EntResult<Vec<LibraryAsset>> {
    Ok(load_index(workspace)?.assets)
}

pub fn list_folders(workspace: &Workspace) -> EntResult<Vec<String>> {
    Ok(load_index(workspace)?.folders)
}

pub fn get_asset(workspace: &Workspace, id: Uuid) -> EntResult<LibraryAsset> {
    load_index(workspace)?
        .assets
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| EntError::NotFound(format!("Asset {id} nicht gefunden")))
}

/// Creates a folder path (normalized, no traversal).
pub fn create_folder(workspace: &Workspace, folder: &str) -> EntResult<String> {
    let normalized = normalize_folder(folder)?;
    let mut idx = load_index(workspace)?;
    if !idx.folders.contains(&normalized) {
        idx.folders.push(normalized.clone());
        idx.folders.sort();
        save_index(workspace, &idx)?;
    }
    Ok(normalized)
}

fn normalize_folder(folder: &str) -> EntResult<String> {
    let trimmed = folder.trim().trim_matches('/').replace('\\', "/");
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(EntError::Invalid(format!(
                "Ungültiger Ordnerpfad '{folder}'"
            )));
        }
    }
    Ok(trimmed)
}

/// Imports a file into the library: copies bytes into `assets/blobs/`,
/// creates a new asset entry (or appends a version to an existing one
/// when `into_existing` is given).
#[allow(clippy::too_many_arguments)]
pub fn import_asset(
    workspace: &Workspace,
    source: &std::path::Path,
    name: &str,
    folder: &str,
    tags: &[String],
    into_existing: Option<Uuid>,
    note: Option<&str>,
) -> EntResult<LibraryAsset> {
    let bytes = std::fs::read(source)?;
    if bytes.is_empty() {
        return Err(EntError::Invalid("Leere Datei".into()));
    }
    let sha = sha256_hex(&bytes);
    let blob_rel = format!("assets/blobs/{sha}.bin");
    let blob_abs = workspace.abs_path(&blob_rel)?;
    if !blob_abs.exists() {
        std::fs::create_dir_all(blob_abs.parent().unwrap())?;
        std::fs::write(&blob_abs, &bytes)?;
    }
    // Verify what we wrote (integrity on ingest).
    let written = std::fs::read(&blob_abs)?;
    if sha256_hex(&written) != sha {
        return Err(EntError::Integrity(
            "Blob-Checksum nach Schreiben falsch".into(),
        ));
    }

    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("");
    let kind = AssetKind::from_extension(ext);
    let origin = source.file_name().map(|n| n.to_string_lossy().into_owned());
    let folder = normalize_folder(folder)?;

    let mut idx = load_index(workspace)?;
    let now = now_unix();
    let version = AssetVersion {
        id: Uuid::new_v4(),
        sha256: sha.clone(),
        blob: blob_rel,
        size_bytes: bytes.len() as u64,
        created_unix: now,
        note: note.map(str::to_string),
    };

    if let Some(existing_id) = into_existing {
        let asset = idx
            .assets
            .iter_mut()
            .find(|a| a.id == existing_id)
            .ok_or_else(|| EntError::NotFound("Asset nicht gefunden".into()))?;
        // Don't duplicate identical consecutive versions.
        if asset.current_version().map(|v| &v.sha256) != Some(&sha) {
            asset.versions.push(version);
        }
        asset.updated_unix = now;
        if !tags.is_empty() {
            for t in tags {
                if !asset.tags.contains(t) {
                    asset.tags.push(t.clone());
                }
            }
            asset.tags.sort();
        }
        let result = asset.clone();
        save_index(workspace, &idx)?;
        return Ok(result);
    }

    let asset = LibraryAsset {
        name: if name.trim().is_empty() {
            origin.clone().unwrap_or_else(|| "Asset".into())
        } else {
            name.trim().to_string()
        },
        folder,
        kind,
        tags: tags.to_vec(),
        origin_name: origin,
        versions: vec![version],
        ..LibraryAsset::default()
    };
    idx.assets.push(asset.clone());
    save_index(workspace, &idx)?;
    Ok(asset)
}

/// Reads current blob bytes and verifies sha256.
pub fn asset_bytes(workspace: &Workspace, asset: &LibraryAsset) -> EntResult<Vec<u8>> {
    let Some(v) = asset.current_version() else {
        return Err(EntError::Invalid("Asset hat keine Version".into()));
    };
    let abs = workspace.abs_path(&v.blob)?;
    let bytes = std::fs::read(&abs)?;
    if sha256_hex(&bytes) != v.sha256 {
        return Err(EntError::Integrity(format!(
            "Asset '{}' Integrität verletzt (sha256-Mismatch)",
            asset.name
        )));
    }
    Ok(bytes)
}

/// Integrity check for every blob in the index.
pub fn verify_library(workspace: &Workspace) -> EntResult<Vec<String>> {
    let idx = load_index(workspace)?;
    let mut problems = Vec::new();
    for asset in &idx.assets {
        for v in &asset.versions {
            let abs = match workspace.abs_path(&v.blob) {
                Ok(p) => p,
                Err(e) => {
                    problems.push(format!("{}: {e}", asset.name));
                    continue;
                }
            };
            match std::fs::read(&abs) {
                Ok(bytes) => {
                    if sha256_hex(&bytes) != v.sha256 {
                        problems.push(format!(
                            "{}: sha256-Mismatch in Version {}",
                            asset.name, v.id
                        ));
                    }
                }
                Err(_) => problems.push(format!("{}: Blob fehlt ({})", asset.name, v.blob)),
            }
        }
    }
    Ok(problems)
}

/// Search by name/folder/tag substring (case-insensitive).
pub fn search_assets(workspace: &Workspace, query: &str) -> EntResult<Vec<LibraryAsset>> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    for asset in list_assets(workspace)? {
        if q.is_empty()
            || asset.name.to_lowercase().contains(&q)
            || asset.folder.to_lowercase().contains(&q)
            || asset.tags.iter().any(|t| t.to_lowercase().contains(&q))
        {
            out.push(asset);
        }
    }
    Ok(out)
}

/// Updates metadata (name/folder/tags/kind) without touching blobs.
pub fn update_asset(
    workspace: &Workspace,
    id: Uuid,
    edit: impl FnOnce(&mut LibraryAsset),
) -> EntResult<LibraryAsset> {
    let mut idx = load_index(workspace)?;
    let asset = idx
        .assets
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| EntError::NotFound("Asset nicht gefunden".into()))?;
    edit(asset);
    asset.updated_unix = now_unix();
    asset.validate()?;
    let result = asset.clone();
    save_index(workspace, &idx)?;
    Ok(result)
}

/// Deletes an asset entry AND its unique blobs (blobs shared with other
/// versions/assets are kept).
pub fn delete_asset(workspace: &Workspace, id: Uuid) -> EntResult<()> {
    let mut idx = load_index(workspace)?;
    let Some(pos) = idx.assets.iter().position(|a| a.id == id) else {
        return Err(EntError::NotFound("Asset nicht gefunden".into()));
    };
    let removed = idx.assets.remove(pos);
    // Collect still-referenced blob paths.
    let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();
    for a in &idx.assets {
        for v in &a.versions {
            referenced.insert(v.blob.clone());
        }
    }
    for v in &removed.versions {
        if !referenced.contains(&v.blob)
            && let Ok(abs) = safe_join(&workspace.root, &v.blob)
        {
            crate::store::remove_if_exists(&abs)?;
        }
    }
    save_index(workspace, &idx)
}

/// Exports an asset's current bytes to an arbitrary destination path.
pub fn export_asset(
    workspace: &Workspace,
    asset: &LibraryAsset,
    dest_dir: &std::path::Path,
) -> EntResult<PathBuf> {
    let bytes = asset_bytes(workspace, asset)?;
    let file_name = sanitize_filename(&asset.name);
    let ext = asset
        .origin_name
        .as_deref()
        .and_then(|n| std::path::Path::new(n).extension())
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let filename = if ext.is_empty() {
        file_name
    } else {
        format!("{file_name}.{ext}")
    };
    let dest = dest_dir.join(filename);
    std::fs::create_dir_all(dest_dir)?;
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Folder stats for admin overview.
pub fn folder_stats(workspace: &Workspace) -> EntResult<BTreeMap<String, usize>> {
    let mut map = BTreeMap::new();
    for asset in list_assets(workspace)? {
        let key = if asset.folder.is_empty() {
            "(wurzel)".to_string()
        } else {
            asset.folder.clone()
        };
        *map.entry(key).or_insert(0) += 1;
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Organization;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("nova-ent-assets-{}-{}", tag, std::process::id()));
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

    fn src_file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn import_list_search_verify() {
        let (ws, dir) = ws("lib");
        let png = src_file(&dir, "logo.png", b"\x89PNG fake");
        let svg = src_file(&dir, "icon.svg", b"<svg/>");
        create_folder(&ws, "marketing/logos").unwrap();

        let a1 = import_asset(
            &ws,
            &png,
            "Logo",
            "marketing/logos",
            &["marke".into()],
            None,
            None,
        )
        .unwrap();
        assert_eq!(a1.kind, AssetKind::Png);
        let a2 = import_asset(&ws, &svg, "Icon", "", &[], None, None).unwrap();
        assert_eq!(a2.kind, AssetKind::Svg);

        assert_eq!(list_assets(&ws).unwrap().len(), 2);
        assert_eq!(search_assets(&ws, "marke").unwrap().len(), 1);
        assert_eq!(search_assets(&ws, "logo").unwrap().len(), 1);
        assert!(verify_library(&ws).unwrap().is_empty());
        assert!(asset_bytes(&ws, &a1).unwrap().starts_with(b"\x89PNG"));

        // New version of same asset.
        let png2 = src_file(&dir, "logo2.png", b"\x89PNG fake v2");
        let updated = import_asset(&ws, &png2, "", "", &[], Some(a1.id), Some("v2")).unwrap();
        assert_eq!(updated.versions.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn integrity_detects_tampering() {
        let (ws, dir) = ws("tamper");
        let f = src_file(&dir, "a.png", b"data");
        let asset = import_asset(&ws, &f, "A", "", &[], None, None).unwrap();
        // Corrupt the blob.
        let v = asset.current_version().unwrap();
        let abs = ws.abs_path(&v.blob).unwrap();
        std::fs::write(&abs, b"evil").unwrap();
        assert!(asset_bytes(&ws, &asset).is_err());
        assert!(!verify_library(&ws).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_unreferenced_blob() {
        let (ws, dir) = ws("del");
        let f = src_file(&dir, "b.png", b"blob");
        let asset = import_asset(&ws, &f, "B", "", &[], None, None).unwrap();
        let blob = asset.current_version().unwrap().blob.clone();
        delete_asset(&ws, asset.id).unwrap();
        assert!(list_assets(&ws).unwrap().is_empty());
        assert!(!ws.abs_path(&blob).unwrap().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
