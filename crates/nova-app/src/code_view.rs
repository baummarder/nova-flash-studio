//! Code mode: script tabs, highlighted ActionScript editor and diagnostics.

use egui::{Color32, RichText, ScrollArea, TextEdit, TextStyle, Ui};
use nova_as::Severity;
use nova_core::{Project, ScriptId};

use crate::highlight;
use crate::state::{AppState, BottomTab, UiAction};
use crate::theme;

const FONT_SIZE: f32 = 13.0;

pub fn show(ui: &mut Ui, project: &mut Project, state: &mut AppState) -> Vec<UiAction> {
    let mut actions = Vec::new();

    // Script tabs ---------------------------------------------------------
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ScrollArea::horizontal()
            .id_salt("script_tabs")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let scripts: Vec<(ScriptId, String)> = project
                    .scripts
                    .iter()
                    .map(|script| (script.id, script.path.clone()))
                    .collect();
                for (id, path) in scripts {
                    let selected = state.active_script == Some(id);
                    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                    let mut text = RichText::new(name).monospace().size(12.0);
                    if selected {
                        text = text.color(theme::ACCENT);
                    } else {
                        text = text.color(theme::TEXT_DIM);
                    }
                    if ui.selectable_label(selected, text).clicked() {
                        actions.push(UiAction::OpenScript(id));
                    }
                }
            });
    });

    theme::hairline(ui);

    // Action bar ----------------------------------------------------------
    ui.horizontal(|ui| {
        let has_script = state.active_script.is_some();
        if ui
            .add_enabled(has_script, egui::Button::new("Prüfen"))
            .on_hover_text("Lexer, Parser und Deklarationsprüfung ausführen")
            .clicked()
        {
            if let Some(id) = state.active_script {
                actions.push(UiAction::CheckScript(id));
            }
        }
        ui.checkbox(&mut state.auto_check, "Automatisch prüfen");

        if let Some(id) = state.diagnostics_for {
            let errors = state
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count();
            let warnings = state
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Warning)
                .count();
            let color = if errors > 0 {
                theme::RED
            } else if warnings > 0 {
                theme::AMBER
            } else {
                theme::GREEN
            };
            let text = if errors == 0 && warnings == 0 {
                "Keine Probleme".to_string()
            } else {
                format!("{errors} Fehler · {warnings} Warnungen")
            };
            if ui
                .selectable_label(false, RichText::new(text).small().color(color))
                .on_hover_text("Problemliste öffnen")
                .clicked()
            {
                state.bottom_tab = BottomTab::Problems;
            }
            let _ = id;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("SWF exportieren").clicked() {
                actions.push(UiAction::Export);
            }
            if ui
                .button("Prüfen als SWF-Build")
                .on_hover_text("Exportiert die aktive Szene und zeigt die Builder-Warnungen")
                .clicked()
            {
                actions.push(UiAction::Export);
            }
        });
    });

    theme::hairline(ui);

    // Editor --------------------------------------------------------------
    let Some(script_id) = state.active_script else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Kein Skript geöffnet. Links ein Skript auswählen oder '+' drücken.")
                    .color(theme::TEXT_DIM),
            );
        });
        return actions;
    };
    let Some(script) = project.script(script_id) else {
        ui.label(RichText::new("Skript nicht gefunden").color(theme::RED));
        return actions;
    };

    let mut source = script.source.clone();
    let editor_id = egui::Id::unique(("as3_editor", script_id));

    if let Some((line, column)) = state.pending_jump.take() {
        let offset = highlight::offset_of(&source, line, column);
        let mut editor_state =
            egui::text_edit::TextEditState::load(ui.ctx(), editor_id).unwrap_or_default();
        editor_state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(offset),
            )));
        editor_state.store(ui.ctx(), editor_id);
        ui.memory_mut(|memory| memory.request_focus(editor_id));
    }

    let lines = highlight::line_count(&source);
    let longest = source
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let desired_width = (longest as f32 * 8.1 + 120.0).max(ui.available_width() - 70.0);
    let gutter: String = (1..=lines)
        .map(|number| format!("{number:>4}"))
        .collect::<Vec<_>>()
        .join("\n");

    ScrollArea::both()
        .id_salt(("as3_scroll", script_id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(gutter)
                            .monospace()
                            .size(FONT_SIZE)
                            .color(theme::TEXT_FAINT),
                    )
                    .selectable(false),
                );
                ui.add_space(2.0);

                let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
                    let job = highlight::job(text.as_str(), FONT_SIZE);
                    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
                };

                let output = TextEdit::multiline(&mut source)
                    .id(editor_id)
                    .font(TextStyle::Monospace)
                    .desired_width(desired_width)
                    .desired_rows(lines + 2)
                    .lock_focus(true)
                    .frame(egui::Frame::NONE)
                    .layouter(&mut layouter)
                    .show(ui);

                if output.response.changed() {
                    if let Some(script) = project.script_mut(script_id) {
                        script.source = source.clone();
                    }
                    state.dirty = true;
                    project.touch();
                    if state.auto_check {
                        let (_, diagnostics) = nova_as::analyze(&source);
                        report_diagnostics(state, script_id, diagnostics, false);
                    }
                }

                if output.response.has_focus() {
                    if let Some(cursor) = output.cursor_range {
                        let char_index = cursor.primary.index.0;
                        let (line, column) = line_column(&source, char_index);
                        state.set_status(format!(
                            "{}:{}:{}",
                            project.script_path(script_id).unwrap_or("Skript"),
                            line,
                            column
                        ));
                    }
                }
            });
        });

    actions
}

pub(crate) fn report_diagnostics(
    state: &mut AppState,
    script_id: ScriptId,
    diagnostics: Vec<nova_as::Diagnostic>,
    verbose: bool,
) {
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .count();

    if errors == 0 {
        state.log(
            crate::state::LogLevel::Success,
            format!("AS3-Syntaxprüfung: 0 Fehler, {warnings} Warnung(en)."),
        );
    } else {
        state.log(
            crate::state::LogLevel::Error,
            format!("AS3-Syntaxprüfung: {errors} Fehler, {warnings} Warnung(en)."),
        );
    }
    if verbose && !diagnostics.is_empty() {
        let source = String::new();
        for diagnostic in diagnostics.iter().take(20) {
            let label = match diagnostic.severity {
                Severity::Error => "Fehler",
                Severity::Warning => "Warnung",
            };
            state.log(
                match diagnostic.severity {
                    Severity::Error => crate::state::LogLevel::Error,
                    Severity::Warning => crate::state::LogLevel::Warn,
                },
                format!(
                    "{label} in Zeile {}, Spalte {}: {}",
                    diagnostic.span.line, diagnostic.span.column, diagnostic.message
                ),
            );
        }
        let _ = source;
    }

    state.diagnostics = diagnostics;
    state.diagnostics_for = Some(script_id);
}

fn line_column(source: &str, char_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in source.chars().enumerate() {
        if index >= char_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

/// Renders the "Probleme" tab of the bottom dock.
pub fn problems(ui: &mut Ui, state: &mut AppState, project: &Project) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        ui.label(RichText::new("AS3-Probleme").strong().color(theme::TEXT));
        ui.label(
            RichText::new(format!(
                "{} Fehler · {} Warnungen",
                state.error_count(),
                state.warning_count()
            ))
            .small()
            .color(theme::TEXT_DIM),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Alles kopieren").clicked() {
                let name = state
                    .diagnostics_for
                    .and_then(|id| project.script_path(id))
                    .unwrap_or("Skript");
                let text = state
                    .diagnostics
                    .iter()
                    .map(|diagnostic| {
                        format!(
                            "{name}:{}:{}: {}: {}",
                            diagnostic.span.line,
                            diagnostic.span.column,
                            match diagnostic.severity {
                                Severity::Error => "Fehler",
                                Severity::Warning => "Warnung",
                            },
                            diagnostic.message
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                ui.ctx().copy_text(text);
            }
        });
    });
    theme::hairline(ui);

    if state.diagnostics.is_empty() {
        ui.add_space(8.0);
        ui.label(
            RichText::new("Keine Probleme. Die Prüfung umfasst Lexer, Parser und Deklarationen – keine Typprüfung.")
                .small()
                .color(theme::TEXT_DIM),
        );
        return actions;
    }

    ScrollArea::vertical()
        .id_salt("problem_list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for diagnostic in &state.diagnostics {
                let (color, label) = match diagnostic.severity {
                    Severity::Error => (theme::RED, "Fehler"),
                    Severity::Warning => (theme::AMBER, "Warnung"),
                };
                let text = format!(
                    "{}:{}  {}: {}",
                    diagnostic.span.line, diagnostic.span.column, label, diagnostic.message
                );
                let response = ui.add(
                    egui::Label::new(RichText::new(text).small().color(color))
                        .sense(egui::Sense::click()),
                );
                if response.clicked() {
                    state.pending_jump = Some((diagnostic.span.line, diagnostic.span.column));
                    actions.push(UiAction::SetMode(crate::state::Mode::Code));
                    state.bottom_tab = BottomTab::Problems;
                }
            }
        });

    actions
}

/// A tiny status strip shown below the editor inside Code mode.
#[allow(dead_code)]
pub fn status_line(ui: &mut Ui, project: &Project, state: &AppState) {
    ui.horizontal(|ui| {
        if let Some(id) = state.active_script {
            if let Some(script) = project.script(id) {
                ui.label(
                    RichText::new(format!("{} · {}", script.path, script.source.len()))
                        .small()
                        .color(theme::TEXT_FAINT),
                );
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("AS3-Frontend: Syntax & Deklarationen · kein ABC-Export")
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        });
    });
}

/// Utility used by the app keyboard handler to focus a source position.
#[allow(dead_code)]
pub fn jump_target(line: usize, column: usize) -> (usize, usize) {
    (line, column)
}

#[allow(dead_code)]
fn dim(color: Color32) -> Color32 {
    color.gamma_multiply(0.7)
}
