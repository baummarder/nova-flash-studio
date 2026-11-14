//! NØVA Flash Studio application shell: panels, menus, actions and dialogs.

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Context, Key, RichText, Ui};
use nova_build::build_swf;
use nova_core::{
    Asset, AssetKind, Frame, Layer, LayerKind, ObjectInstance, ObjectKind, Project,
    ProjectTemplate, Rgba, SoundCue, Symbol, SymbolKind, TweenKind,
};
use nova_script::ScriptEngine;
use uuid::Uuid;

use crate::ai_editor::AiEditor;
use crate::history::History;
use crate::splash::Splash;
use crate::state::{
    AlignMode, AppState, BottomTab, Dialog, KeyframeRef, LogLevel, Mode, PendingAction, Selection,
    Tool, UiAction,
};
use crate::{
    autosave, branding, code_view, console, flash, properties, stage_view, theme, timeline_view,
    tree, util,
};

pub struct NovaApp {
    project: Project,
    state: AppState,
    engine: Option<ScriptEngine>,
    history: History,
    pending_before: Option<Project>,
    suppress_history: bool,
    last_history_label: &'static str,
    flash_settings: flash::FlashSettings,
    pending_swf: Option<PathBuf>,
    splash: Option<Splash>,
    ai: AiEditor,
}

impl NovaApp {
    pub fn try_new(
        cc: &eframe::CreationContext<'_>,
        startup_project: Option<&Path>,
    ) -> Result<Self, String> {
        let project = Project::demo();
        let state = AppState::new(&project);
        let mut warnings = Vec::new();

        let logo = match branding::load_logo_texture(&cc.egui_ctx) {
            Ok(texture) => Some(texture),
            Err(error) => {
                warnings.push(error);
                None
            }
        };
        let flash_settings = match flash::load_settings() {
            Ok(settings) => settings,
            Err(error) => {
                warnings.push(format!("Flash-Player-Einstellungen: {error}"));
                flash::FlashSettings::default()
            }
        };

        let ai = AiEditor::new();
        if let Some(error) = &ai.startup_error {
            warnings.push(format!("KI-Konfiguration: {error}"));
        }
        let warning = if warnings.is_empty() {
            None
        } else {
            Some(warnings.join(" · "))
        };
        let mut app = Self {
            project,
            state,
            engine: None,
            history: History::new(128),
            pending_before: None,
            suppress_history: false,
            last_history_label: "Bearbeiten",
            flash_settings,
            pending_swf: None,
            splash: Some(Splash::new(Instant::now(), logo, warning.clone())),
            ai,
        };
        if let Some(path) = startup_project {
            if is_swf_path(path) {
                // A shell association or `--open` may pass a SWF directly. Do
                // not try to parse it as a project; remember it until the
                // runtime configuration is ready.
                app.pending_swf = Some(path.to_path_buf());
            } else {
                app.load_path(path);
            }
        }

        if flash::needs_first_launch_setup(&app.flash_settings) {
            app.state.dialog = Dialog::RuntimeSetup {
                adobe_path: app
                    .flash_settings
                    .adobe_projector_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                test_result: None,
                error: None,
            };
        } else if app.pending_swf.is_some() {
            app.open_pending_swf();
        } else if let Some((path, recovery)) = autosave::recover_for(None, &app.project) {
            app.state.dialog = Dialog::AutosaveRecovery { path, recovery };
        }
        if let Some(warning) = warning {
            app.state.log(LogLevel::Warn, warning);
        }
        Ok(app)
    }

    // ------------------------------------------------------------- panels

    fn menu_bar(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Datei", |ui| {
                if ui.button("Neu …    Strg+N").clicked() {
                    actions.push(UiAction::NewProjectPrompt);
                    ui.close();
                }
                if ui.button("Öffnen …    Strg+O").clicked() {
                    actions.push(UiAction::OpenPrompt);
                    ui.close();
                }
                ui.separator();
                if ui.button("Speichern    Strg+S").clicked() {
                    actions.push(UiAction::Save);
                    ui.close();
                }
                if ui.button("Speichern unter …").clicked() {
                    actions.push(UiAction::SaveAs);
                    ui.close();
                }
                ui.separator();
                if ui.button("Film exportieren …    Strg+E").clicked() {
                    actions.push(UiAction::Export);
                    ui.close();
                }
                ui.separator();
                ui.menu_button("Autosave", |ui| {
                    if ui
                        .checkbox(&mut self.state.autosave_enabled, "Autosave aktiv")
                        .changed()
                    {
                        actions.push(UiAction::ToggleAutosave);
                    }
                    let mut interval = self.state.autosave_seconds;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Intervall").small().color(theme::TEXT_DIM));
                        if ui
                            .add(
                                egui::DragValue::new(&mut interval)
                                    .range(5..=3600)
                                    .speed(5.0)
                                    .suffix(" s"),
                            )
                            .changed()
                        {
                            actions.push(UiAction::SetAutosaveInterval(interval));
                        }
                    });
                    if let Some(path) = &self.state.last_autosave_path {
                        ui.label(
                            RichText::new(format!("Letzte Sicherung: {}", path.display()))
                                .small()
                                .color(theme::TEXT_FAINT),
                        );
                    }
                });
                ui.separator();
                if ui.button("Beenden").clicked() {
                    actions.push(UiAction::Quit);
                    ui.close();
                }
            });

            ui.menu_button("Bearbeiten", |ui| {
                let undo_label = self
                    .history
                    .undo_label()
                    .map(|label| format!("Rückgängig: {label}"))
                    .unwrap_or_else(|| "Rückgängig".to_string());
                if ui
                    .add_enabled(
                        self.history.can_undo(),
                        egui::Button::new(format!("{undo_label}    Strg+Z")),
                    )
                    .on_hover_text(format!(
                        "{} Schritt(e) in der Historie",
                        self.history.undo_len()
                    ))
                    .clicked()
                {
                    actions.push(UiAction::Undo);
                    ui.close();
                }
                let redo_label = self
                    .history
                    .redo_label()
                    .map(|label| format!("Wiederholen: {label}"))
                    .unwrap_or_else(|| "Wiederholen".to_string());
                if ui
                    .add_enabled(
                        self.history.can_redo(),
                        egui::Button::new(format!("{redo_label}    Strg+Y")),
                    )
                    .on_hover_text(format!(
                        "{} Schritt(e) wiederholbar",
                        self.history.redo_len()
                    ))
                    .clicked()
                {
                    actions.push(UiAction::Redo);
                    ui.close();
                }
                ui.separator();
                if ui.button("Auswahl löschen    Entf").clicked() {
                    actions.push(UiAction::DeleteSelection);
                    ui.close();
                }
                ui.separator();
                if ui.button("Objekte kopieren    Strg+C").clicked() {
                    actions.push(UiAction::CopyObjects);
                    ui.close();
                }
                if ui.button("Objekte einfügen    Strg+V").clicked() {
                    actions.push(UiAction::PasteObjects);
                    ui.close();
                }
                if ui.button("Objekte duplizieren    Strg+D").clicked() {
                    actions.push(UiAction::DuplicateObjects);
                    ui.close();
                }
                ui.separator();
                if ui.button("Schlüsselbilder kopieren    Strg+C").clicked() {
                    actions.push(UiAction::CopyKeyframes);
                    ui.close();
                }
                if ui.button("Schlüsselbilder einfügen    Strg+V").clicked() {
                    actions.push(UiAction::PasteKeyframes);
                    ui.close();
                }
                if ui.button("Schlüsselbilder duplizieren    Strg+D").clicked() {
                    actions.push(UiAction::DuplicateKeyframes);
                    ui.close();
                }
                ui.separator();
                if ui.button("Frame einfügen    F5").clicked() {
                    actions.push(UiAction::InsertFrame);
                    ui.close();
                }
                if ui.button("Frame entfernen    Shift+F5").clicked() {
                    actions.push(UiAction::RemoveFrame);
                    ui.close();
                }
                ui.separator();
                let has_script = self.state.active_script.is_some();
                if ui
                    .add_enabled(has_script, egui::Button::new("Aktives Skript prüfen"))
                    .clicked()
                {
                    if let Some(id) = self.state.active_script {
                        actions.push(UiAction::CheckScript(id));
                    }
                    ui.close();
                }
            });

            ui.menu_button("Einfügen", |ui| {
                if ui.button("Rechteck").clicked() {
                    actions.push(UiAction::AddRect);
                    ui.close();
                }
                if ui.button("Ellipse").clicked() {
                    actions.push(UiAction::AddEllipse);
                    ui.close();
                }
                if ui.button("Text").clicked() {
                    actions.push(UiAction::AddText);
                    ui.close();
                }
                let symbol = self
                    .state
                    .selected_symbol
                    .or_else(|| self.project.symbols.first().map(|symbol| symbol.id));
                if ui
                    .add_enabled(symbol.is_some(), egui::Button::new("Symbol-Instanz"))
                    .clicked()
                {
                    if let Some(id) = symbol {
                        actions.push(UiAction::AddSymbolInstance(id));
                    }
                    ui.close();
                }
                ui.separator();
                if ui.button("Neue Ebene").clicked() {
                    actions.push(UiAction::AddLayer);
                    ui.close();
                }
                if ui.button("Neue Ordnerebene").clicked() {
                    actions.push(UiAction::AddFolder);
                    ui.close();
                }
                if ui.button("Neue Szene").clicked() {
                    actions.push(UiAction::AddScene);
                    ui.close();
                }
                if ui.button("Aktive Szene löschen").clicked() {
                    actions.push(UiAction::DeleteScene);
                    ui.close();
                }
                if ui.button("Neues Skript").clicked() {
                    actions.push(UiAction::AddScript);
                    ui.close();
                }
                if ui.button("Neues Symbol").clicked() {
                    actions.push(UiAction::AddSymbol);
                    ui.close();
                }
                ui.separator();
                if ui.button("Schlüsselbild    F6").clicked() {
                    actions.push(UiAction::InsertKeyframe);
                    ui.close();
                }
                if ui.button("Leeres Schlüsselbild    Shift+F6").clicked() {
                    actions.push(UiAction::InsertBlankKeyframe);
                    ui.close();
                }
                if ui.button("Schlüsselbild entfernen").clicked() {
                    actions.push(UiAction::RemoveKeyframe);
                    ui.close();
                }
            });

            ui.menu_button("Ansicht", |ui| {
                if ui
                    .selectable_label(self.state.mode == Mode::Code, "Code-Modus")
                    .clicked()
                {
                    actions.push(UiAction::SetMode(Mode::Code));
                    ui.close();
                }
                if ui
                    .selectable_label(self.state.mode == Mode::Animation, "Animations-Modus")
                    .clicked()
                {
                    actions.push(UiAction::SetMode(Mode::Animation));
                    ui.close();
                }
                ui.separator();
                if ui.button("Bühne einpassen").clicked() {
                    actions.push(UiAction::FitStage);
                    ui.close();
                }
                if ui.button("Vergrößern").clicked() {
                    actions.push(UiAction::ZoomBy(1.2));
                    ui.close();
                }
                if ui.button("Verkleinern").clicked() {
                    actions.push(UiAction::ZoomBy(1.0 / 1.2));
                    ui.close();
                }
                ui.separator();
                if ui
                    .selectable_label(self.state.onion_skin, "Onion Skin")
                    .clicked()
                {
                    actions.push(UiAction::ToggleOnion);
                    ui.close();
                }
                if ui
                    .selectable_label(self.state.show_grid, "Raster")
                    .clicked()
                {
                    actions.push(UiAction::ToggleGrid);
                    ui.close();
                }
                if ui
                    .selectable_label(self.state.snap_to_grid, "Am Raster einrasten")
                    .clicked()
                {
                    actions.push(UiAction::ToggleSnap);
                    ui.close();
                }
                ui.separator();
                if ui.button("Timeline").clicked() {
                    actions.push(UiAction::SetBottomTab(BottomTab::Timeline));
                    ui.close();
                }
                if ui.button("Konsole").clicked() {
                    actions.push(UiAction::SetBottomTab(BottomTab::Console));
                    ui.close();
                }
                if ui.button("Probleme").clicked() {
                    actions.push(UiAction::SetBottomTab(BottomTab::Problems));
                    ui.close();
                }
            });

            ui.menu_button("Flash Player", |ui| {
                if ui.button("Open in Flash Player").clicked() {
                    actions.push(UiAction::OpenInFlashPlayer);
                    ui.close();
                }
                if ui.button("Runtime-Einstellungen …").clicked() {
                    actions.push(UiAction::OpenFlashSettings);
                    ui.close();
                }
                ui.separator();
                let status = flash::status(&self.flash_settings);
                ui.label(
                    RichText::new(status.summary())
                        .small()
                        .color(if status.ready {
                            theme::TEXT_DIM
                        } else {
                            theme::AMBER
                        }),
                );
                if let Some((path, _, _, _)) = &self.state.last_export {
                    ui.label(
                        RichText::new(format!("Letzter Export: {}", path.display()))
                            .small()
                            .color(theme::TEXT_FAINT),
                    );
                }
            });

            ui.menu_button("KI", |ui| {
                if ui
                    .selectable_label(self.ai.open, "AI Agent Editor")
                    .clicked()
                {
                    actions.push(UiAction::OpenAiEditor);
                    ui.close();
                }
                if ui.button("KI-Anbieter …").clicked() {
                    actions.push(UiAction::OpenAiProviders);
                    ui.close();
                }
                ui.separator();
                ui.label(
                    RichText::new(
                        "Cloud-Anfragen nur nach ausdrücklicher Zustimmung. Dateiänderungen immer mit Diff.",
                    )
                    .small()
                    .color(theme::TEXT_FAINT),
                );
            });

            ui.menu_button("Hilfe", |ui| {
                if ui.button("Hilfe & Grenzen").clicked() {
                    actions.push(UiAction::Help);
                    ui.close();
                }
                if ui.button("Über NØVA Flash Studio").clicked() {
                    actions.push(UiAction::About);
                    ui.close();
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .small()
                        .color(theme::TEXT_FAINT),
                );
            });
        });
    }

    fn toolbar(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("NØVA")
                    .strong()
                    .size(17.0)
                    .color(theme::ACCENT),
            );
            ui.label(
                RichText::new("FLASH STUDIO")
                    .small()
                    .color(theme::TEXT_FAINT),
            );

            ui.add_space(10.0);
            for (mode, label) in [(Mode::Code, "Code"), (Mode::Animation, "Animation")] {
                if ui
                    .selectable_label(self.state.mode == mode, RichText::new(label).strong())
                    .clicked()
                {
                    actions.push(UiAction::SetMode(mode));
                }
            }
            ui.separator();
            if ui
                .selectable_label(
                    self.ai.open,
                    RichText::new("AI Agent").strong().color(theme::ACCENT),
                )
                .on_hover_text("AI Agent Editor öffnen (Strg+Umschalt+A)")
                .clicked()
            {
                actions.push(UiAction::OpenAiEditor);
            }
            ui.separator();

            ui.label(RichText::new(&self.project.metadata.name).strong());
            if self.state.dirty {
                ui.label(RichText::new("●").small().color(theme::AMBER))
                    .on_hover_text("Ungespeicherte Änderungen");
            }
            if let Some(path) = &self.state.path {
                ui.label(
                    RichText::new(path.display().to_string())
                        .small()
                        .color(theme::TEXT_FAINT),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Film exportieren").strong())
                    .on_hover_text("Aktive Szene als SWF oder XFL-Paket schreiben (Strg+E)")
                    .clicked()
                {
                    actions.push(UiAction::Export);
                }
                if ui
                    .button(RichText::new("Open in Flash Player").strong())
                    .on_hover_text("Letzten SWF-Export in der konfigurierten Adobe- oder Ruffle-Runtime öffnen")
                    .clicked()
                {
                    actions.push(UiAction::OpenInFlashPlayer);
                }
                if ui.button("Speichern").clicked() {
                    actions.push(UiAction::Save);
                }
            });
        });
    }

    fn bottom_dock(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        ui.horizontal(|ui| {
            let errors = self.state.error_count();
            let warnings = self.state.warning_count();
            let tabs = [
                (BottomTab::Timeline, "Timeline".to_string()),
                (BottomTab::Console, "Konsole".to_string()),
                (
                    BottomTab::Problems,
                    if errors > 0 {
                        format!("Probleme ({errors})")
                    } else if warnings > 0 {
                        format!("Probleme ({warnings} W)")
                    } else {
                        "Probleme".to_string()
                    },
                ),
            ];
            for (tab, label) in tabs {
                let color = if tab == BottomTab::Problems && errors > 0 {
                    theme::RED
                } else {
                    theme::TEXT
                };
                if ui
                    .selectable_label(
                        self.state.bottom_tab == tab,
                        RichText::new(label).color(color),
                    )
                    .clicked()
                {
                    actions.push(UiAction::SetBottomTab(tab));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(scene) = self.project.edit_scene(self.state.edit_symbol) {
                    ui.label(
                        RichText::new(format!(
                            "{} Frames · {:.0} fps",
                            scene.frame_count(),
                            scene.fps
                        ))
                        .small()
                        .color(theme::TEXT_FAINT),
                    );
                }
            });
        });
        theme::hairline(ui);

        match self.state.bottom_tab {
            BottomTab::Timeline => {
                actions.extend(timeline_view::show(ui, &mut self.project, &mut self.state));
            }
            BottomTab::Console => {
                console::show(ui, &self.project, &mut self.state, &mut self.engine);
            }
            BottomTab::Problems => {
                actions.extend(code_view::problems(ui, &mut self.state, &self.project));
            }
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&self.state.status)
                    .small()
                    .color(theme::TEXT_DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(scene) = self.project.edit_scene(self.state.edit_symbol) {
                    ui.label(
                        RichText::new(format!(
                            "{} · Frame {}/{} · {:.0}%",
                            scene.name,
                            self.state.frame + 1,
                            scene.frame_count(),
                            self.state.zoom * 100.0
                        ))
                        .small()
                        .color(theme::TEXT_FAINT),
                    );
                }
                let errors = self.state.error_count();
                if errors > 0 {
                    ui.label(
                        RichText::new(format!("{errors} Fehler"))
                            .small()
                            .color(theme::RED),
                    );
                }
            });
        });
    }

    // ---------------------------------------------------------- shortcuts

    fn shortcut_actions(&self, ctx: &Context) -> Vec<UiAction> {
        if !matches!(self.state.dialog, Dialog::None) {
            return Vec::new();
        }
        let typing = ctx.egui_wants_keyboard_input();
        let mut actions = Vec::new();

        ctx.input(|input| {
            let command = input.modifiers.command;
            let shift = input.modifiers.shift;
            if command && input.key_pressed(Key::N) {
                actions.push(UiAction::NewProjectPrompt);
            }
            if command && input.key_pressed(Key::O) {
                actions.push(UiAction::OpenPrompt);
            }
            if command && input.key_pressed(Key::S) {
                actions.push(if shift {
                    UiAction::SaveAs
                } else {
                    UiAction::Save
                });
            }
            if command && input.key_pressed(Key::E) {
                actions.push(UiAction::Export);
            }
            if command && input.key_pressed(Key::F) {
                actions.push(UiAction::ToggleFind);
            }

            if !typing {
                if command && shift && input.key_pressed(Key::A) {
                    actions.push(UiAction::OpenAiEditor);
                }
                if command && input.key_pressed(Key::Z) {
                    actions.push(if shift {
                        UiAction::Redo
                    } else {
                        UiAction::Undo
                    });
                }
                if command && input.key_pressed(Key::Y) {
                    actions.push(UiAction::Redo);
                }
                if self.state.mode == Mode::Animation {
                    let has_objects = !self.state.selection.object_ids().is_empty();
                    if command && input.key_pressed(Key::C) {
                        actions.push(if has_objects {
                            UiAction::CopyObjects
                        } else {
                            UiAction::CopyKeyframes
                        });
                    }
                    if command && input.key_pressed(Key::V) {
                        actions.push(if self.state.object_clipboard.is_empty() {
                            UiAction::PasteKeyframes
                        } else {
                            UiAction::PasteObjects
                        });
                    }
                    if command && input.key_pressed(Key::D) {
                        actions.push(if has_objects {
                            UiAction::DuplicateObjects
                        } else {
                            UiAction::DuplicateKeyframes
                        });
                    }
                }
                if input.key_pressed(Key::Space) && self.state.mode == Mode::Animation {
                    actions.push(UiAction::TogglePlay);
                }
                if input.key_pressed(Key::Delete) && self.state.mode == Mode::Animation {
                    actions.push(UiAction::DeleteSelection);
                }
                if input.key_pressed(Key::F6) {
                    actions.push(if shift {
                        UiAction::InsertBlankKeyframe
                    } else {
                        UiAction::InsertKeyframe
                    });
                }
                if input.key_pressed(Key::F5) {
                    actions.push(if shift {
                        UiAction::RemoveFrame
                    } else {
                        UiAction::InsertFrame
                    });
                }
                if input.key_pressed(Key::Escape) {
                    actions.push(UiAction::Deselect);
                }
                if input.key_pressed(Key::ArrowLeft) {
                    actions.push(UiAction::Step(-1));
                }
                if input.key_pressed(Key::ArrowRight) {
                    actions.push(UiAction::Step(1));
                }
                if input.key_pressed(Key::Home) {
                    actions.push(UiAction::FirstFrame);
                }
                if input.key_pressed(Key::End) {
                    actions.push(UiAction::LastFrame);
                }
            }
        });

        actions
    }

    // ------------------------------------------------------------ actions

    fn handle_actions(&mut self, actions: Vec<UiAction>, ctx: &Context) {
        for action in actions {
            match action {
                UiAction::NewProjectPrompt => self.request_new(),
                UiAction::OpenPrompt => self.request_open(),
                UiAction::Save => {
                    self.save();
                }
                UiAction::SaveAs => {
                    self.save_as();
                }
                UiAction::Export => self.export(),
                UiAction::OpenInFlashPlayer => self.open_in_flash_player(),
                UiAction::OpenFlashSettings => self.open_flash_settings_dialog(),
                UiAction::OpenAiEditor => {
                    self.ai.open = !self.ai.open;
                    if self.ai.open {
                        self.state.set_status("AI Agent Editor geöffnet");
                    }
                }
                UiAction::OpenAiProviders => {
                    self.state.dialog = Dialog::AiProviders;
                }
                UiAction::Quit => self.request_quit(ctx),
                UiAction::About => self.state.dialog = Dialog::About,
                UiAction::Help => self.state.dialog = Dialog::Help,
                UiAction::SetMode(mode) => {
                    self.state.mode = mode;
                    if mode == Mode::Code {
                        self.state.playing = false;
                    }
                    self.state.set_status(format!("{}modus", mode.label()));
                }
                UiAction::SetBottomTab(tab) => self.state.bottom_tab = tab,
                UiAction::TogglePlay => self.toggle_play(),
                UiAction::Stop => {
                    self.state.playing = false;
                    self.state.frame = 0;
                    self.state.play_accumulator = 0.0;
                    self.state.set_status("Wiedergabe gestoppt");
                }
                UiAction::Step(delta) => self.step(delta as i64),
                UiAction::FirstFrame => self.jump_to_frame(0),
                UiAction::LastFrame => {
                    let last = self
                        .project
                        .edit_scene(self.state.edit_symbol)
                        .map(|scene| scene.frame_count().saturating_sub(1))
                        .unwrap_or(0);
                    self.jump_to_frame(last);
                }
                UiAction::AddLayer => self.add_layer(),
                UiAction::AddFolder => self.add_folder(),
                UiAction::AddScript => self.add_script(),
                UiAction::AddSymbol => self.add_symbol(),
                UiAction::AddMovieClipSymbol => self.add_symbol_kind(SymbolKind::MovieClip),
                UiAction::AddRect => self.add_shape(false),
                UiAction::AddEllipse => self.add_shape(true),
                UiAction::AddText => self.add_text(),
                UiAction::AddSymbolInstance(id) => self.add_symbol_instance(id),
                UiAction::AddBitmapAsset(id) => self.add_bitmap_instance(id),
                UiAction::AddSoundCue(id) => self.add_sound_cue(id),
                UiAction::PreviewAsset(id) => self.preview_asset(id),
                UiAction::ImportBitmap => self.import_bitmap(),
                UiAction::ImportAudio => self.import_audio(),
                UiAction::EnterSymbol(id) => self.enter_symbol(id),
                UiAction::ExitSymbol => self.exit_symbol(),
                UiAction::DuplicateSymbol(id) => self.duplicate_symbol(id),
                UiAction::RenameSymbol(id, name) => self.rename_symbol(id, name),
                UiAction::SetTool(tool) => self.set_tool(tool),
                UiAction::SetLayerKind(kind) => self.set_layer_kind(kind),
                UiAction::SetTweenKind(kind) => self.set_tween_kind(kind),
                UiAction::AddScene => self.add_scene(),
                UiAction::DeleteScene => self.delete_scene(),
                UiAction::InsertKeyframe => self.insert_keyframe(false),
                UiAction::InsertBlankKeyframe => self.insert_keyframe(true),
                UiAction::RemoveKeyframe => self.remove_keyframe(),
                UiAction::DeleteSelection => self.delete_selection(),
                UiAction::CopyObjects => self.copy_objects(),
                UiAction::PasteObjects => self.paste_objects(),
                UiAction::DuplicateObjects => self.duplicate_objects(),
                UiAction::AlignSelection(mode) => self.align_selection(mode),
                UiAction::CheckScript(id) => self.check_script(id),
                UiAction::OpenScript(id) => self.open_script(id),
                UiAction::FitStage => {
                    self.state.zoom_initialized = false;
                    self.state.set_status("Bühne eingepasst");
                }
                UiAction::ZoomBy(factor) => {
                    self.state.zoom_initialized = true;
                    self.state.zoom = (self.state.zoom * factor).clamp(0.05, 8.0);
                    self.state
                        .set_status(format!("Zoom {:.0}%", self.state.zoom * 100.0));
                }
                UiAction::ToggleOnion => {
                    self.state.onion_skin = !self.state.onion_skin;
                    self.state.set_status(if self.state.onion_skin {
                        "Onion Skin an"
                    } else {
                        "Onion Skin aus"
                    });
                }
                UiAction::ToggleGrid => {
                    self.state.show_grid = !self.state.show_grid;
                }
                UiAction::ToggleSnap => {
                    self.state.snap_to_grid = !self.state.snap_to_grid;
                    self.state.set_status(if self.state.snap_to_grid {
                        "Am Raster einrasten an"
                    } else {
                        "Am Raster einrasten aus"
                    });
                }
                UiAction::ToggleFind => {
                    self.state.find_open = !self.state.find_open;
                    self.state.find_offset = 0;
                }
                UiAction::SelectScene(id) => {
                    if let Some(scene) = self.project.scenes.iter().find(|scene| scene.id == id) {
                        self.project.active_scene = Some(scene.id);
                        self.state.reset_view(&self.project);
                        self.state.selection = Selection::Scene;
                        self.state
                            .set_status(format!("Szene '{}' ausgewählt", scene.name));
                    }
                }
                UiAction::JumpToFrame(frame) => self.jump_to_frame(frame),
                UiAction::Deselect => {
                    self.state.selection = Selection::None;
                    self.state.selected_keyframes.clear();
                    self.state.frame_range = None;
                    self.state.set_status("Auswahl aufgehoben");
                }
                UiAction::Undo => self.undo(),
                UiAction::Redo => self.redo(),
                UiAction::CreateTween => self.create_tween(),
                UiAction::RemoveTween => self.remove_tween(),
                UiAction::SetTweenEasing(easing) => self.set_tween_easing(easing),
                UiAction::CopyKeyframes => self.copy_keyframes(),
                UiAction::PasteKeyframes => self.paste_keyframes(),
                UiAction::DuplicateKeyframes => self.duplicate_keyframes(),
                UiAction::InsertFrame => self.insert_frames(),
                UiAction::RemoveFrame => self.remove_frames(),
                UiAction::MoveKeyframesToLayer(direction) => {
                    self.move_keyframes_to_layer(direction)
                }
                UiAction::ToggleAutosave => {
                    self.state.autosave_enabled = !self.state.autosave_enabled;
                    self.state.set_status(if self.state.autosave_enabled {
                        "Autosave aktiviert"
                    } else {
                        "Autosave deaktiviert"
                    });
                }
                UiAction::SetAutosaveInterval(seconds) => {
                    self.state.autosave_seconds = seconds.max(5);
                    self.state.set_status(format!(
                        "Autosave-Intervall: {} s",
                        self.state.autosave_seconds
                    ));
                }
            }
        }
    }

    // ------------------------------------------------------ history & timers

    fn commit_pending(&mut self) {
        if let Some(before) = self.pending_before.take() {
            self.history
                .record(self.last_history_label, before, &self.project);
        }
    }

    fn undo(&mut self) {
        self.commit_pending();
        if let Some((label, project)) = self.history.undo(&self.project) {
            self.project = project;
            self.state.dirty = true;
            self.state.selection = Selection::None;
            self.state.selected_keyframes.clear();
            self.state.frame_range = None;
            self.state.dragging = None;
            self.state.marquee = None;
            self.suppress_history = true;
            self.state.clamp_frame(&self.project);
            self.state.set_status(format!("Rückgängig: {label}"));
        } else {
            self.state.set_status("Nichts rückgängig zu machen");
        }
    }

    fn redo(&mut self) {
        self.commit_pending();
        if let Some((label, project)) = self.history.redo(&self.project) {
            self.project = project;
            self.state.dirty = true;
            self.state.selection = Selection::None;
            self.state.selected_keyframes.clear();
            self.state.frame_range = None;
            self.state.dragging = None;
            self.state.marquee = None;
            self.suppress_history = true;
            self.state.clamp_frame(&self.project);
            self.state.set_status(format!("Wiederholt: {label}"));
        } else {
            self.state.set_status("Nichts zu wiederholen");
        }
    }

    fn finish_history(&mut self, before: Project, ctx: &Context) {
        if self.suppress_history {
            self.suppress_history = false;
            self.pending_before = None;
            return;
        }
        let changed = self.project != before;
        if changed {
            self.state.dirty = true;
            if self.pending_before.is_none() {
                self.pending_before = Some(before);
            }
        }
        let continuous =
            ctx.egui_wants_keyboard_input() || ctx.input(|input| input.pointer.any_down());
        if !continuous {
            self.commit_pending();
        }
    }

    /// Decodes embedded bitmap assets once per session and caches the GPU
    /// textures used by the stage and the asset preview.
    fn refresh_asset_textures(&mut self, ctx: &Context) {
        for index in 0..self.project.assets.len() {
            let asset = &self.project.assets[index];
            if asset.kind != AssetKind::Bitmap
                || asset.data.is_empty()
                || self.state.asset_textures.contains_key(&asset.id)
            {
                continue;
            }
            let Ok(image) = image::load_from_memory(&asset.data) else {
                continue;
            };
            let rgba = image.to_rgba8();
            let (width, height) = (rgba.width(), rgba.height());
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
            let texture = ctx.load_texture(
                format!("asset-{}", asset.id),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            self.state.asset_textures.insert(asset.id, texture);
        }
    }

    fn maybe_autosave(&mut self, ctx: &Context) {
        if !self.state.autosave_enabled {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(
            self.state.autosave_seconds.max(5),
        ));
        if !self.state.dirty {
            return;
        }
        if self.state.last_autosave.elapsed().as_secs() < self.state.autosave_seconds {
            return;
        }
        match autosave::write(&self.project, self.state.path.as_deref()) {
            Ok(path) => {
                self.state.last_autosave = std::time::Instant::now();
                self.state.last_autosave_path = Some(path);
            }
            Err(error) => {
                self.state
                    .log(LogLevel::Warn, format!("Autosave fehlgeschlagen: {error}"));
            }
        }
    }

    // ------------------------------------------------------------ keyframes

    fn keyframe_selection(&self) -> Vec<KeyframeRef> {
        if !self.state.selected_keyframes.is_empty() {
            return self.state.selected_keyframes.clone();
        }
        let Some(layer) = self.target_layer() else {
            return Vec::new();
        };
        let Some(scene) = self.project.edit_scene(self.state.edit_symbol) else {
            return Vec::new();
        };
        let Some(layer) = scene.layers.iter().find(|candidate| candidate.id == layer) else {
            return Vec::new();
        };
        layer
            .last_keyframe_at_or_before(self.state.frame)
            .map(|frame| {
                vec![KeyframeRef {
                    layer: layer.id,
                    frame,
                }]
            })
            .unwrap_or_default()
    }

    fn ensure_frame_count(&mut self, count: usize) {
        if let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) {
            scene.ensure_frame_count(count);
        }
    }

    fn copy_keyframes(&mut self) {
        let mut refs = self.keyframe_selection();
        if refs.is_empty() {
            self.state.set_status("Kein Schlüsselbild ausgewählt");
            return;
        }
        let first_layer = refs[0].layer;
        if refs.iter().any(|reference| reference.layer != first_layer) {
            self.state.log(
                LogLevel::Warn,
                "Kopieren über mehrere Ebenen wird nicht unterstützt; wähle Schlüsselbilder einer Ebene.",
            );
            self.state
                .set_status("Kopieren über mehrere Ebenen nicht unterstützt");
            return;
        }
        refs.sort_by_key(|reference| (reference.frame, reference.layer));
        let base = refs[0].frame;
        let Some(scene) = self.project.edit_scene(self.state.edit_symbol) else {
            return;
        };
        let mut copied = Vec::new();
        for reference in &refs {
            let Some(layer) = scene
                .layers
                .iter()
                .find(|layer| layer.id == reference.layer)
            else {
                continue;
            };
            let Some(frame) = layer.frames.get(reference.frame) else {
                continue;
            };
            copied.push(crate::state::ClipboardKeyframe {
                offset: reference.frame - base,
                frame: frame.clone(),
            });
        }
        if copied.is_empty() {
            return;
        }
        let count = copied.len();
        self.state.keyframe_clipboard = Some(crate::state::KeyframeClipboard {
            frames: copied,
            source_layer: Some(refs[0].layer),
        });
        self.state.object_clipboard.clear();
        self.state
            .set_status(format!("{count} Schlüsselbild(er) kopiert"));
    }

    fn paste_keyframes(&mut self) {
        let Some(clipboard) = self.state.keyframe_clipboard.clone() else {
            self.state.set_status("Zwischenablage ist leer");
            return;
        };
        let Some(target) = self.target_layer().or(clipboard.source_layer) else {
            return;
        };
        let base = self.state.frame;
        let mut max_frame = 0usize;
        for entry in &clipboard.frames {
            max_frame = max_frame.max(base + entry.offset);
        }
        self.ensure_frame_count(max_frame + 1);
        let mut pasted = Vec::new();
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        for entry in &clipboard.frames {
            let frame_index = base + entry.offset;
            layer.ensure_frames(frame_index + 1);
            layer.frames[frame_index] = entry.frame.clone();
            pasted.push(KeyframeRef {
                layer: target,
                frame: frame_index,
            });
        }
        self.project.touch();
        self.state.dirty = true;
        self.state.selected_keyframes = pasted;
        self.last_history_label = "Schlüsselbilder einfügen";
        self.state.set_status(format!(
            "{} Schlüsselbild(er) eingefügt",
            clipboard.frames.len()
        ));
    }

    fn duplicate_keyframes(&mut self) {
        let refs = self.keyframe_selection();
        if refs.is_empty() {
            return;
        }
        let last = refs
            .iter()
            .map(|reference| reference.frame)
            .max()
            .unwrap_or(0);
        let first = refs
            .iter()
            .map(|reference| reference.frame)
            .min()
            .unwrap_or(0);
        let shift = last + 1 - first;
        let mut copies = Vec::new();
        for reference in &refs {
            let Some(scene) = self.project.edit_scene(self.state.edit_symbol) else {
                return;
            };
            let Some(layer) = scene
                .layers
                .iter()
                .find(|layer| layer.id == reference.layer)
            else {
                continue;
            };
            let Some(frame) = layer.frames.get(reference.frame).cloned() else {
                continue;
            };
            copies.push((reference.layer, reference.frame + shift, frame));
        }
        if copies.is_empty() {
            return;
        }
        let max_frame = copies.iter().map(|(_, frame, _)| *frame).max().unwrap_or(0);
        self.ensure_frame_count(max_frame + 1);
        let mut duplicated = Vec::new();
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        for (layer_id, frame_index, frame) in copies {
            let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == layer_id) else {
                continue;
            };
            layer.ensure_frames(frame_index + 1);
            layer.frames[frame_index] = frame;
            duplicated.push(KeyframeRef {
                layer: layer_id,
                frame: frame_index,
            });
        }
        self.project.touch();
        self.state.dirty = true;
        self.state.selected_keyframes = duplicated.clone();
        if let Some(first) = duplicated.iter().map(|reference| reference.frame).min() {
            self.state.frame = first;
        }
        self.last_history_label = "Schlüsselbilder duplizieren";
        self.state.set_status("Schlüsselbilder dupliziert");
    }

    fn frame_operation_range(&self) -> (usize, usize) {
        self.state
            .frame_range
            .map(|(start, end)| (start.min(end), start.max(end)))
            .unwrap_or((self.state.frame, self.state.frame))
    }

    fn frame_operation_layers(&self) -> Vec<Uuid> {
        let selected: Vec<Uuid> = match &self.state.selection {
            Selection::Layer(id) => vec![*id],
            Selection::Objects(ids) => ids
                .iter()
                .filter_map(|id| self.state.layer_of_object(&self.project, *id))
                .collect(),
            _ => Vec::new(),
        };
        if !selected.is_empty() {
            let mut unique = Vec::new();
            for id in selected {
                if !unique.contains(&id) {
                    unique.push(id);
                }
            }
            return unique;
        }
        self.project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| scene.layers.iter().map(|layer| layer.id).collect())
            .unwrap_or_default()
    }

    fn insert_frames(&mut self) {
        let (start, end) = self.frame_operation_range();
        let count = end.saturating_sub(start) + 1;
        let layers = self.frame_operation_layers();
        let position = end + 1;
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        for layer in scene
            .layers
            .iter_mut()
            .filter(|layer| layers.contains(&layer.id))
        {
            layer.ensure_frames(position + count);
            for _ in 0..count {
                layer.frames.insert(position, nova_core::Frame::default());
            }
        }
        self.project.touch();
        self.state.dirty = true;
        self.state.selected_keyframes.clear();
        self.last_history_label = "Frame einfügen";
        self.state.set_status(format!("{count} Frame(s) eingefügt"));
    }

    fn remove_frames(&mut self) {
        let (start, end) = self.frame_operation_range();
        let start = start.max(1);
        if end < start {
            self.state
                .set_status("Das erste Schlüsselbild bleibt erhalten");
            return;
        }
        let count = end - start + 1;
        let layers = self.frame_operation_layers();
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        for layer in scene
            .layers
            .iter_mut()
            .filter(|layer| layers.contains(&layer.id))
        {
            if layer.frames.len() <= 1 {
                continue;
            }
            let remove_end = (start + count).min(layer.frames.len());
            if start < remove_end {
                layer.frames.drain(start..remove_end);
            }
            if layer.frames.is_empty() {
                layer.frames.push(nova_core::Frame {
                    keyframe: true,
                    ..Default::default()
                });
            }
        }
        self.project.touch();
        self.state.dirty = true;
        self.state.selected_keyframes.clear();
        self.state.frame_range = None;
        self.state.clamp_frame(&self.project);
        self.last_history_label = "Frame entfernen";
        self.state.set_status(format!("{count} Frame(s) entfernt"));
    }

    fn move_keyframes_to_layer(&mut self, direction: i32) {
        let refs = self.keyframe_selection();
        if refs.is_empty() || direction == 0 {
            return;
        }
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let mut moved = Vec::new();
        for reference in refs {
            let Some(source_index) = scene
                .layers
                .iter()
                .position(|layer| layer.id == reference.layer)
            else {
                continue;
            };
            let destination = source_index as i32 + direction;
            if destination < 0 || destination as usize >= scene.layers.len() {
                continue;
            }
            let destination = destination as usize;
            if scene.layers[destination].locked {
                continue;
            }
            let Some(frame) = scene.layers[source_index]
                .frames
                .get(reference.frame)
                .cloned()
            else {
                continue;
            };
            scene.layers[destination].ensure_frames(reference.frame + 1);
            scene.layers[destination].frames[reference.frame] = frame;
            if source_index != destination {
                scene.layers[source_index].frames[reference.frame] = nova_core::Frame::default();
                moved.push(KeyframeRef {
                    layer: scene.layers[destination].id,
                    frame: reference.frame,
                });
            }
        }
        if moved.is_empty() {
            self.state
                .log(LogLevel::Warn, "Keine Ziel-Ebene für die Schlüsselbilder.");
            return;
        }
        self.project.touch();
        self.state.dirty = true;
        self.state.selected_keyframes = moved;
        self.last_history_label = "Schlüsselbilder verschieben";
        self.state
            .set_status("Schlüsselbilder in andere Ebene verschoben");
    }

    fn create_tween(&mut self) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            return;
        };
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        let Some(start) = layer.last_keyframe_at_or_before(frame) else {
            return;
        };
        let next = layer.next_keyframe_after(start);
        if let Some(next) = next {
            let start_objects = layer.frames[start].objects.clone();
            for object in start_objects {
                if !layer.frames[next]
                    .objects
                    .iter()
                    .any(|candidate| candidate.id == object.id)
                {
                    layer.frames[next].objects.push(object);
                }
            }
        } else {
            let end = start + 12;
            layer.ensure_frames(end + 1);
            let objects = layer.frames[start].objects.clone();
            layer.frames[end] = nova_core::Frame {
                keyframe: true,
                objects,
                ..Default::default()
            };
        }
        layer.frames[start].tween = Some(nova_core::Tween::default());
        self.project.touch();
        self.state.dirty = true;
        self.state.selected_keyframes.clear();
        self.last_history_label = "Motion-Tween erstellen";
        self.state
            .set_status("Motion-Tween über zwei Schlüsselbilder erstellt");
    }

    fn remove_tween(&mut self) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            return;
        };
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        let Some(start) = layer.last_keyframe_at_or_before(frame) else {
            return;
        };
        if layer.frames[start].tween.take().is_some() {
            self.project.touch();
            self.state.dirty = true;
            self.last_history_label = "Motion-Tween entfernen";
            self.state.set_status("Motion-Tween entfernt");
        }
    }

    fn set_tween_easing(&mut self, easing: nova_core::Easing) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            return;
        };
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        let Some(start) = layer.last_keyframe_at_or_before(frame) else {
            return;
        };
        let tween = layer.frames[start]
            .tween
            .get_or_insert_with(nova_core::Tween::default);
        tween.easing = easing;
        self.project.touch();
        self.state.dirty = true;
        self.last_history_label = "Tween-Easing ändern";
        self.state
            .set_status(format!("Tween-Easing: {}", easing.label()));
    }

    fn request_new(&mut self) {
        if self.state.dirty {
            self.state.dialog = Dialog::Unsaved {
                next: PendingAction::New,
            };
        } else {
            self.state.dialog = Dialog::NewProject {
                name: "Mein Projekt".into(),
                template: ProjectTemplate::Empty,
            };
        }
    }

    fn request_open(&mut self) {
        if self.state.dirty {
            self.state.dialog = Dialog::Unsaved {
                next: PendingAction::OpenPrompt,
            };
        } else {
            self.open_picker();
        }
    }

    fn request_quit(&mut self, ctx: &Context) {
        if self.state.dirty {
            self.state.dialog = Dialog::Unsaved {
                next: PendingAction::Quit,
            };
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn proceed(&mut self, next: PendingAction, ctx: &Context) {
        self.state.dialog = Dialog::None;
        match next {
            PendingAction::New => {
                self.state.dialog = Dialog::NewProject {
                    name: "Mein Projekt".into(),
                    template: ProjectTemplate::Empty,
                };
            }
            PendingAction::OpenPrompt => self.open_picker(),
            PendingAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn open_picker(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("NØVA Projekt", &["nova"])
            .add_filter("Adobe FLA/XFL", &["fla"])
            .add_filter("SWF Film", &["swf"])
            .add_filter("Alle unterstützten Formate", &["nova", "fla", "swf"])
            .set_title("Projekt oder Film öffnen")
            .pick_file()
        {
            self.load_path(&path);
        }
    }

    fn load_path(&mut self, path: &std::path::Path) {
        match nova_formats::load_path(path) {
            Ok(imported) => {
                let source_format = imported.source_format;
                let report = imported.report;
                let is_nova = source_format == nova_formats::FormatKind::Nova;
                let report_text = report.to_text();
                let has_errors = report.has_errors();
                self.project = imported.project;
                self.state = AppState::new(&self.project);
                self.state.console.clear();
                // Never point Save at an imported .swf/.fla file: Ctrl+S must
                // not overwrite the original with NØVA JSON.
                self.state.path = is_nova.then(|| path.to_path_buf());
                self.state.dirty = !is_nova;
                self.state.mode = Mode::Animation;
                self.history.clear();
                self.pending_before = None;
                self.suppress_history = true;
                self.state.log(
                    LogLevel::Success,
                    format!("Projekt geladen: {}", path.display()),
                );
                if !is_nova {
                    self.state.log(
                        LogLevel::Info,
                        format!(
                            "{} importiert. Speichern legt eine .nova-Datei an; die Originaldatei bleibt unverändert.",
                            source_format.label()
                        ),
                    );
                    self.state.dialog = Dialog::CompatibilityReport {
                        title: format!("{} importiert", source_format.label()),
                        report: report_text,
                        is_error: has_errors,
                    };
                } else if report.has_warnings() {
                    self.state.log(LogLevel::Warn, report_text);
                }
                self.engine = None;
                if let Some((recovery_path, recovery)) =
                    autosave::recover_for(is_nova.then_some(path), &self.project)
                {
                    self.state.dialog = Dialog::AutosaveRecovery {
                        path: recovery_path,
                        recovery,
                    };
                }
            }
            Err(error) => {
                self.state.log(LogLevel::Error, error.to_string());
                self.state.dialog = Dialog::Error {
                    title: "Projekt konnte nicht geöffnet werden".into(),
                    message: format!("{}\n\n{error}", path.display()),
                };
            }
        }
    }

    fn new_project(&mut self, name: String, template: ProjectTemplate) {
        autosave::discard(None);
        let project = template.create(name);
        self.project = project;
        self.state = AppState::new(&self.project);
        self.state.console.clear();
        self.state.path = None;
        self.state.dirty = false;
        self.history.clear();
        self.pending_before = None;
        self.suppress_history = true;
        self.state.log(LogLevel::Info, "Neues Projekt erstellt.");
        self.engine = None;
    }

    fn save(&mut self) -> bool {
        if let Some(path) = self.state.path.clone() {
            self.save_to(&path)
        } else {
            self.save_as()
        }
    }

    fn save_as(&mut self) -> bool {
        let stem = file_stem(&self.project.metadata.name);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("NØVA Projekt", &["nova"])
            .add_filter("Adobe FLA (nicht unterstützt)", &["fla"])
            .set_file_name(format!("{stem}.nova"))
            .set_title("NØVA Projekt speichern")
            .save_file()
        else {
            return false;
        };
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("fla"))
        {
            self.state.dialog = Dialog::Error {
                title: "Nativer FLA-Export nicht verfügbar".into(),
                message: "NØVA speichert Projekte als .nova. Ein .fla-Export wird nicht \
                          angeboten, weil ohne Adobe-Animate-Test keine gültige \
                          Authoring-Datei behauptet werden kann.\n\nNutze „Film exportieren“ \
                          und wähle das klar gekennzeichnete XFL-Kompatibilitätspaket (.zip)."
                    .into(),
            };
            return false;
        }
        self.save_to(&with_extension(path, "nova"))
    }

    fn save_to(&mut self, path: &std::path::Path) -> bool {
        match self.project.save(path) {
            Ok(()) => {
                autosave::discard(Some(path));
                autosave::discard(None);
                self.state.path = Some(path.to_path_buf());
                self.state.dirty = false;
                self.state.last_autosave = std::time::Instant::now();
                self.state.last_autosave_path = None;
                self.state.log(
                    LogLevel::Success,
                    format!("Gespeichert: {}", path.display()),
                );
                self.state
                    .set_status(format!("Gespeichert: {}", path.display()));
                true
            }
            Err(error) => {
                self.state.log(LogLevel::Error, error.to_string());
                self.state.dialog = Dialog::Error {
                    title: "Speichern fehlgeschlagen".into(),
                    message: format!("{}\n\n{error}", path.display()),
                };
                false
            }
        }
    }

    fn export(&mut self) {
        let stem = file_stem(&self.project.metadata.name);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SWF Film", &["swf"])
            .add_filter("XFL-Kompatibilitätspaket (.zip)", &["zip"])
            .add_filter("Adobe FLA (nicht unterstützt)", &["fla"])
            .set_file_name(format!("{stem}.swf"))
            .set_title("Film exportieren")
            .save_file()
        else {
            return;
        };
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension == "fla" {
            self.state.dialog = Dialog::Error {
                title: "Nativer FLA-Export nicht verfügbar".into(),
                message: "NØVA schreibt keine .fla-Datei, weil ein unbelegter XFL-Export \
                          nicht als Adobe-Authoring-Datei ausgegeben werden darf.\n\n\
                          Wähle stattdessen das XFL-Kompatibilitätspaket (.zip). Es ist \
                          ausdrücklich als unverifiziert gekennzeichnet und kein Ersatz \
                          für eine in Adobe Animate getestete .fla-Datei."
                    .into(),
            };
            return;
        }
        if extension == "zip" {
            self.export_xfl_package(path);
            return;
        }

        let path = with_extension(path, "swf");
        match build_swf(&self.project) {
            Ok(output) => match std::fs::write(&path, &output.bytes) {
                Ok(()) => {
                    for warning in &output.warnings {
                        self.state.log(LogLevel::Warn, warning.clone());
                    }
                    self.state.log(
                        LogLevel::Success,
                        format!(
                            "SWF geschrieben: {} ({} Bytes, {} Frames, {} Objektplatzierungen).",
                            path.display(),
                            output.bytes.len(),
                            output.stats.frames,
                            output.stats.placed_objects
                        ),
                    );
                    self.state
                        .set_status(format!("SWF exportiert: {}", path.display()));
                    self.state.last_export = Some((
                        path.clone(),
                        output.bytes.len(),
                        output.stats.frames,
                        output.warnings.clone(),
                    ));
                    self.state.dialog = Dialog::ExportDone {
                        path,
                        bytes: output.bytes.len(),
                        frames: output.stats.frames,
                        warnings: output.warnings,
                    };
                }
                Err(error) => {
                    self.state.log(LogLevel::Error, error.to_string());
                    self.state.dialog = Dialog::Error {
                        title: "SWF konnte nicht geschrieben werden".into(),
                        message: format!("{}\n\n{error}", path.display()),
                    };
                }
            },
            Err(error) => {
                self.state.log(LogLevel::Error, error.to_string());
                self.state.dialog = Dialog::Error {
                    title: "SWF-Build fehlgeschlagen".into(),
                    message: error.to_string(),
                };
            }
        }
    }

    fn export_xfl_package(&mut self, path: PathBuf) {
        match nova_formats::export_xfl(&self.project) {
            Ok(output) => match std::fs::write(&path, &output.bytes) {
                Ok(()) => {
                    for warning in &output.warnings {
                        self.state.log(LogLevel::Warn, warning.clone());
                    }
                    self.state.log(
                        LogLevel::Success,
                        format!(
                            "XFL-Kompatibilitätspaket geschrieben: {} ({} Bytes).",
                            path.display(),
                            output.bytes.len()
                        ),
                    );
                    self.state
                        .set_status(format!("XFL-Paket exportiert: {}", path.display()));
                    self.state.dialog = Dialog::CompatibilityReport {
                        title: format!("XFL-Paket exportiert: {}", path.display()),
                        report: output.report.to_text(),
                        is_error: false,
                    };
                }
                Err(error) => {
                    self.state.log(LogLevel::Error, error.to_string());
                    self.state.dialog = Dialog::Error {
                        title: "XFL-Paket konnte nicht geschrieben werden".into(),
                        message: format!("{}\n\n{error}", path.display()),
                    };
                }
            },
            Err(error) => {
                self.state.log(LogLevel::Error, error.to_string());
                self.state.dialog = Dialog::Error {
                    title: "XFL-Export fehlgeschlagen".into(),
                    message: error.to_string(),
                };
            }
        }
    }

    fn open_flash_settings_dialog(&mut self) {
        self.state.dialog = Dialog::FlashSettings {
            default_runtime: self.flash_settings.default_runtime,
            adobe_path: self
                .flash_settings
                .adobe_projector_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            ruffle_path: self
                .flash_settings
                .ruffle_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            first_launch_setup_done: self.flash_settings.first_launch_setup_done,
            last_validation: self.flash_settings.last_validation.clone(),
            test_result: None,
            error: None,
        };
    }

    /// Persist that first-launch setup was deliberately postponed. The app stays
    /// usable; opening a SWF will show a configuration error until a runtime is
    /// selected in the Runtime settings.
    fn postpone_runtime_setup(&mut self) {
        let mut draft = self.flash_settings.clone();
        draft.first_launch_setup_done = true;
        match flash::save_settings(&draft) {
            Ok(()) => {
                self.flash_settings = draft;
                let pending = self.pending_swf.take();
                self.state.log(
                    LogLevel::Warn,
                    "Runtime-Einrichtung übersprungen. SWF-Wiedergabe benötigt eine konfigurierte Runtime.",
                );
                if let Some(swf) = pending {
                    self.state.log(
                        LogLevel::Warn,
                        format!(
                            "Die Startdatei wurde nicht geöffnet: {}. Konfiguriere zuerst eine Runtime.",
                            swf.display()
                        ),
                    );
                }
                self.state.set_status("Runtime später konfigurieren");
                self.state.dialog = Dialog::None;
            }
            Err(error) => {
                self.state.log(LogLevel::Error, error.to_string());
                self.state.dialog = Dialog::Error {
                    title: "Runtime-Konfiguration konnte nicht gespeichert werden".into(),
                    message: error.to_string(),
                };
            }
        }
    }

    /// Find the most recent SWF. The export path is authoritative; if this
    /// session has not exported yet, a sibling `.swf` next to a saved project is
    /// accepted so reopening a project still works.
    fn swf_for_player(&self) -> Option<PathBuf> {
        if let Some((path, _, _, _)) = &self.state.last_export {
            return Some(path.clone());
        }
        let candidate = self.state.path.as_ref()?.with_extension("swf");
        candidate.is_file().then_some(candidate)
    }

    fn open_in_flash_player(&mut self) {
        let Some(swf) = self.swf_for_player() else {
            let message = "Es wurde noch keine SWF-Datei exportiert.\n\nNutze „SWF exportieren“, um eine Szene als Film zu schreiben. Danach kann sie direkt im Flash Player geöffnet werden.".to_string();
            self.state.log(LogLevel::Warn, &message);
            self.state.dialog = Dialog::Error {
                title: "Keine SWF-Datei vorhanden".into(),
                message,
            };
            return;
        };

        self.open_swf_path(&swf);
    }

    /// Open a SWF passed by a shell association or `--open`. This uses exactly
    /// the same configured-runtime path as the toolbar action.
    fn open_pending_swf(&mut self) {
        if let Some(swf) = self.pending_swf.take() {
            self.open_swf_path(&swf);
        }
    }

    fn open_swf_path(&mut self, swf: &Path) {
        match flash::open_swf(swf, &self.flash_settings) {
            Ok(report) => {
                self.state.log(LogLevel::Info, report.message());
                self.state.set_status(format!(
                    "{} gestartet; Wiedergabe nicht verifiziert: {}",
                    report.runtime.short_label(),
                    swf.display()
                ));
            }
            Err(error) => {
                self.state.log(LogLevel::Error, error.to_string());
                let (title, hint) = if error.is_swf_problem() {
                    (
                        "SWF konnte nicht geöffnet werden",
                        "Prüfe den SWF-Pfad und exportiere die Szene bei Bedarf erneut.",
                    )
                } else {
                    (
                        "Runtime konnte nicht gestartet werden",
                        "Öffne Flash Player → Runtime-Einstellungen …, um den Adobe Flash Player Projector zu konfigurieren oder Ruffle ausdrücklich auszuwählen.",
                    )
                };
                self.state.dialog = Dialog::Error {
                    title: title.into(),
                    message: format!("{error}\n\nDatei: {}\n\n{hint}", swf.display()),
                };
            }
        }
    }

    fn add_layer(&mut self) {
        let (count, name) = self
            .project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| {
                (
                    scene.frame_count(),
                    format!("Ebene {}", scene.layers.len() + 1),
                )
            })
            .unwrap_or((1, "Ebene 1".to_string()));
        if let Some(id) = self.project.add_layer(name.clone(), count) {
            self.state.selection = Selection::Layer(id);
            self.state.dirty = true;
            self.state.mode = Mode::Animation;
            self.state
                .log(LogLevel::Info, format!("Ebene '{name}' angelegt."));
        }
    }

    fn add_folder(&mut self) {
        let (count, name) = self
            .project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| {
                let folders = scene
                    .layers
                    .iter()
                    .filter(|layer| layer.kind == LayerKind::Folder)
                    .count();
                (
                    scene.frame_count().max(1),
                    format!("Ordner {}", folders + 1),
                )
            })
            .unwrap_or((1, "Ordner 1".to_string()));
        let mut layer = Layer::new(name.clone(), count);
        layer.kind = LayerKind::Folder;
        let id = layer.id;
        if let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) {
            scene.layers.push(layer);
        } else {
            return;
        }
        self.project.touch();
        self.state.selection = Selection::Layer(id);
        self.state.mode = Mode::Animation;
        self.state.dirty = true;
        self.state
            .log(LogLevel::Info, format!("Ordnerebene '{name}' angelegt."));
    }

    fn add_scene(&mut self) {
        let name = format!("Szene {}", self.project.scenes.len() + 1);
        self.project.add_scene(name.clone());
        self.state.reset_view(&self.project);
        self.state.selection = Selection::Scene;
        self.state.dirty = true;
        self.state
            .log(LogLevel::Info, format!("Szene '{name}' angelegt."));
    }

    fn delete_scene(&mut self) {
        if self.project.scenes.len() <= 1 {
            self.state.log(
                LogLevel::Warn,
                "Die letzte Szene kann nicht gelöscht werden.",
            );
            return;
        }
        let index = self.project.active_scene_index();
        let removed = self.project.scenes.remove(index);
        let next = index.min(self.project.scenes.len().saturating_sub(1));
        self.project.active_scene = self.project.scenes.get(next).map(|scene| scene.id);
        self.state.reset_view(&self.project);
        self.state.selection = Selection::Scene;
        self.project.touch();
        self.state.dirty = true;
        self.state.log(
            LogLevel::Info,
            format!("Szene '{}' gelöscht.", removed.name),
        );
    }

    fn add_script(&mut self) {
        let id = self.project.add_script("Script");
        self.state.active_script = Some(id);
        self.state.selection = Selection::None;
        self.state.mode = Mode::Code;
        self.state.dirty = true;
        if let Some(path) = self.project.script_path(id) {
            self.state
                .log(LogLevel::Info, format!("Skript '{path}' angelegt."));
        }
    }

    fn add_shape(&mut self, ellipse: bool) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            self.state
                .log(LogLevel::Error, "Keine Ebene zum Einfügen vorhanden.");
            return;
        };
        let color = self.state.new_object_color;
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let (stage_width, stage_height) = (scene.width, scene.height);
        scene.ensure_frame_count(frame + 1);
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };

        if !layer.frames[frame].keyframe {
            let previous = if frame > 0 {
                layer.last_keyframe_at_or_before(frame - 1)
            } else {
                None
            };
            let carried = previous
                .and_then(|index| layer.frames.get(index))
                .map(|key| key.objects.clone())
                .unwrap_or_default();
            layer.frames[frame].keyframe = true;
            layer.frames[frame].objects = carried;
        }

        let name = if ellipse { "Ellipse" } else { "Rechteck" };
        let object = if ellipse {
            ObjectInstance::ellipse(
                name,
                stage_width / 2.0,
                stage_height / 2.0,
                120.0,
                80.0,
                color,
            )
        } else {
            ObjectInstance::rect(
                name,
                stage_width / 2.0,
                stage_height / 2.0,
                140.0,
                90.0,
                color,
            )
        };
        let id = object.id;
        layer.frames[frame].objects.push(object);
        self.project.touch();
        self.state.dirty = true;
        self.state.selection = Selection::object(id);
        self.state.mode = Mode::Animation;
        self.state.set_status(format!("{name} eingefügt"));
    }

    fn add_text(&mut self) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            self.state
                .log(LogLevel::Error, "Keine Ebene zum Einfügen vorhanden.");
            return;
        };
        let color = self.state.new_object_color;
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let (stage_width, stage_height) = (scene.width, scene.height);
        scene.ensure_frame_count(frame + 1);
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        if !layer.frames[frame].keyframe {
            let previous = if frame > 0 {
                layer.last_keyframe_at_or_before(frame - 1)
            } else {
                None
            };
            let carried = previous
                .and_then(|index| layer.frames.get(index))
                .map(|key| key.objects.clone())
                .unwrap_or_default();
            layer.frames[frame].keyframe = true;
            layer.frames[frame].objects = carried;
        }
        let object = ObjectInstance::text(
            "Text",
            stage_width / 2.0,
            stage_height / 2.0,
            "Text",
            48.0,
            color,
        );
        let id = object.id;
        layer.frames[frame].objects.push(object);
        self.project.touch();
        self.state.dirty = true;
        self.state.selection = Selection::object(id);
        self.state.mode = Mode::Animation;
        self.state.set_status("Text eingefügt");
    }

    fn add_symbol_instance(&mut self, symbol_id: Uuid) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            return;
        };
        let Some(name) = self
            .project
            .symbol(symbol_id)
            .map(|symbol| symbol.name.clone())
        else {
            return;
        };
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let (stage_width, stage_height) = (scene.width, scene.height);
        scene.ensure_frame_count(frame + 1);
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };

        if !layer.frames[frame].keyframe {
            let previous = if frame > 0 {
                layer.last_keyframe_at_or_before(frame - 1)
            } else {
                None
            };
            let carried = previous
                .and_then(|index| layer.frames.get(index))
                .map(|key| key.objects.clone())
                .unwrap_or_default();
            layer.frames[frame].keyframe = true;
            layer.frames[frame].objects = carried;
        }

        let object = ObjectInstance::symbol(
            format!("{name} Instanz"),
            stage_width / 2.0,
            stage_height / 2.0,
            symbol_id,
            Rgba::WHITE,
        );
        let id = object.id;
        layer.frames[frame].objects.push(object);
        self.project.touch();
        self.state.dirty = true;
        self.state.selection = Selection::object(id);
        self.state.mode = Mode::Animation;
        self.state
            .set_status(format!("Symbol '{name}' auf die Bühne gelegt"));
    }

    fn add_symbol(&mut self) {
        self.add_symbol_kind(SymbolKind::Graphic);
    }

    fn add_symbol_kind(&mut self, kind: SymbolKind) {
        let index = self.project.symbols.len() + 1;
        let name = format!("{} {index}", kind.label());
        let (width, height) = self
            .project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| (scene.width, scene.height))
            .unwrap_or((550.0, 400.0));
        let mut symbol = Symbol::empty(name.clone(), kind, width, height);
        symbol.fill = self.state.new_object_color;
        if kind == SymbolKind::Graphic
            && let Some(layer) = symbol.timeline.layers.first_mut()
            && let Some(object) = layer.frames[0].objects.first_mut()
            && let ObjectKind::Rect { .. } = object.kind
        {
            object.fill = self.state.new_object_color;
        }
        let id = symbol.id;
        self.project.symbols.push(symbol);
        self.project.touch();
        self.state.dirty = true;
        self.state.selection = Selection::Symbol(id);
        self.state.selected_symbol = Some(id);
        self.last_history_label = "Symbol anlegen";
        self.state
            .set_status(format!("{name} ({}) angelegt", kind.label()));
    }

    fn enter_symbol(&mut self, id: Uuid) {
        if self.project.symbol(id).is_none() {
            return;
        }
        let name = self
            .project
            .symbol(id)
            .map(|symbol| symbol.name.clone())
            .unwrap_or_default();
        self.state.enter_symbol(&self.project, id);
        self.project.touch();
        self.state.set_status(format!("Symbolbearbeitung: {name}"));
    }

    fn exit_symbol(&mut self) {
        if self.state.edit_symbol.is_none() {
            return;
        }
        self.state.exit_symbol(&self.project);
        self.project.touch();
        self.state.set_status("Zurück zur Hauptszene");
    }

    fn duplicate_symbol(&mut self, id: Uuid) {
        match self.project.duplicate_symbol(id) {
            Some(new_id) => {
                self.project.touch();
                self.state.dirty = true;
                self.state.selection = Selection::Symbol(new_id);
                self.state.selected_symbol = Some(new_id);
                self.last_history_label = "Symbol duplizieren";
                self.state.set_status("Symbol dupliziert");
            }
            None => self
                .state
                .log(LogLevel::Warn, "Symbol konnte nicht dupliziert werden."),
        }
    }

    fn rename_symbol(&mut self, id: Uuid, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(symbol) = self.project.symbol_mut(id) else {
            return;
        };
        if symbol.name == name {
            return;
        }
        symbol.name = name.clone();
        symbol.timeline.name = name.clone();
        self.project.touch();
        self.state.dirty = true;
        self.last_history_label = "Symbol umbenennen";
        self.state
            .set_status(format!("Symbol in '{name}' umbenannt"));
    }

    fn set_tool(&mut self, tool: Tool) {
        self.state.tool = tool;
        self.state.draft = None;
        self.state.set_status(format!("Werkzeug: {}", tool.label()));
    }

    fn set_layer_kind(&mut self, kind: LayerKind) {
        let Some(target) = self.target_layer() else {
            self.state.log(LogLevel::Warn, "Keine Ebene ausgewählt.");
            return;
        };
        let Some(layer_index) = self
            .project
            .edit_scene(self.state.edit_symbol)
            .and_then(|scene| scene.layers.iter().position(|layer| layer.id == target))
        else {
            return;
        };
        if kind == LayerKind::Mask
            && self
                .project
                .edit_scene(self.state.edit_symbol)
                .is_some_and(|scene| scene.mask_target_index(layer_index).is_none())
        {
            self.state.log(
                LogLevel::Warn,
                "Maske braucht eine sichtbare Ebene darunter.",
            );
            return;
        }
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        if layer.kind == kind {
            return;
        }
        layer.kind = kind;
        self.project.touch();
        self.state.dirty = true;
        self.last_history_label = "Ebenentyp ändern";
        self.state
            .set_status(format!("Ebene ist jetzt {}", kind.label()));
    }

    fn set_tween_kind(&mut self, kind: TweenKind) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            return;
        };
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        let Some(start) = layer.last_keyframe_at_or_before(frame) else {
            self.state
                .log(LogLevel::Warn, "Kein Schlüsselbild für den Tween-Typ.");
            return;
        };
        let tween = layer.frames[start]
            .tween
            .get_or_insert_with(nova_core::Tween::default);
        tween.kind = kind;
        self.project.touch();
        self.state.dirty = true;
        self.last_history_label = "Tween-Typ ändern";
        self.state
            .set_status(format!("Tween-Typ: {}", kind.label()));
    }

    fn import_bitmap(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Bitmap", &["png", "jpg", "jpeg", "gif"])
            .pick_file()
        else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            self.state.log(
                LogLevel::Error,
                format!("Datei konnte nicht gelesen werden: {}", path.display()),
            );
            return;
        };
        let image = match image::load_from_memory(&bytes) {
            Ok(image) => image,
            Err(error) => {
                self.state.log(
                    LogLevel::Error,
                    format!("Bild konnte nicht dekodiert werden: {error}"),
                );
                return;
            }
        };
        let (width, height) = (image.width(), image.height());
        let mime = match path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "gif" => "image/gif",
            _ => "image/jpeg",
        };
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Bitmap")
            .to_string();
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("asset")
            .to_string();
        let asset = Asset::bitmap(name.clone(), file, mime, width, height, bytes);
        let id = asset.id;
        self.project.assets.push(asset);
        self.project.touch();
        self.state.dirty = true;
        self.state.selection = Selection::Asset(id);
        self.state.selected_asset = Some(id);
        self.state.asset_textures.remove(&id);
        self.last_history_label = "Bitmap importieren";
        self.state
            .set_status(format!("{name} importiert ({width}×{height})"));
    }

    fn import_audio(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "mp3"])
            .pick_file()
        else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            self.state.log(
                LogLevel::Error,
                format!("Datei konnte nicht gelesen werden: {}", path.display()),
            );
            return;
        };
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let (mime, sample_rate, channels, duration) = if extension == "wav" {
            match wav_info(&bytes) {
                Some(info) => ("audio/wav", info.0, info.1, info.2),
                None => {
                    self.state
                        .log(LogLevel::Error, "Keine gültige WAV-Datei (PCM erwartet).");
                    return;
                }
            }
        } else {
            ("audio/mpeg", 0, 0, 0.0)
        };
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Audio")
            .to_string();
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("asset")
            .to_string();
        let asset = Asset::audio(
            name.clone(),
            file,
            mime,
            sample_rate,
            channels,
            duration,
            bytes,
        );
        let id = asset.id;
        self.project.assets.push(asset);
        self.project.touch();
        self.state.dirty = true;
        self.state.selection = Selection::Asset(id);
        self.state.selected_asset = Some(id);
        self.last_history_label = "Audio importieren";
        self.state.set_status(format!(
            "{name} importiert{}",
            if duration > 0.0 {
                format!(" ({duration:.1} s)")
            } else {
                String::new()
            }
        ));
    }

    /// Places an embedded bitmap asset as an `ObjectKind::Bitmap` instance at
    /// the centre of the stage (or symbol timeline) currently being edited.
    fn add_bitmap_instance(&mut self, asset_id: Uuid) {
        let Some(asset) = self.project.asset(asset_id).cloned() else {
            return;
        };
        if asset.kind != AssetKind::Bitmap || !asset.is_embedded() {
            self.state
                .log(LogLevel::Warn, "Asset ist kein eingebettetes Bitmap.");
            return;
        }
        let (width, height) = (asset.width.max(1) as f32, asset.height.max(1) as f32);
        let (cx, cy) = self
            .project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| (scene.width / 2.0, scene.height / 2.0))
            .unwrap_or((275.0, 200.0));
        let object = ObjectInstance::bitmap(asset.name.clone(), cx, cy, asset_id, width, height);
        self.insert_objects(vec![object], "Bitmap einfügen");
    }

    /// Attaches a sound cue to the current frame of a sound layer. A dedicated
    /// `Sound` layer is created when none exists yet.
    fn add_sound_cue(&mut self, asset_id: Uuid) {
        let Some(asset) = self.project.asset(asset_id).cloned() else {
            return;
        };
        if asset.kind != AssetKind::Audio || !asset.is_embedded() {
            self.state
                .log(LogLevel::Warn, "Asset ist kein eingebettetes Audio.");
            return;
        }
        let frame_index = self.state.frame;
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let frame_count = scene.frame_count();
        scene.ensure_frame_count(frame_index + 1);
        let sound_layer = scene
            .layers
            .iter_mut()
            .find(|layer| layer.kind == LayerKind::Sound)
            .map(|layer| layer.id);
        let layer_id = match sound_layer {
            Some(id) => Some(id),
            None => {
                let mut layer = Layer::new("Sound", frame_count.max(1));
                layer.kind = LayerKind::Sound;
                layer.ensure_frames(frame_count.max(frame_index + 1));
                scene.layers.push(layer);
                scene.layers.last().map(|layer| layer.id)
            }
        };
        let Some(layer_id) = layer_id else {
            return;
        };
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };
        layer.ensure_frames(frame_index + 1);
        layer.frames[frame_index].keyframe = true;
        layer.frames[frame_index].sound = Some(SoundCue::event(asset_id));
        self.project.touch();
        self.state.dirty = true;
        self.last_history_label = "Sound setzen";
        self.state.set_status(format!(
            "Sound '{}' auf Frame {}",
            asset.name,
            frame_index + 1
        ));
    }

    fn asset_is_used(&self, id: Uuid) -> bool {
        for scene in self
            .project
            .scenes
            .iter()
            .chain(self.project.symbols.iter().map(|symbol| &symbol.timeline))
        {
            for layer in &scene.layers {
                for frame in &layer.frames {
                    if frame.sound.is_some_and(|cue| cue.asset == id) {
                        return true;
                    }
                    for object in &frame.objects {
                        if matches!(object.kind, ObjectKind::Bitmap { asset, .. } if asset == id) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Plays an embedded audio asset through the Windows `PlaySound` API.
    /// Non-Windows builds and undecodable data report honestly instead of
    /// pretending to play.
    fn preview_asset(&mut self, id: Uuid) {
        let Some(asset) = self.project.asset(id).cloned() else {
            return;
        };
        if asset.kind != AssetKind::Audio {
            self.state.log(
                LogLevel::Warn,
                "Vorschau ist nur für Audio-Assets verfügbar.",
            );
            return;
        }
        #[cfg(windows)]
        {
            match play_sound_bytes(&asset.data, &asset.mime) {
                Ok(()) => {
                    self.state
                        .set_status(format!("Audio-Vorschau: {}", asset.name));
                    return;
                }
                Err(error) => {
                    self.state.log(
                        LogLevel::Warn,
                        format!("Audiovorschau nicht möglich: {error}"),
                    );
                    return;
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = asset;
            self.state
                .log(LogLevel::Warn, "Audiovorschau ist unter Windows verfügbar.");
        }
    }

    fn target_layer(&self) -> Option<Uuid> {
        match &self.state.selection {
            Selection::Layer(id) => Some(*id),
            Selection::Objects(ids) => ids
                .first()
                .and_then(|id| self.state.layer_of_object(&self.project, *id)),
            _ => self
                .project
                .edit_scene(self.state.edit_symbol)
                .and_then(|scene| scene.layers.last().map(|layer| layer.id)),
        }
    }

    fn insert_keyframe(&mut self, blank: bool) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            return;
        };
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        scene.ensure_frame_count(frame + 1);
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };

        if !blank && !layer.frames[frame].keyframe {
            let previous = if frame > 0 {
                layer.last_keyframe_at_or_before(frame - 1)
            } else {
                None
            };
            let carried = previous
                .and_then(|index| layer.frames.get(index))
                .map(|key| key.objects.clone())
                .unwrap_or_default();
            layer.frames[frame].objects = carried;
        }
        if blank {
            layer.frames[frame].objects.clear();
            layer.frames[frame].label = None;
        }
        layer.frames[frame].keyframe = true;
        layer.frames[frame].tween = None;

        self.project.touch();
        self.state.dirty = true;
        self.state.selection = Selection::Layer(target);
        self.state.set_status(if blank {
            "Leeres Schlüsselbild eingefügt"
        } else {
            "Schlüsselbild eingefügt"
        });
    }

    fn remove_keyframe(&mut self) {
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            return;
        };
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        if frame >= layer.frames.len() {
            return;
        }
        if frame == 0 {
            self.state.log(
                LogLevel::Warn,
                "Das erste Schlüsselbild kann nicht entfernt werden.",
            );
            return;
        }
        layer.frames[frame].keyframe = false;
        layer.frames[frame].objects.clear();
        layer.frames[frame].tween = None;
        layer.frames[frame].label = None;
        self.project.touch();
        self.state.dirty = true;
        self.state.set_status("Schlüsselbild entfernt");
    }

    fn copy_objects(&mut self) {
        let ids = self.state.selection.object_ids().to_vec();
        if ids.is_empty() {
            self.state.log(
                LogLevel::Warn,
                "Keine Objekte für die Zwischenablage ausgewählt.",
            );
            return;
        }
        let Some(scene) = self.project.edit_scene(self.state.edit_symbol) else {
            return;
        };
        let objects: Vec<ObjectInstance> = nova_core::resolve_scene(scene, self.state.frame)
            .into_iter()
            .filter(|object| ids.contains(&object.instance.id))
            .map(|object| object.instance)
            .collect();
        if objects.is_empty() {
            self.state.log(
                LogLevel::Warn,
                "Die Auswahl ist in diesem Frame nicht sichtbar.",
            );
            return;
        }
        let count = objects.len();
        self.state.object_clipboard = objects;
        self.state.keyframe_clipboard = None;
        self.state.set_status(format!("{count} Objekt(e) kopiert"));
    }

    fn insert_objects(&mut self, mut objects: Vec<ObjectInstance>, label: &'static str) {
        if objects.is_empty() {
            return;
        }
        let frame = self.state.frame;
        let Some(target) = self.target_layer() else {
            self.state
                .log(LogLevel::Error, "Keine Ebene zum Einfügen vorhanden.");
            return;
        };
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        scene.ensure_frame_count(frame + 1);
        let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == target) else {
            return;
        };
        if !layer.frames[frame].keyframe {
            let previous = if frame > 0 {
                layer.last_keyframe_at_or_before(frame - 1)
            } else {
                None
            };
            let carried = previous
                .and_then(|index| layer.frames.get(index))
                .map(|key| key.objects.clone())
                .unwrap_or_default();
            layer.frames[frame].keyframe = true;
            layer.frames[frame].objects = carried;
        }
        let mut ids = Vec::with_capacity(objects.len());
        for object in &mut objects {
            object.id = Uuid::new_v4();
            object.script = None;
            object.source_character = None;
            ids.push(object.id);
        }
        let count = objects.len();
        layer.frames[frame].objects.extend(objects);
        self.project.touch();
        self.state.dirty = true;
        self.state.selection = Selection::Objects(ids);
        self.state.mode = Mode::Animation;
        self.last_history_label = label;
        self.state
            .set_status(format!("{count} Objekt(e) eingefügt"));
    }

    fn paste_objects(&mut self) {
        if self.state.object_clipboard.is_empty() {
            self.state
                .log(LogLevel::Warn, "Die Objekt-Zwischenablage ist leer.");
            return;
        }
        let offset = 20.0;
        let mut objects = self.state.object_clipboard.clone();
        for object in &mut objects {
            object.name = format!("{} Kopie", object.name);
            object.transform.x += offset;
            object.transform.y += offset;
        }
        self.insert_objects(objects, "Objekte einfügen");
        for source in &mut self.state.object_clipboard {
            source.transform.x += offset;
            source.transform.y += offset;
        }
    }

    fn duplicate_objects(&mut self) {
        let ids = self.state.selection.object_ids().to_vec();
        if ids.is_empty() {
            return;
        }
        let Some(scene) = self.project.edit_scene(self.state.edit_symbol) else {
            return;
        };
        let mut objects: Vec<ObjectInstance> = nova_core::resolve_scene(scene, self.state.frame)
            .into_iter()
            .filter(|object| ids.contains(&object.instance.id))
            .map(|object| object.instance)
            .collect();
        for object in &mut objects {
            object.name = format!("{} Kopie", object.name);
            object.transform.x += 20.0;
            object.transform.y += 20.0;
        }
        self.insert_objects(objects, "Objekte duplizieren");
    }

    fn align_selection(&mut self, mode: AlignMode) {
        let ids = self.state.selection.object_ids().to_vec();
        if ids.is_empty() {
            return;
        }
        let mut targets = Vec::new();
        for id in ids {
            let Some(resolved) = self.state.resolved_object(&self.project, id) else {
                continue;
            };
            let Some((width, height)) =
                util::object_size(&resolved.instance, &self.project.symbols)
            else {
                continue;
            };
            targets.push((
                id,
                resolved.layer_id,
                resolved.instance.transform,
                width * resolved.instance.transform.scale_x.abs(),
                height * resolved.instance.transform.scale_y.abs(),
            ));
        }
        if targets.is_empty() {
            return;
        }

        let min_left = targets
            .iter()
            .map(|(_, _, transform, width, _)| transform.x - width / 2.0)
            .fold(f32::INFINITY, f32::min);
        let max_right = targets
            .iter()
            .map(|(_, _, transform, width, _)| transform.x + width / 2.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_top = targets
            .iter()
            .map(|(_, _, transform, _, height)| transform.y - height / 2.0)
            .fold(f32::INFINITY, f32::min);
        let max_bottom = targets
            .iter()
            .map(|(_, _, transform, _, height)| transform.y + height / 2.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let center_x = (min_left + max_right) / 2.0;
        let center_y = (min_top + max_bottom) / 2.0;

        let mut positions: Vec<(Uuid, Uuid, f32, f32)> = Vec::new();
        if matches!(
            mode,
            AlignMode::DistributeHorizontal | AlignMode::DistributeVertical
        ) {
            if targets.len() < 3 {
                self.state.log(
                    LogLevel::Warn,
                    "Verteilen benötigt mindestens drei Objekte.",
                );
                return;
            }
            let horizontal = mode == AlignMode::DistributeHorizontal;
            let mut sorted = targets;
            sorted.sort_by(|a, b| {
                let a_start = if horizontal {
                    a.2.x - a.3 / 2.0
                } else {
                    a.2.y - a.4 / 2.0
                };
                let b_start = if horizontal {
                    b.2.x - b.3 / 2.0
                } else {
                    b.2.y - b.4 / 2.0
                };
                a_start
                    .partial_cmp(&b_start)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let group_start = if horizontal { min_left } else { min_top };
            let group_end = if horizontal { max_right } else { max_bottom };
            let total: f32 = sorted
                .iter()
                .map(|(_, _, _, width, height)| if horizontal { *width } else { *height })
                .sum();
            let gap = ((group_end - group_start - total) / (sorted.len() - 1) as f32).max(0.0);
            let mut cursor = group_start;
            for (id, layer, transform, width, height) in sorted {
                if horizontal {
                    positions.push((id, layer, cursor + width / 2.0, transform.y));
                    cursor += width + gap;
                } else {
                    positions.push((id, layer, transform.x, cursor + height / 2.0));
                    cursor += height + gap;
                }
            }
        } else {
            for (id, layer, transform, width, height) in targets {
                let (x, y) = match mode {
                    AlignMode::Left => (min_left + width / 2.0, transform.y),
                    AlignMode::HorizontalCenter => (center_x, transform.y),
                    AlignMode::Right => (max_right - width / 2.0, transform.y),
                    AlignMode::Top => (transform.x, min_top + height / 2.0),
                    AlignMode::VerticalCenter => (transform.x, center_y),
                    AlignMode::Bottom => (transform.x, max_bottom - height / 2.0),
                    AlignMode::DistributeHorizontal | AlignMode::DistributeVertical => {
                        unreachable!()
                    }
                };
                positions.push((id, layer, x, y));
            }
        }

        let frame = self.state.frame;
        let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
            return;
        };
        for (id, layer_id, x, y) in positions {
            if let Some(object) = util::governing_object_mut(scene, layer_id, frame, id) {
                object.transform.x = x;
                object.transform.y = y;
            }
        }
        self.project.touch();
        self.state.dirty = true;
        self.last_history_label = "Objekte ausrichten";
        self.state.set_status(format!("{mode:?} ausgeführt"));
    }

    fn delete_selection(&mut self) {
        match self.state.selection.clone() {
            Selection::Objects(ids) => {
                if ids.is_empty() {
                    return;
                }
                let selected: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
                let mut removed = 0usize;
                if let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) {
                    for layer in &mut scene.layers {
                        for frame in &mut layer.frames {
                            let before = frame.objects.len();
                            frame
                                .objects
                                .retain(|object| !selected.contains(&object.id));
                            removed += before - frame.objects.len();
                        }
                    }
                }
                if removed > 0 {
                    self.project.touch();
                    self.state.dirty = true;
                    self.state.selection = Selection::None;
                    self.state.selected_keyframes.clear();
                    self.state.log(
                        LogLevel::Info,
                        format!(
                            "{} Objekt(e) aus {removed} Frame-Fassung(en) entfernt.",
                            selected.len()
                        ),
                    );
                }
            }
            Selection::Layer(id) => {
                let Some(scene) = self.project.edit_scene_mut(self.state.edit_symbol) else {
                    return;
                };
                if !scene.remove_layer(id) {
                    self.state.log(
                        LogLevel::Warn,
                        "Die letzte Ebene kann nicht gelöscht werden.",
                    );
                    return;
                }
                self.project.touch();
                self.state.dirty = true;
                self.state.selection = Selection::None;
                self.state.log(LogLevel::Info, "Ebene gelöscht.");
            }
            Selection::Symbol(id) => {
                let used = self.project.symbol_is_used(id);
                if used {
                    self.state.log(
                        LogLevel::Error,
                        "Symbol wird noch verwendet und kann nicht gelöscht werden.",
                    );
                    self.state.set_status("Symbol ist noch in Benutzung");
                    return;
                }
                self.project.symbols.retain(|symbol| symbol.id != id);
                self.project.touch();
                self.state.dirty = true;
                self.state.selection = Selection::None;
                if self.state.selected_symbol == Some(id) {
                    self.state.selected_symbol =
                        self.project.symbols.first().map(|symbol| symbol.id);
                }
                self.last_history_label = "Symbol löschen";
                self.state.log(LogLevel::Info, "Symbol gelöscht.");
            }
            Selection::Asset(id) => {
                let used = self.asset_is_used(id);
                if used {
                    self.state.log(
                        LogLevel::Error,
                        "Asset wird noch verwendet und kann nicht gelöscht werden.",
                    );
                    self.state.set_status("Asset ist noch in Benutzung");
                    return;
                }
                self.project.assets.retain(|asset| asset.id != id);
                self.state.asset_textures.remove(&id);
                self.project.touch();
                self.state.dirty = true;
                self.state.selection = Selection::None;
                self.state.selected_asset = self.project.assets.first().map(|asset| asset.id);
                self.last_history_label = "Asset löschen";
                self.state.log(LogLevel::Info, "Asset gelöscht.");
            }
            Selection::Scene | Selection::None => {
                self.state
                    .log(LogLevel::Info, "Nichts zu löschen ausgewählt.");
            }
        }
    }

    fn check_script(&mut self, id: Uuid) {
        let Some(script) = self.project.script(id) else {
            return;
        };
        let source = script.source.clone();
        let (_, diagnostics) = nova_as::analyze(&source);
        let errors = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == nova_as::Severity::Error)
            .count();
        code_view::report_diagnostics(&mut self.state, id, diagnostics, true);
        if errors > 0 {
            self.state.bottom_tab = BottomTab::Problems;
        }
    }

    fn open_script(&mut self, id: Uuid) {
        self.state.active_script = Some(id);
        self.state.mode = Mode::Code;
        self.state.selection = Selection::None;
        self.state.playing = false;
        if let Some(path) = self.project.script_path(id) {
            self.state.set_status(format!("Skript geöffnet: {path}"));
        }
        if self.state.auto_check && self.state.diagnostics_for != Some(id) {
            self.check_script(id);
        }
    }

    fn toggle_play(&mut self) {
        if self.state.playing {
            self.state.playing = false;
            self.state.set_status("Wiedergabe pausiert");
            return;
        }
        let count = self
            .project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| scene.frame_count())
            .unwrap_or(1)
            .max(1);
        if self.state.frame >= count - 1 {
            self.state.frame = 0;
        }
        self.state.playing = true;
        self.state.play_accumulator = 0.0;
        self.state.mode = Mode::Animation;
        self.state.set_status("Wiedergabe läuft");
    }

    fn step(&mut self, delta: i64) {
        let count = self
            .project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| scene.frame_count())
            .unwrap_or(1)
            .max(1) as i64;
        let next = (self.state.frame as i64 + delta).rem_euclid(count);
        self.state.frame = next as usize;
        self.state.playing = false;
        self.state
            .set_status(format!("Frame {}", self.state.frame + 1));
    }

    fn jump_to_frame(&mut self, frame: usize) {
        let last = self
            .project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| scene.frame_count().saturating_sub(1))
            .unwrap_or(0);
        self.state.frame = frame.min(last);
        self.state.playing = false;
        self.state
            .set_status(format!("Frame {}", self.state.frame + 1));
    }

    fn update_playback(&mut self, ctx: &Context) {
        if !self.state.playing {
            return;
        }
        let (count, fps) = self
            .project
            .edit_scene(self.state.edit_symbol)
            .map(|scene| (scene.frame_count().max(1), scene.fps.max(1.0)))
            .unwrap_or((1, 24.0));
        let dt = ctx.input(|input| input.stable_dt).min(0.25);
        self.state.play_accumulator += dt * fps;
        let steps = self.state.play_accumulator.floor() as usize;
        if steps > 0 {
            self.state.play_accumulator -= steps as f32;
            let next = self.state.frame + steps;
            if next < count {
                self.state.frame = next;
            } else if self.state.loop_playback {
                self.state.frame = next % count;
            } else {
                self.state.frame = count - 1;
                self.state.playing = false;
            }
        }
        ctx.request_repaint();
    }

    // ------------------------------------------------------------ dialogs

    fn dialogs(&mut self, ctx: &Context) {
        if !matches!(self.state.dialog, Dialog::None)
            && ctx.input(|input| input.key_pressed(Key::Escape))
        {
            if matches!(self.state.dialog, Dialog::RuntimeSetup { .. }) {
                self.postpone_runtime_setup();
            } else {
                self.state.dialog = Dialog::None;
            }
            return;
        }

        match self.state.dialog.clone() {
            Dialog::None => {}
            Dialog::AiProviders => self.ai.show_provider_dialog(ctx, &mut self.state),
            Dialog::NewProject {
                mut name,
                mut template,
            } => {
                let mut create = false;
                let mut cancel = false;
                egui::Modal::new(egui::Id::unique("new_project_dialog")).show(ctx, |ui| {
                    ui.set_min_width(430.0);
                    ui.heading("Neues Projekt");
                    ui.label(
                        RichText::new("Vorlage wählen und Projektnamen festlegen.")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(6.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("Projektname")
                            .desired_width(f32::INFINITY),
                    );
                    if !response.has_focus() {
                        response.request_focus();
                    }
                    ui.add_space(8.0);
                    egui::Grid::new("template_grid")
                        .num_columns(2)
                        .spacing(egui::vec2(12.0, 8.0))
                        .show(ui, |ui| {
                            for candidate in ProjectTemplate::ALL {
                                ui.radio_value(&mut template, candidate, candidate.label());
                                ui.label(
                                    RichText::new(candidate.description())
                                        .small()
                                        .color(theme::TEXT_DIM),
                                );
                                ui.end_row();
                            }
                        });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!name.trim().is_empty(), egui::Button::new("Anlegen"))
                            .clicked()
                        {
                            create = true;
                        }
                        if ui.button("Abbrechen").clicked() {
                            cancel = true;
                        }
                    });
                });

                if create && !name.trim().is_empty() {
                    self.new_project(name.trim().to_string(), template);
                } else if cancel {
                    self.state.dialog = Dialog::None;
                } else {
                    self.state.dialog = Dialog::NewProject { name, template };
                }
            }
            Dialog::Unsaved { next } => {
                let mut save = false;
                let mut discard = false;
                let mut cancel = false;
                egui::Modal::new(egui::Id::unique("unsaved_dialog")).show(ctx, |ui| {
                    ui.set_min_width(380.0);
                    ui.heading("Ungespeicherte Änderungen");
                    ui.label(
                        RichText::new(
                            "Das aktuelle Projekt wurde geändert. Vor dem Fortfahren speichern?",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Speichern").clicked() {
                            save = true;
                        }
                        if ui.button("Nicht speichern").clicked() {
                            discard = true;
                        }
                        if ui.button("Abbrechen").clicked() {
                            cancel = true;
                        }
                    });
                });

                if save {
                    if self.save() {
                        self.proceed(next, ctx);
                    } else if matches!(self.state.dialog, Dialog::Error { .. }) {
                        // Keep the detailed error visible.
                    } else {
                        self.state.dialog = Dialog::Unsaved { next };
                    }
                } else if discard {
                    self.proceed(next, ctx);
                } else if cancel {
                    self.state.dialog = Dialog::None;
                } else {
                    self.state.dialog = Dialog::Unsaved { next };
                }
            }
            Dialog::About => {
                let mut close = false;
                let mut open_help = false;
                egui::Modal::new(egui::Id::unique("about_dialog")).show(ctx, |ui| {
                    ui.set_min_width(440.0);
                    ui.heading("NØVA Flash Studio");
                    ui.label(
                        RichText::new(format!("Version {}", branding::VERSION))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        "Rust · egui/eframe · wgpu · QuickJS\nEigene Timeline, eigenes Projektdatenmodell, eigener SWF-Writer.",
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new("Was bereits funktioniert").strong());
                    ui.label(
                        RichText::new(
                            "• Bühne, Ebenen, Ebenenordner, Frames, Schlüsselbilder, Motion-Tweens und Onion Skin\n• Mehrfachauswahl, Ausrichten/Verteilen, Raster-Snapping, Undo/Redo\n• Objekt- und Schlüsselbild-Workflow mit Kopieren, Einfügen, Duplizieren und Ebenenwechsel\n• Textobjekte mit eingebauter Vektor-Schrift (Export als Shapes)\n• Mehrere Szenen, vier echte Projektvorlagen und Symbolbibliothek\n• Suchen/Ersetzen im Code und projektweite Textsuche\n• Autosave mit Crash-Recovery\n• SWF-Export mit DefineShape3, DefineSprite und Frame-für-Frame-Backen\n• AS3-Teilmenge als echter AVM2-ABC-Bytecode (DoABC2 + SymbolClass)\n• JavaScript-Konsole über QuickJS\n• Branded Splash, natives App-Icon und plattformspezifische Packaging-Skripte\n• „Open in Flash Player“ mit Adobe-Projector als Standard und optionalem Ruffle",
                        )
                        .small(),
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new("Bewusste Grenzen").strong());
                    ui.label(
                        RichText::new(
                            "• Nur eine dokumentierte AS3-Teilmenge; nicht unterstützte Konstrukte sind Compilerfehler\n• Script-Objekte werden einmal platziert und danach von ActionScript gesteuert\n• Keine Bitmaps, Sounds, System-/OpenType-Fonts oder editierbaren Laufzeittext\n• Keine verschachtelten Symbol-Timelines und keine Masken/Shape-Tweens\n• ABC/SWF strukturell validiert; keine automatische Runtime-/Playback-Prüfung",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Hilfe öffnen").clicked() {
                            open_help = true;
                            close = true;
                        }
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                });
                if open_help {
                    self.state.dialog = Dialog::Help;
                } else if close {
                    self.state.dialog = Dialog::None;
                }
            }
            Dialog::Help => {
                let mut close = false;
                egui::Modal::new(egui::Id::unique("help_dialog")).show(ctx, |ui| {
                    ui.set_min_width(500.0);
                    ui.heading("Hilfe & Grenzen");
                    ui.label(
                        RichText::new(
                            "Zwei Modi teilen ein Projekt: Code (AS3) und Animation (Bühne/Timeline).",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Tastenkürzel").strong());
                            egui::Grid::new("shortcut_grid")
                                .num_columns(2)
                                .spacing(egui::vec2(16.0, 4.0))
                                .show(ui, |ui| {
                                    for (keys, description) in [
                                        ("Strg+N / Strg+O", "Neu / Öffnen"),
                                        ("Strg+S / Strg+Shift+S", "Speichern / Speichern unter"),
                                        ("Strg+E", "SWF exportieren"),
                                        ("Strg+F", "Suchen/Ersetzen bzw. Projekt-Suche"),
                                        ("Strg+Z / Strg+Y", "Rückgängig / Wiederholen"),
                                        (
                                            "Strg+C / Strg+V / Strg+D",
                                            "Objekt oder Schlüsselbild kopieren/einfügen/duplizieren",
                                        ),
                                        ("Entf / Esc", "Auswahl löschen / Auswahl aufheben"),
                                        ("Leertaste", "Wiedergabe an/aus"),
                                        ("F5 / Shift+F5", "Frame einfügen / entfernen"),
                                        ("F6 / Shift+F6", "Schlüsselbild / leeres Schlüsselbild"),
                                        ("Pfeil links/rechts", "Frame zurück/vor"),
                                        ("Home / End", "Erster/letzter Frame"),
                                    ] {
                                        ui.label(RichText::new(keys).monospace().small());
                                        ui.label(RichText::new(description).small());
                                        ui.end_row();
                                    }
                                });

                            ui.add_space(8.0);
                            ui.label(RichText::new("Workflow").strong());
                            ui.label(
                                RichText::new(
                                    "1. Objekte auf der Bühne anlegen und im Eigenschaften-Panel positionieren.\n2. Mit F6 Schlüsselbilder setzen, Objekte bewegen und im Timeline-Kontext oder Eigenschaften-Panel einen Motion-Tween mit Easing und Dauer anlegen.\n3. Mehrfachauswahl per Shift-Klick oder Auswahlrechteck; gemeinsame Transformationen wirken um den Gruppenmittelpunkt.\n4. Skripte im Code-Modus bearbeiten und einem Objekt im Eigenschaften-Panel zuweisen. Ein MovieClip/Sprite-Skript ohne Zuweisung kann als Dokumentklasse dienen.\n5. Mit Strg+E eine SWF schreiben; Skripte werden als DoABC2 eingebettet und per SymbolClass verknüpft.",
                                )
                                .small(),
                            );

                            ui.add_space(8.0);
                            ui.label(RichText::new("Ehrliche Grenzen").strong());
                            ui.label(
                                RichText::new(
                                    "• ActionScript wird für eine dokumentierte Teilmenge zu echtem AVM2-ABC kompiliert. Nicht unterstützte Syntax ist ein Compilerfehler mit Datei/Zeile.\n• Die ABC- und SWF-Struktur wird mit einem unabhängigen Parser validiert. „Open in Flash Player“ startet die konfigurierte Runtime (Adobe-Projector als Standard, Ruffle nur nach ausdrücklicher Auswahl); eine automatische Playback-Prüfung oder eingebettete Runtime gibt es nicht.\n• Skript-Objekte werden einmal platziert und danach von ActionScript gesteuert; ihre Timeline-Tweens werden nicht zusätzlich gebacken.\n• Nicht-Skript-Objekte werden weiterhin Frame für Frame gebacken.\n• Text ist statisch: die eingebaute 5×7-Vektorschrift wird als Shapes exportiert. System-/OpenType-Fonts, editierbarer Laufzeittext, Bitmaps, Sounds, verschachtelte Symbol-Timelines, Masken und Shape-Tweens fehlen.\n• Array-, Objekt- und Index-Zugriffe in AS3 sind strukturell geprüft, aber nicht gegen eine echte Runtime getestet.",
                                )
                                .small()
                                .color(theme::TEXT_DIM),
                            );
                        });
                    ui.add_space(8.0);
                    if ui.button("Schließen").clicked() {
                        close = true;
                    }
                });
                if close {
                    self.state.dialog = Dialog::None;
                }
            }
            Dialog::CompatibilityReport {
                title,
                report,
                is_error,
            } => {
                let mut close = false;
                egui::Modal::new(egui::Id::unique("compatibility_report_dialog")).show(ctx, |ui| {
                    ui.set_min_width(560.0);
                    ui.heading(RichText::new(&title).color(if is_error {
                        theme::RED
                    } else {
                        theme::ACCENT
                    }));
                    ui.label(
                        RichText::new(
                            "Bericht zur Formatkompatibilität. Nicht unterstützte Daten bleiben in der Originaldatei erhalten.",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&report).monospace().small())
                                    .wrap(),
                            );
                        });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Bericht kopieren").clicked() {
                            ui.ctx().copy_text(format!("{title}\n\n{report}"));
                        }
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                });
                if close {
                    self.state.dialog = Dialog::None;
                }
            }
            Dialog::Error { title, message } => {
                let mut close = false;
                egui::Modal::new(egui::Id::unique("error_dialog")).show(ctx, |ui| {
                    ui.set_min_width(440.0);
                    ui.heading(RichText::new(&title).color(theme::RED));
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&message).monospace().small())
                                    .wrap(),
                            );
                        });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Meldung kopieren").clicked() {
                            ui.ctx().copy_text(format!("{title}\n\n{message}"));
                        }
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                });
                if close {
                    self.state.dialog = Dialog::None;
                }
            }
            Dialog::ExportDone {
                path,
                bytes,
                frames,
                warnings,
            } => {
                let mut close = false;
                let mut open_player = false;
                let mut open_settings = false;
                egui::Modal::new(egui::Id::unique("export_done_dialog")).show(ctx, |ui| {
                    ui.set_min_width(470.0);
                    ui.heading("SWF exportiert");
                    ui.label(
                        RichText::new(path.display().to_string())
                            .monospace()
                            .small()
                            .color(theme::ACCENT),
                    );
                    ui.label(format!("{bytes} Bytes · {frames} Frames"));
                    if warnings.is_empty() {
                        ui.label(RichText::new("Keine Builder-Warnungen.").color(theme::GREEN));
                    } else {
                        ui.label(
                            RichText::new(format!("{} Warnung(en):", warnings.len()))
                                .color(theme::AMBER),
                        );
                        egui::ScrollArea::vertical()
                            .max_height(160.0)
                            .show(ui, |ui| {
                                for warning in &warnings {
                                    ui.label(
                                        RichText::new(format!("• {warning}"))
                                            .small()
                                            .color(theme::TEXT_DIM),
                                    );
                                }
                            });
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Öffne den Film in der konfigurierten Runtime. Standard ist der Adobe Flash Player Projector; Ruffle wird nur nach ausdrücklicher Auswahl verwendet. Die tatsächliche Wiedergabe wird von NØVA nicht simuliert.",
                        )
                        .small()
                        .color(theme::TEXT_FAINT),
                    );
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(RichText::new("Open in Flash Player").strong())
                            .clicked()
                        {
                            open_player = true;
                        }
                        if ui.button("Runtime-Einstellungen …").clicked() {
                            open_settings = true;
                        }
                        if ui.button("Pfad kopieren").clicked() {
                            ui.ctx().copy_text(path.display().to_string());
                        }
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                });
                if open_player {
                    self.state.dialog = Dialog::None;
                    self.open_in_flash_player();
                } else if open_settings {
                    self.open_flash_settings_dialog();
                } else if close {
                    self.state.dialog = Dialog::None;
                }
            }
            Dialog::RuntimeSetup {
                mut adobe_path,
                mut test_result,
                mut error,
            } => {
                let mut save = false;
                let mut later = false;
                egui::Modal::new(egui::Id::unique("runtime_setup_dialog")).show(ctx, |ui| {
                    ui.set_min_width(600.0);
                    ui.heading("Flash Player Projector einrichten");
                    ui.label(
                        RichText::new(
                            "NØVA rendert SWF-Dateien nicht selbst. Zum Abspielen eines exportierten SWF wird eine externe Runtime benötigt. Standard ist der Adobe Flash Player Projector; Ruffle ist optional und wird nur nach ausdrücklicher Auswahl verwendet.",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "NØVA lädt keine Adobe-Binärdateien herunter und bündelt sie nicht. Wähle eine bereits vorhandene Projector-Datei aus. Pfade mit Leerzeichen und Unicode-Zeichen werden unterstützt.",
                        )
                        .small()
                        .color(theme::TEXT_FAINT),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Adobe Flash Player Projector")
                            .small()
                            .strong()
                            .color(theme::TEXT_DIM),
                    );
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut adobe_path)
                                .hint_text("z. B. C:\\Program Files\\Flash Player\\flashplayer.exe")
                                .desired_width(360.0),
                        );
                        if response.changed() {
                            test_result = None;
                            error = None;
                        }
                        if ui.button("Durchsuchen …").clicked()
                            && let Some(path) = projector_file_dialog("Adobe Flash Player Projector auswählen")
                        {
                            adobe_path = path.display().to_string();
                            test_result = None;
                            error = None;
                        }
                        if ui
                            .add_enabled(
                                !adobe_path.trim().is_empty(),
                                egui::Button::new("Prüfen"),
                            )
                            .clicked()
                        {
                            let path = PathBuf::from(adobe_path.trim());
                            test_result = Some(match flash::test_runtime(
                                &path,
                                flash::RuntimeKind::AdobeProjector,
                            ) {
                                Ok(record) => record,
                                Err(error) => flash::ValidationRecord::failure(
                                    flash::RuntimeKind::AdobeProjector,
                                    &path,
                                    error.to_string(),
                                ),
                            });
                            error = None;
                        }
                    });
                    if let Some(record) = &test_result {
                        ui.colored_label(
                            if record.ok { theme::GREEN } else { theme::RED },
                            record.summary(),
                        );
                    }
                    if let Some(message) = &error {
                        ui.colored_label(theme::RED, message);
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Du kannst die Einrichtung später nachholen. NØVA bleibt nutzbar; SWF-Wiedergabe benötigt dann eine konfigurierte Runtime.",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !adobe_path.trim().is_empty(),
                                egui::Button::new(RichText::new("Speichern und fortfahren").strong()),
                            )
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button("Später einrichten").clicked() {
                            later = true;
                        }
                    });
                });

                if save {
                    let path = PathBuf::from(adobe_path.trim());
                    match flash::test_runtime(&path, flash::RuntimeKind::AdobeProjector) {
                        Ok(record) => {
                            let mut draft = self.flash_settings.clone();
                            draft.default_runtime = flash::RuntimeKind::AdobeProjector;
                            draft.adobe_projector_path = Some(path);
                            draft.first_launch_setup_done = true;
                            draft.last_validation = Some(record);
                            match flash::save_settings(&draft) {
                                Ok(()) => {
                                    self.flash_settings = draft;
                                    self.state.log(
                                        LogLevel::Success,
                                        "Adobe Flash Player Projector gespeichert.",
                                    );
                                    self.state.set_status("Runtime konfiguriert");
                                    self.state.dialog = Dialog::None;
                                    self.open_pending_swf();
                                }
                                Err(save_error) => {
                                    self.state.log(LogLevel::Error, save_error.to_string());
                                    self.state.dialog = Dialog::RuntimeSetup {
                                        adobe_path,
                                        test_result,
                                        error: Some(format!(
                                            "Speichern fehlgeschlagen: {save_error}"
                                        )),
                                    };
                                }
                            }
                        }
                        Err(validation_error) => {
                            self.state
                                .log(LogLevel::Error, validation_error.to_string());
                            self.state.dialog = Dialog::RuntimeSetup {
                                adobe_path,
                                test_result: Some(flash::ValidationRecord::failure(
                                    flash::RuntimeKind::AdobeProjector,
                                    &path,
                                    validation_error.to_string(),
                                )),
                                error: Some(
                                    "Die ausgewählte Datei ist keine gültige Runtime.".to_string(),
                                ),
                            };
                        }
                    }
                } else if later {
                    self.postpone_runtime_setup();
                } else {
                    self.state.dialog = Dialog::RuntimeSetup {
                        adobe_path,
                        test_result,
                        error,
                    };
                }
            }
            Dialog::FlashSettings {
                mut default_runtime,
                mut adobe_path,
                mut ruffle_path,
                mut first_launch_setup_done,
                mut last_validation,
                mut test_result,
                mut error,
            } => {
                let mut save = false;
                let mut cancel = false;
                let mut reset = false;
                egui::Modal::new(egui::Id::unique("flash_settings_dialog")).show(ctx, |ui| {
                    ui.set_min_width(640.0);
                    ui.heading("Runtime");
                    ui.label(
                        RichText::new(
                            "NØVA startet nur die hier ausgewählte Runtime. Adobe Flash Player Projector ist der Standard; Ruffle wird niemals automatisch verwendet. Dateizuordnungen werden nicht stillschweigend verändert.",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(8.0);

                    ui.label(RichText::new("Standard-Runtime").strong());
                    ui.radio_value(
                        &mut default_runtime,
                        flash::RuntimeKind::AdobeProjector,
                        "Adobe Flash Player Projector (empfohlen)",
                    );
                    ui.radio_value(
                        &mut default_runtime,
                        flash::RuntimeKind::Ruffle,
                        "Ruffle (nur nach ausdrücklicher Auswahl)",
                    );
                    ui.add_space(8.0);

                    ui.label(
                        RichText::new("Adobe Flash Player Projector")
                            .small()
                            .strong()
                            .color(theme::TEXT_DIM),
                    );
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut adobe_path)
                                .hint_text("z. B. C:\\Program Files\\Flash Player\\flashplayer.exe")
                                .desired_width(360.0),
                        );
                        if response.changed() {
                            test_result = None;
                            error = None;
                        }
                        if ui.button("Durchsuchen …").clicked()
                            && let Some(path) = projector_file_dialog("Adobe Flash Player Projector auswählen")
                        {
                            adobe_path = path.display().to_string();
                            test_result = None;
                            error = None;
                        }
                        #[cfg(target_os = "macos")]
                        if ui.button("App-Bundle …").clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .set_title("Adobe Flash Player App-Bundle auswählen")
                                .pick_folder()
                        {
                            adobe_path = path.display().to_string();
                            test_result = None;
                            error = None;
                        }
                        if ui
                            .add_enabled(!adobe_path.trim().is_empty(), egui::Button::new("Prüfen"))
                            .clicked()
                        {
                            let path = PathBuf::from(adobe_path.trim());
                            test_result = Some(match flash::test_runtime(
                                &path,
                                flash::RuntimeKind::AdobeProjector,
                            ) {
                                Ok(record) => record,
                                Err(error) => flash::ValidationRecord::failure(
                                    flash::RuntimeKind::AdobeProjector,
                                    &path,
                                    error.to_string(),
                                ),
                            });
                        }
                    });

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Ruffle (optional)")
                            .small()
                            .strong()
                            .color(theme::TEXT_DIM),
                    );
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut ruffle_path)
                                .hint_text("z. B. C:\\Tools\\Ruffle\\ruffle.exe")
                                .desired_width(360.0),
                        );
                        if response.changed() {
                            test_result = None;
                            error = None;
                        }
                        if ui.button("Durchsuchen …").clicked()
                            && let Some(path) = projector_file_dialog("Ruffle auswählen")
                        {
                            ruffle_path = path.display().to_string();
                            test_result = None;
                            error = None;
                        }
                        if ui
                            .add_enabled(!ruffle_path.trim().is_empty(), egui::Button::new("Prüfen"))
                            .clicked()
                        {
                            let path = PathBuf::from(ruffle_path.trim());
                            test_result = Some(match flash::test_runtime(
                                &path,
                                flash::RuntimeKind::Ruffle,
                            ) {
                                Ok(record) => record,
                                Err(error) => flash::ValidationRecord::failure(
                                    flash::RuntimeKind::Ruffle,
                                    &path,
                                    error.to_string(),
                                ),
                            });
                        }
                    });

                    let draft = flash::FlashSettings {
                        default_runtime,
                        adobe_projector_path: path_from_text(&adobe_path),
                        ruffle_path: path_from_text(&ruffle_path),
                        first_launch_setup_done,
                        last_validation: last_validation.clone(),
                    };
                    let status = flash::status(&draft);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Status:").strong());
                        ui.colored_label(
                            if status.ready {
                                theme::GREEN
                            } else {
                                theme::AMBER
                            },
                            status.summary(),
                        );
                    });

                    let shown_validation = test_result
                        .as_ref()
                        .filter(|record| {
                            validation_matches(
                                record,
                                default_runtime,
                                &adobe_path,
                                &ruffle_path,
                            )
                        })
                        .or_else(|| {
                            last_validation.as_ref().filter(|record| {
                                validation_matches(
                                    record,
                                    default_runtime,
                                    &adobe_path,
                                    &ruffle_path,
                                )
                            })
                        });
                    if let Some(record) = shown_validation {
                        ui.colored_label(
                            if record.ok { theme::GREEN } else { theme::RED },
                            format!("Letzte Prüfung: {}", record.summary()),
                        );
                    }
                    if let Some(message) = &error {
                        ui.colored_label(theme::RED, message);
                    }

                    ui.add_space(6.0);
                    ui.collapsing("Dateizuordnung (.swf)", |ui| {
                        ui.label(
                            RichText::new(
                                "NØVA registriert sich bewusst nicht stillschweigend als Standard-App für .swf. Eine Zuordnung muss der Benutzer im Betriebssystem bestätigen. Details und plattformspezifische Hinweise stehen in docs/FLASH_PLAYER_INTEGRATION.md.",
                            )
                            .small()
                            .color(theme::TEXT_DIM),
                        );
                    });
                    ui.add_space(6.0);
                    match flash::settings_path() {
                        Ok(path) => {
                            ui.label(
                                RichText::new(format!("Konfiguration: {}", path.display()))
                                    .small()
                                    .color(theme::TEXT_FAINT),
                            );
                        }
                        Err(error) => {
                            ui.label(
                                RichText::new(format!("Konfiguration: {error}"))
                                    .small()
                                    .color(theme::AMBER),
                            );
                        }
                    }
                    if ui.button("Diagnose kopieren").clicked() {
                        let config = flash::settings_path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|error| error.to_string());
                        ui.ctx().copy_text(format!(
                            "NØVA Flash Studio {}\nOS: {}\nRuntime: {}\nStatus: {}\nAdobe: {}\nRuffle: {}\nLetzte Prüfung: {}\nKonfiguration: {}",
                            branding::VERSION,
                            std::env::consts::OS,
                            default_runtime.label(),
                            status.detail,
                            adobe_path,
                            ruffle_path,
                            last_validation
                                .as_ref()
                                .map(flash::ValidationRecord::summary)
                                .unwrap_or_else(|| "—".to_string()),
                            config
                        ));
                    }

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Speichern").strong()).clicked() {
                            save = true;
                        }
                        if ui.button("Zurücksetzen").clicked() {
                            reset = true;
                        }
                        if ui.button("Abbrechen").clicked() {
                            cancel = true;
                        }
                    });
                    if reset {
                        default_runtime = flash::RuntimeKind::AdobeProjector;
                        adobe_path.clear();
                        ruffle_path.clear();
                        first_launch_setup_done = false;
                        last_validation = None;
                        test_result = None;
                        error = None;
                        ui.label(
                            RichText::new(
                                "Zurückgesetzt. Speichere die Änderung, um die Konfiguration zu leeren; beim nächsten Start wird die Ersteinrichtung erneut angeboten.",
                            )
                            .small()
                            .color(theme::AMBER),
                        );
                    }
                });

                if save {
                    let mut draft = flash::FlashSettings {
                        default_runtime,
                        adobe_projector_path: path_from_text(&adobe_path),
                        ruffle_path: path_from_text(&ruffle_path),
                        first_launch_setup_done,
                        last_validation: last_validation.clone(),
                    };
                    match flash::validate_settings_paths(&draft) {
                        Err(record) => {
                            self.state.dialog = Dialog::FlashSettings {
                                default_runtime,
                                adobe_path,
                                ruffle_path,
                                first_launch_setup_done,
                                last_validation,
                                test_result: Some(record),
                                error: Some(
                                    "Die Konfiguration wurde nicht gespeichert, weil ein Pfad ungültig ist."
                                        .to_string(),
                                ),
                            };
                        }
                        Ok(fresh_validation) => {
                            draft.mark_setup_done_if_configured();
                            draft.last_validation = fresh_validation.or_else(|| {
                                last_validation.clone().filter(|record| {
                                    validation_matches(
                                        record,
                                        default_runtime,
                                        &adobe_path,
                                        &ruffle_path,
                                    )
                                })
                            });
                            let failed_path = draft
                                .adobe_projector_path
                                .as_deref()
                                .or(draft.ruffle_path.as_deref())
                                .map(Path::to_path_buf)
                                .unwrap_or_default();
                            match flash::save_settings(&draft) {
                                Ok(()) => {
                                    self.flash_settings = draft;
                                    self.state.log(
                                        LogLevel::Success,
                                        "Runtime-Einstellungen gespeichert.",
                                    );
                                    self.state.set_status("Runtime-Einstellungen gespeichert");
                                    self.state.dialog = Dialog::None;
                                }
                                Err(error) => {
                                    self.state.log(LogLevel::Error, error.to_string());
                                    self.state.dialog = Dialog::FlashSettings {
                                        default_runtime,
                                        adobe_path,
                                        ruffle_path,
                                        first_launch_setup_done,
                                        last_validation,
                                        test_result: None,
                                        error: Some(format!(
                                            "Speichern fehlgeschlagen: {error}\n\nPfad: {}",
                                            failed_path.display()
                                        )),
                                    };
                                }
                            }
                        }
                    }
                } else if cancel {
                    self.state.dialog = Dialog::None;
                } else {
                    // The dialog is cloned for rendering, so persist edits made
                    // by the text fields and radio buttons for the next frame.
                    self.state.dialog = Dialog::FlashSettings {
                        default_runtime,
                        adobe_path,
                        ruffle_path,
                        first_launch_setup_done,
                        last_validation,
                        test_result,
                        error,
                    };
                }
            }
            Dialog::AutosaveRecovery { path, recovery } => {
                let mut restore = false;
                let mut discard = false;
                let mut later = false;
                egui::Modal::new(egui::Id::unique("autosave_recovery_dialog")).show(ctx, |ui| {
                    ui.set_min_width(460.0);
                    ui.heading("Autosave gefunden");
                    ui.label(
                        RichText::new(
                            "Eine automatische Sicherung ist neuer als der geöffnete Projektstand.",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(path.display().to_string())
                            .monospace()
                            .small()
                            .color(theme::ACCENT),
                    );
                    ui.label(
                        RichText::new(format!(
                            "Projekt '{}' · Revision {}",
                            recovery.project.metadata.name, recovery.project.metadata.modified_unix
                        ))
                        .small()
                        .color(theme::TEXT_FAINT),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Wiederherstellen").clicked() {
                            restore = true;
                        }
                        if ui.button("Verwerfen").clicked() {
                            discard = true;
                        }
                        if ui.button("Später").clicked() {
                            later = true;
                        }
                    });
                });
                if restore {
                    self.project = recovery.project;
                    let source = recovery.source.clone();
                    self.state = AppState::new(&self.project);
                    self.state.path = source;
                    self.state.dirty = true;
                    self.history.clear();
                    self.pending_before = None;
                    self.suppress_history = true;
                    self.state
                        .log(LogLevel::Success, "Autosave wiederhergestellt.");
                    self.state.dialog = Dialog::None;
                } else if discard {
                    let _ = std::fs::remove_file(&path);
                    self.state.dialog = Dialog::None;
                    self.state.log(LogLevel::Info, "Autosave verworfen.");
                } else if later {
                    self.state.dialog = Dialog::None;
                    self.state
                        .set_status("Autosave-Recovery kann über die Datei erneut geprüft werden");
                } else {
                    self.state.dialog = Dialog::AutosaveRecovery { path, recovery };
                }
            }
        }
    }
}

impl eframe::App for NovaApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.splash.as_ref().is_some_and(Splash::is_active) {
            if let Some(splash) = &mut self.splash {
                splash.show(&ctx);
            }
            ctx.request_repaint();
            return;
        }
        self.splash = None;
        self.state.clamp_frame(&self.project);
        self.last_history_label = "Bearbeiten";
        let before = self.project.clone();

        let mut actions = self.shortcut_actions(&ctx);

        egui::Panel::top("menu_bar").show(ui, |ui| {
            self.menu_bar(ui, &mut actions);
        });
        egui::Panel::top("toolbar").exact_size(40.0).show(ui, |ui| {
            self.toolbar(ui, &mut actions);
        });

        egui::Panel::bottom("status_bar")
            .exact_size(22.0)
            .show(ui, |ui| {
                self.status_bar(ui);
            });
        if self.ai.open {
            // The AI workspace has its own resizable explorer/chat/output
            // panels. Hiding the normal panels gives it the full central area
            // while preserving all editor state for returning.
            egui::CentralPanel::default().show(ui, |ui| {
                actions.extend(self.ai.show(ui, &mut self.project, &mut self.state));
            });
        } else {
            egui::Panel::bottom("bottom_dock")
                .resizable(true)
                .default_size(224.0)
                .min_size(130.0)
                .max_size(460.0)
                .show(ui, |ui| {
                    self.bottom_dock(ui, &mut actions);
                });

            egui::Panel::left("project_tree")
                .resizable(true)
                .default_size(232.0)
                .size_range(180.0..=360.0)
                .show(ui, |ui| {
                    actions.extend(tree::show(ui, &mut self.project, &mut self.state));
                });
            egui::Panel::right("properties_panel")
                .resizable(true)
                .default_size(268.0)
                .size_range(210.0..=390.0)
                .show(ui, |ui| {
                    actions.extend(properties::show(ui, &mut self.project, &mut self.state));
                });

            egui::CentralPanel::default().show(ui, |ui| match self.state.mode {
                Mode::Code => {
                    actions.extend(code_view::show(ui, &mut self.project, &mut self.state));
                }
                Mode::Animation => {
                    actions.extend(stage_view::show(ui, &mut self.project, &mut self.state));
                }
            });
        }

        self.refresh_asset_textures(&ctx);
        self.update_playback(&ctx);
        self.handle_actions(actions, &ctx);
        self.dialogs(&ctx);
        self.finish_history(before, &ctx);
        self.maybe_autosave(&ctx);
    }
}

fn projector_file_dialog(title: &str) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    #[cfg(target_os = "windows")]
    {
        dialog = dialog.add_filter("Programm", &["exe"]);
    }
    dialog.add_filter("Alle Dateien", &["*"]).pick_file()
}

fn is_swf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("swf"))
}

fn path_from_text(text: &str) -> Option<PathBuf> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

fn validation_matches(
    record: &flash::ValidationRecord,
    _runtime: flash::RuntimeKind,
    adobe_path: &str,
    ruffle_path: &str,
) -> bool {
    let current = match record.runtime {
        flash::RuntimeKind::AdobeProjector => path_from_text(adobe_path),
        flash::RuntimeKind::Ruffle => path_from_text(ruffle_path),
    };
    current.as_deref() == Some(record.path.as_path())
}

fn file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_').trim();
    if trimmed.is_empty() {
        "nova".to_string()
    } else {
        trimmed.to_string()
    }
}

fn with_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    let matches = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension));
    if !matches {
        path.set_extension(extension);
    }
    path
}

/// Extracts `(sample_rate, channels, duration_seconds)` from a PCM WAV file.
/// Returns `None` for files that are not RIFF/WAVE or not 16-bit PCM.
fn wav_info(data: &[u8]) -> Option<(u32, u16, f32)> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12usize;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut data_size = 0u32;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;
        let body = offset + 8;
        if body + size > data.len() {
            break;
        }
        if id == b"fmt " && size >= 16 {
            let audio_format = u16::from_le_bytes([data[body], data[body + 1]]);
            channels = u16::from_le_bytes([data[body + 2], data[body + 3]]);
            sample_rate = u32::from_le_bytes([
                data[body + 4],
                data[body + 5],
                data[body + 6],
                data[body + 7],
            ]);
            let bits = u16::from_le_bytes([data[body + 14], data[body + 15]]);
            if audio_format != 1 || bits != 16 || channels == 0 || sample_rate == 0 {
                return None;
            }
        } else if id == b"data" {
            data_size = size as u32;
        }
        offset = body + size + (size & 1);
    }
    if sample_rate == 0 || data_size == 0 {
        return None;
    }
    let bytes_per_second = sample_rate as f32 * channels as f32 * 2.0;
    let duration = data_size as f32 / bytes_per_second;
    Some((sample_rate, channels, duration))
}

/// Writes embedded audio bytes to a temp file and plays it via `PlaySoundW`.
#[cfg(windows)]
fn play_sound_bytes(data: &[u8], mime: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    if data.is_empty() {
        return Err("Audio-Daten sind leer".into());
    }
    let extension = if mime.contains("mpeg") { "mp3" } else { "wav" };
    let temp =
        std::env::temp_dir().join(format!("nova-preview-{}.{extension}", uuid::Uuid::new_v4()));
    std::fs::write(&temp, data).map_err(|error| error.to_string())?;
    let wide: Vec<u16> = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SND_FILENAME | SND_ASYNC — fire and forget; the file stays until the
    // next boot, which is acceptable for short preview clips.
    const SND_FILENAME: u32 = 0x0002_0000;
    const SND_ASYNC: u32 = 0x0001;
    let ok = unsafe {
        windows_sys::Win32::Media::Audio::PlaySoundW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            SND_FILENAME | SND_ASYNC,
        )
    };
    let _ = std::fs::remove_file(&temp);
    if ok == 0 {
        return Err("PlaySoundW hat die Datei abgelehnt".into());
    }
    Ok(())
}

#[allow(dead_code)]
fn frame_for(scene: &nova_core::Scene, index: usize) -> Option<&Frame> {
    scene
        .layers
        .iter()
        .find_map(|layer| layer.frames.get(index))
}

#[cfg(test)]
mod tests {
    use super::is_swf_path;
    use std::path::Path;

    #[test]
    fn recognizes_swf_startup_paths_case_insensitively() {
        assert!(is_swf_path(Path::new("Mein Film.swf")));
        assert!(is_swf_path(Path::new("C:/Filme/MEIN FILM.SWF")));
        assert!(!is_swf_path(Path::new("Mein Film.nova")));
        assert!(!is_swf_path(Path::new("Mein Film")));
    }
}
