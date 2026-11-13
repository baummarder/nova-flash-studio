//! Build and diagnostic integration for the agent.
//!
//! This module calls the existing compiler and SWF exporter; it does not
//! implement a second build pipeline.

use nova_as::{Diagnostic, Severity, analyze};
use nova_build::{BuildOutput, build_swf};
use nova_core::Project;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDiagnostic {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub message: String,
}

impl BuildDiagnostic {
    pub fn render(&self) -> String {
        format!(
            "{}:{}:{}: {}: {}",
            self.path,
            self.line,
            self.column,
            match self.severity {
                Severity::Error => "Fehler",
                Severity::Warning => "Warnung",
            },
            self.message
        )
    }
}

#[derive(Debug, Clone)]
pub struct BuildReport {
    pub ok: bool,
    pub summary: String,
    pub warnings: Vec<String>,
    pub diagnostics: Vec<BuildDiagnostic>,
    pub output: Option<BuildOutput>,
}

impl BuildReport {
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Warning)
            .count()
    }

    pub fn text(&self) -> String {
        let mut lines = vec![self.summary.clone()];
        for diagnostic in &self.diagnostics {
            lines.push(diagnostic.render());
        }
        for warning in &self.warnings {
            lines.push(format!("Hinweis: {warning}"));
        }
        lines.join("\n")
    }
}

pub fn analyze_project_scripts(project: &Project) -> Vec<BuildDiagnostic> {
    let mut diagnostics = Vec::new();
    for script in &project.scripts {
        // Preserved ABC is opaque. Its source text is not original source and
        // must not be compiled or presented as a compiler result.
        if script.abc.is_some() {
            continue;
        }
        let (_, script_diagnostics) = analyze(&script.source);
        diagnostics.extend(
            script_diagnostics
                .into_iter()
                .map(|diagnostic| from_diagnostic(&script.path, diagnostic)),
        );
    }
    diagnostics
}

pub fn build_project(project: &Project) -> BuildReport {
    let mut diagnostics = analyze_project_scripts(project);
    match build_swf(project) {
        Ok(output) => {
            let summary = format!(
                "Build OK: {} Bytes, {} Frames, {} Objekte, {} Shapes, {} Sprites, {} Skript(e)",
                output.bytes.len(),
                output.stats.frames,
                output.stats.placed_objects,
                output.stats.defined_shapes,
                output.stats.defined_sprites,
                output.stats.embedded_scripts
            );
            BuildReport {
                ok: true,
                summary,
                warnings: output.warnings.clone(),
                diagnostics,
                output: Some(output),
            }
        }
        Err(error) => {
            let message = error.to_string();
            if diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != Severity::Error)
            {
                diagnostics.push(BuildDiagnostic {
                    path: "<build>".into(),
                    line: 1,
                    column: 1,
                    severity: Severity::Error,
                    message: message.clone(),
                });
            }
            BuildReport {
                ok: false,
                summary: format!("Build fehlgeschlagen: {message}"),
                warnings: Vec::new(),
                diagnostics,
                output: None,
            }
        }
    }
}

fn from_diagnostic(path: &str, diagnostic: Diagnostic) -> BuildDiagnostic {
    BuildDiagnostic {
        path: path.to_string(),
        line: diagnostic.span.line,
        column: diagnostic.span.column,
        severity: diagnostic.severity,
        message: diagnostic.message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_project_builds() {
        let report = build_project(&Project::demo());
        assert!(report.ok, "{}", report.text());
        assert!(report.output.is_some());
    }

    #[test]
    fn compiler_errors_are_reported_with_path_and_line() {
        let mut project = Project::demo();
        project.scripts[0].source =
            "package { public class Main { public function Main( { } }".into();
        let report = build_project(&project);
        assert!(!report.ok);
        assert!(report.error_count() > 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path == "scripts/Main.as")
        );
    }

    #[test]
    fn preserved_abc_is_not_treated_as_source() {
        let mut project = Project::demo();
        project.scripts[0].abc = Some(vec![1, 2, 3]);
        project.scripts[0].provenance = nova_core::CodeProvenance::CompiledBytecode;
        project.scripts[0].source = "this is not ActionScript".into();
        let diagnostics = analyze_project_scripts(&project);
        assert!(diagnostics.is_empty());
    }
}
