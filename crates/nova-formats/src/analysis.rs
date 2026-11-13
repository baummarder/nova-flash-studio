//! Format-aware project analysis for the CLI and future AI tooling.
//!
//! The analysis never claims that preserved ABC is source code. Every script
//! carries its [`nova_core::CodeProvenance`] so a caller can distinguish
//! original, decompiled, reconstructed, generated and compiled content.

use std::collections::BTreeMap;
use std::path::Path;

use nova_core::{CodeProvenance, Project, SourceFormat};
use serde::{Deserialize, Serialize};

use crate::capabilities::{CompatibilityReport, FormatCapabilities, FormatKind, capabilities};
use crate::conversion::load_path;
use crate::error::FormatResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptAnalysis {
    pub path: String,
    pub provenance: CodeProvenance,
    pub provenance_label: String,
    pub has_preserved_abc: bool,
    pub abc_bytes: usize,
    pub source_bytes: usize,
    pub is_original_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAnalysis {
    pub format: FormatKind,
    pub source_version: Option<String>,
    pub project_name: String,
    pub scenes: usize,
    pub layers: usize,
    pub frames: usize,
    pub objects: usize,
    pub symbols: usize,
    pub scripts: usize,
    pub assets: usize,
    pub stage_width: f32,
    pub stage_height: f32,
    pub frame_rate: f32,
    pub object_kinds: BTreeMap<String, usize>,
    pub code: Vec<ScriptAnalysis>,
    pub warnings: Vec<String>,
    pub capabilities: FormatCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub analysis: ProjectAnalysis,
    pub report: CompatibilityReport,
}

pub fn analyze_path(path: &Path) -> FormatResult<AnalysisResult> {
    let imported = load_path(path)?;
    Ok(AnalysisResult {
        analysis: analyze_project(
            &imported.project,
            imported.source_format,
            imported.source_version.clone(),
            &imported.report,
        ),
        report: imported.report,
    })
}

pub fn analyze_project(
    project: &Project,
    format: FormatKind,
    source_version: Option<String>,
    report: &CompatibilityReport,
) -> ProjectAnalysis {
    let mut layers = 0;
    let mut frames = 0;
    let mut objects = 0;
    let mut object_kinds: BTreeMap<String, usize> = BTreeMap::new();
    for scene in &project.scenes {
        layers += scene.layers.len();
        frames += scene.frame_count();
        for layer in &scene.layers {
            for frame in &layer.frames {
                for object in &frame.objects {
                    objects += 1;
                    *object_kinds
                        .entry(object.kind.type_name().to_string())
                        .or_default() += 1;
                }
            }
        }
    }
    let scene = project.active_scene();
    let code = project
        .scripts
        .iter()
        .map(|script| ScriptAnalysis {
            path: script.path.clone(),
            provenance: script.provenance,
            provenance_label: script.provenance.label().to_string(),
            has_preserved_abc: script.abc.is_some(),
            abc_bytes: script.abc.as_ref().map_or(0, Vec::len),
            source_bytes: script.source.len(),
            is_original_source: script.provenance.is_original(),
        })
        .collect();
    let warnings = report
        .issues
        .iter()
        .filter(|issue| issue.severity >= crate::capabilities::IssueSeverity::Warning)
        .map(|issue| issue.message.clone())
        .collect();
    ProjectAnalysis {
        format,
        source_version,
        project_name: project.metadata.name.clone(),
        scenes: project.scenes.len(),
        layers,
        frames,
        objects,
        symbols: project.symbols.len(),
        scripts: project.scripts.len(),
        assets: project.assets.len(),
        stage_width: scene.map_or(0.0, |scene| scene.width),
        stage_height: scene.map_or(0.0, |scene| scene.height),
        frame_rate: scene.map_or(0.0, |scene| scene.fps),
        object_kinds,
        code,
        warnings,
        capabilities: capabilities(format),
    }
}

pub fn source_format_label(format: SourceFormat) -> &'static str {
    match format {
        SourceFormat::Nova => "NØVA",
        SourceFormat::Fla => "FLA/XFL",
        SourceFormat::Swf => "SWF",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_distinguishes_preserved_abc() {
        let mut project = Project::demo();
        project.scripts[0].abc = Some(vec![1, 2, 3]);
        project.scripts[0].provenance = CodeProvenance::CompiledBytecode;
        let report = CompatibilityReport::new(FormatKind::Swf);
        let analysis = analyze_project(&project, FormatKind::Swf, None, &report);
        assert_eq!(analysis.scripts, 1);
        assert!(analysis.code[0].has_preserved_abc);
        assert!(!analysis.code[0].is_original_source);
        assert_eq!(analysis.code[0].abc_bytes, 3);
    }
}
