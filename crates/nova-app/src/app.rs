//! NØVA Flash Studio application shell: panels, menus, actions and dialogs.

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Context, Key, RichText, Sense, Ui};
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
    autosave, branding, classroom_view, code_view, console, edition, enterprise_view, flash, i18n,
    properties, stage_view, theme, tilemap_view, timeline_view, toolbox, tree, util,
};

fn egui_key_for_letter(index: usize) -> Key {
    const KEYS: [Key; 26] = [
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
    ];
    KEYS[index.min(25)]
}

fn egui_key_for_digit(index: usize) -> Key {
    const KEYS: [Key; 10] = [
        Key::Num0,
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
    ];
    KEYS[index.min(9)]
}

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
    audio: crate::audio::AudioEngine,
    edu: crate::edu_state::EduUi,
    ent: crate::ent_state::EntUi,
    /// First-run / settings language wizard. `Some(lang)` = active preview.
    wizard: Option<i18n::Lang>,
    /// Right dock split: fraction of height given to Properties (rest = library).
    right_split: f32,
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
            audio: crate::audio::AudioEngine::new(),
            edu: crate::edu_state::EduUi::load_local(
                &nova_core::Edition::current()
                    .config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from(".")),
            ),
            ent: crate::ent_state::EntUi::load_local(
                &nova_core::Edition::current()
                    .config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from(".")),
            ),
            wizard: if i18n::language_is_chosen() {
                None
            } else {
                Some(i18n::language())
            },
            right_split: 0.55,
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
            app.state.dialog = Dialog::AutosaveRecovery {
                path,
                recovery: Box::new(recovery),
            };
        }
        if let Some(warning) = warning {
            app.state.log(LogLevel::Warn, warning);
        }
        Ok(app)
    }

    // ------------------------------------------------------------- panels

    fn menu_bar(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(i18n::t("menu.file"), |ui| {
                if ui.button(i18n::t("menu.file.new")).clicked() {
                    actions.push(UiAction::NewProjectPrompt);
                    ui.close();
                }
                if ui.button(i18n::t("menu.file.open")).clicked() {
                    actions.push(UiAction::OpenPrompt);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.file.save")).clicked() {
                    actions.push(UiAction::Save);
                    ui.close();
                }
                if ui.button(i18n::t("menu.file.save_as")).clicked() {
                    actions.push(UiAction::SaveAs);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.file.export")).clicked() {
                    actions.push(UiAction::Export);
                    ui.close();
                }
                if ui
                    .button(i18n::t("menu.file.compat"))
                    .on_hover_text(i18n::t("menu.file.compat_tip"))
                    .clicked()
                {
                    actions.push(UiAction::ShowCompatibilityReport);
                    ui.close();
                }
                ui.separator();
                ui.menu_button(i18n::t("menu.file.autosave"), |ui| {
                    if ui
                        .checkbox(
                            &mut self.state.autosave_enabled,
                            i18n::t("menu.file.autosave_on"),
                        )
                        .changed()
                    {
                        actions.push(UiAction::ToggleAutosave);
                    }
                    let mut interval = self.state.autosave_seconds;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(i18n::t("menu.file.autosave_interval"))
                                .small()
                                .color(theme::TEXT_DIM),
                        );
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
                            RichText::new(i18n::tf(
                                "menu.file.autosave_last",
                                &[("path", &path.display().to_string())],
                            ))
                            .small()
                            .color(theme::TEXT_FAINT),
                        );
                    }
                });
                ui.separator();
                if ui.button(i18n::t("menu.file.quit")).clicked() {
                    actions.push(UiAction::Quit);
                    ui.close();
                }
            });

            ui.menu_button(i18n::t("menu.edit"), |ui| {
                let undo_label = self
                    .history
                    .undo_label()
                    .map(|label| i18n::tf("menu.edit.undo", &[("label", label)]))
                    .unwrap_or_else(|| i18n::t("menu.edit.undo_plain"));
                if ui
                    .add_enabled(
                        self.history.can_undo(),
                        egui::Button::new(format!("{undo_label}    Strg+Z")),
                    )
                    .on_hover_text(i18n::tf(
                        "menu.edit.undo_steps",
                        &[("count", &self.history.undo_len().to_string())],
                    ))
                    .clicked()
                {
                    actions.push(UiAction::Undo);
                    ui.close();
                }
                let redo_label = self
                    .history
                    .redo_label()
                    .map(|label| i18n::tf("menu.edit.redo", &[("label", label)]))
                    .unwrap_or_else(|| i18n::t("menu.edit.redo_plain"));
                if ui
                    .add_enabled(
                        self.history.can_redo(),
                        egui::Button::new(format!("{redo_label}    Strg+Y")),
                    )
                    .on_hover_text(i18n::tf(
                        "menu.edit.redo_steps",
                        &[("count", &self.history.redo_len().to_string())],
                    ))
                    .clicked()
                {
                    actions.push(UiAction::Redo);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.edit.delete_selection")).clicked() {
                    actions.push(UiAction::DeleteSelection);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.edit.copy_objects")).clicked() {
                    actions.push(UiAction::CopyObjects);
                    ui.close();
                }
                if ui.button(i18n::t("menu.edit.paste_objects")).clicked() {
                    actions.push(UiAction::PasteObjects);
                    ui.close();
                }
                if ui.button(i18n::t("menu.edit.duplicate_objects")).clicked() {
                    actions.push(UiAction::DuplicateObjects);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.edit.copy_keyframes")).clicked() {
                    actions.push(UiAction::CopyKeyframes);
                    ui.close();
                }
                if ui.button(i18n::t("menu.edit.paste_keyframes")).clicked() {
                    actions.push(UiAction::PasteKeyframes);
                    ui.close();
                }
                if ui
                    .button(i18n::t("menu.edit.duplicate_keyframes"))
                    .clicked()
                {
                    actions.push(UiAction::DuplicateKeyframes);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.edit.insert_frame")).clicked() {
                    actions.push(UiAction::InsertFrame);
                    ui.close();
                }
                if ui.button(i18n::t("menu.edit.remove_frame")).clicked() {
                    actions.push(UiAction::RemoveFrame);
                    ui.close();
                }
                ui.separator();
                let has_script = self.state.active_script.is_some();
                if ui
                    .add_enabled(
                        has_script,
                        egui::Button::new(i18n::t("menu.edit.check_script")),
                    )
                    .clicked()
                {
                    if let Some(id) = self.state.active_script {
                        actions.push(UiAction::CheckScript(id));
                    }
                    ui.close();
                }
            });

            ui.menu_button(i18n::t("menu.insert"), |ui| {
                if ui.button(i18n::t("menu.insert.rect")).clicked() {
                    actions.push(UiAction::AddRect);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.ellipse")).clicked() {
                    actions.push(UiAction::AddEllipse);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.text")).clicked() {
                    actions.push(UiAction::AddText);
                    ui.close();
                }
                let symbol = self
                    .state
                    .selected_symbol
                    .or_else(|| self.project.symbols.first().map(|symbol| symbol.id));
                if ui
                    .add_enabled(
                        symbol.is_some(),
                        egui::Button::new(i18n::t("menu.insert.symbol_instance")),
                    )
                    .clicked()
                {
                    if let Some(id) = symbol {
                        actions.push(UiAction::AddSymbolInstance(id));
                    }
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.insert.layer")).clicked() {
                    actions.push(UiAction::AddLayer);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.folder")).clicked() {
                    actions.push(UiAction::AddFolder);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.scene")).clicked() {
                    actions.push(UiAction::AddScene);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.delete_scene")).clicked() {
                    actions.push(UiAction::DeleteScene);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.script")).clicked() {
                    actions.push(UiAction::AddScript);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.symbol")).clicked() {
                    actions.push(UiAction::AddSymbol);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.insert.keyframe")).clicked() {
                    actions.push(UiAction::InsertKeyframe);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.blank_keyframe")).clicked() {
                    actions.push(UiAction::InsertBlankKeyframe);
                    ui.close();
                }
                if ui.button(i18n::t("menu.insert.remove_keyframe")).clicked() {
                    actions.push(UiAction::RemoveKeyframe);
                    ui.close();
                }
            });

            ui.menu_button(i18n::t("menu.view"), |ui| {
                if ui
                    .selectable_label(
                        self.state.mode == Mode::Code,
                        i18n::t("menu.view.code_mode"),
                    )
                    .clicked()
                {
                    actions.push(UiAction::SetMode(Mode::Code));
                    ui.close();
                }
                if ui
                    .selectable_label(
                        self.state.mode == Mode::Animation,
                        i18n::t("menu.view.anim_mode"),
                    )
                    .clicked()
                {
                    actions.push(UiAction::SetMode(Mode::Animation));
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.view.fit_stage")).clicked() {
                    actions.push(UiAction::FitStage);
                    ui.close();
                }
                if ui.button(i18n::t("menu.view.zoom_in")).clicked() {
                    actions.push(UiAction::ZoomBy(1.2));
                    ui.close();
                }
                if ui.button(i18n::t("menu.view.zoom_out")).clicked() {
                    actions.push(UiAction::ZoomBy(1.0 / 1.2));
                    ui.close();
                }
                ui.separator();
                if ui
                    .selectable_label(self.state.onion_skin, i18n::t("menu.view.onion"))
                    .clicked()
                {
                    actions.push(UiAction::ToggleOnion);
                    ui.close();
                }
                if ui
                    .selectable_label(self.state.show_grid, i18n::t("menu.view.grid"))
                    .clicked()
                {
                    actions.push(UiAction::ToggleGrid);
                    ui.close();
                }
                if ui
                    .selectable_label(self.state.snap_to_grid, i18n::t("menu.view.snap"))
                    .clicked()
                {
                    actions.push(UiAction::ToggleSnap);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("menu.view.tab_timeline")).clicked() {
                    actions.push(UiAction::SetBottomTab(BottomTab::Timeline));
                    ui.close();
                }
                if ui.button(i18n::t("menu.view.tab_console")).clicked() {
                    actions.push(UiAction::SetBottomTab(BottomTab::Console));
                    ui.close();
                }
                if ui.button(i18n::t("menu.view.tab_problems")).clicked() {
                    actions.push(UiAction::SetBottomTab(BottomTab::Problems));
                    ui.close();
                }
            });

            ui.menu_button(i18n::t("menu.flash"), |ui| {
                if ui.button(i18n::t("menu.flash.open")).clicked() {
                    actions.push(UiAction::OpenInFlashPlayer);
                    ui.close();
                }
                if ui.button(i18n::t("menu.flash.runtime_settings")).clicked() {
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
                        RichText::new(i18n::tf(
                            "menu.flash.last_export",
                            &[("path", &path.display().to_string())],
                        ))
                        .small()
                        .color(theme::TEXT_FAINT),
                    );
                }
            });

            ui.menu_button(i18n::t("menu.ai"), |ui| {
                if ui
                    .selectable_label(self.ai.open, i18n::t("menu.ai.editor"))
                    .clicked()
                {
                    actions.push(UiAction::OpenAiEditor);
                    ui.close();
                }
                if ui.button(i18n::t("menu.ai.providers")).clicked() {
                    actions.push(UiAction::OpenAiProviders);
                    ui.close();
                }
                ui.separator();
                ui.label(
                    RichText::new(i18n::t("menu.ai.notice"))
                        .small()
                        .color(theme::TEXT_FAINT),
                );
            });

            ui.menu_button(i18n::t("menu.help"), |ui| {
                if ui.button(i18n::t("menu.help.help_limits")).clicked() {
                    actions.push(UiAction::Help);
                    ui.close();
                }
                if ui.button(i18n::t("menu.help.about")).clicked() {
                    actions.push(UiAction::About);
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("settings.language_wizard")).clicked() {
                    self.wizard = Some(i18n::language());
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

    /// Right dock: Properties on top, project library (tree) below,
    /// separated by a draggable classic divider.
    fn right_dock(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        let total = ui.available_height();
        if total < 80.0 {
            return;
        }
        let top_h = (total * self.right_split).clamp(120.0, total - 120.0);
        let width = ui.available_width();
        let origin = ui.cursor().min;
        let top_rect = egui::Rect::from_min_size(origin, egui::vec2(width, top_h));
        {
            let mut top_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(top_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            actions.extend(properties::show(
                &mut top_ui,
                &mut self.project,
                &mut self.state,
            ));
        }

        // Divider strip between Properties and the library.
        let div_rect = egui::Rect::from_min_size(
            egui::pos2(origin.x, top_rect.bottom()),
            egui::vec2(width, 5.0),
        );
        let div_response = ui.interact(
            div_rect,
            ui.scope_id().with("right_dock_divider"),
            Sense::drag(),
        );
        if div_response.dragged()
            && let Some(pos) = div_response.interact_pointer_pos()
        {
            let rel = (pos.y - origin.y) / total.max(1.0);
            self.right_split = rel.clamp(0.2, 0.8);
        }
        {
            let painter = ui.painter();
            let mid_y = div_rect.center().y;
            painter.rect_filled(
                div_rect,
                egui::CornerRadius::ZERO,
                if div_response.hovered() {
                    theme::BG_HOVER
                } else {
                    theme::BG_INPUT
                },
            );
            painter.line_segment(
                [
                    egui::pos2(div_rect.left(), mid_y),
                    egui::pos2(div_rect.right(), mid_y),
                ],
                egui::Stroke::new(1.0, theme::BORDER_SOFT),
            );
        }
        ui.allocate_rect(div_rect, Sense::hover());

        // Bottom: project tree as the "library" dock.
        let remaining = ui.available_height();
        if remaining > 40.0 {
            let bottom_rect =
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, remaining));
            let mut bottom_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bottom_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            actions.extend(tree::show(
                &mut bottom_ui,
                &mut self.project,
                &mut self.state,
            ));
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, actions: &mut Vec<UiAction>) {
        ui.horizontal(|ui| {
            // Compact classic workspace selector.
            let mut modes = vec![
                (Mode::Code, i18n::t("mode.code")),
                (Mode::Animation, i18n::t("mode.animation")),
            ];
            if edition::has_active_module("classroom") {
                modes.push((Mode::Classroom, i18n::t("mode.classroom")));
            }
            if edition::has_active_module("administration") {
                modes.push((Mode::Enterprise, i18n::t("mode.enterprise")));
            }
            for (mode, label) in modes {
                let active = self.state.mode == mode && !self.ai.open;
                let text = if active {
                    RichText::new(&label).strong().color(theme::ACCENT)
                } else {
                    RichText::new(&label).color(theme::TEXT_DIM)
                };
                if ui
                    .add(egui::Button::new(text).min_size(egui::vec2(0.0, 22.0)))
                    .clicked()
                {
                    actions.push(UiAction::SetMode(mode));
                }
            }
            if ui
                .add(
                    egui::Button::new(RichText::new(i18n::t("toolbar.ai_agent")).color(
                        if self.ai.open {
                            theme::ACCENT
                        } else {
                            theme::TEXT_DIM
                        },
                    ))
                    .min_size(egui::vec2(0.0, 22.0)),
                )
                .on_hover_text(i18n::t("toolbar.ai_agent_tip"))
                .clicked()
            {
                actions.push(UiAction::OpenAiEditor);
            }
            ui.separator();
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

            ui.separator();

            ui.label(RichText::new(&self.project.metadata.name).strong());
            if self.state.dirty {
                ui.label(RichText::new("●").small().color(theme::AMBER))
                    .on_hover_text(i18n::t("toolbar.dirty"));
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
                    .button(RichText::new(i18n::t("toolbar.export")).strong())
                    .on_hover_text(i18n::t("toolbar.export_tip"))
                    .clicked()
                {
                    actions.push(UiAction::Export);
                }
                if ui
                    .button(RichText::new(i18n::t("toolbar.open_fp")).strong())
                    .on_hover_text(i18n::t("toolbar.open_fp_tip"))
                    .clicked()
                {
                    actions.push(UiAction::OpenInFlashPlayer);
                }
                if ui.button(i18n::t("toolbar.save")).clicked() {
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
                (BottomTab::Timeline, i18n::t("dock.tab.timeline")),
                (BottomTab::Tiles, i18n::t("dock.tab.tiles")),
                (BottomTab::Console, i18n::t("dock.tab.console")),
                (
                    BottomTab::Problems,
                    if errors > 0 {
                        i18n::tf(
                            "dock.tab.problems_errors",
                            &[("count", &errors.to_string())],
                        )
                    } else if warnings > 0 {
                        i18n::tf(
                            "dock.tab.problems_warnings",
                            &[("count", &warnings.to_string())],
                        )
                    } else {
                        i18n::t("dock.tab.problems")
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
                        RichText::new(i18n::tf(
                            "dock.scene_stats",
                            &[
                                ("frames", &scene.frame_count().to_string()),
                                ("fps", &format!("{:.0}", scene.fps)),
                            ],
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
            BottomTab::Tiles => {
                self.last_history_label = "Kacheln bearbeiten";
                tilemap_view::show(ui, &mut self.project, &mut self.state);
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
                        RichText::new(i18n::tf(
                            "status.frame_info",
                            &[
                                ("scene", &scene.name),
                                ("frame", &(self.state.frame + 1).to_string()),
                                ("total", &scene.frame_count().to_string()),
                                ("zoom", &format!("{:.0}", self.state.zoom * 100.0)),
                            ],
                        ))
                        .small()
                        .color(theme::TEXT_FAINT),
                    );
                }
                let errors = self.state.error_count();
                if errors > 0 {
                    ui.label(
                        RichText::new(i18n::tf("status.errors", &[("count", &errors.to_string())]))
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
                UiAction::ShowCompatibilityReport => {
                    let report = nova_ai::project_compatibility_report(&self.project);
                    self.state.dialog = Dialog::CompatibilityReport {
                        title: "Projekt-Kompatibilität".into(),
                        report,
                        is_error: false,
                    };
                }
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
                    // Classroom is gated by the edition module registry —
                    // it cannot be reached in User/Enterprise builds.
                    if mode == Mode::Classroom && !edition::has_active_module("classroom") {
                        self.state
                            .set_status("Klassenzimmer ist in dieser Edition nicht verfügbar");
                        continue;
                    }
                    if mode == Mode::Enterprise && !edition::has_active_module("administration") {
                        self.state
                            .set_status("Enterprise-Modus ist in dieser Edition nicht verfügbar");
                        continue;
                    }
                    self.state.mode = mode;
                    if mode == Mode::Code {
                        self.state.playing = false;
                    }
                    self.state.set_status(format!("{}modus", mode.label()));
                }
                UiAction::OpenProjectPath(path) => {
                    self.load_path(&path);
                    self.state.mode = Mode::Animation;
                }
                UiAction::SetBottomTab(tab) => self.state.bottom_tab = tab,
                UiAction::TogglePlay => self.toggle_play(),
                UiAction::Stop => {
                    self.state.playing = false;
                    self.state.frame = 0;
                    self.state.play_accumulator = 0.0;
                    if let Some(runtime) = self.state.game_preview.as_mut() {
                        runtime.stop(&self.project);
                    }
                    self.audio.stop_all();
                    self.state.set_status("Wiedergabe gestoppt");
                }
                UiAction::RestartPlayback => self.restart_playback(),
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
                UiAction::SetTool(tool) => self.set_tool(tool),
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
                        recovery: Box::new(recovery),
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

    fn set_tool(&mut self, tool: Tool) {
        self.state.tool = tool;
        self.state.draft = None;
        self.state.set_status(format!("Werkzeug: {}", tool.label()));
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
                }
                Err(error) => {
                    self.state.log(
                        LogLevel::Warn,
                        format!("Audiovorschau nicht möglich: {error}"),
                    );
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
            self.audio.pause_all();
            if let Some(runtime) = self.state.game_preview.as_mut() {
                runtime.pause();
            }
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
        if let Some(runtime) = self.state.game_preview.as_mut() {
            runtime.start();
        } else {
            self.start_game_preview();
        }
        self.audio.resume_all();
        self.play_sound_cues();
        self.state.set_status("Wiedergabe läuft");
    }

    fn restart_playback(&mut self) {
        self.state.frame = 0;
        self.state.play_accumulator = 0.0;
        self.state.playing = true;
        self.state.mode = Mode::Animation;
        self.audio.stop_all();
        if let Some(runtime) = self.state.game_preview.as_mut() {
            runtime.restart(&self.project);
        } else {
            self.start_game_preview();
        }
        self.play_sound_cues();
        self.state.set_status("Wiedergabe neu gestartet");
    }

    /// (Re)builds the game-preview runtime for the active scene. Only
    /// scenes that carry game metadata (scene config or game objects) get a
    /// runtime; plain timelines keep the classic frame playback.
    fn start_game_preview(&mut self) {
        self.state.game_preview = None;
        if self.state.edit_symbol.is_some() {
            return;
        }
        let Some(scene) = self.project.edit_scene(None) else {
            return;
        };
        let has_game_data = scene.game.is_some()
            || nova_core::game::collect_bodies(scene, &self.project.symbols)
                .iter()
                .any(|body| body.collision.is_some() || body.physics.is_some());
        if !has_game_data {
            return;
        }
        match nova_core::game::runtime::GameRuntime::for_active_scene(&self.project) {
            Ok(mut runtime) => {
                runtime.start();
                self.state.game_preview = Some(runtime);
                self.state
                    .log(LogLevel::Info, "Spiel-Vorschau gestartet (Editor-Runtime).");
            }
            Err(error) => {
                self.state.log(
                    LogLevel::Warn,
                    format!("Spiel-Vorschau nicht gestartet: {error}"),
                );
            }
        }
    }

    /// Maps an egui-style binding name ("ArrowLeft", "Space", "KeyE", "A")
    /// to a live key state.
    fn binding_pressed(ctx: &Context, name: &str) -> bool {
        match name {
            "Shift" => return ctx.input(|input| input.modifiers.shift),
            "Control" | "Ctrl" => return ctx.input(|input| input.modifiers.ctrl),
            _ => {}
        }
        let key = match name {
            "ArrowLeft" | "Left" => Some(Key::ArrowLeft),
            "ArrowRight" | "Right" => Some(Key::ArrowRight),
            "ArrowUp" | "Up" => Some(Key::ArrowUp),
            "ArrowDown" | "Down" => Some(Key::ArrowDown),
            "Space" => Some(Key::Space),
            "Enter" => Some(Key::Enter),
            "Escape" | "Esc" => Some(Key::Escape),
            "Backspace" => Some(Key::Backspace),
            "Tab" => Some(Key::Tab),
            _ => None,
        };
        if let Some(key) = key {
            return ctx.input(|input| input.key_down(key));
        }
        // Single letters and digits: "A".."Z", "KeyA".."KeyZ", "0".."9".
        let stripped = name.strip_prefix("Key").unwrap_or(name);
        if stripped.chars().count() == 1 {
            let c = stripped.chars().next().unwrap();
            if c.is_ascii_alphabetic() {
                let index = c.to_ascii_uppercase() as usize - 'A' as usize;
                if index < 26 {
                    return ctx.input(|input| input.key_down(egui_key_for_letter(index)));
                }
            }
            if c.is_ascii_digit() {
                let index = c.to_digit(10).unwrap() as usize;
                return ctx.input(|input| input.key_down(egui_key_for_digit(index)));
            }
        }
        false
    }

    /// Advances the game runtime one render frame while the timeline plays.
    fn step_game_preview(&mut self, ctx: &Context, dt: f32) {
        // Route external scene switches (scene tree during play) through the
        // runtime so unload/load events and per-scene state memory apply.
        let active_scene = self
            .project
            .edit_scene(None)
            .map(|s| (s.id, s.name.clone()));
        let mut scene_switched = false;
        let mut scene_failed = false;
        if let (Some(runtime), Some((scene_id, scene_name))) =
            (self.state.game_preview.as_mut(), active_scene)
            && runtime.scene != scene_id
        {
            match runtime.goto_scene(&self.project, &scene_name) {
                Ok(()) => scene_switched = true,
                Err(_) => scene_failed = true,
            }
        }
        if scene_failed {
            self.state.game_preview = None;
            return;
        }
        if scene_switched {
            return;
        }
        let Some(runtime) = self.state.game_preview.as_mut() else {
            return;
        };
        let bindings = runtime.config.input.clone().unwrap_or_default();
        let input = nova_core::game::InputState {
            left: Self::binding_pressed(ctx, &bindings.left),
            right: Self::binding_pressed(ctx, &bindings.right),
            up: Self::binding_pressed(ctx, &bindings.up),
            down: Self::binding_pressed(ctx, &bindings.down),
            jump: Self::binding_pressed(ctx, &bindings.jump),
            action: Self::binding_pressed(ctx, &bindings.action),
        };
        runtime.step(
            &self.project,
            &input,
            &|key| Self::binding_pressed(ctx, key),
            dt,
        );
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
        let old_frame = self.state.frame;
        let dt = ctx.input(|input| input.stable_dt).min(0.25);
        self.step_game_preview(ctx, dt);
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
            if self.state.frame != old_frame {
                self.play_sound_cues();
            }
        }
        ctx.request_repaint();
    }

    /// Plays the sound cues scheduled on the current timeline frame of the
    /// active scene. Missing assets and decode failures are logged, never
    /// silent.
    fn play_sound_cues(&mut self) {
        if self.state.edit_symbol.is_some() {
            return;
        }
        let Some(scene) = self.project.edit_scene(None) else {
            return;
        };
        let frame = self.state.frame.min(scene.frame_count().saturating_sub(1));
        let mut missing = Vec::new();
        for (asset, _cue) in crate::audio::cues_on_frame(scene, frame, &self.project.assets) {
            if let Err(error) = self.audio.play(asset) {
                self.state.log(
                    LogLevel::Warn,
                    format!(
                        "Audio '{}' konnte nicht abgespielt werden: {error}",
                        asset.name
                    ),
                );
            }
        }
        for layer in &scene.layers {
            if layer.kind != nova_core::LayerKind::Sound {
                continue;
            }
            let Some(cue) = layer.frames.get(frame).and_then(|f| f.sound) else {
                continue;
            };
            if !self.project.assets.iter().any(|a| a.id == cue.asset) {
                missing.push(cue.asset);
            }
        }
        for id in missing {
            self.state.log(
                LogLevel::Warn,
                format!("Audio-Cue verweist auf fehlendes Asset {id}"),
            );
        }
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
                    ui.heading(i18n::t("dialog.new_project"));
                    ui.label(
                        RichText::new(i18n::t("dialog.new_project_hint"))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    if let Some(reserved) = crate::edition::profile()
                        .modules
                        .iter()
                        .find(|module| module.reserved)
                    {
                        ui.label(
                            RichText::new(format!(
                                "{}: {} (geplant)",
                                branding::app_name(),
                                reserved.label
                            ))
                            .small()
                            .italics()
                            .color(theme::TEXT_FAINT),
                        );
                    }
                    ui.add_space(6.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text(i18n::t("dialog.project_name_hint"))
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
                            .add_enabled(
                                !name.trim().is_empty(),
                                egui::Button::new(i18n::t("dialog.create")),
                            )
                            .clicked()
                        {
                            create = true;
                        }
                        if ui.button(i18n::t("dialog.cancel")).clicked() {
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
                    ui.heading(i18n::t("dialog.unsaved"));
                    ui.label(
                        RichText::new(i18n::t("dialog.unsaved_text"))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(i18n::t("dialog.save")).clicked() {
                            save = true;
                        }
                        if ui.button(i18n::t("dialog.dont_save")).clicked() {
                            discard = true;
                        }
                        if ui.button(i18n::t("dialog.cancel")).clicked() {
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
                    ui.heading(branding::app_name());
                    ui.label(
                        RichText::new(format!(
                            "Version {} \u{00B7} {}",
                            branding::VERSION,
                            branding::audience()
                        ))
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(i18n::tf("about.edition", &[("edition", edition::edition_name())]))
                            .small()
                            .color(theme::ACCENT),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        "Rust · egui/eframe · wgpu · QuickJS\nEigene Timeline, eigenes Projektdatenmodell, eigener SWF-Writer.",
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new(i18n::t("dialog.about_works")).strong());
                    ui.label(
                        RichText::new(
                            "• Bühne, Ebenen, Ebenenordner, Frames, Schlüsselbilder, Motion-/Shape-Tweens, Masken und Onion Skin\n• Text, Bitmaps und Sounds als Projekt-Assets; verschachtelte Symbol-Timelines und Skelett-Animation (SKINNING)\n• Mehrfachauswahl, Ausrichten/Verteilen, Raster-Snapping, Undo/Redo\n• Szenen, Projektvorlagen, Symbolbibliothek, Suchen/Ersetzen und projektweite Textsuche\n• Autosave mit Crash-Recovery\n• Tilemaps, Partikel und Game-Vorschau mit Physik\n• SWF-Export (DefineShape3, DefineSprite, Frame-für-Frame-Backen) sowie FLA/XFL-Import und -Export im dokumentierten Umfang\n• AS3-Teilmenge als echter AVM2-ABC-Bytecode (DoABC2 + SymbolClass)\n• JavaScript-Konsole über QuickJS und optionale KI-Unterstützung (lokal oder Cloud mit Zustimmung)\n• „Open in Flash Player“ mit Adobe-Projector als Standard und optionalem Ruffle",
                        )
                        .small(),
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new(i18n::t("dialog.about_modules")).strong());
                    ui.label(
                        RichText::new(
                            edition::active_modules()
                                .iter()
                                .map(|module| module.label)
                                .collect::<Vec<_>>()
                                .join(" · "),
                        )
                        .small(),
                    );
                    let planned: Vec<_> = edition::profile()
                        .modules
                        .iter()
                        .filter(|module| module.reserved)
                        .map(|module| module.label)
                        .collect();
                    if !planned.is_empty() {
                        ui.label(
                            RichText::new(i18n::tf(
                                "dialog.about_planned",
                                &[("list", &planned.join(" · "))],
                            ))
                            .small()
                            .color(theme::AMBER),
                        );
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new(i18n::t("dialog.about_limits")).strong());
                    ui.label(
                        RichText::new(
                            "• Nur eine dokumentierte AS3-Teilmenge; nicht unterstützte Konstrukte sind Compilerfehler\n• Script-Objekte werden einmal platziert und danach von ActionScript gesteuert; ihre Timeline-Tweens werden nicht zusätzlich gebacken\n• Text nutzt die eingebaute Vektor-Schrift und wird als Shapes exportiert; System-/OpenType-Fonts und editierbarer Laufzeittext fehlen\n• Die Audio-Vorschau spielt jede Cue einmal pro Frame-Eintrag; Offset- und Dauer-Trimming werden in der Vorschau nicht angewendet\n• ABC/SWF strukturell validiert; keine automatische Runtime-/Playback-Prüfung\n• Classroom-, Lehrkräfte-/Schülerrollen und Enterprise-Administration sind noch nicht implementiert\n• Die optionale .nova-Dateizuordnung des Installers ist nicht auf einem frischen System verifiziert; die vollständige Spezifikation sieht „Öffnen mit“-Registrierung und Portable-Opt-in vor",
                        )
                        .small()
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(i18n::t("dialog.open_help")).clicked() {
                            open_help = true;
                            close = true;
                        }
                        if ui.button(i18n::t("dialog.close")).clicked() {
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
                    ui.heading(i18n::t("help.title"));
                    ui.label(
                        RichText::new(i18n::t("help.two_modes"))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new(i18n::t("help.shortcuts")).strong());
                            egui::Grid::new("shortcut_grid")
                                .num_columns(2)
                                .spacing(egui::vec2(16.0, 4.0))
                                .show(ui, |ui| {
                                    for (keys, key_id) in [
                                        ("Strg+N / Strg+O", "shortcut.new_open"),
                                        ("Strg+S / Strg+Shift+S", "shortcut.save"),
                                        ("Strg+E", "shortcut.export"),
                                        ("Strg+F", "shortcut.search"),
                                        ("Strg+Z / Strg+Y", "shortcut.undo"),
                                        ("Strg+C / Strg+V / Strg+D", "shortcut.clipboard"),
                                        ("Entf / Esc", "shortcut.delete"),
                                        ("Leertaste", "shortcut.play"),
                                        ("F5 / Shift+F5", "shortcut.frame"),
                                        ("F6 / Shift+F6", "shortcut.keyframe"),
                                        ("Pfeil links/rechts", "shortcut.frames"),
                                        ("Home / End", "shortcut.firstlast"),
                                    ] {
                                        ui.label(RichText::new(keys).monospace().small());
                                        ui.label(RichText::new(i18n::t(key_id)).small());
                                        ui.end_row();
                                    }
                                });

                            ui.add_space(8.0);
                            ui.label(RichText::new(i18n::t("help.workflow")).strong());
                            ui.label(
                                RichText::new(
                                    "1. Objekte auf der Bühne anlegen und im Eigenschaften-Panel positionieren.\n2. Mit F6 Schlüsselbilder setzen, Objekte bewegen und im Timeline-Kontext oder Eigenschaften-Panel einen Motion-Tween mit Easing und Dauer anlegen.\n3. Mehrfachauswahl per Shift-Klick oder Auswahlrechteck; gemeinsame Transformationen wirken um den Gruppenmittelpunkt.\n4. Skripte im Code-Modus bearbeiten und einem Objekt im Eigenschaften-Panel zuweisen. Ein MovieClip/Sprite-Skript ohne Zuweisung kann als Dokumentklasse dienen.\n5. Mit Strg+E eine SWF schreiben; Skripte werden als DoABC2 eingebettet und per SymbolClass verknüpft.",
                                )
                                .small(),
                            );

                            ui.add_space(8.0);
                            ui.label(RichText::new(i18n::t("help.limits")).strong());
                            ui.label(
                                RichText::new(
                                    "• ActionScript wird für eine dokumentierte Teilmenge zu echtem AVM2-ABC kompiliert. Nicht unterstützte Syntax ist ein Compilerfehler mit Datei/Zeile.\n• Die ABC- und SWF-Struktur wird mit einem unabhängigen Parser validiert. „Open in Flash Player“ startet die konfigurierte Runtime (Adobe-Projector als Standard, Ruffle nur nach ausdrücklicher Auswahl); eine automatische Playback-Prüfung oder eingebettete Runtime gibt es nicht.\n• Skript-Objekte werden einmal platziert und danach von ActionScript gesteuert; ihre Timeline-Tweens werden nicht zusätzlich gebacken.\n• Nicht-Skript-Objekte werden weiterhin Frame für Frame gebacken.\n• Text ist statisch: die eingebaute 5×7-Vektorschrift wird als Shapes exportiert. System-/OpenType-Fonts und editierbarer Laufzeittext fehlen; Bitmaps und Sounds werden als Assets unterstützt.\n• Bitmaps, Sounds, verschachtelte Symbol-Timelines, Masken, Shape-Tweens und Skelett-Animation sind implementiert; die Audio-Vorschau wendet Offset-/Dauer-Trimming nicht an.\n• Array-, Objekt- und Index-Zugriffe in AS3 sind strukturell geprüft, aber nicht gegen eine echte Runtime getestet.\n• Klassenzimmer- (Edu) und Enterprise-Funktionen (Administration, Teams, Assets, Versionen, Reviews, Builds) sind editionsscharf getrennt und ab M20/M21 implementiert; Rollen sind Workflow-Labels, keine Authentifizierung.",
                                )
                                .small()
                                .color(theme::TEXT_DIM),
                            );
                        });
                    ui.add_space(8.0);
                    if ui.button(i18n::t("dialog.close")).clicked() {
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
                    ui.heading(i18n::t("settings.section_general"));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(i18n::t("settings.language")).strong());
                        let mut current = i18n::language();
                        egui::ComboBox::from_id_salt("language_picker")
                            .selected_text(current.native_name())
                            .show_ui(ui, |ui| {
                                for lang in i18n::Lang::ALL {
                                    ui.selectable_value(
                                        &mut current,
                                        *lang,
                                        lang.native_name(),
                                    );
                                }
                            });
                        if current != i18n::language() {
                            i18n::set_language(current);
                        }
                    });
                    ui.label(
                        RichText::new(i18n::t("settings.language_tip"))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    if ui.button(i18n::t("settings.language_wizard")).clicked() {
                        // Close settings and reopen the wizard.
                        self.wizard = Some(i18n::language());
                        self.state.dialog = Dialog::None;
                    }
                    ui.add_space(10.0);
                    theme::hairline(ui);
                    ui.add_space(10.0);
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
        if let Some(selected) = &mut self.wizard {
            if crate::language_wizard::show(ui, selected) {
                i18n::set_language(*selected);
                self.wizard = None;
            }
            ctx.request_repaint();
            return;
        }
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
        egui::Panel::top("toolbar")
            .exact_size(36.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_PANEL_ALT)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER_SOFT)),
            )
            .show(ui, |ui| {
                self.toolbar(ui, &mut actions);
            });

        // M22 classic layout: no mode rail — mode selection lives in the
        // toolbar (compact segmented control) and the menus.

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
        } else if self.state.mode == Mode::Classroom {
            // Classroom mode owns the full central area; project panels are
            // hidden (classroom data is not part of the project document).
            egui::CentralPanel::default().show(ui, |ui| {
                let json = self.project.to_json().ok().map(String::into_bytes);
                actions.extend(classroom_view::show(
                    ui,
                    &mut self.edu,
                    json.as_deref(),
                    &self.project.metadata.name,
                ));
            });
        } else if self.state.mode == Mode::Enterprise {
            // Enterprise administration owns the full central area.
            egui::CentralPanel::default().show(ui, |ui| {
                let path = self.state.path.clone();
                actions.extend(enterprise_view::show(
                    ui,
                    &mut self.ent,
                    path.as_deref(),
                    &self.project.metadata.name,
                ));
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

            // Classic 2009 layout: narrow toolbox far left (Animation
            // mode only — code mode has no stage tools), a single right
            // dock with Properties on top and the project library below,
            // and the timeline dock at the bottom.
            if self.state.mode == Mode::Animation {
                egui::Panel::left("toolbox")
                    .exact_size(48.0)
                    .frame(egui::Frame::new().fill(theme::BG_DEEPEST))
                    .show(ui, |ui| {
                        actions.extend(toolbox::show(ui, &mut self.state));
                    });
            }

            egui::Panel::right("right_dock")
                .resizable(true)
                .default_size(320.0)
                .size_range(240.0..=560.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme::BG_PANEL)
                        .stroke(egui::Stroke::new(1.0, theme::BORDER_SOFT)),
                )
                .show(ui, |ui| {
                    self.right_dock(ui, &mut actions);
                });

            egui::CentralPanel::default().show(ui, |ui| match self.state.mode {
                Mode::Classroom => {
                    let json = self.project.to_json().ok().map(String::into_bytes);
                    actions.extend(classroom_view::show(
                        ui,
                        &mut self.edu,
                        json.as_deref(),
                        &self.project.metadata.name,
                    ));
                }
                Mode::Enterprise => {
                    let path = self.state.path.clone();
                    actions.extend(enterprise_view::show(
                        ui,
                        &mut self.ent,
                        path.as_deref(),
                        &self.project.metadata.name,
                    ));
                }
                Mode::Code => {
                    actions.extend(code_view::show(ui, &mut self.project, &mut self.state));
                }
                Mode::Animation => {
                    actions.extend(stage_view::show(ui, &mut self.project, &mut self.state));
                }
            });
        }

        self.refresh_asset_textures(&ctx);
        self.audio.set_volume(self.state.preview_volume);
        self.audio.prune();
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
