//! Right-hand properties inspector for the scene, layers, objects and symbols.

use egui::{RichText, ScrollArea, Ui};
use nova_core::{
    LayerKind, ObjectKind, Project, Rgba, SwfCompression, TargetRuntime, TextAlign, TextFont,
};
use uuid::Uuid;

use crate::state::{AlignMode, AppState, Selection, UiAction};
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
                Selection::Asset(_) => "Asset",
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
                Selection::Asset(id) => asset_properties(ui, project, state, *id, &mut actions),
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
    if let Some(scene) = project.edit_scene(state.edit_symbol) {
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
    game_scene_properties(ui, project, state);
    ui.add_space(4.0);
    export_properties(ui, project, state);
}

fn game_scene_properties(ui: &mut Ui, project: &mut Project, state: &mut AppState) {
    let index = project.active_scene_index();
    let mut changed = false;
    theme::section(ui, "Spiel");

    let scene = &mut project.scenes[index];
    let mut enabled = scene.game.is_some();
    if ui
        .checkbox(&mut enabled, "Spiel-Konfiguration")
        .on_hover_text("Kamera und Eingabe-Bindings für die Spiel-Vorschau")
        .changed()
    {
        scene.game = if enabled {
            Some(nova_core::GameSceneConfig::default())
        } else {
            None
        };
        changed = true;
    }

    if let Some(game) = scene.game.as_mut() {
        // Camera ------------------------------------------------------------------
        let mut cam_enabled = game.camera.as_ref().is_some_and(|c| c.enabled);
        if ui
            .checkbox(&mut cam_enabled, "Kamera")
            .on_hover_text("Kamera folgt einem Objekt und bleibt in den Grenzen")
            .changed()
        {
            match game.camera.as_mut() {
                Some(camera) => camera.enabled = cam_enabled,
                None if cam_enabled => {
                    game.camera = Some(nova_core::CameraConfig {
                        enabled: true,
                        ..Default::default()
                    });
                }
                None => {}
            }
            changed = true;
        }
        if let Some(camera) = game.camera.as_mut().filter(|c| c.enabled) {
            egui::Grid::new("game_camera_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Folgt Objekt");
                    let mut follow = camera.follow.clone();
                    let names: Vec<String> = scene
                        .layers
                        .iter()
                        .flat_map(|layer| {
                            layer
                                .frames
                                .iter()
                                .flat_map(|frame| frame.objects.iter().map(|o| o.name.clone()))
                        })
                        .collect();
                    egui::ComboBox::from_id_salt("game_camera_follow")
                        .selected_text(if follow.is_empty() {
                            "(Szene zentriert)".to_string()
                        } else {
                            follow.clone()
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(follow.is_empty(), "(Szene zentriert)")
                                .clicked()
                            {
                                follow.clear();
                            }
                            for name in &names {
                                if ui.selectable_label(&follow == name, name).clicked() {
                                    follow = name.clone();
                                }
                            }
                        });
                    if follow != camera.follow {
                        camera.follow = follow;
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Zoom");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut camera.zoom)
                                .range(0.1..=8.0)
                                .speed(0.05),
                        )
                        .changed();
                    ui.end_row();

                    for (label, field) in [
                        ("Links", &mut camera.left),
                        ("Oben", &mut camera.top),
                        ("Rechts", &mut camera.right),
                        ("Unten", &mut camera.bottom),
                    ] {
                        ui.label(label);
                        changed |= ui
                            .add(
                                egui::DragValue::new(field)
                                    .speed(1.0)
                                    .suffix(" px"),
                            )
                            .changed();
                        ui.end_row();
                    }
                });
        }

        // Input bindings -----------------------------------------------------------
        let mut input_enabled = game.input.is_some();
        if ui
            .checkbox(&mut input_enabled, "Eingabe-Bindings")
            .on_hover_text("Tasten für die Spiel-Vorschau (egui-Tastennamen)")
            .changed()
        {
            game.input = if input_enabled {
                Some(nova_core::InputBindings::default())
            } else {
                None
            };
            changed = true;
        }
        if let Some(input) = game.input.as_mut() {
            egui::Grid::new("game_input_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    for (label, field) in [
                        ("Links", &mut input.left),
                        ("Rechts", &mut input.right),
                        ("Hoch", &mut input.up),
                        ("Runter", &mut input.down),
                        ("Springen", &mut input.jump),
                        ("Aktion", &mut input.action),
                    ] {
                        ui.label(label);
                        changed |= ui.text_edit_singleline(field).changed();
                        ui.end_row();
                    }
                });
        }
    }

    if changed {
        project.touch();
        state.dirty = true;
    }
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

            ui.label("Szenen");
            changed |= ui
                .checkbox(
                    &mut project.export.export_all_scenes,
                    "Alle Szenen aneinanderhängen",
                )
                .on_hover_text(
                    "Exportiert alle Szenen als eine SWF-Timeline. Bühnenmaße, FPS und Hintergrund stammen aus der ersten Szene; Abweichungen werden gemeldet.",
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
    let folder_options: Vec<(Uuid, String)> = project
        .edit_scene(state.edit_symbol)
        .map(|scene| {
            scene
                .layers
                .iter()
                .filter(|candidate| {
                    candidate.kind == LayerKind::Folder
                        && candidate.id != id
                        && !scene.layer_is_descendant_of(candidate.id, id)
                })
                .map(|candidate| (candidate.id, candidate.name.clone()))
                .collect()
        })
        .unwrap_or_default();

    if let Some(scene) = project.edit_scene_mut(state.edit_symbol) {
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
                                LayerKind::Sound,
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

                    ui.label("Ordner");
                    let parent_name = layer
                        .parent
                        .and_then(|parent| {
                            folder_options
                                .iter()
                                .find(|(id, _)| *id == parent)
                                .map(|(_, name)| name.clone())
                        })
                        .unwrap_or_else(|| "Kein Ordner".into());
                    egui::ComboBox::from_id_salt("layer_parent")
                        .selected_text(parent_name)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(layer.parent.is_none(), "Kein Ordner")
                                .clicked()
                            {
                                layer.parent = None;
                                changed = true;
                            }
                            for (folder_id, folder_name) in &folder_options {
                                if ui
                                    .selectable_label(layer.parent == Some(*folder_id), folder_name)
                                    .clicked()
                                {
                                    layer.parent = Some(*folder_id);
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    if layer.kind == LayerKind::Folder {
                        ui.label("Aufgeklappt");
                        changed |= ui
                            .checkbox(&mut layer.expanded, "Unterebenen in Timeline zeigen")
                            .changed();
                        ui.end_row();
                    }

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
                theme::section(ui, "Tween");
                if has_tween {
                    let tween_kind = layer.frames[start]
                        .tween
                        .as_ref()
                        .map(|tween| tween.kind)
                        .unwrap_or_default();
                    let mut selected_kind = tween_kind;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Typ").small());
                        for candidate in [nova_core::TweenKind::Motion, nova_core::TweenKind::Shape]
                        {
                            if ui
                                .selectable_label(selected_kind == candidate, candidate.label())
                                .clicked()
                            {
                                selected_kind = candidate;
                            }
                        }
                    });
                    if selected_kind != tween_kind {
                        actions.push(UiAction::SetTweenKind(selected_kind));
                    }
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
        if let Some(scene) = project.edit_scene_mut(state.edit_symbol) {
            scene.repair_folder_links();
        }
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
        if let Some(scene) = project.edit_scene(state.edit_symbol) {
            let _ = scene;
        }
        return;
    };

    let layer_id = resolved.layer_id;
    let instance = resolved.instance.clone();
    let size = util::object_size(&instance, &project.symbols).unwrap_or((40.0, 40.0));

    let (keyframe, next_keyframe, on_keyframe) = project
        .edit_scene(state.edit_symbol)
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
    let mut text = match &instance.kind {
        ObjectKind::Text(text) => text.clone(),
        _ => nova_core::TextObject::default(),
    };
    let mut bitmap_crop = match &instance.kind {
        ObjectKind::Bitmap { crop, .. } => *crop,
        _ => None,
    };
    let mut game_cfg = instance.game.clone();
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
        ObjectKind::Text(_) => {
            ui.label("Text");
            changed |= ui
                .add(
                    egui::TextEdit::multiline(&mut text.text)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("Text eingeben"),
                )
                .changed();
            egui::Grid::new("text_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Schrift");
                    egui::ComboBox::from_id_salt("text_font")
                        .selected_text(text.font.label())
                        .show_ui(ui, |ui| {
                            for candidate in [TextFont::NoveSans, TextFont::NoveMono] {
                                if ui
                                    .selectable_label(text.font == candidate, candidate.label())
                                    .clicked()
                                {
                                    text.font = candidate;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Größe");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut text.size)
                                .range(6.0..=512.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();

                    ui.label("Ausrichtung");
                    egui::ComboBox::from_id_salt("text_align")
                        .selected_text(text.align.label())
                        .show_ui(ui, |ui| {
                            for candidate in [TextAlign::Left, TextAlign::Center, TextAlign::Right]
                            {
                                if ui
                                    .selectable_label(text.align == candidate, candidate.label())
                                    .clicked()
                                {
                                    text.align = candidate;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Stil");
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut text.bold, "Fett").changed();
                        changed |= ui.checkbox(&mut text.italic, "Kursiv").changed();
                    });
                    ui.end_row();

                    ui.label("Gemessen");
                    ui.label(
                        RichText::new(format!("{:.0}×{:.0} px", width, height))
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                    ui.end_row();
                });
            ui.label(
                RichText::new(
                    "Text wird als eingebauter Vektorfont gerendert und beim SWF-Export in Formen gebacken. Kein DefineText, keine Laufzeitbearbeitung.",
                )
                .small()
                .color(theme::TEXT_FAINT),
            );
        }
        ObjectKind::Bitmap { asset, .. } => {
            if let Some(definition) = project.asset(*asset) {
                ui.label(
                    RichText::new(format!(
                        "Asset '{}' · {}×{} px · {}",
                        definition.name,
                        definition.width,
                        definition.height,
                        if definition.mime.is_empty() {
                            "unbekannt"
                        } else {
                            &definition.mime
                        }
                    ))
                    .small()
                    .color(theme::TEXT_DIM),
                );
            }
            egui::Grid::new("bitmap_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Breite");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut width)
                                .range(1.0..=16384.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Höhe");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut height)
                                .range(1.0..=16384.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                });
            let mut crop_enabled = bitmap_crop.is_some();
            if ui.checkbox(&mut crop_enabled, "Ausschnitt").changed() {
                bitmap_crop = crop_enabled.then_some(nova_core::BitmapCrop::FULL);
                changed = true;
            }
            if let Some(crop) = &mut bitmap_crop {
                egui::Grid::new("bitmap_crop_grid")
                    .num_columns(2)
                    .spacing(egui::vec2(8.0, 6.0))
                    .show(ui, |ui| {
                        for (label, value) in [
                            ("X", &mut crop.x),
                            ("Y", &mut crop.y),
                            ("Breite", &mut crop.width),
                            ("Höhe", &mut crop.height),
                        ] {
                            ui.label(label);
                            changed |= ui
                                .add(
                                    egui::DragValue::new(value)
                                        .range(0.0..=1.0)
                                        .speed(0.01)
                                        .max_decimals(3),
                                )
                                .changed();
                            ui.end_row();
                        }
                    });
                *crop = crop.normalized();
            }
            ui.label(
                RichText::new(
                    "Ausschnitt wird normalisiert gespeichert und beim SWF-Export auf die Pixeldaten angewendet.",
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

    // Game components ---------------------------------------------------
    ui.add_space(4.0);
    theme::section(ui, "Spiel");
    let mut game_enabled = game_cfg.is_some();
    if ui
        .checkbox(&mut game_enabled, "Game-Objekt")
        .on_hover_text("Aktiviert Kollisions- und Physik-Metadaten für die Spiel-Vorschau")
        .changed()
    {
        game_cfg = if game_enabled {
            Some(nova_core::GameObjectConfig::default())
        } else {
            None
        };
        changed = true;
    }
    if let Some(cfg) = game_cfg.as_mut() {
        let mut has_collision = cfg.collision.is_some();
        if ui
            .checkbox(&mut has_collision, "Kollision")
            .on_hover_text("Objekt nimmt an Kollisions-Tests der Vorschau teil")
            .changed()
        {
            cfg.collision = if has_collision {
                Some(nova_core::CollisionSpec::default())
            } else {
                None
            };
            changed = true;
        }
        if let Some(collision) = cfg.collision.as_mut() {
            egui::Grid::new("game_collision_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Form");
                    egui::ComboBox::from_id_salt("game_collision_shape")
                        .selected_text(match collision.shape {
                            nova_core::CollisionShape::Rectangle => "Rechteck",
                            nova_core::CollisionShape::Circle => "Kreis",
                        })
                        .show_ui(ui, |ui| {
                            for (shape, label) in [
                                (nova_core::CollisionShape::Rectangle, "Rechteck"),
                                (nova_core::CollisionShape::Circle, "Kreis"),
                            ] {
                                if ui.selectable_label(collision.shape == shape, label).clicked() {
                                    collision.shape = shape;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                    ui.label("Breite");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut collision.width)
                                .range(0.0..=2000.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Höhe");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut collision.height)
                                .range(0.0..=2000.0)
                                .speed(1.0)
                                .suffix(" px"),
                        )
                        .changed();
                    ui.end_row();
                });
        }

        let mut has_physics = cfg.physics.is_some();
        if ui
            .checkbox(&mut has_physics, "Physik")
            .on_hover_text("Objekt wird von der Spiel-Vorschau-Simulation bewegt")
            .changed()
        {
            cfg.physics = if has_physics {
                Some(nova_core::PhysicsSpec::default())
            } else {
                None
            };
            changed = true;
        }
        if let Some(physics) = cfg.physics.as_mut() {
            egui::Grid::new("game_physics_grid")
                .num_columns(2)
                .spacing(egui::vec2(8.0, 6.0))
                .show(ui, |ui| {
                    ui.label("Körpertyp");
                    egui::ComboBox::from_id_salt("game_body_type")
                        .selected_text(match physics.body {
                            nova_core::BodyType::Static => "Statisch",
                            nova_core::BodyType::Dynamic => "Dynamisch",
                            nova_core::BodyType::Kinematic => "Kinematisch",
                        })
                        .show_ui(ui, |ui| {
                            for (body, label) in [
                                (nova_core::BodyType::Static, "Statisch"),
                                (nova_core::BodyType::Dynamic, "Dynamisch"),
                                (nova_core::BodyType::Kinematic, "Kinematisch"),
                            ] {
                                if ui.selectable_label(physics.body == body, label).clicked() {
                                    physics.body = body;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                    for (label, value, range, speed) in [
                        ("Schwerkraft", &mut physics.gravity_scale, -10.0..=10.0, 0.05),
                        ("Reibung", &mut physics.friction, 0.0..=20.0, 0.05),
                        ("Federung", &mut physics.bounce, 0.0..=1.0, 0.01),
                    ] {
                        ui.label(label);
                        changed |= ui
                            .add(
                                egui::DragValue::new(value)
                                    .range(range)
                                    .speed(speed)
                                    .max_decimals(3),
                            )
                            .changed();
                        ui.end_row();
                    }
                    ui.label("Gesteuert");
                    changed |= ui
                        .checkbox(&mut physics.controlled, "Auf WASD/Pfeile reagieren")
                        .changed();
                    ui.end_row();
                });
        }
        ui.label(
            RichText::new("Wirkt in der Spiel-Vorschau; AS3-Logik bleibt im Skript.")
                .small()
                .color(theme::TEXT_FAINT),
        );
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
            ObjectKind::Bitmap { asset, .. } => ObjectKind::Bitmap {
                asset: *asset,
                width,
                height,
                crop: bitmap_crop,
            },
            ObjectKind::Text(_) => ObjectKind::Text(text),
        };
        if let Some(scene) = project.edit_scene_mut(state.edit_symbol) {
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
                object.game = game_cfg;
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
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (label, mode) in [
            ("Links", AlignMode::Left),
            ("Mitte X", AlignMode::HorizontalCenter),
            ("Rechts", AlignMode::Right),
            ("Oben", AlignMode::Top),
            ("Mitte Y", AlignMode::VerticalCenter),
            ("Unten", AlignMode::Bottom),
            ("Verteilen X", AlignMode::DistributeHorizontal),
            ("Verteilen Y", AlignMode::DistributeVertical),
        ] {
            if ui.small_button(label).clicked() {
                actions.push(UiAction::AlignSelection(mode));
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.small_button("Kopieren").clicked() {
            actions.push(UiAction::CopyObjects);
        }
        if ui.small_button("Duplizieren").clicked() {
            actions.push(UiAction::DuplicateObjects);
        }
    });
    ui.add_space(4.0);

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
        let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
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

                ui.label("Typ");
                let mut kind = symbol.kind;
                egui::ComboBox::from_id_salt("symbol_kind")
                    .selected_text(kind.label())
                    .show_ui(ui, |ui| {
                        for candidate in [
                            nova_core::SymbolKind::Graphic,
                            nova_core::SymbolKind::MovieClip,
                            nova_core::SymbolKind::Button,
                        ] {
                            if ui
                                .selectable_label(kind == candidate, candidate.label())
                                .clicked()
                            {
                                kind = candidate;
                            }
                        }
                    });
                if kind != symbol.kind {
                    symbol.kind = kind;
                    changed = true;
                }
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
        if let Some(symbol) = project.symbol_mut(id) {
            symbol.timeline.width = symbol.width;
            symbol.timeline.height = symbol.height;
            symbol.timeline.name = symbol.name.clone();
        }
        project.touch();
        state.dirty = true;
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .button("Symbol bearbeiten")
            .on_hover_text("Öffnet die eigene Timeline des Symbols")
            .clicked()
        {
            actions.push(UiAction::EnterSymbol(id));
        }
        if ui.button("Duplizieren").clicked() {
            actions.push(UiAction::DuplicateSymbol(id));
        }
    });
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
            "Jedes Symbol besitzt eine eigene Timeline mit Ebenen, Keyframes und Tweens. MovieClips lassen sich verschachteln; Button-Zustände werden als MovieClip exportiert.",
        )
        .small()
        .color(theme::TEXT_FAINT),
    );
}

fn asset_properties(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut AppState,
    id: Uuid,
    actions: &mut Vec<UiAction>,
) {
    let Some(index) = project.assets.iter().position(|asset| asset.id == id) else {
        ui.label(RichText::new("Asset nicht gefunden").color(theme::RED));
        return;
    };
    let mut changed = false;
    {
        let asset = &mut project.assets[index];
        theme::section(ui, "Bibliothek-Asset");
        egui::Grid::new("asset_grid")
            .num_columns(2)
            .spacing(egui::vec2(8.0, 6.0))
            .show(ui, |ui| {
                ui.label("Name");
                changed |= ui.text_edit_singleline(&mut asset.name).changed();
                ui.end_row();
                ui.label("Typ");
                ui.label(
                    RichText::new(match asset.kind {
                        nova_core::AssetKind::Bitmap => "Bitmap",
                        nova_core::AssetKind::Audio => "Audio",
                        nova_core::AssetKind::Font => "Font",
                    })
                    .small(),
                );
                ui.end_row();
                ui.label("Format");
                ui.label(
                    RichText::new(if asset.mime.is_empty() {
                        "unbekannt".to_string()
                    } else {
                        asset.mime.clone()
                    })
                    .small()
                    .monospace(),
                );
                ui.end_row();
                match asset.kind {
                    nova_core::AssetKind::Bitmap => {
                        ui.label("Größe");
                        ui.label(format!("{} × {} px", asset.width, asset.height));
                        ui.end_row();
                        ui.label("Daten");
                        ui.label(format!("{} KB", asset.data.len() / 1024));
                        ui.end_row();
                    }
                    nova_core::AssetKind::Audio => {
                        ui.label("Dauer");
                        ui.label(if asset.duration > 0.0 {
                            format!("{:.1} s", asset.duration)
                        } else {
                            "unbekannt".to_string()
                        });
                        ui.end_row();
                        if asset.sample_rate > 0 {
                            ui.label("Abtastrate");
                            ui.label(format!(
                                "{} Hz · {} Kanal/kanäle",
                                asset.sample_rate, asset.channels
                            ));
                            ui.end_row();
                        }
                    }
                    nova_core::AssetKind::Font => {}
                }
                ui.label("Quelle");
                ui.label(RichText::new(&asset.file).small().color(theme::TEXT_FAINT));
                ui.end_row();
            });
    }
    if changed {
        project.touch();
        state.dirty = true;
    }

    ui.add_space(6.0);
    match project.assets[index].kind {
        nova_core::AssetKind::Bitmap => {
            if ui.button("Bitmap auf Bühne platzieren").clicked() {
                actions.push(UiAction::AddBitmapAsset(id));
            }
        }
        nova_core::AssetKind::Audio => {
            ui.horizontal(|ui| {
                if ui.button("Vorschau").clicked() {
                    actions.push(UiAction::PreviewAsset(id));
                }
                if ui.button("Sound setzen").clicked() {
                    actions.push(UiAction::AddSoundCue(id));
                }
            });
        }
        nova_core::AssetKind::Font => {}
    }
    if ui
        .button(RichText::new("Asset löschen").color(theme::RED))
        .on_hover_text("Nur möglich, wenn nichts das Asset verwendet")
        .clicked()
    {
        actions.push(UiAction::DeleteSelection);
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Bitmaps werden als DefineBits exportiert, Audio als DefineSound/StartSound.",
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
