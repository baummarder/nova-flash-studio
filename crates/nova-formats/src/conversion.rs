//! Format dispatch and conversion results.
//!
//! Every import keeps the original bytes alongside the editable project. The
//! editor can therefore save a `.nova` file without losing unsupported source
//! data, and the compatibility report explains what was reconstructed.

use std::path::{Path, PathBuf};

use nova_core::{Project, SourceFormat};

use crate::capabilities::{CompatibilityReport, FormatKind, IssueCategory, IssueSeverity};
use crate::error::{FormatError, FormatResult};
use crate::formats::{fla, nova, swf};

#[derive(Debug)]
pub struct ImportedData {
    pub project: Project,
    pub report: CompatibilityReport,
    pub source_version: Option<String>,
}

#[derive(Debug)]
pub struct ImportedProject {
    pub project: Project,
    pub report: CompatibilityReport,
    pub source_format: FormatKind,
    pub source_version: Option<String>,
    pub source_path: Option<PathBuf>,
    /// Original bytes are kept in memory so a save can preserve unsupported
    /// data without embedding it in the `.nova` JSON.
    pub source_bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct ExportData {
    pub bytes: Vec<u8>,
    pub report: CompatibilityReport,
}

#[derive(Debug)]
pub struct ExportResult {
    pub bytes: Vec<u8>,
    pub report: CompatibilityReport,
    pub warnings: Vec<String>,
}

pub fn load_path(path: &Path) -> FormatResult<ImportedProject> {
    if path.is_dir() {
        // Uncompressed XFL folders contain DOMDocument.xml (or a .xfl
        // marker file). Anything else is rejected explicitly.
        let has_document =
            path.join("DOMDocument.xml").is_file() || path.join("domdocument.xml").is_file();
        let has_marker = std::fs::read_dir(path)
            .map(|mut entries| {
                entries.any(|entry| {
                    entry
                        .map(|e| {
                            e.path()
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("xfl"))
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !(has_document || has_marker) {
            return Err(FormatError::Unsupported(format!(
                "Verzeichnis ist kein XFL-Projekt (DOMDocument.xml fehlt): {}",
                path.display()
            )));
        }
        let imported = fla::import_dir(path)?;
        return Ok(ImportedProject {
            project: imported.project,
            report: imported.report,
            source_format: FormatKind::XflPackage,
            source_version: imported.source_version,
            source_path: Some(path.to_path_buf()),
            source_bytes: Vec::new(),
        });
    }
    let bytes = std::fs::read(path)?;
    let format = FormatKind::from_path(path).unwrap_or_else(|| detect_format(&bytes));
    let mut imported = import_bytes(format, Some(path), bytes)?;
    if matches!(format, FormatKind::Fla | FormatKind::Swf) {
        if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            if imported.project.metadata.name == "FLA-Import"
                || imported.project.metadata.name == "SWF-Import"
            {
                imported.project.metadata.name = stem.to_string();
            }
        }
        if let Some(source) = &mut imported.project.source {
            source.original_path = Some(path.to_string_lossy().into_owned());
        }
    }
    imported.source_path = Some(path.to_path_buf());
    Ok(imported)
}

pub fn import_bytes(
    format: FormatKind,
    path: Option<&Path>,
    bytes: Vec<u8>,
) -> FormatResult<ImportedProject> {
    let imported = match format {
        FormatKind::Nova => nova::import(&bytes)?,
        FormatKind::Fla | FormatKind::XflPackage => fla::import(&bytes)?,
        FormatKind::Swf => swf::import(&bytes)?,
    };
    Ok(ImportedProject {
        project: imported.project,
        report: imported.report,
        source_format: format,
        source_version: imported.source_version,
        source_path: path.map(Path::to_path_buf),
        source_bytes: bytes,
    })
}

pub fn detect_format(bytes: &[u8]) -> FormatKind {
    if bytes.starts_with(b"FWS") || bytes.starts_with(b"CWS") || bytes.starts_with(b"ZWS") {
        return FormatKind::Swf;
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return FormatKind::Fla;
    }
    // OLE2 compound document = native binary .fla (pre-CS5). Routed to the
    // FLA importer which reports it as unsupported with guidance.
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return FormatKind::Fla;
    }
    let trimmed = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .and_then(|start| bytes.get(start))
        .copied();
    if trimmed == Some(b'{') {
        return FormatKind::Nova;
    }
    FormatKind::Nova
}

pub fn save_nova(project: &Project, path: &Path) -> FormatResult<()> {
    nova::save(project, path)
}

pub fn export_swf(project: &Project) -> FormatResult<ExportResult> {
    let output = nova_build::build_swf(project)?;
    let source = project
        .source
        .as_ref()
        .map(|source| match source.format {
            SourceFormat::Nova => FormatKind::Nova,
            SourceFormat::Fla => FormatKind::Fla,
            SourceFormat::Swf => FormatKind::Swf,
        })
        .unwrap_or(FormatKind::Nova);
    let mut report = CompatibilityReport::new(source);
    report.set_stat("Ziel", "SWF");
    report.set_stat("Bytes", output.bytes.len().to_string());
    report.set_stat("Frames", output.stats.frames.to_string());
    report.set_stat(
        "Platzierte Objekte",
        output.stats.placed_objects.to_string(),
    );
    report.set_stat("Formen", output.stats.defined_shapes.to_string());
    report.set_stat("Sprites", output.stats.defined_sprites.to_string());
    report.set_stat("Skripte", output.stats.embedded_scripts.to_string());
    for warning in &output.warnings {
        report.push(crate::capabilities::CompatibilityIssue::new(
            IssueSeverity::Warning,
            IssueCategory::Export,
            warning.clone(),
        ));
    }
    if source != FormatKind::Nova {
        report.warning(
            IssueCategory::Export,
            format!(
                "Das Projekt wurde aus {} importiert. Der SWF-Export ist eine Rekonstruktion, keine bytegenaue Kopie.",
                source.label()
            ),
        );
    }
    Ok(ExportResult {
        bytes: output.bytes,
        report,
        warnings: output.warnings,
    })
}

pub fn export_xfl(project: &Project) -> FormatResult<ExportResult> {
    let export = fla::export_xfl(project)?;
    let warnings = export
        .report
        .issues
        .iter()
        .filter(|issue| issue.severity >= IssueSeverity::Warning)
        .map(|issue| issue.message.clone())
        .collect();
    Ok(ExportResult {
        bytes: export.bytes,
        report: export.report,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats_by_magic() {
        assert_eq!(detect_format(b"FWS\x0a"), FormatKind::Swf);
        assert_eq!(detect_format(b"PK\x03\x04rest"), FormatKind::Fla);
        assert_eq!(detect_format(b"  {\"format\":\"x\"}"), FormatKind::Nova);
    }

    #[test]
    fn nova_roundtrip_preserves_source() {
        let project = Project::demo();
        let json = project.to_json().unwrap();
        let imported = import_bytes(FormatKind::Nova, None, json.into_bytes()).unwrap();
        assert_eq!(imported.project.metadata.name, project.metadata.name);
        assert_eq!(imported.source_format, FormatKind::Nova);
    }

    #[test]
    fn nova_swf_import_export_import_roundtrip_preserves_structure_and_abc() {
        let project = Project::demo();
        let first = export_swf(&project).unwrap();
        let imported = import_bytes(FormatKind::Swf, None, first.bytes).unwrap();
        assert_eq!(imported.project.scenes[0].width, 550.0);
        assert!(imported.project.scenes[0].frame_count() >= 1);
        assert!(!imported.project.scripts.is_empty());
        let abc = imported.project.scripts[0].abc.clone().unwrap();

        let second = export_swf(&imported.project).unwrap();
        let reimported = import_bytes(FormatKind::Swf, None, second.bytes).unwrap();
        assert_eq!(reimported.project.scenes[0].width, 550.0);
        assert_eq!(
            reimported.project.scripts[0].abc.as_deref(),
            Some(abc.as_slice())
        );
        assert!(
            second
                .report
                .issues
                .iter()
                .any(|issue| issue.message.contains("Rekonstruktion"))
        );
    }

    #[test]
    fn nova_xfl_import_roundtrip_preserves_document_and_scripts() {
        let project = Project::demo();
        let export = export_xfl(&project).unwrap();
        let imported = import_bytes(FormatKind::XflPackage, None, export.bytes).unwrap();
        assert_eq!(imported.project.scenes[0].width, 550.0);
        assert_eq!(imported.project.scenes[0].height, 400.0);
        assert!(!imported.project.scenes[0].layers.is_empty());
        assert!(!imported.project.scripts.is_empty());
        assert!(
            export
                .report
                .issues
                .iter()
                .any(|issue| issue.message.contains("kein nativer .fla-Export"))
        );
    }

    #[test]
    fn fla_to_nova_to_swf_conversion_keeps_document_and_timeline() {
        let project = Project::demo();
        let xfl = export_xfl(&project).unwrap();
        let imported = import_bytes(FormatKind::XflPackage, None, xfl.bytes).unwrap();
        assert_eq!(imported.project.scenes[0].width, 550.0);
        assert!(imported.project.scenes[0].frame_count() >= 1);

        let swf = export_swf(&imported.project).unwrap();
        let reimported = import_bytes(FormatKind::Swf, None, swf.bytes).unwrap();
        assert_eq!(reimported.project.scenes[0].width, 550.0);
        assert_eq!(reimported.project.scenes[0].height, 400.0);
        assert!(reimported.project.scenes[0].frame_count() >= 1);
        assert!(
            swf.report
                .issues
                .iter()
                .any(|issue| issue.message.contains("Rekonstruktion"))
        );
    }
}
