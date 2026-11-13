//! Right-hand properties inspector for the scene, layers, objects and symbols.

use egui::{RichText, ScrollArea, Ui};
use nova_core::{LayerKind, ObjectKind, Project, Rgba, SwfCompression, TargetRuntime};
use uuid::Uuid;

use crate::state::{AppState, Selection, UiAction};
use crate::theme;
use crate::util;

pub fn show(ui: &mut Ui, project: &mut Project, state: &mut AppState) -> Vec<UiAction> {
    let mut actions = Vec::new();

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Eigenschaften").strong().size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let kind = match &state.selection {
                Selection::None => "Projekt",
                Selection::Scene => "Szene",
                Selection::Layer(_) => "Ebene",
                Selection::Objects(ids) => {
                    if ids.len() == 1 {
                        "Objekt"
                    } else {
                        "Mehrfachauswahl"
                    }
                }
                Selection::Symbol(_) => "Symbol",
            };
            ui.label(RichText::new(kind).small().color(theme::TEXT_FAINT));
        });
    });
    theme::hairline(ui);

    ScrollArea::vertical()
        .id_salt("properties_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let selection = state.selection.clone();
            match &selection {
                Selection::None => project_info(ui, project, state),
                Selection::Scene => scene_properties(ui, project, state),
                Selection::Layer(id) => layer_properties(ui, project, state, *id, &mut actions),
                Selection::Objects(ids) if ids.len() == 1 => {
                    object_properties(ui, project, state, ids[0], &mut actions)
                }
                Selection::Objects(ids) => {
                    multi_object_properties(ui, project, state, ids, &mut actions)
                }
                Selection::Symbol(id) => symbol_properties(ui, project, state, *id, &mut actions),
            }
        });

    actions
}

fn project_info(ui: &mut Ui, project: &Project, state: &mut AppState) {
    theme::section(ui, "Projekt");
    ui.label(RichText::new(&project.metadata.name).strong());
    if !project.metadata.description.is_empty() {
        ui.label(
            RichText::new(&project.metadata.description)
                .small()
                .color(theme::TEXT_DIM),
        );
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "{} Szenen · {} Symbole · {} Skripte · {} Assets",
            project.scenes.len(),
            project.symbols.len(),
            project.scripts.len(),
            project.assets.len()
        ))
        .small()
        .color(theme::TEXT_DIM),
    );
    if let Some(scene) = project.active_scene() {
        ui.label(
            RichText::new(format!(
                "Aktiv: {} · {:.0}×{:.0} px · {:.0} fps · {} Frames",
                scene.name,
                scene.width,
                scene.height,
                scene.fps,
                scene.frame_count()
            ))
            .small()
            .color(theme::TEXT_DIM),
        );
    }

    ui.add_space(6.0);
    if ui.button("Szene auswählen").clicked() {
        state.selection = Selection::Scene;
    }
    if ui
        .button("Bühne einpassen")
        .on_hover_text("Zoom und Kamera auf die Bühne zurücksetzen")
        .clicked()
    {
        state.zoom_initialized = false;
    }
    theme::hairline(ui);
    ui.label(
        RichText::new(
            "Tipp: Objekte auf der Bühne anklicken oder in der Timeline ein Schlüsselbild wählen.",
        )
        .small()
        .color(theme::TEXT_FAINT),
    );
}

fn scene_properties(ui: &mut Ui, project: &mut Project, state: &mut AppState) {
    let index = project.active_scene_index();
    let mut changed = false;

    {
        let scene = &mut project.scenes[index];
        theme::section(ui, "Szene");
        egui::Grid::new("scene_grid")
            .num_columns(2)
            .spacing(egui::vec2(8.0, 6.0))
            .show(ui, |ui| {
                ui.label("Name");
                changed |= ui.text_edit_singleline(&mut scene.name).changed();
                ui.end_row();

                ui.label("Breite");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut scene.width)
                            .range(1.0..=8192.0)
                            .speed(1.0)
                            .suffix(" px"),
                    )
                    .changed();
                ui.end_row();

                ui.label("Höhe");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut scene.height)
                            .range(1.0..=8192.0)
                            .speed(1.0)
                            .suffix(" px"),
                    )
                    .changed();
                ui.end_row();

                ui.label("FPS");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut scene.fps)
                            .range(1.0..=120.0)
                            .speed(0.25),
                    )
                    .changed();
                ui.end_row();

                ui.label("Hintergrund");
                let mut color = util::to_color32(scene.background);
                ui.horizontal(|ui| {
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        scene.background = util::from_color32(color);
                        changed = true;
                    }
                    ui.label(
                        RichText::new(scene.background.to_hex())
                            .small()
                            .monospace()
                            .color(theme::TEXT_FAINT),
                    );
                });
                ui.end_row();

                ui.label("Frames");
                let mut frames = scene.frame_count() as u32;
                let minimum = frames.max(1);
                if ui
                    .add(
                        egui::DragValue::new(&mut frames)
                            .range(minimum..=10000)
                            .speed(1.0),
                    )
                    .on_hover_text("Verlängert alle Ebenen. Kürzen ist noch nicht implementiert.")
                    .changed()
                {
                    scene.ensure_frame_count(frames as usize);
                    changed = true;
                }
                ui.end_row();
            });
    }

    if changed {
        project.touch();
        state.dirty = true;
    }

    ui.add_space(4.0);
    export_properties(ui, project, state);
}

fn export_properties(ui: &mut Ui, project: &mut Project, state: &mut AppState) {
    let mut changed = false;
    theme::section(ui, "SWF-Export");
    egui::Grid::new("export_grid")
        .num_columns(2)
        .spacing(egui::vec2(8.0, 6.0))
        .show(ui, |ui| {
            ui.label("Ziel");
            let export = &mut project.export;
            egui::ComboBox::from_id_salt("export_target")
                .selected_text(export.target.label())
                .show_ui(ui, |ui| {
                    for target in [TargetRuntime::Avm2, TargetRuntime::Avm1] {
                        if ui
                            .selectable_label(export.target == target, target.label())
                            .clicked()
                        {
                            export.target = target;
                            changed = true;
                        }
                    }
                });
            ui.end_row();

            ui.label("SWF-Version");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut project.export.swf_version)
                        .range(6..=12)
                        .speed(0.1),
                )
                .changed();
            ui.end_row();

            ui.label("Kompression");
            egui::ComboBox::from_id_salt("export_compression")
                .selected_text(match project.export.compression {
                    SwfCompression::None => "Keine",
                    SwfCompression::Zlib => "Zlib",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            project.export.compression == SwfCompression::None,
                            "Keine",
                        )
                        .clicked()
                    {
                        project.export.compression = SwfCompression::None;
                        changed = true;
                    }
                    if ui
                        .selectable_label(
                            project.export.compression == SwfCompression::Zlib,
                            "Zlib",
                        )
                        .clicked()
                    {
                        project.export.compression = SwfCompression::Zlib;
                        changed = true;
                    }
                });
            ui.end_row();

            ui.label("Skripte");
            changed |= ui
                .checkbox(&mut project.export.include_scripts, "Skripte einbeziehen")
                .on_hover_text(
                    "Kompiliert die AS3-Teilmenge zu ABC und bettet sie per DoABC2/SymbolClass ein.",
                )
                .changed();
            ui.end_row();
        });

    if changed {
        project.touch();
        state.dirty = true;
    }

    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Export backt Nicht-Skript-Objekte Frame für Frame. Zugewiesene AS3-Klassen werden als ABC eingebettet; Sounds, Bitmaps und Text bleiben außen vor.",
        )
        .small()
        .color(theme::TEXT_FAINT),
    );
}

fn layer_properties(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut AppState,
    id: Uuid,
    actions: &mut Vec<UiAction>,
) {
    let mut changed = false;
    let mut found = false;

    if let Some(scene) = project.active_scene_mut() {
        if let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == id) {
            found = true;
            theme::section(ui, "Ebene");
            egui::Grid::new("layer_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Name");
                    changed |= ui.text_edit_singleline(&mut layer.name).changed();
                    ui.end_row();

                    ui.label("Sichtbar");
                    changed |= ui.checkbox(&mut layer.visible, "Sichtbar").changed();
                    ui.end_row();

                    ui.label("Gesperrt");
                    changed |= ui.checkbox(&mut layer.locked, "Gesperrt").changed();
                    ui.end_row();

                    ui.label("Typ");
                    let mut kind = layer.kind;
                    egui::ComboBox::from_id_salt("layer_kind")
                        .selected_text(kind.label())
                        .show_ui(ui, |ui| {
                            for candidate in [
                                LayerKind::Normal,
                                LayerKind::Folder,
                                LayerKind::Guide,
                                LayerKind::Mask,
                            ] {
                                if ui
                                    .selectable_label(kind == candidate, candidate.label())
                                    .clicked()
                                {
                                    kind = candidate;
                                }
                            }
                        });
                    if kind != layer.kind {
                        layer.kind = kind;
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Frames");
                    ui.label(
                        RichText::new(layer.frames.len().to_string())
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.end_row();
                });

            let keyframe = layer.last_keyframe_at_or_before(state.frame);
            let next = layer.next_keyframe_after(state.frame);
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Aktuell: Frame {} · Startschlüsselbild {}{}",
                    state.frame + 1,
                    keyframe
                        .map(|index| (index + 1).to_string())
                        .unwrap_or_else(|| "-".into()),
                    next.map(|index| format!(" · nächstes {index}"))
                        .unwrap_or_default()
                ))
                .small()
                .color(theme::TEXT_FAINT),
            );

            if let Some(start) = keyframe {
                let end = layer.next_keyframe_after(start);
                let duration = end.map(|end| end - start + 1).unwrap_or(1);
                let has_tween = layer.frames[start].tween.is_some();
                ui.add_space(4.0);
                theme::section(ui, "Motion-Tween");
                if has_tween {
                    let easing = layer.frames[start]
                        .tween
                        .as_ref()
                        .map(|tween| tween.easing)
                        .unwrap_or_default();
                    let mut selected = easing;
                    egui::Grid::new("tween_grid")
                        .num_columns(2)
                        .spacing(egui::vec2(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.label("Easing");
                            egui::ComboBox::from_id_salt("tween_easing")
                                .selected_text(selected.label())
                                .show_ui(ui, |ui| {
                                    for candidate in [
                                        nova_core::Easing::Linear,
                                        nova_core::Easing::EaseIn,
                                        nova_core::Easing::EaseOut,
                                        nova_core::Easing::EaseInOut,
                                    ] {
                                        if ui
                                            .selectable_label(
                                                selected == candidate,
                                                candidate.label(),
                                            )
                                            .clicked()
                                        {
                                            selected = candidate;
                                        }
                                    }
                                });
                            if selected != easing {
                                actions.push(UiAction::SetTweenEasing(selected));
                            }
                            ui.end_row();

                            ui.label("Dauer");
                            let mut frames = duration;
                            if let Some(end) = end {
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut frames)
                                            .range(2..=999)
                                            .speed(1.0)
                                            .suffix(" Frames"),
                                    )
                                    .changed()
                                {
                                    let new_end = start + frames - 1;
                                    layer.ensure_frames(new_end + 1);
                                    let end_frame = layer.frames[end].clone();
                                    layer.frames[end] = nova_core::Frame::default();
                                    layer.frames[new_end] = end_frame;
                                    changed = true;
                                }
                            } else {
                                ui.label(
                                    RichText::new("Kein End-Schlüsselbild")
                                        .small()
                                        .color(theme::AMBER),
                                );
                            }
                            ui.end_row();
                        });
                } else {
                    ui.label(
                        RichText::new("Zwischen diesem und dem nächsten Schlüsselbild wird nicht interpoliert.")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Tween erstellen").clicked() {
                        actions.push(UiAction::CreateTween);
                    }
                    if ui
                        .add_enabled(has_tween, egui::Button::new("Tween entfernen"))
                        .clicked()
                    {
                        actions.push(UiAction::RemoveTween);
                    }
                });
            }
        }
    }

    if !found {
        ui.label(RichText::new("Ebene nicht gefunden").color(theme::RED));
        return;
    }

    if changed {
        project.touch();
        state.dirty = true;
    }

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Schlüsselbild").clicked() {
            actions.push(UiAction::InsertKeyframe);
        }
        if ui.button("Leeres KB").clicked() {
            actions.push(UiAction::InsertBlankKeyframe);
        }
        if ui.button("KB entfernen").clicked() {
            actions.push(UiAction::RemoveKeyframe);
        }
    });
    ui.add_space(4.0);
    if ui
        .button(RichText::new("Ebene löschen").color(theme::RED))
        .clicked()
    {
        actions.push(UiAction::DeleteSelection);
    }
    ui.label(
        RichText::new("F6 fügt ein Schlüsselbild ein, Shift+F6 ein leeres.")
            .small()
            .color(theme::TEXT_FAINT),
    );
}

fn object_properties(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut AppState,
    id: Uuid,
    actions: &mut Vec<UiAction>,
) {
    let Some(resolved) = state.resolved_object(project, id) else {
        ui.label(
            RichText::new("Das Objekt ist im aktuellen Frame nicht sichtbar.")
                .small()
                .color(theme::TEXT_DIM),
        );
        if let Some(scene) = project.active_scene() {
            let _ = scene;
        }
        return;
    };

    let layer_id = resolved.layer_id;
    let instance = resolved.instance.clone();
    let size = util::object_size(&instance, &project.symbols).unwrap_or((40.0, 40.0));

    let (keyframe, next_keyframe, on_keyframe) = project
        .active_scene()
        .and_then(|scene| {
            scene
                .layers
                .iter()
                .find(|layer| layer.id == layer_id)
                .map(|layer| {
                    (
                        layer.last_keyframe_at_or_before(state.frame),
                        layer.next_keyframe_after(state.frame),
                        layer
                            .frames
                            .get(state.frame)
                            .is_some_and(|frame| frame.keyframe),
                    )
                })
        })
        .unwrap_or((None, None, false));

    theme::section(ui, "Objekt");
    ui.label(RichText::new(&instance.name).strong());
    ui.label(
        RichText::new(format!(
            "{} · {}{}",
            instance.kind.type_name(),
            if on_keyframe {
                "Schlüsselbild"
            } else {
                "gehalten / Tween"
            },
            keyframe
                .map(|index| format!(" · Start {}", index + 1))
                .unwrap_or_default()
        ))
        .small()
        .color(theme::TEXT_FAINT),
    );
    if !on_keyframe && next_keyframe.is_some() {
        ui.label(
            RichText::new("Änderungen schreiben in das Start-Schlüsselbild.")
                .small()
                .color(theme::AMBER),
        );
    }

    let mut name = instance.name.clone();
    let mut x = instance.transform.x;
    let mut y = instance.transform.y;
    let mut scale_x = instance.transform.scale_x;
    let mut scale_y = instance.transform.scale_y;
    let mut rotation = instance.transform.rotation;
    let mut fill = instance.fill;
    let mut script = instance.script;
    let mut width = size.0;
    let mut height = size.1;
    let mut corner_radius = match &instance.kind {
        ObjectKind::Rect { corner_radius, .. } => *corner_radius,
        _ => 0.0,
    };
    let mut changed = false;

    egui::Grid::new("object_grid")
        .num_columns(2)
        .spacing(egui::vec2(8.0, 6.0))
        .show(ui, |ui| {
            ui.label("Name");
            changed |= ui.text_edit_singleline(&mut name).changed();
            ui.end_row();

            ui.label("X");
            changed |= ui
                .add(egui::DragValue::new(&mut x).speed(1.0).suffix(" px"))
                .changed();
            ui.end_row();

            ui.label("Y");
            changed |= ui
                .add(egui::DragValue::new(&mut y).speed(1.0).suffix(" px"))
                .changed();
            ui.end_row();

            ui.label("Skalierung X");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut scale_x)
                        .range(-20.0..=20.0)
                        .speed(0.01),
                )
                .changed();
            ui.end_row();

            ui.label("Skalierung Y");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut scale_y)
                        .range(-20.0..=20.0)
                        .speed(0.01),
                )
                .changed();
            ui.end_row();

            ui.label("Rotation");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut rotation)
                        .range(-3600.0..=3600.0)
                        .speed(0.5)
                        .suffix("°"),
                )
                .changed();
            ui.end_row();

            ui.label("Füllung");
            let mut color = util::to_color32(fill);
            ui.horizontal(|ui| {
                if ui.color_edit_button_srgba(&mut color).changed() {
                    fill = util::from_color32(color);
                    changed = true;
                }
                ui.label(
                    RichText::new(fill.to_hex())
                        .small()
                        .monospace()
                        .color(theme::TEXT_FAINT),
                );
            });
            ui.end_row();

            ui.label("Deckkraft");
            let mut alpha = fill.a;
            if ui
                .add(
                    egui::DragValue::new(&mut alpha)
                        .range(0..=255)
                        .speed(1.0)
                        .suffix(" / 255"),
                )
                .changed()
            {
                fill.a = alpha;
                changed = true;
            }
            ui.end_row();
        });

    // Geometry ---------------------------------------------------------
    ui.add_space(4.0);
    theme::section(ui, "Geometrie");
    match &instance.kind {
        ObjectKind::Rect { .. } => {
            egui::Grid::new("rect_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Breite");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut width)
                                .range(0.5..=8192.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Höhe");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut height)
                                .range(0.5..=8192.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Eckenradius");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut corner_radius)
                                .range(0.0..=4096.0)
                                .speed(0.5)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                });
        }
        ObjectKind::Ellipse { .. } => {
            egui::Grid::new("ellipse_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Breite");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut width)
                                .range(0.5..=8192.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Höhe");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut height)
                                .range(0.5..=8192.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                });
        }
        ObjectKind::Path { .. } => {
            egui::Grid::new("path_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Breite");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut width)
                                .range(0.5..=8192.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Höhe");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut height)
                                .range(0.5..=8192.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                });
            ui.label(
                RichText::new(
                    "Importierte Vektorgrafik: Pfade bleiben erhalten; Breite/Höhe sind die Auswahlgrenzen.",
                )
                .small()
                .color(theme::TEXT_FAINT),
            );
        }
        ObjectKind::Symbol { symbol } => {
            if let Some(definition) = project.symbol(*symbol) {
                ui.label(
                    RichText::new(format!(
                        "Symbol '{}' · {:.0}×{:.0} px",
                        definition.name, definition.width, definition.height
                    ))
                    .small()
                    .color(theme::TEXT_DIM),
                );
            }
            ui.label(
                RichText::new("Größe und Farbe werden in der Bibliothek bearbeitet.")
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        }
    }

    // Script link ------------------------------------------------------
    ui.add_space(4.0);
    theme::section(ui, "Skript");
    let script_label = script
        .and_then(|script_id| project.script_path(script_id))
        .unwrap_or("Kein Skript verknüpft")
        .to_string();
    egui::ComboBox::from_id_salt("object_script")
        .width(ui.available_width() - 12.0)
        .selected_text(script_label)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(script.is_none(), "Kein Skript")
                .clicked()
            {
                script = None;
                changed = true;
            }
            for candidate in &project.scripts {
                if ui
                    .selectable_label(script == Some(candidate.id), &candidate.path)
                    .clicked()
                {
                    script = Some(candidate.id);
                    changed = true;
                }
            }
        });

    ui.horizontal_wrapped(|ui| {
        if let Some(script_id) = script {
            if ui.button("Skript öffnen").clicked() {
                actions.push(UiAction::OpenScript(script_id));
            }
            if ui.button("Prüfen").clicked() {
                actions.push(UiAction::CheckScript(script_id));
            }
        }
    });

    if changed {
        let kind = match &instance.kind {
            ObjectKind::Rect { .. } => ObjectKind::Rect {
                width,
                height,
                corner_radius,
            },
            ObjectKind::Ellipse { .. } => ObjectKind::Ellipse { width, height },
            ObjectKind::Path { paths, .. } => ObjectKind::Path {
                width,
                height,
                paths: paths.clone(),
            },
            ObjectKind::Symbol { symbol } => ObjectKind::Symbol { symbol: *symbol },
        };
        if let Some(scene) = project.active_scene_mut() {
            if let Some(object) = util::governing_object_mut(scene, layer_id, state.frame, id) {
                object.name = name;
                object.transform.x = x;
                object.transform.y = y;
                object.transform.scale_x = scale_x;
                object.transform.scale_y = scale_y;
                object.transform.rotation = rotation;
                object.fill = fill;
                object.kind = kind;
                object.script = script;
            }
        }
        project.touch();
        state.dirty = true;
    }

    ui.add_space(6.0);
    if ui
        .button(RichText::new("Objekt löschen").color(theme::RED))
        .on_hover_text("Entfernt das Objekt aus allen Frames")
        .clicked()
    {
        actions.push(UiAction::DeleteSelection);
    }
}

fn multi_object_properties(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut AppState,
    ids: &[Uuid],
    actions: &mut Vec<UiAction>,
) {
    theme::section(ui, "Mehrfachauswahl");
    ui.label(
        RichText::new(format!("{} Objekte ausgewählt", ids.len()))
            .strong()
            .color(theme::ACCENT),
    );
    ui.label(
        RichText::new(
            "Verschieben, Skalieren und Rotieren wirken gemeinsam um den Mittelpunkt der Auswahl.",
        )
        .small()
        .color(theme::TEXT_FAINT),
    );

    let mut targets = Vec::new();
    for id in ids {
        if let Some(resolved) = state.resolved_object(project, *id) {
            targets.push((
                *id,
                resolved.layer_id,
                resolved.instance.transform,
                resolved.instance.fill,
            ));
        }
    }
    if targets.is_empty() {
        ui.label(
            RichText::new("Keines der ausgewählten Objekte ist in diesem Frame sichtbar.")
                .small()
                .color(theme::AMBER),
        );
        if ui
            .button(RichText::new("Auswahl löschen").color(theme::RED))
            .clicked()
        {
            actions.push(UiAction::DeleteSelection);
        }
        return;
    }

    let count = targets.len() as f32;
    let center = targets.iter().fold(egui::Pos2::ZERO, |sum, target| {
        sum + egui::vec2(target.2.x, target.2.y)
    }) / count;

    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    let mut factor = 1.0f32;
    let mut angle = 0.0f32;
    let first_fill = targets[0].3;
    let mut fill_color = util::to_color32(first_fill);
    let mut color_changed = false;
    let mut alpha = first_fill.a;
    let mut alpha_changed = false;
    let mut script_choice: Option<Option<Uuid>> = None;

    egui::Grid::new("multi_object_grid")
        .num_columns(2)
        .spacing(egui::vec2(8.0, 6.0))
        .show(ui, |ui| {
            ui.label("X verschieben");
            ui.add(
                egui::DragValue::new(&mut dx)
                    .range(-8192.0..=8192.0)
                    .speed(1.0)
                    .suffix(" px"),
            );
            ui.end_row();
            ui.label("Y verschieben");
            ui.add(
                egui::DragValue::new(&mut dy)
                    .range(-8192.0..=8192.0)
                    .speed(1.0)
                    .suffix(" px"),
            );
            ui.end_row();
            ui.label("Skalieren");
            ui.add(
                egui::DragValue::new(&mut factor)
                    .range(0.01..=20.0)
                    .speed(0.01)
                    .suffix(" ×"),
            );
            ui.end_row();
            ui.label("Rotieren");
            ui.add(
                egui::DragValue::new(&mut angle)
                    .range(-3600.0..=3600.0)
                    .speed(0.5)
                    .suffix("°"),
            );
            ui.end_row();
            ui.label("Gemeinsame Füllung");
            ui.horizontal(|ui| {
                if ui.color_edit_button_srgba(&mut fill_color).changed() {
                    color_changed = true;
                }
                ui.label(
                    RichText::new(util::from_color32(fill_color).to_hex())
                        .small()
                        .monospace()
                        .color(theme::TEXT_FAINT),
                );
            });
            ui.end_row();
            ui.label("Deckkraft");
            if ui
                .add(
                    egui::DragValue::new(&mut alpha)
                        .range(0..=255)
                        .speed(1.0)
                        .suffix(" / 255"),
                )
                .changed()
            {
                alpha_changed = true;
            }
            ui.end_row();
            ui.label("Skript");
            egui::ComboBox::from_id_salt("multi_object_script")
                .selected_text("Skript zuweisen …")
                .show_ui(ui, |ui| {
                    if ui.selectable_label(false, "Kein Skript").clicked() {
                        script_choice = Some(None);
                    }
                    for script in &project.scripts {
                        if ui.selectable_label(false, &script.path).clicked() {
                            script_choice = Some(Some(script.id));
                        }
                    }
                });
            ui.end_row();
        });

    let translate = dx.abs() > f32::EPSILON || dy.abs() > f32::EPSILON;
    let scale = (factor - 1.0).abs() > f32::EPSILON;
    let rotate = angle.abs() > f32::EPSILON;
    if translate || scale || rotate || color_changed || alpha_changed || script_choice.is_some() {
        let new_fill = util::from_color32(fill_color);
        let Some(scene) = project.active_scene_mut() else {
            return;
        };
        for (id, layer_id, start, _) in &targets {
            let Some(object) = util::governing_object_mut(scene, *layer_id, state.frame, *id)
            else {
                continue;
            };
            let mut transform = *start;
            if scale || rotate || translate {
                transform =
                    util::shared_transform(*start, center, egui::vec2(dx, dy), factor, angle);
            }
            object.transform = transform;
            if let Some(script) = script_choice {
                object.script = script;
            }
            if color_changed {
                object.fill = new_fill;
            }
            if alpha_changed {
                object.fill.a = alpha;
            }
        }
        project.touch();
        state.dirty = true;
    }

    ui.add_space(6.0);
    if ui
        .button(RichText::new("Auswahl löschen").color(theme::RED))
        .on_hover_text("Entfernt alle ausgewählten Objekte aus allen Frames")
        .clicked()
    {
        actions.push(UiAction::DeleteSelection);
    }
}

fn symbol_properties(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut AppState,
    id: Uuid,
    actions: &mut Vec<UiAction>,
) {
    let Some(index) = project.symbols.iter().position(|symbol| symbol.id == id) else {
        ui.label(RichText::new("Symbol nicht gefunden").color(theme::RED));
        return;
    };

    let mut changed = false;
    {
        let symbol = &mut project.symbols[index];
        theme::section(ui, "Bibliothekssymbol");
        egui::Grid::new("symbol_grid")
            .num_columns(2)
            .spacing(egui::vec2(8.0, 6.0))
            .show(ui, |ui| {
                ui.label("Name");
                changed |= ui.text_edit_singleline(&mut symbol.name).changed();
                ui.end_row();

                ui.label("Breite");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut symbol.width)
                            .range(1.0..=8192.0)
                            .speed(1.0)
                            .suffix(" px"),
                    )
                    .changed();
                ui.end_row();

                ui.label("Höhe");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut symbol.height)
                            .range(1.0..=8192.0)
                            .speed(1.0)
                            .suffix(" px"),
                    )
                    .changed();
                ui.end_row();

                ui.label("Eckenradius");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut symbol.corner_radius)
                            .range(0.0..=4096.0)
                            .speed(0.5)
                            .suffix(" px"),
                    )
                    .changed();
                ui.end_row();

                ui.label("Füllung");
                let mut color = util::to_color32(symbol.fill);
                ui.horizontal(|ui| {
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        symbol.fill = util::from_color32(color);
                        changed = true;
                    }
                    ui.label(
                        RichText::new(symbol.fill.to_hex())
                            .small()
                            .monospace()
                            .color(theme::TEXT_FAINT),
                    );
                });
                ui.end_row();
            });
    }

    if changed {
        project.touch();
        state.dirty = true;
    }

    ui.add_space(6.0);
    if ui
        .button("Instanz auf Bühne platzieren")
        .on_hover_text("Fügt das Symbol an der Bühnenmitte ein")
        .clicked()
    {
        actions.push(UiAction::AddSymbolInstance(id));
    }
    if ui
        .button(RichText::new("Symbol löschen").color(theme::RED))
        .on_hover_text("Nur möglich, wenn keine Instanz das Symbol verwendet")
        .clicked()
    {
        actions.push(UiAction::DeleteSelection);
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Symbole werden beim SWF-Export als ein-Frame-Sprites angelegt. Verschachtelte Symbol-Timelines folgen später.",
        )
        .small()
        .color(theme::TEXT_FAINT),
    );
}

/// Small helper used by the object panel when a property dialog is needed.
#[allow(dead_code)]
pub fn color_hex(color: Rgba) -> String {
    color.to_hex()
}
