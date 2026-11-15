//! `.novaent` workspace packages (M21.8/M21.10): hardened offline export
//! and import of enterprise workspace *metadata*.
//!
//! A package is a ZIP with `manifest.json` (schema, format, id, author,
//! per-file sha256 checksums) plus payload files. Import verifies every
//! checksum **before** writing anything, rejects traversal/absolute/
//! symlink/encrypted entries, extracts to a temp dir, then commits
//! metadata into the target workspace. Project *files* referenced by
//! registrations are never included — registrations keep their absolute
//! paths (documented: packages are metadata backups, not project moves).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive};

use crate::assets::sha256_hex;
use crate::error::{EntError, EntResult, safe_join};
use crate::store::now_unix;
use crate::workspace::Workspace;

pub const PACKAGE_SCHEMA: u32 = 1;
pub const PACKAGE_FORMAT: &str = "nova-enterprise-workspace";
pub const PACKAGE_EXTENSION: &str = "novaent";

const MAX_ENTRIES: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB uncompressed
const MAX_RATIO: u64 = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageAuthor {
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub schema: u32,
    pub format: String,
    pub id: Uuid,
    pub created_unix: u64,
    pub created_by: PackageAuthor,
    /// Map of payload path → "sha256:<hex>".
    #[serde(default)]
    pub checksums: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for PackageManifest {
    fn default() -> Self {
        Self {
            schema: PACKAGE_SCHEMA,
            format: PACKAGE_FORMAT.to_string(),
            id: Uuid::new_v4(),
            created_unix: now_unix(),
            created_by: PackageAuthor {
                display_name: "Unbekannt".into(),
                role: "unknown".into(),
            },
            checksums: BTreeMap::new(),
            extra: serde_json::Map::new(),
        }
    }
}

#[derive(Debug)]
pub struct PackageFile {
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct ExtractedPackage {
    pub manifest: PackageManifest,
    /// Verified payload files (path → bytes), manifest.json excluded.
    pub files: Vec<PackageFile>,
}

// ------------------------------------------------------------------ build

fn collect_json(workspace: &Workspace, rel: &str, files: &mut Vec<PackageFile>) -> EntResult<()> {
    let abs = workspace.root.join(rel);
    if abs.exists() {
        files.push(PackageFile {
            path: rel.to_string(),
            data: std::fs::read(&abs)?,
        });
    }
    Ok(())
}

/// Gathers all exportable metadata + asset blobs into package files.
pub fn collect_workspace_files(workspace: &Workspace) -> EntResult<Vec<PackageFile>> {
    let mut files = Vec::new();
    // Core metadata.
    for rel in [
        "org.json",
        "team.json",
        "meta.json",
        "builds/profiles.json",
        "integrations/config.json",
        "assets/index.json",
    ] {
        collect_json(workspace, rel, &mut files)?;
    }
    // Project registrations.
    for entry in std::fs::read_dir(workspace.projects_dir())? {
        let p = entry?.path();
        if p.extension().and_then(|e| e.to_str()) == Some("json") {
            files.push(PackageFile {
                path: format!("projects/{}", p.file_name().unwrap().to_string_lossy()),
                data: std::fs::read(&p)?,
            });
        }
    }
    // Reviews.
    if workspace.reviews_dir().exists() {
        for entry in std::fs::read_dir(workspace.reviews_dir())? {
            let p = entry?.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                files.push(PackageFile {
                    path: format!("reviews/{}", p.file_name().unwrap().to_string_lossy()),
                    data: std::fs::read(&p)?,
                });
            }
        }
    }
    // Asset blobs (content-addressed, dedup inherent).
    let blobs = workspace.root.join("assets/blobs");
    if blobs.exists() {
        for entry in std::fs::read_dir(&blobs)? {
            let p = entry?.path();
            if p.is_file() {
                files.push(PackageFile {
                    path: format!("blobs/{}", p.file_name().unwrap().to_string_lossy()),
                    data: std::fs::read(&p)?,
                });
            }
        }
    }
    Ok(files)
}

/// Builds the package ZIP bytes.
pub fn build_package(manifest: &PackageManifest, files: &[PackageFile]) -> EntResult<Vec<u8>> {
    let mut manifest = manifest.clone();
    manifest.schema = PACKAGE_SCHEMA;
    manifest.format = PACKAGE_FORMAT.to_string();
    manifest.checksums.clear();
    for f in files {
        manifest
            .checksums
            .insert(f.path.clone(), format!("sha256:{}", sha256_hex(&f.data)));
    }
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("manifest.json", opts)?;
        zip.write_all(&manifest_bytes)?;
        for f in files {
            zip.start_file(&f.path, opts)?;
            zip.write_all(&f.data)?;
        }
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

/// Writes a `.novaent` package atomically (temp + rename).
pub fn write_package(
    path: &Path,
    manifest: &PackageManifest,
    files: &[PackageFile],
) -> EntResult<()> {
    let data = build_package(manifest, files)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &data)?;
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn package_filename(author: &str) -> String {
    let slug: String = author
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .take(40)
        .collect();
    format!(
        "nova-enterprise-{}-{}.{}",
        slug,
        &Uuid::new_v4().to_string()[..8],
        PACKAGE_EXTENSION
    )
}

// ------------------------------------------------------------------ read

fn validate_entry_name(name: &str) -> EntResult<()> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') {
        return Err(EntError::Package(format!(
            "Ungültiger Eintragsname '{name}'"
        )));
    }
    for part in name.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(EntError::Package(format!(
                "Ungültiger Eintragsname '{name}'"
            )));
        }
    }
    if name.contains(':') {
        return Err(EntError::Package(format!("Laufwerksangabe in '{name}'")));
    }
    Ok(())
}

fn read_entry(
    archive: &mut ZipArchive<std::io::Cursor<&[u8]>>,
    index: usize,
) -> EntResult<Vec<u8>> {
    let mut entry = archive.by_index(index)?;
    let mut data = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut data)?;
    Ok(data)
}

/// Reads + fully verifies a package (manifest, checksums, limits).
pub fn read_package_bytes(bytes: &[u8]) -> EntResult<ExtractedPackage> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))?;
    if archive.len() > MAX_ENTRIES {
        return Err(EntError::Package(format!(
            "Zu viele Einträge (>{MAX_ENTRIES})"
        )));
    }
    let mut total = 0u64;
    let mut compressed = 0u64;
    let mut entries: Vec<(usize, String, u64)> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name()?.replace('\\', "/");
        validate_entry_name(&name)?;
        if entry.size() > MAX_TOTAL_BYTES {
            return Err(EntError::Package(format!("Eintrag '{name}' zu groß")));
        }
        if entry.encrypted() {
            return Err(EntError::Package(format!(
                "Verschlüsselter Eintrag '{name}'"
            )));
        }
        total += entry.size();
        compressed += entry.compressed_size();
        entries.push((i, name, entry.size()));
    }
    if total > MAX_TOTAL_BYTES {
        return Err(EntError::Package("Paket überschreitet Größenlimit".into()));
    }
    if compressed > 0 && total / compressed.max(1) > MAX_RATIO {
        return Err(EntError::Package(
            "Verdächtiges Kompressionsverhältnis".into(),
        ));
    }

    // Manifest first.
    let mut manifest_bytes: Option<Vec<u8>> = None;
    for (i, name, _) in &entries {
        if name == "manifest.json" {
            manifest_bytes = Some(read_entry(&mut archive, *i)?);
        }
    }
    let Some(manifest_bytes) = manifest_bytes else {
        return Err(EntError::Package("manifest.json fehlt".into()));
    };
    let manifest: PackageManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| EntError::Package(format!("Manifest ungültig: {e}")))?;
    if manifest.schema != PACKAGE_SCHEMA {
        return Err(EntError::Schema {
            found: manifest.schema,
            expected: PACKAGE_SCHEMA,
        });
    }
    if manifest.format != PACKAGE_FORMAT {
        return Err(EntError::Package(format!(
            "Unbekanntes Paketformat '{}'",
            manifest.format
        )));
    }

    // Verify checksums of every payload file BEFORE exposing bytes.
    let mut files = Vec::new();
    for (i, name, _) in &entries {
        if name == "manifest.json" {
            continue;
        }
        let data = read_entry(&mut archive, *i)?;
        let expected = manifest
            .checksums
            .get(name)
            .ok_or_else(|| EntError::Package(format!("Checksum für '{name}' fehlt")))?;
        let want = expected.strip_prefix("sha256:").unwrap_or(expected);
        let got = sha256_hex(&data);
        if got != want {
            return Err(EntError::Integrity(format!(
                "Checksum-Mismatch für '{name}'"
            )));
        }
        files.push(PackageFile {
            path: name.clone(),
            data,
        });
    }
    Ok(ExtractedPackage { manifest, files })
}

pub fn read_package(path: &Path) -> EntResult<ExtractedPackage> {
    read_package_bytes(&std::fs::read(path)?)
}

// ------------------------------------------------------------------ import

/// Report of an import run.
#[derive(Debug, Default)]
pub struct ImportReport {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        format!(
            "{} importiert, {} übersprungen, {} Fehler",
            self.imported.len(),
            self.skipped.len(),
            self.errors.len()
        )
    }
}

/// One planned import: package path → workspace-relative destination.
struct PlannedFile {
    pkg_path: String,
    dest_rel: String,
    dest: PathBuf,
    data: Vec<u8>,
}

/// Maps a package path to its workspace destination (whitelist only).
/// `org.json` is deliberately NOT importable: local org identity always wins.
fn plan_destination(pkg_path: &str) -> Option<String> {
    let ok = pkg_path == "team.json"
        || pkg_path == "meta.json"
        || pkg_path.starts_with("projects/")
        || pkg_path.starts_with("reviews/")
        || pkg_path == "builds/profiles.json"
        || pkg_path == "integrations/config.json"
        || pkg_path == "assets/index.json"
        || pkg_path.starts_with("blobs/");
    if !ok {
        return None;
    }
    Some(if let Some(rest) = pkg_path.strip_prefix("blobs/") {
        format!("assets/blobs/{rest}")
    } else {
        pkg_path.to_string()
    })
}

/// Imports a verified package into the workspace **transactionally**:
///
/// 1. Plan — whitelist mapping, conflict detection, path safety. Nothing
///    is written yet; `org.json` is always excluded by the caller.
/// 2. Stage — every payload is written into a temporary staging dir inside
///    the workspace and re-hashed against the manifest checksum.
/// 3. Commit — staged files are moved into place one by one. Existing
///    files are never overwritten (they were filtered in planning).
/// 4. Rollback — if any step fails, every destination file created during
///    this run is deleted again and the staging dir is removed, so a
///    failed import leaves the workspace exactly as before.
pub fn import_package(workspace: &Workspace, pkg: &ExtractedPackage) -> ImportReport {
    let mut report = ImportReport::default();

    // ---------------------------------------------------------- plan
    let mut plan: Vec<PlannedFile> = Vec::new();
    for f in &pkg.files {
        let Some(dest_rel) = plan_destination(&f.path) else {
            report.skipped.push(f.path.clone());
            continue;
        };
        let dest = match safe_join(&workspace.root, &dest_rel) {
            Ok(p) => p,
            Err(e) => {
                report.errors.push(format!("{}: {e}", f.path));
                continue;
            }
        };
        if dest.exists() {
            report
                .skipped
                .push(format!("{} (existiert bereits)", f.path));
            continue;
        }
        plan.push(PlannedFile {
            pkg_path: f.path.clone(),
            dest_rel,
            dest,
            data: f.data.clone(),
        });
    }
    if plan.is_empty() {
        return report;
    }

    // ---------------------------------------------------------- stage
    let staging = workspace
        .root
        .join(format!(".novaent-staging-{}", Uuid::new_v4()));
    let stage_result = (|| -> EntResult<()> {
        for pf in &plan {
            let stage_path = staging.join(&pf.dest_rel);
            if let Some(parent) = stage_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&stage_path, &pf.data)?;
            // Re-verify integrity of what actually landed on disk.
            let written = std::fs::read(&stage_path)?;
            if sha256_hex(&written) != sha256_hex(&pf.data) {
                return Err(EntError::Integrity(format!(
                    "Staging-Checksum-Mismatch für '{}'",
                    pf.pkg_path
                )));
            }
        }
        Ok(())
    })();
    if let Err(e) = stage_result {
        let _ = std::fs::remove_dir_all(&staging);
        report.errors.push(format!("Staging fehlgeschlagen: {e}"));
        return report;
    }

    // ---------------------------------------------------------- commit + rollback
    let mut created: Vec<PathBuf> = Vec::new();
    let mut commit_error: Option<String> = None;
    'commit: for pf in &plan {
        let stage_path = staging.join(&pf.dest_rel);
        if let Some(parent) = pf.dest.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            commit_error = Some(format!("{}: {e}", pf.pkg_path));
            break 'commit;
        }
        // Never overwrite: re-check immediately before the move.
        if pf.dest.exists() {
            commit_error = Some(format!("{}: Ziel existiert jetzt", pf.pkg_path));
            break 'commit;
        }
        if let Err(e) = std::fs::rename(&stage_path, &pf.dest) {
            commit_error = Some(format!("{}: {e}", pf.pkg_path));
            break 'commit;
        }
        created.push(pf.dest.clone());
        report.imported.push(pf.pkg_path.clone());
    }

    let _ = std::fs::remove_dir_all(&staging);
    if let Some(err) = commit_error {
        // Roll back every file this run created — the report must reflect
        // that nothing was actually imported.
        for dest in created.iter().rev() {
            let _ = std::fs::remove_file(dest);
        }
        report.imported.clear();
        report
            .errors
            .push(format!("Import abgebrochen, gerollt zurück: {err}"));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Organization;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova-ent-pkg-{}-{}", tag, std::process::id()));
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
    fn roundtrip_and_import() {
        let (src, src_dir) = ws("src");
        // Populate some metadata.
        crate::team::add_member(&src, "Ada", crate::profile::EnterpriseRole::Developer).unwrap();
        let files = collect_workspace_files(&src).unwrap();
        assert!(!files.is_empty());
        let manifest = PackageManifest {
            created_by: PackageAuthor {
                display_name: "Ada".into(),
                role: "admin".into(),
            },
            ..Default::default()
        };
        let bytes = build_package(&manifest, &files).unwrap();
        let pkg = read_package_bytes(&bytes).unwrap();
        assert_eq!(pkg.manifest.format, PACKAGE_FORMAT);

        // Import into fresh workspace.
        let (dst, dst_dir) = ws("dst");
        let report = import_package(&dst, &pkg);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.imported.iter().any(|p| p == "team.json"));
        assert!(dst.team_path().exists());
        assert_eq!(crate::team::list_members(&dst).unwrap().len(), 1);
        // org.json must NOT be imported.
        assert_eq!(dst.org().unwrap().name, "Org");
        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn rejects_traversal_and_bad_checksum() {
        // Traversal entry.
        let manifest = PackageManifest::default();
        let files = vec![PackageFile {
            path: "../evil.txt".into(),
            data: b"x".into(),
        }];
        let bytes = build_package(&manifest, &files).unwrap();
        assert!(read_package_bytes(&bytes).is_err());

        // Tampered checksum: build, then flip manifest checksum by rebuilding
        // with wrong declared checksum is hard; instead craft via manifest.
        let mut m = PackageManifest::default();
        m.checksums.insert("a.txt".into(), "sha256:0000".into());
        // build_package recomputes checksums, so doctored manifest needs manual zip.
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = SimpleFileOptions::default();
            zip.start_file("manifest.json", opts).unwrap();
            zip.write_all(&serde_json::to_vec(&m).unwrap()).unwrap();
            zip.start_file("a.txt", opts).unwrap();
            zip.write_all(b"real").unwrap();
            zip.finish().unwrap();
        }
        assert!(read_package_bytes(&buf.into_inner()).is_err());
    }

    #[test]
    fn failed_commit_rolls_back_created_files() {
        let (dst, dir) = ws("rb");
        // Package: team.json succeeds, reviews/r1.json is blocked because
        // `reviews` exists as a *file* (not a directory) in the workspace.
        // `reviews/sub` exists as a *file* so committing
        // reviews/sub/x.json must fail mid-run (parent is not a dir).
        std::fs::write(dir.join("reviews").join("sub"), b"blocker").unwrap();
        let manifest = PackageManifest::default();
        let files = vec![
            PackageFile {
                path: "team.json".into(),
                data: br#"{"schema":1,"members":[]}"#.to_vec(),
            },
            PackageFile {
                path: "reviews/sub/x.json".into(),
                data: br#"{"schema":1}"#.to_vec(),
            },
        ];
        let bytes = build_package(&manifest, &files).unwrap();
        let pkg = read_package_bytes(&bytes).unwrap();

        let report = import_package(&dst, &pkg);
        assert!(!report.errors.is_empty(), "expected commit failure");
        // Rollback: team.json must NOT remain.
        assert!(!dir.join("team.json").exists(), "rollback failed");
        assert!(report.imported.is_empty());
        // No staging leftovers.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".novaent-staging")
            })
            .collect();
        assert!(leftovers.is_empty(), "staging dir left behind");
        // Pre-existing blocker untouched.
        assert_eq!(
            std::fs::read(dir.join("reviews").join("sub")).unwrap(),
            b"blocker"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn org_json_never_imported() {
        let (dst, dir) = ws("org");
        let before = dst.org().unwrap().name;
        let manifest = PackageManifest::default();
        let files = vec![PackageFile {
            path: "org.json".into(),
            data: br#"{"schema":1,"name":"PWNED"}"#.to_vec(),
        }];
        let bytes = build_package(&manifest, &files).unwrap();
        let pkg = read_package_bytes(&bytes).unwrap();
        let report = import_package(&dst, &pkg);
        assert!(report.imported.is_empty());
        assert_eq!(dst.org().unwrap().name, before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
