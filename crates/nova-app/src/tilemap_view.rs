//! Tilemap editor: bottom-dock painting panel for `ObjectKind::Tilemap`.
//!
//! The canvas renders the real tileset bitmap (shared texture, UV sub-rects)
//! and converts pointer drags into [`nova_core::tilemap`] operations, so
//! pencil/eraser/rect/flood/eyedrop/replace behave identically to the AI ops.
//! Undo is handled by the app's project-snapshot history.

use std::collections::HashMap;

use egui::{Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2, pos2};
use nova_core::model::{ObjectKind, TilemapData, Tileset};
use nova_core::tilemap::{self, PaintTool};
use uuid::Uuid;

use crate::state::{AppState, Selection, TilemapTool, UiAction};
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
        ] {
            if ui
                .selectable_label(state.tilemap.tool == tool, label)
                .on_hover_text(format!("{} (Werkzeug)", tool.label()))
                .clicked()
            {
                state.tilemap.tool = tool;
                state.tilemap.stroke_start = None;
            }
        }
        ui.label("Pinsel");
        ui.add(egui::DragValue::new(&mut state.tilemap.brush).range(1..=8));
    });

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
    canvas(ui, project, state, &map_snapshot);

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
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Kacheln:").small().color(theme::TEXT_FAINT));
        let cell = 28.0f32;
        for index in 0..shown {
            let tile_col = (index as u32 % tileset.columns) * tileset.tile_width;
            let tile_row = (index as u32 / tileset.columns) * tileset.tile_height;
            let tex = texture.size();
            let uv = Rect::from_min_max(
                pos2(
                    tile_col as f32 / tex[0] as f32,
                    tile_row as f32 / tex[1] as f32,
                ),
                pos2(
                    (tile_col + tileset.tile_width) as f32 / tex[0] as f32,
                    (tile_row + tileset.tile_height) as f32 / tex[1] as f32,
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

/// The paint canvas: textured cells + pointer painting.
fn canvas(ui: &mut Ui, project: &mut nova_core::Project, state: &mut AppState, map: &TilemapData) {
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

    // Auto-fit zoom: whole map visible, capped for usability.
    let map_w = map.width().max(1.0);
    let map_h = map.height().max(1.0);
    let zoom = (rect.width() / map_w)
        .min(rect.height() / map_h)
        .clamp(0.05, 12.0);
    let cell_w = map.tile_width * zoom;
    let cell_h = map.tile_height * zoom;
    let origin = pos2(
        rect.center().x - map_w * zoom / 2.0,
        rect.center().y - map_h * zoom / 2.0,
    );

    // Texture for real tile rendering.
    let tileset = project
        .tilesets
        .iter()
        .find(|t| t.id == map.tileset)
        .cloned();
    let texture = tileset
        .as_ref()
        .and_then(|t| state.asset_textures.get(&t.asset));

    let tex_size: [f32; 2] = texture
        .map(|t| [t.size()[0] as f32, t.size()[1] as f32])
        .unwrap_or([1.0, 1.0]);

    // Viewport culling in cell coordinates.
    let cols = map.columns.max(1);
    let rows = map.rows.max(1);
    let mut mesh = texture.map(|t| egui::Mesh::with_texture(t.id()));
    let mut grid_lines: Vec<(Pos2, Pos2)> = Vec::new();

    for (layer_index, layer) in map.layers.iter().enumerate() {
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
                    if let Some(ts) = &tileset {
                        let index = cell as usize - 1;
                        let tc = (index as u32 % ts.columns) * ts.tile_width;
                        let tr = (index as u32 / ts.columns) * ts.tile_height;
                        let u0 = tc as f32 / tex_size[0];
                        let v0 = tr as f32 / tex_size[1];
                        let u1 = (tc + ts.tile_width) as f32 / tex_size[0];
                        let v1 = (tr + ts.tile_height) as f32 / tex_size[1];
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
        let _ = layer_index;
    }
    if let Some(mesh) = mesh {
        if !mesh.indices.is_empty() {
            painter.add(Shape::mesh(mesh));
        }
    }
    // Grid outline.
    let grid_rect = Rect::from_min_size(origin, Vec2::new(map_w * zoom, map_h * zoom));
    painter.rect_stroke(
        grid_rect,
        0.0,
        Stroke::new(1.0, theme::TEXT_FAINT),
        egui::StrokeKind::Inside,
    );
    if cell_w >= 10.0 && cell_h >= 10.0 {
        for x in 0..=cols {
            let px = origin.x + x as f32 * cell_w;
            grid_lines.push((pos2(px, grid_rect.top()), pos2(px, grid_rect.bottom())));
        }
        for y in 0..=rows {
            let py = origin.y + y as f32 * cell_h;
            grid_lines.push((pos2(grid_rect.left(), py), pos2(grid_rect.right(), py)));
        }
        for (a, b) in grid_lines {
            painter.line_segment([a, b], Stroke::new(0.5, Color32::from_white_alpha(28)));
        }
    }

    // ------------------------------------------------------- painting
    if !response.clicked() && !response.dragged() {
        state.tilemap.stroke_start = None;
        return;
    }
    let Some(cell) = screen_to_cell(
        response.interact_pointer_pos().unwrap_or(rect.center()),
        rect,
        origin,
        cell_w,
        cell_h,
        cols,
        rows,
    ) else {
        return;
    };
    let tool = match state.tilemap.tool {
        TilemapTool::Pencil => PaintTool::Pencil,
        TilemapTool::Eraser => PaintTool::Eraser,
        TilemapTool::Rect => PaintTool::Rect,
        TilemapTool::Flood => PaintTool::Flood,
        TilemapTool::Eyedrop => PaintTool::Eyedrop,
        TilemapTool::Replace => PaintTool::Replace,
    };

    // Read-only queries first (eyedrop/replace need the cell value).
    let current = tilemap::sample(map, state.tilemap.layer, cell.0, cell.1).unwrap_or(0);
    match state.tilemap.tool {
        TilemapTool::Eyedrop => {
            if response.clicked() {
                state.tilemap.tile = current;
                state.tilemap.replace_from = current;
            }
            return;
        }
        TilemapTool::Replace => {
            if response.clicked() {
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
            return;
        }
        _ => {}
    }

    match state.tilemap.tool {
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
            // Pencil / eraser: brush stamp centred on the cell.
            let brush = state.tilemap.brush.max(1) as i32;
            let stamp = tilemap::BrushStamp::from_tileset(
                tileset.as_ref().unwrap_or(&Tileset::default()),
                usize::MAX,
                (state.tilemap.tile.saturating_sub(1)) as usize,
                brush as usize,
                brush as usize,
            );
            // For brush > 1 we stamp consecutive tiles from the tileset; for
            // the eraser we clear the stamp area.
            if state.tilemap.tool == TilemapTool::Eraser {
                for dy in 0..brush {
                    for dx in 0..brush {
                        paint_cells(
                            project,
                            state,
                            PaintTool::Eraser,
                            cell.0 + dx - brush / 2,
                            cell.1 + dy - brush / 2,
                            cell.0 + dx - brush / 2,
                            cell.1 + dy - brush / 2,
                            0,
                            0,
                            0,
                        );
                    }
                }
            } else {
                let _ = stamp;
                paint_stamp_area(project, state, cell, brush);
            }
        }
    }
    let _ = tool;
}

fn screen_to_cell(
    pos: Pos2,
    _rect: Rect,
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
    let _ = tilemap::paint(map, layer, tool, x0, y0, x1, y1, tile, from, to);
    project.touch();
}

/// Paints the brush stamp (NxN consecutive tiles starting at the selected
/// tile) centred on the cell.
fn paint_stamp_area(
    project: &mut nova_core::Project,
    state: &AppState,
    cell: (i32, i32),
    brush: i32,
) {
    let Some((layer_id, object_id)) = state.tilemap.object else {
        return;
    };
    // Tileset lookup happens before the mutable scene borrow.
    let tileset_id = {
        let Some((_, object_id_probe)) = state.tilemap.object else {
            return;
        };
        let _ = object_id_probe;
        project.edit_scene(state.edit_symbol).and_then(|scene| {
            for resolved in nova_core::resolve_scene(scene, state.frame) {
                if resolved.instance.id == object_id {
                    if let ObjectKind::Tilemap(map) = &resolved.instance.kind {
                        return Some(map.tileset);
                    }
                }
            }
            None
        })
    };
    let tileset = tileset_id.and_then(|id| project.tilesets.iter().find(|t| t.id == id).cloned());
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
    let Some(target) = map.layers.get_mut(layer) else {
        return;
    };
    if target.locked {
        return;
    }
    let start_tile = state.tilemap.tile.saturating_sub(1) as usize;
    let brush = brush.max(1) as usize;
    let stamp = tilemap::BrushStamp::from_tileset(
        tileset.as_ref().unwrap_or(&Tileset::default()),
        usize::MAX,
        start_tile,
        brush,
        brush,
    );
    // Centre the stamp on the cell.
    let ox = cell.0 - (brush as i32) / 2;
    let oy = cell.1 - (brush as i32) / 2;
    let _ = tilemap::paint_stamp(map, layer, ox, oy, &stamp);
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

#[allow(dead_code)]
fn _unused(_: &HashMap<Uuid, egui::TextureHandle>, _: &Selection) {}
