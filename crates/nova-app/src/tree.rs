//! Left-hand project tree: scene, layers, library, scripts and assets.

use egui::{Color32, RichText, Ui};
use nova_core::Project;
use uuid::Uuid;

use crate::state::{AppState, BottomTab, Mode, Selection, UiAction};
use crate::theme;

pub fn show(ui: &mut Ui, project: &mut Project, state: &mut AppState) -> Vec<UiAction> {
    let mut actions = Vec::new();

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Projekt").strong().size(14.0));
        if state.dirty {
            ui.label(RichText::new("●").color(theme::AMBER).small());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+").on_hover_text("Neues Skript").clicked() {
                actions.push(UiAction::AddScript);
            }
        });
    });
    ui.label(
        RichText::new(&project.metadata.name)
            .small()
            .color(theme::TEXT_DIM),
    );

    theme::hairline(ui);

    // Scene selector ------------------------------------------------------
    theme::section(ui, "Szene");
    if project.scenes.len() > 1 {
        let current = project
            .active_scene()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("scene_selector")
            .selected_text(current)
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                let scenes: Vec<(Uuid, String)> = project
                    .scenes
                    .iter()
                    .map(|scene| (scene.id, scene.name.clone()))
                    .collect();
                for (id, name) in scenes {
                    if ui
                        .selectable_label(project.active_scene == Some(id), name)
                        .clicked()
                    {
                        actions.push(UiAction::SelectScene(id));
                    }
                }
            });
    } else if let Some(scene) = project.active_scene() {
        ui.label(
            RichText::new(format!(
                "{} · {:.0}×{:.0}",
                scene.name, scene.width, scene.height
            ))
            .small()
            .color(theme::TEXT_DIM),
        );
    }

    let is_scene = matches!(state.selection, Selection::Scene);
    if ui
        .selectable_label(is_scene, "  Bühne & Eigenschaften")
        .clicked()
    {
        state.selection = Selection::Scene;
    }

    // Layers --------------------------------------------------------------
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("EBENEN")
                .small()
                .strong()
                .color(theme::TEXT_FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+").on_hover_text("Neue Ebene").clicked() {
                actions.push(UiAction::AddLayer);
            }
        });
    });

    struct LayerInfo {
        index: usize,
        id: Uuid,
        name: String,
        visible: bool,
        locked: bool,
        kind: String,
    }

    let layers: Vec<LayerInfo> = project
        .active_scene()
        .map(|scene| {
            scene
                .layers
                .iter()
                .enumerate()
                .rev()
                .map(|(index, layer)| LayerInfo {
                    index,
                    id: layer.id,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    locked: layer.locked,
                    kind: layer.kind.label().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    for layer in layers {
        ui.horizontal(|ui| {
            let mut visible = layer.visible;
            let eye = if visible { "●" } else { "○" };
            if ui
                .small_button(RichText::new(eye).color(if visible {
                    theme::ACCENT
                } else {
                    theme::TEXT_FAINT
                }))
                .on_hover_text("Sichtbarkeit umschalten")
                .clicked()
            {
                visible = !visible;
                if let Some(scene) = project.active_scene_mut() {
                    if let Some(layer) = scene.layers.get_mut(layer.index) {
                        layer.visible = visible;
                    }
                }
                state.dirty = true;
            }

            let mut locked = layer.locked;
            if ui
                .small_button(RichText::new("L").small().color(if locked {
                    theme::AMBER
                } else {
                    theme::TEXT_FAINT
                }))
                .on_hover_text("Ebene sperren")
                .clicked()
            {
                locked = !locked;
                if let Some(scene) = project.active_scene_mut() {
                    if let Some(layer) = scene.layers.get_mut(layer.index) {
                        layer.locked = locked;
                    }
                }
                state.dirty = true;
            }

            let selected = state.selection == Selection::Layer(layer.id);
            let mut text = RichText::new(&layer.name);
            if !layer.visible {
                text = text.color(theme::TEXT_FAINT);
            }
            if layer.locked {
                text = text.italics();
            }
            let response = ui.selectable_label(selected, text);
            if response.clicked() {
                state.selection = Selection::Layer(layer.id);
                state.set_status(format!("Ebene '{}' ausgewählt", layer.name));
            }
            if response.double_clicked() {
                state.selection = Selection::Layer(layer.id);
                actions.push(UiAction::SetMode(Mode::Animation));
            }
            if layer.kind != "Normal" {
                ui.label(RichText::new(&layer.kind).small().color(theme::TEXT_FAINT));
            }
        });
    }

    // Library -------------------------------------------------------------
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("BIBLIOTHEK")
                .small()
                .strong()
                .color(theme::TEXT_FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+").on_hover_text("Neues Symbol").clicked() {
                actions.push(UiAction::AddSymbol);
            }
        });
    });

    if project.symbols.is_empty() {
        ui.label(
            RichText::new("Noch keine Symbole")
                .small()
                .italics()
                .color(theme::TEXT_FAINT),
        );
    }
    let symbols: Vec<(Uuid, String, Color32)> = project
        .symbols
        .iter()
        .map(|symbol| {
            (
                symbol.id,
                symbol.name.clone(),
                Color32::from_rgba_unmultiplied(
                    symbol.fill.r,
                    symbol.fill.g,
                    symbol.fill.b,
                    symbol.fill.a,
                ),
            )
        })
        .collect();
    for (id, name, color) in symbols {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, color);
            let selected = state.selection == Selection::Symbol(id);
            if ui
                .selectable_label(selected, RichText::new(name).small())
                .clicked()
            {
                state.selection = Selection::Symbol(id);
            }
        });
    }

    // Scripts -------------------------------------------------------------
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("SKRIPTE")
                .small()
                .strong()
                .color(theme::TEXT_FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("+")
                .on_hover_text("Neues ActionScript")
                .clicked()
            {
                actions.push(UiAction::AddScript);
            }
        });
    });

    let scripts: Vec<(Uuid, String)> = project
        .scripts
        .iter()
        .map(|script| (script.id, script.path.clone()))
        .collect();
    for (id, path) in scripts {
        let selected = state.active_script == Some(id);
        let response = ui.selectable_label(selected, RichText::new(&path).small());
        if response.clicked() {
            actions.push(UiAction::OpenScript(id));
        }
        if state.diagnostics_for == Some(id) && !state.diagnostics.is_empty() {
            let errors = state.error_count();
            let warnings = state.warning_count();
            let color = if errors > 0 { theme::RED } else { theme::AMBER };
            let label = if errors > 0 {
                format!("{errors} F")
            } else {
                format!("{warnings} W")
            };
            ui.label(RichText::new(label).small().color(color));
        }
    }
    if project.scripts.is_empty() {
        ui.label(
            RichText::new("Keine Skripte")
                .small()
                .italics()
                .color(theme::TEXT_FAINT),
        );
    }

    // Assets --------------------------------------------------------------
    ui.add_space(6.0);
    ui.label(
        RichText::new("ASSETS")
            .small()
            .strong()
            .color(theme::TEXT_FAINT),
    );
    if project.assets.is_empty() {
        ui.label(
            RichText::new("Keine Assets · Import folgt")
                .small()
                .italics()
                .color(theme::TEXT_FAINT),
        )
        .on_hover_text("Bitmaps, Sounds und Fonts sind noch nicht implementiert.");
    } else {
        for asset in &project.assets {
            ui.label(RichText::new(&asset.name).small().color(theme::TEXT_DIM));
        }
    }

    ui.add_space(8.0);
    theme::hairline(ui);
    if let Some(scene) = project.active_scene() {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} Frames · {:.0} fps · {} Ebenen",
                    scene.frame_count(),
                    scene.fps,
                    scene.layers.len()
                ))
                .small()
                .color(theme::TEXT_FAINT),
            );
        });
    }
    if ui
        .small_button("Konsole anzeigen")
        .on_hover_text("Editor-Konsole und JavaScript-Werkzeuge")
        .clicked()
    {
        state.bottom_tab = BottomTab::Console;
    }

    actions
}
