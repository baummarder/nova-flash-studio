//! Console dock: QuickJS tools and extensions, with honest AS3 labelling.
//!
//! The console evaluates JavaScript. ActionScript sources are never executed
//! here; they are only parsed by the AS3 front-end in code mode.

use egui::{Key, RichText, ScrollArea, TextEdit, Ui};
use nova_core::Project;
use nova_script::ScriptEngine;

use crate::state::{AppState, LogLevel};
use crate::theme;

pub fn show(
    ui: &mut Ui,
    project: &Project,
    state: &mut AppState,
    engine: &mut Option<ScriptEngine>,
) {
    let mut run = ui.input(|input| input.modifiers.command && input.key_pressed(Key::Enter));
    let mut pending: Option<String> = None;

    ui.horizontal(|ui| {
        ui.label(RichText::new("Konsole").strong());
        ui.label(
            RichText::new("JavaScript · QuickJS")
                .small()
                .color(theme::TEXT_FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Leeren").clicked() {
                state.console.clear();
            }
            if ui.button("Ausführen").on_hover_text("Strg+Enter").clicked() {
                run = true;
            }
        });
    });
    ui.label(
        RichText::new(
            "Werkzeuge und Erweiterungen laufen als JavaScript. ActionScript wird beim SWF-Export zu AVM2-ABC kompiliert, aber im Editor nicht ausgeführt.",
        )
        .small()
        .color(theme::TEXT_FAINT),
    );

    theme::hairline(ui);

    let log_height = (ui.available_height() - 104.0).max(50.0);
    ScrollArea::vertical()
        .id_salt("console_output")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .max_height(log_height)
        .show(ui, |ui| {
            if state.console.is_empty() {
                ui.label(
                    RichText::new("Noch keine Ausgabe. Beispiele unten ausprobieren.")
                        .small()
                        .italics()
                        .color(theme::TEXT_FAINT),
                );
            }
            for line in &state.console {
                let (color, prefix) = match line.level {
                    LogLevel::Echo => (theme::ACCENT, "> "),
                    LogLevel::Info => (theme::TEXT_DIM, ""),
                    LogLevel::Success => (theme::GREEN, ""),
                    LogLevel::Warn => (theme::AMBER, "Warnung: "),
                    LogLevel::Error => (theme::RED, "Fehler: "),
                };
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{prefix}{}", line.text))
                            .monospace()
                            .size(11.5)
                            .color(color),
                    )
                    .wrap(),
                );
            }
        });

    theme::hairline(ui);

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Beispiele:").small().color(theme::TEXT_FAINT));
        for example in [
            "nova.project.metadata.name",
            "nova.project.scenes.length",
            "nova.project.scenes[0].layers.length",
            "nova.version",
        ] {
            if ui.small_button(example).clicked() {
                pending = Some(example.to_string());
            }
        }
    });

    ui.horizontal(|ui| {
        ui.add(
            TextEdit::multiline(&mut state.console_input)
                .id_salt("console_input")
                .code_editor()
                .desired_rows(2)
                .desired_width(ui.available_width() - 100.0)
                .hint_text("JavaScript eingeben … (Strg+Enter ausführen)"),
        );
        if ui
            .button("Ausführen")
            .on_hover_text("Das Skript im persistenten QuickJS-Kontext ausführen")
            .clicked()
        {
            run = true;
        }
    });

    if let Some(source) = pending {
        execute(&source, project, state, engine);
    } else if run {
        let source = state.console_input.trim().to_string();
        if !source.is_empty() {
            execute(&source, project, state, engine);
            state.console_input.clear();
        }
    }
}

fn execute(
    source: &str,
    project: &Project,
    state: &mut AppState,
    engine: &mut Option<ScriptEngine>,
) {
    state.log(LogLevel::Echo, source);

    if engine.is_none() {
        match ScriptEngine::new() {
            Ok(created) => *engine = Some(created),
            Err(error) => {
                state.log(LogLevel::Error, error.to_string());
                return;
            }
        }
    }

    let project_json = match project.to_json() {
        Ok(json) => json,
        Err(error) => {
            state.log(
                LogLevel::Error,
                format!("Projektkontext konnte nicht erzeugt werden: {error}"),
            );
            return;
        }
    };

    let engine = engine.as_mut().expect("engine created above");
    if let Err(error) = engine.set_project_context(&project_json) {
        state.log(LogLevel::Error, error.to_string());
        return;
    }

    match engine.eval(source) {
        Ok(output) => {
            for line in output.logs {
                state.log(LogLevel::Info, line);
            }
            if !output.value.is_empty() && output.value != "undefined" {
                state.log(LogLevel::Success, output.value);
            }
        }
        Err(error) => {
            for line in error.logs() {
                state.log(LogLevel::Error, line);
            }
            state.log(LogLevel::Error, error.to_string());
            if error.is_timeout() {
                state.log(
                    LogLevel::Warn,
                    "Die Ausführung wurde nach dem Zeitlimit abgebrochen; der Kontext bleibt nutzbar.",
                );
            }
        }
    }
}
