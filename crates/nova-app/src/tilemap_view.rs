//! Tilemap editor: bottom-dock painting panel for `ObjectKind::Tilemap`.
//!
//! The canvas renders the real tileset bitmap (shared texture, UV sub-rects)
//! and converts pointer drags into [`nova_core::tilemap`] operations, so
//! pencil/eraser/rect/flood/eyedrop/replace behave identically to the AI ops.
//! Undo is handled by the app's project-snapshot history (one entry per
//! stroke/gesture).
//!
//! M11.1 additions: wheel zoom around the cursor, middle-mouse / Space+drag
//! panning, configurable grid + collision overlay, cell selection with
//! copy/paste/cut/move and horizontal/vertical flip toggles.

use egui::{Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2, pos2};
use nova_core::model::{ObjectKind, TilemapData, Tileset};
use nova_core::tilemap::{self, PaintTool, TileRegion};
use uuid::Uuid;

use crate::state::{AppState, TilemapTool, UiAction};
use crate::theme;
use crate::util;

/// Shows the tilemap editor. Returns UI actions (currently unused; edits go
/// straight to the project like the properties panel).
pub fn show(ui: &mut Ui, project: &mut nova_core::Project, state: &mut AppState) -> Vec<UiAction> {
    let actions: Vec<UiAction> = Vec::new();

    // ------------------------------------------------------- toolbar
    ui.horizontal_wrapped(|ui| {
        // Tilemap object picker: all tilemap objects on the current frame.
        let tilemaps: Vec<(Uuid, Uuid, String)> = project
            .edit_scene(state.edit_symbol)
            .map(|scene| {
                nova_core::resolve_scene(scene, state.frame)
                    .into_iter()
                    .filter_map(|resolved| {
                        if let ObjectKind::Tilemap(map) = &resolved.instance.kind {
                            Some((
                                resolved.layer_id,
                                resolved.instance.id,
                                format!(
                                    "{} ({}×{}, {} F.)",
                                    resolved.instance.name,
                                    map.columns,
                                    map.rows,
                                    map.layers.len()
                                ),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let current_label = state
            .tilemap
            .object
            .and_then(|key| tilemaps.iter().find(|t| (t.0, t.1) == key))
            .map(|t| t.2.clone());
        egui::ComboBox::from_id_salt("tilemap_object")
            .selected_text(current_label.unwrap_or_else(|| "Keine Kachelkarte".into()))
            .show_ui(ui, |ui| {
                for (layer, object, label) in &tilemaps {
                    if ui
                        .selectable_label(
                            state.tilemap.object == Some((*layer, *object)),
                            label,
                        )
                        .clicked()
                    {
                        state.tilemap.object = Some((*layer, *object));
                        state.tilemap.layer = 0;
                        state.tilemap.stroke_start = None;
                        state.tilemap.selection = None;
                        state.tilemap.select_start = None;
                        state.tilemap.move_drag = None;
                        state.tilemap.view_custom = false;
                        state.tilemap.zoom = 0.0;
                        state.tilemap.pan = Vec2::ZERO;
                    }
                }
            });

        if tilemaps.is_empty() {
            ui.label(
                RichText::new("Keine Kachelkarte in dieser Szene — über Platzieren oder KI-Operation anlegen.")
                    .small()
                    .italics()
                    .color(theme::TEXT_FAINT),
            );
        }

        for (tool, label) in [
            (TilemapTool::Pencil, "Stift"),
            (TilemapTool::Eraser, "Radierer"),
            (TilemapTool::Rect, "Rechteck"),
            (TilemapTool::Flood, "Füllen"),
            (TilemapTool::Eyedrop, "Pipette"),
            (TilemapTool::Replace, "Ersetzen"),
            (TilemapTool::Select, "Auswahl"),
        ] {
            if ui
                .selectable_label(state.tilemap.tool == tool, label)
                .on_hover_text(format!("{} (Werkzeug)", tool.label()))
                .clicked()
            {
                state.tilemap.tool = tool;
                state.tilemap.stroke_start = None;
                state.tilemap.select_start = None;
                state.tilemap.move_drag = None;
            }
        }
        ui.label("Pinsel");
        ui.add(egui::DragValue::new(&mut state.tilemap.brush).range(1..=8));

        // Selection / clipboard actions.
        ui.separator();
        if ui
            .add_enabled(state.tilemap.selection.is_some(), egui::Button::new("Kopieren"))
            .on_hover_text("Auswahl in die Zwischenablage kopieren")
            .clicked()
        {
            copy_selection(project, state);
        }
        if ui
            .add_enabled(state.tilemap.selection.is_some(), egui::Button::new("Ausschneiden"))
            .on_hover_text("Auswahl kopieren und löschen")
            .clicked()
        {
            cut_selection(project, state);
        }
        if ui
            .add_enabled(state.tilemap.clipboard.is_some(), egui::Button::new("Einfügen"))
            .on_hover_text("Zwischenablage einfügen: nächster Klick setzt die linke obere Ecke")
            .clicked()
        {
            state.tilemap.paste_armed = true;
        }
        if ui
            .add_enabled(state.tilemap.selection.is_some(), egui::Button::new("Löschen"))
            .on_hover_text("Auswahl leeren")
            .clicked()
        {
            clear_selection(project, state);
        }
        if ui
            .add_enabled(state.tilemap.selection.is_some(), egui::Button::new("Aufheben"))
            .on_hover_text("Auswahl aufheben")
            .clicked()
        {
            state.tilemap.selection = None;
        }
        ui.separator();
        ui.label("Spiegeln");
        if ui
            .selectable_label(state.tilemap.flip_h, "↔")
            .on_hover_text("Horizontal spiegeln (Pinselstempel & Einfügen)")
            .clicked()
        {
            state.tilemap.flip_h = !state.tilemap.flip_h;
        }
        if ui
            .selectable_label(state.tilemap.flip_v, "↕")
            .on_hover_text("Vertikal spiegeln (Pinselstempel & Einfügen)")
            .clicked()
        {
            state.tilemap.flip_v = !state.tilemap.flip_v;
        }
        ui.separator();
        if ui.button("Tileset…").clicked() {
            state.tilemap.tileset_dialog = true;
        }
    });

    // View options row.
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.tilemap.show_grid, "Gitter");
        ui.add_enabled(
            state.tilemap.show_grid,
            egui::DragValue::new(&mut state.tilemap.grid_opacity)
                .range(0.0..=1.0)
                .speed(0.02)
                .prefix("Opazität "),
        );
        ui.checkbox(&mut state.tilemap.show_collision, "Kollisions-Overlay");
        if ui.button("Einpassen").clicked() {
            state.tilemap.view_custom = false;
            state.tilemap.zoom = 0.0;
            state.tilemap.pan = Vec2::ZERO;
        }
        if ui.button("100%").clicked() {
            state.tilemap.view_custom = true;
            state.tilemap.zoom = 1.0;
            state.tilemap.pan = Vec2::ZERO;
        }
        ui.label(
            RichText::new("Mausrad: Zoom · Mittlere Taste / Leertaste+Ziehen: Pan")
                .small()
                .color(theme::TEXT_FAINT),
        );
    });

    // Tileset slicing dialog works independently of the selection.
    tileset_dialog(ui, project, state);

    // Tilemap metadata row: tileset, layer, tile selection info.
    let Some((map_snapshot, tileset_snapshot, asset_id)) = tilemap_context(project, state) else {
        ui.label(
            RichText::new("Wähle eine Kachelkarte aus, um sie zu bearbeiten.")
                .small()
                .color(theme::TEXT_FAINT),
        );
        return actions;
    };

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!(
                "Tileset: {} · Kachel {}×{} · Raster {}×{}",
                tileset_snapshot.name,
                map_snapshot.tile_width,
                map_snapshot.tile_height,
                map_snapshot.columns,
                map_snapshot.rows
            ))
            .small()
            .color(theme::TEXT_DIM),
        );
        // Layer selector.
        let layer_names: Vec<String> = map_snapshot
            .layers
            .iter()
            .map(|l| format!("{}{}", l.name, if l.locked { " 🔒" } else { "" }))
            .collect();
        egui::ComboBox::from_id_salt("tilemap_layer")
            .selected_text(
                layer_names
                    .get(state.tilemap.layer.min(layer_names.len().saturating_sub(1)))
                    .cloned()
                    .unwrap_or_default(),
            )
            .show_ui(ui, |ui| {
                for (index, name) in layer_names.iter().enumerate() {
                    if ui
                        .selectable_label(state.tilemap.layer == index, name)
                        .clicked()
                    {
                        state.tilemap.layer = index;
                    }
                }
                ui.separator();
                if ui.button("Ebene hinzufügen").clicked() {
                    add_tile_layer(project, state);
                }
            });
        // Layer flags for the active layer.
        layer_flags_ui(ui, project, state);
        ui.label(
            RichText::new(format!(
                "Kachel {} (0 = leer)",
                if state.tilemap.tile == 0 {
                    "leer".to_string()
                } else {
                    (state.tilemap.tile - 1).to_string()
                }
            ))
            .small()
            .color(theme::TEXT_DIM),
        );
    });

    // ------------------------------------------------------- tileset palette
    tileset_palette(ui, state, asset_id, &tileset_snapshot);

    // ------------------------------------------------------- canvas
    canvas(ui, project, state, &map_snapshot, &tileset_snapshot);

    actions
}

/// Reads the tilemap + tileset + texture asset for the editor selection.
fn tilemap_context(
    project: &nova_core::Project,
    state: &AppState,
) -> Option<(TilemapData, Tileset, Uuid)> {
    let (layer_id, object_id) = state.tilemap.object?;
    let scene = project.edit_scene(state.edit_symbol)?;
    let mut found = None;
    for frame in 0..scene.frame_count() {
        for resolved in nova_core::resolve_scene(scene, frame) {
            if resolved.layer_id == layer_id && resolved.instance.id == object_id {
                if let ObjectKind::Tilemap(map) = &resolved.instance.kind {
                    found = Some(map.clone());
                }
            }
        }
    }
    let map = found?;
    let tileset = project
        .tilesets
        .iter()
        .find(|t| t.id == map.tileset)?
        .clone();
    Some((map, tileset.clone(), tileset.asset))
}

/// Visibility/lock/opacity/name controls for the active tile layer.
fn layer_flags_ui(ui: &mut Ui, project: &mut nova_core::Project, state: &mut AppState) {
    let Some((layer_id, object_id)) = state.tilemap.object else {
        return;
    };
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let Some(object) = util::governing_object_mut(scene, layer_id, state.frame, object_id) else {
        return;
    };
    let ObjectKind::Tilemap(map) = &mut object.kind else {
        return;
    };
    let index = state.tilemap.layer.min(map.layers.len().saturating_sub(1));
    let Some(layer) = map.layers.get_mut(index) else {
        return;
    };
    let mut changed = false;
    changed |= ui.checkbox(&mut layer.visible, "Sichtbar").changed();
    changed |= ui.checkbox(&mut layer.locked, "Gesperrt").changed();
    changed |= ui
        .add(
            egui::DragValue::new(&mut layer.opacity)
                .range(0.0..=1.0)
                .speed(0.05),
        )
        .changed();
    if changed {
        project.touch();
    }
}

/// Horizontal palette of tileset tiles; click selects the tile.
fn tileset_palette(ui: &mut Ui, state: &mut AppState, asset_id: Uuid, tileset: &Tileset) {
    let Some(texture) = state.asset_textures.get(&asset_id) else {
        ui.label(
            RichText::new("Tileset-Textur wird noch dekodiert …")
                .small()
                .color(theme::TEXT_FAINT),
        );
        return;
    };
    let tile_count =
        tilemap::tileset_tile_count(tileset, texture.size()[0] as u32, texture.size()[1] as u32);
    if tile_count == 0 {
        ui.label(
            RichText::new("Tileset enthält keine Kacheln.")
                .small()
                .color(theme::RED),
        );
        return;
    }
    // Cap the palette; huge tilesets scroll horizontally.
    let shown = tile_count.min(512);
    let (tex_w, tex_h) = (texture.size()[0] as u32, texture.size()[1] as u32);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Kacheln:").small().color(theme::TEXT_FAINT));
        let cell = 28.0f32;
        let tex = texture.size();
        for index in 0..shown {
            let Some((tile_col, tile_row, tw, th)) = tileset.tile_rect(index, tex_w, tex_h) else {
                continue;
            };
            let uv = Rect::from_min_max(
                pos2(
                    tile_col as f32 / tex[0] as f32,
                    tile_row as f32 / tex[1] as f32,
                ),
                pos2(
                    (tile_col + tw) as f32 / tex[0] as f32,
                    (tile_row + th) as f32 / tex[1] as f32,
                ),
            );
            let is_selected = state.tilemap.tile == index as u16 + 1;
            let response = ui
                .add(egui::Image::new((texture.id(), egui::vec2(cell, cell))).uv(uv))
                .on_hover_text(format!("{} (#{})", tileset.tile_name(index), index));
            if is_selected {
                ui.painter().rect_stroke(
                    response.rect.expand(1.0),
                    2.0,
                    Stroke::new(2.0, theme::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                state.tilemap.tile = index as u16 + 1;
            }
        }
    });
}

/// The paint canvas: textured cells, navigation, selection and overlays.
fn canvas(
    ui: &mut Ui,
    project: &mut nova_core::Project,
    state: &mut AppState,
    map: &TilemapData,
    tileset: &Tileset,
) {
    if state.tilemap.object.is_none() {
        return;
    }
    let available = ui.available_size();
    if available.x < 40.0 || available.y < 40.0 {
        return;
    }
    let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
    let rect = response.rect;
    painter.rect_filled(rect, 2.0, theme::BG_DEEPEST);

    // ------------------------------------------------- view transform
    let map_w = map.width().max(1.0);
    let map_h = map.height().max(1.0);
    let fit_zoom = (rect.width() / map_w)
        .min(rect.height() / map_h)
        .clamp(0.05, 16.0);
    let zoom = if state.tilemap.view_custom {
        state.tilemap.zoom.clamp(0.02, 32.0)
    } else {
        fit_zoom
    };
    // Fit origin centres the map; pan is a screen-space offset on top.
    let fit_origin = pos2(
        rect.center().x - map_w * zoom / 2.0,
        rect.center().y - map_h * zoom / 2.0,
    );
    let origin = fit_origin + state.tilemap.pan;
    let cell_w = map.tile_width.max(0.001) * zoom;
    let cell_h = map.tile_height.max(0.001) * zoom;

    // Wheel zoom around the cursor: keep the tile-space point under the
    // pointer fixed by adjusting the pan offset.
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if response.hovered() && scroll.abs() > 0.01 {
        let pointer = ui
            .input(|i| i.pointer.hover_pos())
            .unwrap_or_else(|| rect.center());
        let factor = if scroll > 0.0 { 1.12 } else { 1.0 / 1.12 };
        let new_zoom = (zoom * factor).clamp(0.02, 32.0);
        // Tile-space coordinate under the pointer (zoom = px per tile px).
        let tw_x = (pointer.x - origin.x) / zoom;
        let tw_y = (pointer.y - origin.y) / zoom;
        let new_origin = pos2(pointer.x - tw_x * new_zoom, pointer.y - tw_y * new_zoom);
        state.tilemap.pan = new_origin - fit_origin;
        state.tilemap.zoom = new_zoom;
        state.tilemap.view_custom = true;
    }

    // Middle-mouse or Space+drag panning.
    let space_down = ui.input(|i| i.key_down(egui::Key::Space));
    let middle_drag = response.dragged_by(egui::PointerButton::Middle);
    let space_drag = space_down && response.dragged_by(egui::PointerButton::Primary);
    if middle_drag || space_drag {
        state.tilemap.pan += response.drag_delta();
        state.tilemap.view_custom = true;
    }
    let panning = middle_drag || space_drag;

    // ------------------------------------------------- tiles
    let texture = state.asset_textures.get(&tileset.asset);
    let tex_size: [f32; 2] = texture
        .map(|t| [t.size()[0] as f32, t.size()[1] as f32])
        .unwrap_or([1.0, 1.0]);
    let (tex_w, tex_h) = (tex_size[0] as u32, tex_size[1] as u32);

    let cols = map.columns.max(1);
    let rows = map.rows.max(1);
    let mut mesh = texture.map(|t| egui::Mesh::with_texture(t.id()));

    for layer in &map.layers {
        if !layer.visible {
            continue;
        }
        let alpha = (layer.opacity.clamp(0.0, 1.0) * 255.0) as u8;
        for cy in 0..rows {
            for cx in 0..cols {
                let cell = layer.cells.get(cy * cols + cx).copied().unwrap_or(0);
                if cell == 0 {
                    continue;
                }
                let x0 = origin.x + cx as f32 * cell_w;
                let y0 = origin.y + cy as f32 * cell_h;
                if x0 + cell_w < rect.min.x
                    || x0 > rect.max.x
                    || y0 + cell_h < rect.min.y
                    || y0 > rect.max.y
                {
                    continue;
                }
                if let Some(mesh) = mesh.as_mut() {
                    let index = cell as usize - 1;
                    if let Some((sx, sy, sw, sh)) = tileset.tile_rect(index, tex_w, tex_h) {
                        let u0 = sx as f32 / tex_size[0];
                        let v0 = sy as f32 / tex_size[1];
                        let u1 = (sx + sw) as f32 / tex_size[0];
                        let v1 = (sy + sh) as f32 / tex_size[1];
                        let quad = [
                            (pos2(x0, y0), pos2(u0, v0)),
                            (pos2(x0 + cell_w, y0), pos2(u1, v0)),
                            (pos2(x0 + cell_w, y0 + cell_h), pos2(u1, v1)),
                            (pos2(x0, y0 + cell_h), pos2(u0, v1)),
                        ];
                        let base = mesh.vertices.len() as u32;
                        for (pos, uv) in quad {
                            mesh.vertices.push(egui::epaint::Vertex {
                                pos,
                                uv,
                                color: Color32::from_white_alpha(alpha),
                            });
                        }
                        mesh.indices.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                        continue;
                    }
                }
                // Fallback: hashed colour per tile index.
                let hue = (cell as u32).wrapping_mul(2654435761) % 360;
                let color = hsv(hue as f32 / 360.0, 0.55, 0.8).gamma_multiply(alpha as f32 / 255.0);
                painter.rect_filled(
                    Rect::from_min_size(pos2(x0, y0), Vec2::new(cell_w, cell_h)),
                    0.0,
                    color,
                );
            }
        }
    }
    if let Some(mesh) = mesh {
        if !mesh.indices.is_empty() {
            painter.add(Shape::mesh(mesh));
        }
    }

    // ------------------------------------------------- map outline
    let grid_rect = Rect::from_min_size(origin, Vec2::new(map_w * zoom, map_h * zoom));
    painter.rect_stroke(
        grid_rect,
        0.0,
        Stroke::new(1.0, theme::TEXT_FAINT),
        egui::StrokeKind::Inside,
    );

    // ------------------------------------------------- grid lines
    if state.tilemap.show_grid && cell_w >= 4.0 && cell_h >= 4.0 {
        let alpha = (state.tilemap.grid_opacity.clamp(0.0, 1.0) * 120.0) as u8;
        let stroke = Stroke::new(1.0, Color32::from_white_alpha(alpha));
        let x0 = (rect.min.x - origin.x) / cell_w;
        let x1 = (rect.max.x - origin.x) / cell_w;
        let y0 = (rect.min.y - origin.y) / cell_h;
        let y1 = (rect.max.y - origin.y) / cell_h;
        for cx in x0.floor() as i32..=x1.ceil() as i32 {
            if cx < 0 || cx > cols as i32 {
                continue;
            }
            let px = origin.x + cx as f32 * cell_w;
            painter.line_segment([pos2(px, rect.min.y), pos2(px, rect.max.y)], stroke);
        }
        for cy in y0.floor() as i32..=y1.ceil() as i32 {
            if cy < 0 || cy > rows as i32 {
                continue;
            }
            let py = origin.y + cy as f32 * cell_h;
            painter.line_segment([pos2(rect.min.x, py), pos2(rect.max.x, py)], stroke);
        }
    }

    // ------------------------------------------------- collision overlay
    if state.tilemap.show_collision {
        let overlay = Color32::from_rgba_unmultiplied(255, 60, 60, 90);
        for cy in 0..rows {
            for cx in 0..cols {
                // Solid if ANY visible layer holds a solid tile here.
                let mut solid = false;
                for layer in &map.layers {
                    if !layer.visible {
                        continue;
                    }
                    let cell = layer.cells.get(cy * cols + cx).copied().unwrap_or(0);
                    if cell > 0 && tileset.is_solid(cell as usize - 1) {
                        solid = true;
                        break;
                    }
                }
                if !solid {
                    continue;
                }
                let x0 = origin.x + cx as f32 * cell_w;
                let y0 = origin.y + cy as f32 * cell_h;
                if x0 + cell_w < rect.min.x
                    || x0 > rect.max.x
                    || y0 + cell_h < rect.min.y
                    || y0 > rect.max.y
                {
                    continue;
                }
                painter.rect_filled(
                    Rect::from_min_size(pos2(x0, y0), Vec2::new(cell_w, cell_h)),
                    0.0,
                    overlay,
                );
            }
        }
    }

    // ------------------------------------------------- pointer cell
    let pointer_cell = if response.hovered() {
        screen_to_cell(
            ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center()),
            origin,
            cell_w,
            cell_h,
            cols,
            rows,
        )
    } else {
        None
    };
    if let Some(cell) = pointer_cell {
        state.tilemap.cursor_cell = Some(cell);
    }

    // ------------------------------------------------- selection visuals
    let sel_rect_cells = selection_rect(state, map);
    if let Some((sx0, sy0, sx1, sy1)) = sel_rect_cells {
        let r = cell_rect_screen(origin, cell_w, cell_h, sx0, sy0, sx1, sy1);
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(80, 160, 255, 40));
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.5, Color32::from_rgba_unmultiplied(80, 160, 255, 220)),
            egui::StrokeKind::Inside,
        );
    }

    // Paste ghost: clipboard outline at the hovered cell.
    if state.tilemap.paste_armed {
        if let (Some(cell), Some(region)) = (pointer_cell, &state.tilemap.clipboard) {
            let mut preview = region.clone();
            tilemap::flip_region_cells(&mut preview, state.tilemap.flip_h, state.tilemap.flip_v);
            let r = cell_rect_screen(
                origin,
                cell_w,
                cell_h,
                cell.0,
                cell.1,
                cell.0 + preview.width as i32 - 1,
                cell.1 + preview.height as i32 - 1,
            );
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.5, Color32::from_rgba_unmultiplied(120, 255, 120, 230)),
                egui::StrokeKind::Inside,
            );
        }
    }

    // Brush preview for pencil/eraser.
    if matches!(
        state.tilemap.tool,
        TilemapTool::Pencil | TilemapTool::Eraser
    ) {
        if let Some(cell) = pointer_cell {
            let brush = state.tilemap.brush.max(1) as i32;
            let half = brush / 2;
            let r = cell_rect_screen(
                origin,
                cell_w,
                cell_h,
                cell.0 - half,
                cell.1 - half,
                cell.0 - half + brush - 1,
                cell.1 - half + brush - 1,
            );
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, Color32::from_white_alpha(140)),
                egui::StrokeKind::Inside,
            );
        }
    }

    // ------------------------------------------------- interaction
    if panning {
        return;
    }
    if !response.hovered() {
        return;
    }
    let Some(cell) = pointer_cell else {
        return;
    };

    // Paste armed: click places the clipboard.
    if state.tilemap.paste_armed {
        if response.clicked() {
            paste_clipboard(project, state, cell);
        }
        return;
    }

    match state.tilemap.tool {
        TilemapTool::Select => {
            select_tool(ui, project, state, response, cell, map);
        }
        TilemapTool::Eyedrop => {
            if response.clicked() {
                let current =
                    tilemap::sample(map, state.tilemap.layer, cell.0, cell.1).unwrap_or(0);
                state.tilemap.tile = current;
                state.tilemap.replace_from = current;
            }
        }
        TilemapTool::Replace => {
            if response.clicked() {
                let current =
                    tilemap::sample(map, state.tilemap.layer, cell.0, cell.1).unwrap_or(0);
                state.tilemap.replace_from = current;
                paint_cells(
                    project,
                    state,
                    PaintTool::Replace,
                    cell.0,
                    cell.1,
                    cell.0,
                    cell.1,
                    0,
                    current,
                    state.tilemap.tile,
                );
            }
        }
        TilemapTool::Rect => {
            if response.drag_started() {
                state.tilemap.stroke_start = Some(cell);
            }
            if let Some(start) = state.tilemap.stroke_start {
                if response.drag_stopped() {
                    paint_cells(
                        project,
                        state,
                        PaintTool::Rect,
                        start.0,
                        start.1,
                        cell.0,
                        cell.1,
                        state.tilemap.tile,
                        0,
                        0,
                    );
                    state.tilemap.stroke_start = None;
                }
            }
        }
        TilemapTool::Flood => {
            if response.clicked() {
                paint_cells(
                    project,
                    state,
                    PaintTool::Flood,
                    cell.0,
                    cell.1,
                    cell.0,
                    cell.1,
                    state.tilemap.tile,
                    0,
                    0,
                );
            }
        }
        _ => {
            // Pencil / eraser painting.
            if layer_is_locked(project, state) {
                return;
            }
            match state.tilemap.tool {
                TilemapTool::Eraser => {
                    if response.drag_started() || response.dragged() || response.clicked() {
                        let brush = state.tilemap.brush.max(1) as i32;
                        let half = brush / 2;
                        for dy in 0..brush {
                            for dx in 0..brush {
                                paint_cells(
                                    project,
                                    state,
                                    PaintTool::Eraser,
                                    cell.0 + dx - half,
                                    cell.1 + dy - half,
                                    cell.0 + dx - half,
                                    cell.1 + dy - half,
                                    0,
                                    0,
                                    0,
                                );
                            }
                        }
                    }
                }
                _ => {
                    if response.drag_started() || response.dragged() || response.clicked() {
                        paint_stamp_at_cursor(project, state, cell);
                    }
                }
            }
        }
    }
}

/// Select-tool interaction: drag creates a selection; dragging inside an
/// existing selection moves it; plain click outside deselects.
fn select_tool(
    _ui: &mut Ui,
    project: &mut nova_core::Project,
    state: &mut AppState,
    response: egui::Response,
    cell: (i32, i32),
    map: &TilemapData,
) {
    let _ = map;
    if response.drag_started() {
        // Inside an existing selection -> move mode, else new selection.
        let inside = state.tilemap.selection.is_some_and(|(x0, y0, x1, y1)| {
            let (ax, bx) = (x0.min(x1), x0.max(x1));
            let (ay, by) = (y0.min(y1), y0.max(y1));
            cell.0 >= ax && cell.0 <= bx && cell.1 >= ay && cell.1 <= by
        });
        if inside {
            state.tilemap.move_drag = Some((cell, (0, 0)));
            state.tilemap.select_start = None;
        } else {
            state.tilemap.select_start = Some(cell);
            state.tilemap.move_drag = None;
        }
    }
    if let Some(start) = state.tilemap.select_start {
        let (ax, bx) = (start.0.min(cell.0), start.0.max(cell.0));
        let (ay, by) = (start.1.min(cell.1), start.1.max(cell.1));
        state.tilemap.selection = Some((ax, ay, bx, by));
    }
    if let Some((grab, _)) = state.tilemap.move_drag {
        let offset = (cell.0 - grab.0, cell.1 - grab.1);
        state.tilemap.move_drag = Some((grab, offset));
    }
    if response.drag_stopped() {
        if let Some((_, offset)) = state.tilemap.move_drag.take() {
            commit_move(project, state, offset);
        }
        state.tilemap.select_start = None;
    }
    if response.clicked() && !response.dragged() {
        // Plain click outside the selection clears it.
        let inside = state.tilemap.selection.is_some_and(|(x0, y0, x1, y1)| {
            let (ax, bx) = (x0.min(x1), x0.max(x1));
            let (ay, by) = (y0.min(y1), y0.max(y1));
            cell.0 >= ax && cell.0 <= bx && cell.1 >= ay && cell.1 <= by
        });
        if !inside {
            state.tilemap.selection = None;
        }
    }
}

/// Effective selection rect including a live move offset.
fn selection_rect(state: &AppState, _map: &TilemapData) -> Option<(i32, i32, i32, i32)> {
    let (x0, y0, x1, y1) = state.tilemap.selection?;
    let (mut ax, mut ay, mut bx, mut by) = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
    if let Some((_, (dx, dy))) = state.tilemap.move_drag {
        ax += dx;
        ay += dy;
        bx += dx;
        by += dy;
    }
    Some((ax, ay, bx, by))
}

fn cell_rect_screen(
    origin: Pos2,
    cell_w: f32,
    cell_h: f32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> Rect {
    let (ax, bx) = (x0.min(x1), x0.max(x1));
    let (ay, by) = (y0.min(y1), y0.max(y1));
    Rect::from_min_max(
        pos2(origin.x + ax as f32 * cell_w, origin.y + ay as f32 * cell_h),
        pos2(
            origin.x + (bx + 1) as f32 * cell_w,
            origin.y + (by + 1) as f32 * cell_h,
        ),
    )
}

fn screen_to_cell(
    pos: Pos2,
    origin: Pos2,
    cell_w: f32,
    cell_h: f32,
    cols: usize,
    rows: usize,
) -> Option<(i32, i32)> {
    let cx = ((pos.x - origin.x) / cell_w).floor() as i32;
    let cy = ((pos.y - origin.y) / cell_h).floor() as i32;
    if cx < 0 || cy < 0 || cx as usize >= cols || cy as usize >= rows {
        return None;
    }
    Some((cx, cy))
}

// ------------------------------------------------------------ editing helpers

/// Runs `f` on the governing tilemap's active layer inside one history
/// gesture; `f` returns the number of changed cells.
fn edit_active_layer(
    project: &mut nova_core::Project,
    state: &AppState,
    f: impl FnOnce(&mut TilemapData) -> usize,
) {
    let Some((layer_id, object_id)) = state.tilemap.object else {
        return;
    };
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let Some(object) = util::governing_object_mut(scene, layer_id, state.frame, object_id) else {
        return;
    };
    let ObjectKind::Tilemap(map) = &mut object.kind else {
        return;
    };
    let layer = state.tilemap.layer.min(map.layers.len().saturating_sub(1));
    let Some(target) = map.layers.get(layer) else {
        return;
    };
    if target.locked {
        return;
    }
    let _ = f(map);
    project.touch();
}

fn layer_is_locked(project: &nova_core::Project, state: &AppState) -> bool {
    let Some((layer_id, object_id)) = state.tilemap.object else {
        return true;
    };
    let Some(scene) = project.edit_scene(state.edit_symbol) else {
        return true;
    };
    for resolved in nova_core::resolve_scene(scene, state.frame) {
        if resolved.layer_id == layer_id && resolved.instance.id == object_id {
            if let ObjectKind::Tilemap(map) = &resolved.instance.kind {
                let layer = state.tilemap.layer.min(map.layers.len().saturating_sub(1));
                return map.layers.get(layer).is_none_or(|l| l.locked);
            }
        }
    }
    true
}

/// Applies one paint call to the governing tilemap object.
fn paint_cells(
    project: &mut nova_core::Project,
    state: &AppState,
    tool: PaintTool,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    tile: u16,
    from: u16,
    to: u16,
) {
    edit_active_layer(project, state, |map| {
        tilemap::paint(
            map,
            state.tilemap.layer.min(map.layers.len().saturating_sub(1)),
            tool,
            x0,
            y0,
            x1,
            y1,
            tile,
            from,
            to,
        )
        .unwrap_or(0)
    });
}

/// Paints the brush stamp (NxN consecutive tiles from the tileset, honouring
/// the flip toggles) centred on the cell.
fn paint_stamp_at_cursor(project: &mut nova_core::Project, state: &AppState, cell: (i32, i32)) {
    // Resolve the tileset for the stamp before mutating.
    let tileset = tilemap_tileset(project, state);
    let brush = state.tilemap.brush.max(1) as usize;
    let start_tile = state.tilemap.tile.saturating_sub(1) as usize;
    let mut stamp = tilemap::BrushStamp::from_tileset(
        tileset.as_ref().unwrap_or(&Tileset::default()),
        usize::MAX,
        start_tile,
        brush,
        brush,
    );
    tilemap::flip_stamp(&mut stamp, state.tilemap.flip_h, state.tilemap.flip_v);
    let half = brush as i32 / 2;
    edit_active_layer(project, state, |map| {
        tilemap::paint_stamp(
            map,
            state.tilemap.layer.min(map.layers.len().saturating_sub(1)),
            cell.0 - half,
            cell.1 - half,
            &stamp,
        )
    });
}

fn tilemap_tileset(project: &nova_core::Project, state: &AppState) -> Option<Tileset> {
    let (_, object_id) = state.tilemap.object?;
    let scene = project.edit_scene(state.edit_symbol)?;
    for resolved in nova_core::resolve_scene(scene, state.frame) {
        if resolved.instance.id == object_id {
            if let ObjectKind::Tilemap(map) = &resolved.instance.kind {
                return project
                    .tilesets
                    .iter()
                    .find(|t| t.id == map.tileset)
                    .cloned();
            }
        }
    }
    None
}

// ------------------------------------------------------- selection ops

fn copy_selection(project: &nova_core::Project, state: &mut AppState) {
    let Some((x0, y0, x1, y1)) = selection_rect(state, &TilemapData::default()) else {
        return;
    };
    if let Some(region) = read_region(project, state, x0, y0, x1, y1) {
        state.tilemap.clipboard = Some(region);
    }
}

fn cut_selection(project: &mut nova_core::Project, state: &mut AppState) {
    copy_selection(project, state);
    clear_selection(project, state);
}

fn clear_selection(project: &mut nova_core::Project, state: &mut AppState) {
    let Some((x0, y0, x1, y1)) = selection_rect(state, &TilemapData::default()) else {
        return;
    };
    // Move offset must not affect the cleared source rect.
    let (ox, oy) = state.tilemap.move_drag.map(|(_, o)| o).unwrap_or((0, 0));
    let _ = (ox, oy);
    edit_active_layer(project, state, |map| {
        tilemap::clear_region(
            map,
            state.tilemap.layer.min(map.layers.len().saturating_sub(1)),
            x0,
            y0,
            x1,
            y1,
        )
    });
    state.tilemap.selection = None;
    state.tilemap.move_drag = None;
}

/// Reads the selection rect from the map (no mutation).
fn read_region(
    project: &nova_core::Project,
    state: &AppState,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> Option<TileRegion> {
    let (_, object_id) = state.tilemap.object?;
    let scene = project.edit_scene(state.edit_symbol)?;
    for resolved in nova_core::resolve_scene(scene, state.frame) {
        if resolved.instance.id == object_id {
            if let ObjectKind::Tilemap(map) = &resolved.instance.kind {
                return tilemap::copy_region(
                    map,
                    state.tilemap.layer.min(map.layers.len().saturating_sub(1)),
                    x0,
                    y0,
                    x1,
                    y1,
                );
            }
        }
    }
    None
}

/// Commits a finished move drag: clears the source rect and pastes the
/// region at the offset target — one history gesture. The selection follows
/// the moved region.
fn commit_move(project: &mut nova_core::Project, state: &mut AppState, offset: (i32, i32)) {
    let Some((sx0, sy0, sx1, sy1)) = state.tilemap.selection else {
        return;
    };
    let (ax, ay, bx, by) = (sx0.min(sx1), sy0.min(sy1), sx0.max(sx1), sy0.max(sy1));
    if offset == (0, 0) {
        return;
    }
    // Snapshot the source BEFORE clearing. A move preserves the cells
    // verbatim — flip toggles only affect stamps and paste.
    let Some(region) = read_region(project, state, ax, ay, bx, by) else {
        return;
    };
    let layer = state.tilemap.layer;
    edit_active_layer(project, state, |map| {
        let li = layer.min(map.layers.len().saturating_sub(1));
        tilemap::clear_region(map, li, ax, ay, bx, by);
        tilemap::paste_region(map, li, ax + offset.0, ay + offset.1, &region);
        0
    });
    state.tilemap.selection = Some((ax + offset.0, ay + offset.1, bx + offset.0, by + offset.1));
}

/// Pastes the clipboard with its top-left at `cell` (flips applied).
fn paste_clipboard(project: &mut nova_core::Project, state: &mut AppState, cell: (i32, i32)) {
    let Some(mut region) = state.tilemap.clipboard.clone() else {
        return;
    };
    tilemap::flip_region_cells(&mut region, state.tilemap.flip_h, state.tilemap.flip_v);
    let layer = state.tilemap.layer;
    edit_active_layer(project, state, |map| {
        let li = layer.min(map.layers.len().saturating_sub(1));
        tilemap::paste_region(map, li, cell.0, cell.1, &region);
        0
    });
    state.tilemap.paste_armed = false;
}

/// Adds an empty tile layer to the active tilemap.
fn add_tile_layer(project: &mut nova_core::Project, state: &mut AppState) {
    let Some((layer_id, object_id)) = state.tilemap.object else {
        return;
    };
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let Some(object) = util::governing_object_mut(scene, layer_id, state.frame, object_id) else {
        return;
    };
    let ObjectKind::Tilemap(map) = &mut object.kind else {
        return;
    };
    let index = tilemap::add_layer(map, &format!("Ebene {}", map.layers.len() + 1), None);
    state.tilemap.layer = index;
    project.touch();
}

/// Small HSV→RGB helper for the no-texture fallback.
fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let i = (h * 6.0) as i32;
    let f = h * 6.0 - i as f32;
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, v * (1.0 - s * (1.0 - f)), v * (1.0 - s)),
        1 => (v * (1.0 - s * f), v, v * (1.0 - s)),
        2 => (v * (1.0 - s), v, v * (1.0 - s * (1.0 - f))),
        3 => (v * (1.0 - s), v * (1.0 - s * f), v),
        4 => (v * (1.0 - s * (1.0 - f)), v * (1.0 - s), v),
        _ => (v, v * (1.0 - s), v * (1.0 - s * f)),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

// ------------------------------------------------------------ tileset dialog

/// Dedicated tileset slicing dialog: pick a bitmap asset, configure tile
/// size/margins/spacing, preview the grid, toggle per-tile solidity and
/// create a [`nova_core::Tileset`]. The source bitmap is never duplicated —
/// the tileset references the shared asset.
fn tileset_dialog(ui: &mut Ui, project: &mut nova_core::Project, state: &mut AppState) {
    if !state.tilemap.tileset_dialog {
        return;
    }
    let mut open = true;
    egui::Window::new("Tileset zuschneiden")
        .open(&mut open)
        .default_width(560.0)
        .show(ui.ctx(), |ui| {
            let dialog = &mut state.tileset_dialog;

            // ------------------------------------------------ asset picker
            let bitmaps: Vec<(Uuid, String, u32, u32)> = project
                .assets
                .iter()
                .filter(|a| a.kind == nova_core::AssetKind::Bitmap)
                .map(|a| (a.id, a.name.clone(), a.width, a.height))
                .collect();
            if bitmaps.is_empty() {
                ui.label(
                    RichText::new("Keine Bitmap-Assets im Projekt — zuerst eines importieren.")
                        .small()
                        .color(theme::RED),
                );
            }
            let current_name = dialog
                .asset
                .and_then(|id| bitmaps.iter().find(|b| b.0 == id))
                .map(|b| b.1.clone())
                .unwrap_or_else(|| "— Asset wählen —".into());
            egui::ComboBox::from_id_salt("tileset_asset")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    for (id, name, w, h) in &bitmaps {
                        if ui
                            .selectable_label(
                                dialog.asset == Some(*id),
                                format!("{name} ({w}×{h})"),
                            )
                            .clicked()
                        {
                            dialog.asset = Some(*id);
                            dialog.solid.clear();
                        }
                    }
                });

            // ------------------------------------------------ parameters
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(160.0));
                ui.label("Präfix:");
                ui.add(
                    egui::TextEdit::singleline(&mut dialog.name_prefix)
                        .desired_width(80.0)
                        .hint_text("optional"),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Kachel");
                ui.add(egui::DragValue::new(&mut dialog.tile_width).range(1..=512));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut dialog.tile_height).range(1..=512));
                ui.label("Rand");
                ui.add(egui::DragValue::new(&mut dialog.margin_x).range(0..=4096));
                ui.label(",");
                ui.add(egui::DragValue::new(&mut dialog.margin_y).range(0..=4096));
                ui.label("Abstand");
                ui.add(egui::DragValue::new(&mut dialog.spacing_x).range(0..=256));
                ui.label(",");
                ui.add(egui::DragValue::new(&mut dialog.spacing_y).range(0..=256));
            });

            // ------------------------------------------------ preview + validation
            let probe = dialog
                .asset
                .and_then(|id| bitmaps.iter().find(|b| b.0 == id))
                .map(|b| (b.2, b.3));
            let mut draft = nova_core::Tileset {
                name: dialog.name.clone(),
                asset: dialog.asset.unwrap_or_default(),
                tile_width: dialog.tile_width,
                tile_height: dialog.tile_height,
                columns: 0,
                margin_x: dialog.margin_x,
                margin_y: dialog.margin_y,
                spacing_x: dialog.spacing_x,
                spacing_y: dialog.spacing_y,
                ..Default::default()
            };
            let mut error: Option<String> = None;
            let mut tile_count = 0usize;
            if let Some((w, h)) = probe {
                draft.columns = draft.columns_for(w);
                tile_count = draft.tile_count(w, h);
                if let Err(e) = nova_core::tilemap::validate_tileset(&draft, w, h) {
                    error = Some(e);
                } else if tile_count == 0 {
                    error = Some("Keine Kacheln im Raster.".into());
                }
                if dialog.solid.len() < tile_count {
                    dialog.solid.resize(tile_count, false);
                }
            } else {
                error = Some("Bitte ein Bitmap-Asset wählen.".into());
            }
            if dialog.name.trim().is_empty() {
                error = Some("Tileset-Name darf nicht leer sein.".into());
            } else if project
                .tilesets
                .iter()
                .any(|t| t.name == dialog.name.trim())
            {
                error = Some(format!(
                    "Tileset '{}' existiert bereits.",
                    dialog.name.trim()
                ));
            }
            ui.label(
                RichText::new(format!(
                    "{tile_count} Kacheln · {} Spalten",
                    probe.map(|(w, _)| draft.columns_for(w)).unwrap_or(0)
                ))
                .small()
                .color(theme::TEXT_DIM),
            );

            // Preview: image fitted into a fixed area with the tile grid;
            // clicking a tile toggles its solidity flag.
            if let (Some((img_w, img_h)), Some(asset_id)) = (probe, dialog.asset) {
                let (pw, ph) = (520.0f32, 240.0f32);
                let (response, painter) = ui.allocate_painter(egui::vec2(pw, ph), Sense::click());
                painter.rect_filled(response.rect, 2.0, theme::BG_DEEPEST);
                if let Some(texture) = state.asset_textures.get(&asset_id) {
                    let scale = (pw / img_w as f32).min(ph / img_h as f32);
                    let draw_w = img_w as f32 * scale;
                    let draw_h = img_h as f32 * scale;
                    let off = pos2(
                        response.rect.center().x - draw_w / 2.0,
                        response.rect.center().y - draw_h / 2.0,
                    );
                    painter.image(
                        texture.id(),
                        Rect::from_min_size(off, egui::vec2(draw_w, draw_h)),
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    // Grid + solidity overlay at tile stride.
                    let tw = draft.tile_width as f32 * scale;
                    let th = draft.tile_height as f32 * scale;
                    let stride_x = draft.stride_x() as f32;
                    let stride_y = draft.stride_y() as f32;
                    let stroke = Stroke::new(1.0, Color32::from_white_alpha(90));
                    let rows = draft.rows_for(img_h);
                    for row in 0..rows {
                        for col in 0..draft.columns {
                            let index = row as usize * draft.columns as usize + col as usize;
                            if index >= tile_count {
                                break;
                            }
                            let tx =
                                off.x + (draft.margin_x as f32 + col as f32 * stride_x) * scale;
                            let ty =
                                off.y + (draft.margin_y as f32 + row as f32 * stride_y) * scale;
                            let solid = dialog.solid.get(index).copied().unwrap_or(false);
                            if solid {
                                painter.rect_filled(
                                    Rect::from_min_size(pos2(tx, ty), egui::vec2(tw, th)),
                                    0.0,
                                    Color32::from_rgba_unmultiplied(255, 80, 80, 70),
                                );
                            }
                            painter.rect_stroke(
                                Rect::from_min_size(pos2(tx, ty), egui::vec2(tw, th)),
                                0.0,
                                stroke,
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                    // Click toggles solidity of the tile under the pointer.
                    if response.clicked() {
                        if let Some(pointer) = response.interact_pointer_pos() {
                            let lx = (pointer.x - off.x) / scale;
                            let ly = (pointer.y - off.y) / scale;
                            let col = ((lx - draft.margin_x as f32) / stride_x).floor() as i64;
                            let row = ((ly - draft.margin_y as f32) / stride_y).floor() as i64;
                            if col >= 0
                                && row >= 0
                                && (col as u32) < draft.columns
                                && row < rows as i64
                            {
                                let idx = row as usize * draft.columns as usize + col as usize;
                                if idx < tile_count {
                                    if dialog.solid.len() <= idx {
                                        dialog.solid.resize(tile_count, false);
                                    }
                                    dialog.solid[idx] = !dialog.solid[idx];
                                }
                            }
                        }
                    }
                } else {
                    painter.text(
                        response.rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Textur wird dekodiert …",
                        egui::FontId::default(),
                        theme::TEXT_FAINT,
                    );
                }
            }

            if let Some(ref error) = error {
                ui.label(RichText::new(error).small().color(theme::RED));
            } else {
                ui.label(
                    RichText::new("Klick auf eine Kachel markiert sie als solide (rot).")
                        .small()
                        .color(theme::TEXT_FAINT),
                );
            }

            ui.horizontal(|ui| {
                if ui.button("Abbrechen").clicked() {
                    state.tilemap.tileset_dialog = false;
                }
                let can_create = error.is_none() && tile_count > 0;
                if ui
                    .add_enabled(can_create, egui::Button::new("Tileset erstellen"))
                    .clicked()
                {
                    let names: Vec<String> = if dialog.name_prefix.trim().is_empty() {
                        vec![String::new(); tile_count]
                    } else {
                        (0..tile_count)
                            .map(|i| format!("{} {}", dialog.name_prefix.trim(), i + 1))
                            .collect()
                    };
                    project.tilesets.push(nova_core::Tileset {
                        name: dialog.name.trim().to_string(),
                        asset: dialog.asset.unwrap(),
                        tile_width: dialog.tile_width,
                        tile_height: dialog.tile_height,
                        columns: draft.columns,
                        margin_x: dialog.margin_x,
                        margin_y: dialog.margin_y,
                        spacing_x: dialog.spacing_x,
                        spacing_y: dialog.spacing_y,
                        solid: dialog.solid[..tile_count].to_vec(),
                        names,
                        ..Default::default()
                    });
                    project.touch();
                    state.tilemap.tileset_dialog = false;
                }
            });
        });
    if !open {
        state.tilemap.tileset_dialog = false;
    }
}
