//! Native `.nova` reading and writing.

use std::path::Path;

use nova_core::{CodeProvenance, Project};

use crate::capabilities::{CompatibilityReport, FormatKind, IssueCategory};
use crate::conversion::ImportedData;
use crate::error::FormatResult;

pub fn import(bytes: &[u8]) -> FormatResult<ImportedData> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        crate::error::FormatError::Nova(nova_core::ProjectError::Validation(format!(
            "Projektdatei ist kein gültiges UTF-8: {error}"
        )))
    })?;
    let project = Project::from_json(text)?;
    let mut report = CompatibilityReport::new(FormatKind::Nova);
    report.source_version = Some(format!(
        "formatVersion {}",
        nova_core::CURRENT_FORMAT_VERSION
    ));
    report.info(
        IssueCategory::Structure,
        format!(
            "{} Szene(n), {} Symbol(e), {} Skript(e), {} Asset(s).",
            project.scenes.len(),
            project.symbols.len(),
            project.scripts.len(),
            project.assets.len()
        ),
    );
    if project
        .scripts
        .iter()
        .any(|script| script.provenance != CodeProvenance::OriginalSource)
    {
        report.warning(
            IssueCategory::Code,
            "Mindestens ein Skript ist nicht als Originalquelle markiert.",
        );
    }
    if project.source.is_some() {
        report.info(
            IssueCategory::Structure,
            "Projekt enthält Herkunftsinformationen aus einem früheren Import.",
        );
    }
    Ok(ImportedData {
        project,
        report,
        source_version: Some(format!(
            "nova-flash-studio/{}",
            nova_core::CURRENT_FORMAT_VERSION
        )),
    })
}

pub fn save(project: &Project, path: &Path) -> FormatResult<()> {
    project.save(path)?;
    Ok(())
}
