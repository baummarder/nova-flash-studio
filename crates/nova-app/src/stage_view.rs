//! Animation mode: the stage, camera, selection and direct manipulation.
//!
//! The stage draws exactly what the SWF exporter bakes: keyframes resolved by
//! `nova_core::resolve_scene`, with the same centre-based transform origin.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use nova_core::{
    ObjectInstance, ObjectKind, PathCommand, PathShape, Project, ResolvedObject, Rgba, Symbol,
};
use uuid::Uuid;

use crate::state::{AppState, PenAnchor, PenDraft, Selection, UiAction};
use crate::theme;
use crate::util;

const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 8.0;

/// Screen-space camera for one stage frame.
struct View {
    rect: Rect,
    center: Pos2,
    zoom: f32,
}

impl View {
    fn to_screen(&self, point: Pos2) -> Pos2 {
        self.rect.center() + (point.to_vec2() - self.center.to_vec2()) * self.zoom
    }

    fn to_stage(&self, point: Pos2) -> Pos2 {
        let delta = point - self.rect.center();
        Pos2::new(
            self.center.x + delta.x / self.zoom,
            self.center.y + delta.y / self.zoom,
        )
    }
}

pub fn show(ui: &mut Ui, project: &mut Project, state: &mut AppState) -> Vec<UiAction> {
    let mut actions = Vec::new();
    tool_strip(ui, state, &mut actions);
    let available = ui.available_size();
    if available.x < 20.0 || available.y < 20.0 {
        return actions;
    }

    let Some(scene) = project.edit_scene(state.edit_symbol) else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Keine Szene vorhanden.").color(theme::RED));
        });
        return actions;
    };

    let stage_size = Vec2::new(scene.width.max(1.0), scene.height.max(1.0));
    let background = scene.background;
    let scene_name = if state.edit_symbol.is_some() {
        format!("Symbol: {}", scene.name)
    } else {
        scene.name.clone()
    };
    let fps = scene.fps;

    let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
    let rect = response.rect;

    // Fit the stage the first time it is shown, and after "Ansicht einpassen".
    if !state.zoom_initialized {
        let fitting = rect.size() - Vec2::new(72.0, 72.0);
        let zoom = (fitting.x / stage_size.x)
            .min(fitting.y / stage_size.y)
            .clamp(MIN_ZOOM, 4.0);
        state.zoom = zoom;
        state.camera_center = Pos2::new(stage_size.x / 2.0, stage_size.y / 2.0);
        state.zoom_initialized = true;
        state.last_stage_size = stage_size;
    }

    // Wheel zoom around the pointer.
    if response.hovered() {
        let scroll: f32 = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > 0.5 {
            if let Some(pointer) = response.hover_pos() {
                let before = View {
                    rect,
                    center: state.camera_center,
                    zoom: state.zoom,
                }
                .to_stage(pointer);
                let factor = (1.0 + scroll * 0.0015).clamp(0.5, 2.0);
                state.zoom = (state.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                // Keep the stage point under the cursor pinned.
                let delta = pointer - rect.center();
                state.camera_center = Pos2::new(
                    before.x - delta.x / state.zoom,
                    before.y - delta.y / state.zoom,
                );
            }
        }
    }

    let mut view = View {
        rect,
        center: state.camera_center,
        zoom: state.zoom,
    };

    // Game preview camera: an enabled scene camera drives the view while the
    // simulation runs.
    if let Some(preview) = &state.game_preview {
        let camera_enabled = project
            .edit_scene(state.edit_symbol)
            .and_then(|scene| scene.game.as_ref())
            .and_then(|game| game.camera.as_ref())
            .is_some_and(|camera| camera.enabled);
        if camera_enabled {
            view.center = Pos2::new(preview.camera.x, preview.camera.y);
            view.zoom = preview.camera.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }

    // ---------------------------------------------------------------- input
    let shift = ui.input(|input| input.modifiers.shift);
    match state.tool {
        crate::state::Tool::Select => {
            if !bone_joint_input(project, state, &view, &response) {
                select_input(ui, project, state, &view, &response, shift, &mut actions);
            }
        }
        crate::state::Tool::PathEdit => {
            path_edit_input(project, state, &view, &response);
        }
        crate::state::Tool::Pen => {
            pen_input(ui, project, state, &view, &response);
        }
        _ => {
            draw_tool_input(ui, project, state, &view, &response, shift);
        }
    }

    if response.dragged_by(egui::PointerButton::Middle) {
        state.camera_center -= response.drag_delta() / state.zoom;
    }

    // ------------------------------------------------------------- painting
    painter.rect_filled(rect, 0.0, theme::BG_DEEPEST);

    let stage_rect = Rect::from_min_max(
        view.to_screen(Pos2::ZERO),
        view.to_screen(Pos2::new(stage_size.x, stage_size.y)),
    );

    painter.rect_filled(
        stage_rect.translate(Vec2::new(0.0, 7.0)),
        3.0,
        Color32::from_black_alpha(90),
    );
    painter.rect_filled(stage_rect, 2.0, util::to_color32(background));
    painter.rect_stroke(
        stage_rect,
        2.0,
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Outside,
    );

    if state.show_grid {
        draw_grid(&painter, &view, stage_rect, stage_size);
    }

    if state.onion_skin {
        draw_onion(&painter, &view, project, state.edit_symbol, state.frame);
    }

    let scene = project
        .edit_scene(state.edit_symbol)
        .expect("scene checked above");
    // Rigged symbol timelines resolve through the skeleton pose so bound
    // parts follow their bones while scrubbing the timeline.
    let mut objects = match state.edit_symbol.and_then(|id| project.symbol(id)) {
        Some(symbol) if symbol.skeleton.is_some() => {
            nova_core::resolve_symbol_frame(symbol, state.frame)
        }
        _ => nova_core::resolve_scene(scene, state.frame),
    };

    // Game preview: simulated body positions replace authored transforms.
    if let Some(preview) = &state.game_preview {
        for object in &mut objects {
            if let Some(body_state) = preview.states.get(&object.instance.id) {
                object.instance.transform.x = body_state.x;
                object.instance.transform.y = body_state.y;
            }
        }
    }
    for object in &objects {
        draw_object(
            &painter,
            &view,
            object,
            project,
            &state.asset_textures,
            &state.selection,
        );
    }

    // Collision overlays for the game preview.
    if let Some(preview) = &state.game_preview {
        for body in &preview.bodies {
            let Some(body_state) = preview.states.get(&body.id) else {
                continue;
            };
            let color = Color32::from_rgba_unmultiplied(80, 220, 120, 220);
            let stroke = Stroke::new(1.5, color);
            match body.collision.as_ref() {
                Some(spec) if matches!(spec.shape, nova_core::CollisionShape::Polygon { .. }) => {
                    if let nova_core::CollisionShape::Polygon { points } = &spec.shape {
                        let center = Pos2::new(body_state.x, body_state.y);
                        let poly: Vec<Pos2> = points
                            .iter()
                            .map(|(px, py)| view.to_screen(Pos2::new(center.x + px, center.y + py)))
                            .collect();
                        if poly.len() >= 3 {
                            painter.add(egui::Shape::closed_line(poly, stroke));
                        }
                    }
                }
                Some(spec) => {
                    let (w, h) = (spec.width, spec.height);
                    if w <= 0.0 || h <= 0.0 {
                        continue;
                    }
                    let center = view.to_screen(Pos2::new(body_state.x, body_state.y));
                    let half = Vec2::new(w / 2.0, h / 2.0) * view.zoom;
                    if spec.shape == nova_core::CollisionShape::Circle {
                        painter.circle_stroke(center, half.x.max(half.y), stroke);
                    } else {
                        let rect = Rect::from_center_size(center, half * 2.0);
                        painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Inside);
                    }
                }
                None => {
                    let (w, h) = (body.width, body.height);
                    if w <= 0.0 || h <= 0.0 {
                        continue;
                    }
                    let center = view.to_screen(Pos2::new(body_state.x, body_state.y));
                    let half = Vec2::new(w / 2.0, h / 2.0) * view.zoom;
                    let rect = Rect::from_center_size(center, half * 2.0);
                    painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Inside);
                }
            }
        }
    }

    // Particle emitters: game preview uses the runtime simulation; in edit
    // mode a lightweight deterministic preview runs on the authored objects.
    if let Some(preview) = &state.game_preview {
        for (_emitter, views) in preview.particle_views() {
            draw_particle_views(&painter, &view, &views);
        }
    } else {
        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
        if dt > 0.0 {
            step_edit_particles(state, &objects, dt);
        }
        for views in edit_particle_views(state) {
            draw_particle_views(&painter, &view, &views);
        }
    }

    if let Some(draft) = &state.draft {
        draw_draft(&painter, &view, draft, state.new_object_color);
    }
    draw_pen_preview(&painter, &view, state);
    if state.tool == crate::state::Tool::Select {
        draw_transform_handles(&painter, &view, project, state);
    }
    if state.tool == crate::state::Tool::PathEdit {
        draw_node_handles(&painter, &view, project, state);
    }
    draw_bone_overlay(&painter, &view, project, state);

    draw_playhead_hint(&painter, &stage_rect, state, &scene_name, fps);

    if let Some(marquee) = &state.marquee {
        let start = view.to_screen(marquee.start);
        let current = view.to_screen(marquee.current);
        let rect = Rect::from_two_pos(start, current);
        painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(70, 160, 255, 26));
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, theme::BLUE), StrokeKind::Inside);
    }

    actions
}

fn toggle_object_selection(state: &mut AppState, id: Uuid, additive: bool) {
    if additive {
        let mut ids = state.selection.object_ids().to_vec();
        if let Some(index) = ids.iter().position(|candidate| *candidate == id) {
            ids.remove(index);
        } else {
            ids.push(id);
        }
        state.selection = if ids.is_empty() {
            Selection::None
        } else {
            Selection::Objects(ids)
        };
    } else if !state.selection.contains_object(id) {
        state.selection = Selection::object(id);
    }
}

fn move_objects(
    project: &mut Project,
    state: &AppState,
    drag: &crate::state::ObjectDragStart,
    delta: Vec2,
) {
    let targets: Vec<(Uuid, Uuid, nova_core::Transform)> = drag
        .objects
        .iter()
        .filter_map(|(id, _, start)| {
            state
                .layer_of_object(project, *id)
                .map(|layer| (*id, layer, *start))
        })
        .collect();
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    for (id, layer_id, start) in targets {
        if let Some(object) = util::governing_object_mut(scene, layer_id, drag.frame, id) {
            let mut x = start.x + delta.x;
            let mut y = start.y + delta.y;
            if state.snap_to_grid && state.grid_size > 0.0 {
                x = (x / state.grid_size).round() * state.grid_size;
                y = (y / state.grid_size).round() * state.grid_size;
            }
            object.transform.x = x;
            object.transform.y = y;
        }
    }
}

fn marquee_hits(
    project: &Project,
    edit_symbol: Option<Uuid>,
    frame: usize,
    marquee: &crate::state::Marquee,
) -> Vec<Uuid> {
    let rect = Rect::from_two_pos(marquee.start, marquee.current);
    let mut ids = marquee.base.clone();
    let Some(scene) = project.edit_scene(edit_symbol) else {
        return ids;
    };
    for object in nova_core::resolve_scene(scene, frame) {
        let Some(size) = util::object_size(&object.instance, &project.symbols) else {
            continue;
        };
        let corners = util::corners(&object.instance, size);
        let bounds = Rect::from_points(&corners);
        if rect.intersects(bounds) && !ids.contains(&object.instance.id) {
            ids.push(object.instance.id);
        }
    }
    ids
}

fn draw_grid(painter: &egui::Painter, view: &View, stage_rect: Rect, stage_size: Vec2) {
    if view.zoom < 0.18 {
        return;
    }
    let step = 50.0;
    let color = Color32::from_white_alpha(14);
    let mut x = step;
    while x < stage_size.x {
        let screen_x = view.to_screen(Pos2::new(x, 0.0)).x;
        painter.line_segment(
            [
                Pos2::new(screen_x, stage_rect.top()),
                Pos2::new(screen_x, stage_rect.bottom()),
            ],
            Stroke::new(1.0, color),
        );
        x += step;
    }
    let mut y = step;
    while y < stage_size.y {
        let screen_y = view.to_screen(Pos2::new(0.0, y)).y;
        painter.line_segment(
            [
                Pos2::new(stage_rect.left(), screen_y),
                Pos2::new(stage_rect.right(), screen_y),
            ],
            Stroke::new(1.0, color),
        );
        y += step;
    }

    let center = view.to_screen(Pos2::new(stage_size.x / 2.0, stage_size.y / 2.0));
    painter.line_segment(
        [
            Pos2::new(center.x, stage_rect.top()),
            Pos2::new(center.x, stage_rect.bottom()),
        ],
        Stroke::new(1.0, Color32::from_white_alpha(24)),
    );
    painter.line_segment(
        [
            Pos2::new(stage_rect.left(), center.y),
            Pos2::new(stage_rect.right(), center.y),
        ],
        Stroke::new(1.0, Color32::from_white_alpha(24)),
    );
}

fn draw_onion(
    painter: &egui::Painter,
    view: &View,
    project: &Project,
    edit_symbol: Option<Uuid>,
    frame: usize,
) {
    let Some(scene) = project.edit_scene(edit_symbol) else {
        return;
    };
    let count = scene.frame_count();
    for offset in [2usize, 1] {
        if let Some(previous) = frame.checked_sub(offset) {
            if previous < count {
                draw_ghost(
                    painter,
                    view,
                    scene,
                    &project.symbols,
                    previous,
                    theme::BLUE,
                );
            }
        }
    }
    for offset in 1..=2 {
        let next = frame + offset;
        if next < count {
            draw_ghost(painter, view, scene, &project.symbols, next, theme::GREEN);
        }
    }
}

fn draw_ghost(
    painter: &egui::Painter,
    view: &View,
    scene: &nova_core::Scene,
    symbols: &[Symbol],
    frame: usize,
    tint: Color32,
) {
    let fill = Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 46);
    let stroke = Stroke::new(
        1.0,
        Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 90),
    );
    for object in nova_core::resolve_scene(scene, frame) {
        let Some(size) = util::object_size(&object.instance, symbols) else {
            continue;
        };
        let points = outline_points(view, &object.instance, size);
        painter.add(Shape::convex_polygon(points, fill, stroke));
    }
}

fn draw_object(
    painter: &egui::Painter,
    view: &View,
    object: &ResolvedObject,
    project: &Project,
    textures: &std::collections::HashMap<Uuid, egui::TextureHandle>,
    selection: &Selection,
) {
    let Some(size) = util::object_size(&object.instance, &project.symbols) else {
        return;
    };
    match &object.instance.kind {
        ObjectKind::Bitmap { asset, .. } => {
            if let Some(texture) = textures.get(asset) {
                draw_bitmap(painter, view, &object.instance, size, texture);
            } else {
                // Asset bytes are still decoding or missing: show the bounds
                // so the placement stays editable.
                let fill = Color32::from_rgba_unmultiplied(120, 140, 160, 60);
                let points = outline_points(view, &object.instance, size);
                painter.add(Shape::convex_polygon(
                    points,
                    fill,
                    Stroke::new(1.0, theme::TEXT_FAINT),
                ));
            }
        }
        ObjectKind::Path { paths, .. } => {
            draw_paths(painter, view, &object.instance, paths);
        }
        ObjectKind::Tilemap(map) => {
            draw_tilemap(painter, view, &object.instance, map, project, textures);
        }
        ObjectKind::Text(text) => {
            draw_paths(
                painter,
                view,
                &object.instance,
                &text.paths(object.instance.fill),
            );
        }
        _ => {
            let fill = util::to_color32(object.instance.fill);
            let stroke = if fill.a() < 20 {
                Stroke::new(1.0, Color32::from_white_alpha(80))
            } else {
                Stroke::new(1.0, fill.gamma_multiply(0.55))
            };
            let points = outline_points(view, &object.instance, size);
            painter.add(Shape::convex_polygon(points, fill, stroke));
        }
    }

    if selection.contains_object(object.instance.id) {
        draw_selection(painter, view, &object.instance, size);
    }
}

/// Draws a tilemap as textured quads sampled from the tileset bitmap.
/// Only cells inside the current viewport are emitted, so large maps stay
/// cheap; the tileset texture is shared — no per-tile textures are created.
fn draw_tilemap(
    painter: &egui::Painter,
    view: &View,
    instance: &nova_core::ObjectInstance,
    map: &nova_core::TilemapData,
    project: &Project,
    textures: &std::collections::HashMap<Uuid, egui::TextureHandle>,
) {
    let Some(tileset) = project.tilesets.iter().find(|t| t.id == map.tileset) else {
        // Missing tileset: draw the grid outline so the object stays editable.
        let points = outline_points(
            view,
            instance,
            util::object_size(instance, &project.symbols).unwrap_or((map.width(), map.height())),
        );
        painter.add(Shape::convex_polygon(
            points,
            Color32::from_rgba_unmultiplied(255, 80, 80, 40),
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 80, 80, 160)),
        ));
        return;
    };
    let Some(texture) = textures.get(&tileset.asset) else {
        return; // texture still decoding
    };
    let tex_size = texture.size();
    if tex_size[0] <= 0
        || tex_size[1] <= 0
        || tileset.tile_width == 0
        || tileset.tile_height == 0
        || tileset.columns == 0
    {
        return;
    }
    let tile_w = map.tile_width.max(1.0);
    let tile_h = map.tile_height.max(1.0);

    // Viewport culling in tile coordinates. The object's local space is
    // centre-based, matching stage_point.
    let top_left = view.to_stage(view.rect.left_top());
    let bottom_right = view.to_stage(view.rect.right_bottom());
    let pad = 2.0f32;
    let cx0 = (((top_left.x - instance.transform.x) + map.width() / 2.0) / tile_w - pad)
        .floor()
        .max(0.0) as usize;
    let cy0 = (((top_left.y - instance.transform.y) + map.height() / 2.0) / tile_h - pad)
        .floor()
        .max(0.0) as usize;
    let cx1 = (((bottom_right.x - instance.transform.x) + map.width() / 2.0) / tile_w + pad)
        .ceil()
        .min(map.columns as f32) as usize;
    let cy1 = (((bottom_right.y - instance.transform.y) + map.height() / 2.0) / tile_h + pad)
        .ceil()
        .min(map.rows as f32) as usize;

    let mut mesh = egui::Mesh::with_texture(texture.id());
    let tw = tex_size[0] as f32;
    let th = tex_size[1] as f32;
    for layer in &map.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let alpha = (layer.opacity.clamp(0.0, 1.0) * 255.0) as u8;
        for cy in cy0..cy1.min(map.rows) {
            for cx in cx0..cx1.min(map.columns) {
                let cell = layer.cells.get(cy * map.columns + cx).copied().unwrap_or(0);
                if cell == 0 {
                    continue;
                }
                let index = cell as usize - 1;
                let Some((tile_col, tile_row, tile_pw, tile_ph)) =
                    tileset.tile_rect(index, tex_size[0] as u32, tex_size[1] as u32)
                else {
                    continue;
                };
                // UV rect of the tile inside the texture.
                let u0 = tile_col as f32 / tw;
                let v0 = tile_row as f32 / th;
                let u1 = (tile_col + tile_pw) as f32 / tw;
                let v1 = (tile_row + tile_ph) as f32 / th;
                // Local quad, centre-based.
                let lx0 = cx as f32 * tile_w - map.width() / 2.0;
                let ly0 = cy as f32 * tile_h - map.height() / 2.0;
                let corners_local = [
                    (lx0, ly0, u0, v0),
                    (lx0 + tile_w, ly0, u1, v0),
                    (lx0 + tile_w, ly0 + tile_h, u1, v1),
                    (lx0, ly0 + tile_h, u0, v1),
                ];
                let vertex_base = mesh.vertices.len() as u32;
                for (lx, ly, u, v) in corners_local {
                    let point = util::stage_point(instance, egui::vec2(lx, ly));
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: view.to_screen(Pos2::new(point.x, point.y)),
                        uv: egui::pos2(u, v),
                        color: egui::Color32::from_white_alpha(alpha),
                    });
                }
                mesh.indices.extend_from_slice(&[
                    vertex_base,
                    vertex_base + 1,
                    vertex_base + 2,
                    vertex_base,
                    vertex_base + 2,
                    vertex_base + 3,
                ]);
            }
        }
    }
    if !mesh.indices.is_empty() {
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Draws imported or generated vector sub-paths with the same transforms the
/// exporter uses. Quadratic curves are tessellated for the screen only; the
/// stored/exported geometry remains untouched.
fn draw_paths(
    painter: &egui::Painter,
    view: &View,
    instance: &nova_core::ObjectInstance,
    paths: &[PathShape],
) {
    for path in paths {
        let points = path_points(instance, path)
            .into_iter()
            .map(|point| view.to_screen(point))
            .collect::<Vec<_>>();
        if points.len() < 3 {
            continue;
        }
        let fill = util::to_color32(path.fill);
        if fill.a() == 0 {
            continue;
        }
        painter.add(Shape::convex_polygon(points, fill, Stroke::NONE));
    }
}

fn path_points(instance: &nova_core::ObjectInstance, path: &PathShape) -> Vec<Pos2> {
    let mut points = Vec::new();
    let mut cursor = egui::Vec2::ZERO;
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                cursor = egui::vec2(x, y);
                points.push(util::stage_point(instance, cursor));
            }
            PathCommand::LineTo { x, y } => {
                cursor = egui::vec2(x, y);
                points.push(util::stage_point(instance, cursor));
            }
            PathCommand::CurveTo {
                control_x,
                control_y,
                x,
                y,
            } => {
                let start = cursor;
                let control = egui::vec2(control_x, control_y);
                let end = egui::vec2(x, y);
                for step in 1..=8 {
                    let t = step as f32 / 8.0;
                    let one = 1.0 - t;
                    let local = start * (one * one) + control * (2.0 * one * t) + end * (t * t);
                    points.push(util::stage_point(instance, local));
                }
                cursor = end;
            }
        }
    }
    points
}

fn outline_points(
    view: &View,
    instance: &nova_core::ObjectInstance,
    size: (f32, f32),
) -> Vec<Pos2> {
    match instance.kind {
        nova_core::ObjectKind::Ellipse { .. } => util::ellipse_outline(instance, size, 48)
            .into_iter()
            .map(|point| view.to_screen(point))
            .collect(),
        _ => util::corners(instance, size)
            .into_iter()
            .map(|point| view.to_screen(point))
            .collect(),
    }
}

fn draw_selection(
    painter: &egui::Painter,
    view: &View,
    instance: &nova_core::ObjectInstance,
    size: (f32, f32),
) {
    let corners: Vec<Pos2> = util::corners(instance, size)
        .into_iter()
        .map(|point| view.to_screen(point))
        .collect();
    painter.add(Shape::closed_line(
        corners.clone(),
        Stroke::new(1.5, theme::ACCENT),
    ));
    for corner in &corners {
        let handle = Rect::from_center_size(*corner, Vec2::splat(5.0));
        painter.rect_filled(handle, 1.0, theme::ACCENT);
        painter.rect_stroke(
            handle,
            1.0,
            Stroke::new(1.0, theme::BG_DEEPEST),
            StrokeKind::Outside,
        );
    }

    let top = corners
        .iter()
        .fold(f32::INFINITY, |left, point| left.min(point.y));
    let left = corners
        .iter()
        .fold(f32::INFINITY, |left, point| left.min(point.x));
    painter.text(
        Pos2::new(left, top - 7.0),
        Align2::LEFT_BOTTOM,
        &instance.name,
        FontId::proportional(11.0),
        theme::ACCENT,
    );
}

fn draw_playhead_hint(
    painter: &egui::Painter,
    stage_rect: &Rect,
    state: &AppState,
    scene_name: &str,
    fps: f32,
) {
    painter.text(
        Pos2::new(stage_rect.left(), stage_rect.top() - 8.0),
        Align2::LEFT_BOTTOM,
        format!("{scene_name} · Frame {}", state.frame + 1),
        FontId::proportional(11.0),
        theme::TEXT_DIM,
    );
    painter.text(
        Pos2::new(stage_rect.right(), stage_rect.top() - 8.0),
        Align2::RIGHT_BOTTOM,
        format!("{:.0}% · {fps:.0} fps", state.zoom * 100.0),
        FontId::proportional(11.0),
        theme::TEXT_FAINT,
    );

    if let Some(runtime) = &state.game_preview {
        let status = match runtime.status {
            nova_core::game::runtime::RuntimeStatus::Running => "läuft",
            nova_core::game::runtime::RuntimeStatus::Paused => "pausiert",
            nova_core::game::runtime::RuntimeStatus::Stopped => "gestoppt",
        };
        painter.text(
            Pos2::new(stage_rect.center().x, stage_rect.top() - 8.0),
            Align2::CENTER_BOTTOM,
            format!(
                "{} · t={:.1}s · {}",
                runtime.scene_name, runtime.time, status
            ),
            FontId::proportional(11.0),
            theme::ACCENT,
        );
    }

    if state.onion_skin {
        painter.text(
            Pos2::new(stage_rect.left(), stage_rect.bottom() + 8.0),
            Align2::LEFT_TOP,
            "Onion Skin: 2 Frames",
            FontId::proportional(10.0),
            theme::TEXT_FAINT,
        );
    }
}

// ------------------------------------------------------------------ tools

fn tool_strip(ui: &mut Ui, state: &mut AppState, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        for tool in [
            crate::state::Tool::Select,
            crate::state::Tool::Line,
            crate::state::Tool::Pencil,
            crate::state::Tool::Brush,
            crate::state::Tool::Eraser,
            crate::state::Tool::Fill,
            crate::state::Tool::PathEdit,
            crate::state::Tool::Pen,
        ] {
            let selected = state.tool == tool;
            if ui
                .selectable_label(
                    selected,
                    egui::RichText::new(tool.label())
                        .small()
                        .color(if selected {
                            theme::ACCENT
                        } else {
                            theme::TEXT_DIM
                        }),
                )
                .on_hover_text(tool.hint())
                .clicked()
            {
                actions.push(UiAction::SetTool(tool));
            }
        }
        ui.separator();
        ui.label(
            egui::RichText::new(state.tool.hint())
                .small()
                .color(theme::TEXT_FAINT),
        );
    });
}

/// Topmost visual object under a stage-space point at the current frame.
fn hit_object(
    project: &Project,
    state: &AppState,
    stage: Pos2,
) -> Option<nova_core::ResolvedObject> {
    let scene = project.edit_scene(state.edit_symbol)?;
    nova_core::resolve_scene(scene, state.frame)
        .into_iter()
        .rev()
        .find(|object| {
            util::object_size(&object.instance, &project.symbols)
                .is_some_and(|size| util::hit_instance(&object.instance, size, stage))
        })
}

fn select_input(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut AppState,
    view: &View,
    response: &egui::Response,
    shift: bool,
    actions: &mut Vec<UiAction>,
) {
    let hit = if response.clicked() || response.drag_started() {
        response.interact_pointer_pos().and_then(|pointer| {
            hit_object(project, state, view.to_stage(pointer)).map(|object| object.instance.id)
        })
    } else {
        None
    };

    // Transform handles take precedence over object dragging.
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some((mode, pointer)) = response.interact_pointer_pos().and_then(|pointer| {
            transform_handle_at(project, state, view, pointer).map(|mode| (mode, pointer))
        }) {
            start_transform_drag(project, state, view, mode, pointer);
        }
    }
    if state.transform_drag.is_some() && response.dragged_by(egui::PointerButton::Primary) {
        if let (Some(pointer), Some(drag)) = (
            response.interact_pointer_pos(),
            state.transform_drag.clone(),
        ) {
            apply_transform_drag(project, state, view, &drag, pointer);
            state.dirty = true;
        }
    }

    if response.clicked() && state.transform_drag.is_none() {
        match hit {
            Some(id) => {
                toggle_object_selection(state, id, shift);
                if let Some(resolved) = state.resolved_object(project, id) {
                    state.set_status(format!(
                        "{} · Frame {} · {:.0}%",
                        resolved.instance.name,
                        state.frame + 1,
                        state.zoom * 100.0
                    ));
                    if response.double_clicked()
                        && let Some(script) = resolved.instance.script
                    {
                        actions.push(UiAction::OpenScript(script));
                    }
                }
            }
            None => {
                if !shift {
                    state.selection = Selection::Scene;
                }
                state.dragging = None;
                state.marquee = None;
            }
        }
    }

    if response.drag_started_by(egui::PointerButton::Primary) && state.transform_drag.is_none() {
        if let Some(id) = hit {
            toggle_object_selection(state, id, shift);
            let ids = state.selection.object_ids().to_vec();
            let mut objects = Vec::new();
            for object_id in ids {
                if let Some(resolved) = state.resolved_object(project, object_id) {
                    objects.push((object_id, resolved.layer_id, resolved.instance.transform));
                }
            }
            state.dragging = Some(crate::state::ObjectDragStart {
                frame: state.frame,
                objects,
            });
            state.marquee = None;
        } else {
            let base = if shift {
                state.selection.object_ids().to_vec()
            } else {
                Vec::new()
            };
            if !shift {
                state.selection = Selection::None;
            }
            let stage = response
                .interact_pointer_pos()
                .map(|pointer| view.to_stage(pointer))
                .unwrap_or(Pos2::ZERO);
            state.dragging = None;
            state.marquee = Some(crate::state::Marquee {
                start: stage,
                current: stage,
                base,
            });
        }
    }

    if response.dragged_by(egui::PointerButton::Primary) && state.transform_drag.is_none() {
        if let Some(drag) = state.dragging.clone() {
            let delta = response
                .interact_pointer_pos()
                .and_then(|pointer| {
                    ui.input(|input| input.pointer.press_origin())
                        .map(|origin| (pointer - origin) / state.zoom)
                })
                .unwrap_or_else(|| response.drag_delta() / state.zoom);
            move_objects(project, state, &drag, delta);
            state.dirty = true;
        } else if let Some(mut marquee) = state.marquee.take() {
            if let Some(pointer) = response.interact_pointer_pos() {
                marquee.current = view.to_stage(pointer);
            }
            let ids = marquee_hits(project, state.edit_symbol, state.frame, &marquee);
            state.selection = if ids.is_empty() {
                Selection::None
            } else {
                Selection::Objects(ids)
            };
            state.marquee = Some(marquee);
        }
    }

    if response.drag_stopped() {
        state.dragging = None;
        state.marquee = None;
        state.transform_drag = None;
        state.node_drag = None;
    }
}

// ------------------------------------------------------- transform handles

/// Axis-aligned stage bounds of the current object selection plus the
/// per-object transforms captured for dragging.
fn selection_bounds(
    project: &Project,
    state: &AppState,
) -> Option<(Pos2, Pos2, Vec<(Uuid, Uuid, nova_core::Transform)>)> {
    let ids = state.selection.object_ids();
    if ids.is_empty() {
        return None;
    }
    let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut transforms = Vec::new();
    for id in ids {
        let resolved = state.resolved_object(project, *id)?;
        let size = util::object_size(&resolved.instance, &project.symbols)?;
        for corner in util::corners(&resolved.instance, size) {
            min.x = min.x.min(corner.x);
            min.y = min.y.min(corner.y);
            max.x = max.x.max(corner.x);
            max.y = max.y.max(corner.y);
        }
        transforms.push((*id, resolved.layer_id, resolved.instance.transform));
    }
    Some((min, max, transforms))
}

fn transform_handle_at(
    project: &Project,
    state: &AppState,
    view: &View,
    pointer: Pos2,
) -> Option<crate::state::TransformMode> {
    let (min, max, _) = selection_bounds(project, state)?;
    let tl = view.to_screen(min);
    let br = view.to_screen(max);
    let rect = Rect::from_min_max(tl, br);
    let rotate_pos = Pos2::new(rect.center().x, rect.top() - 26.0);

    if pointer.distance(rotate_pos) <= 9.0 {
        return Some(crate::state::TransformMode::Rotate);
    }
    for (corner, mode) in [
        (rect.left_top(), crate::state::TransformMode::ScaleTopLeft),
        (rect.right_top(), crate::state::TransformMode::ScaleTopRight),
        (
            rect.left_bottom(),
            crate::state::TransformMode::ScaleBottomLeft,
        ),
        (
            rect.right_bottom(),
            crate::state::TransformMode::ScaleBottomRight,
        ),
    ] {
        if Rect::from_center_size(corner, Vec2::splat(10.0)).contains(pointer) {
            return Some(mode);
        }
    }
    None
}

fn start_transform_drag(
    project: &Project,
    state: &mut AppState,
    view: &View,
    mode: crate::state::TransformMode,
    pointer: Pos2,
) {
    let Some((min, max, transforms)) = selection_bounds(project, state) else {
        return;
    };
    let centroid = Pos2::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);
    let delta = view.to_stage(pointer) - centroid;
    let start_angle = delta.y.atan2(delta.x);
    let scale = transforms
        .first()
        .map(|(_, _, transform)| (transform.scale_x, transform.scale_y))
        .unwrap_or((1.0, 1.0));
    state.transform_drag = Some(crate::state::TransformDrag {
        mode,
        start_pointer: view.to_stage(pointer),
        centroid,
        start_angle,
        start_transforms: transforms,
        start_scale: scale,
    });
    state.dragging = None;
    state.marquee = None;
}

fn apply_transform_drag(
    project: &mut Project,
    state: &mut AppState,
    view: &View,
    drag: &crate::state::TransformDrag,
    pointer: Pos2,
) {
    let stage = view.to_stage(pointer);
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let frame = state.frame;
    match drag.mode {
        crate::state::TransformMode::Rotate => {
            let delta = stage - drag.centroid;
            let angle = delta.y.atan2(delta.x);
            let degrees = (angle - drag.start_angle).to_degrees();
            for (object_id, layer_id, start) in &drag.start_transforms {
                if let Some(target) =
                    util::governing_object_mut(scene, *layer_id, frame, *object_id)
                {
                    target.transform.rotation = start.rotation + degrees;
                }
            }
        }
        _ => {
            // Uniform scale about the selection centroid (the documented
            // centre-based pivot). Free aspect ratio remains available in the
            // properties panel.
            let start_delta = drag.start_pointer - drag.centroid;
            let current_delta = stage - drag.centroid;
            let start_len = start_delta.length().max(0.001);
            let factor = (current_delta.length() / start_len).clamp(0.02, 50.0);
            for (object_id, layer_id, start) in &drag.start_transforms {
                if let Some(target) =
                    util::governing_object_mut(scene, *layer_id, frame, *object_id)
                {
                    target.transform.scale_x = start.scale_x * factor;
                    target.transform.scale_y = start.scale_y * factor;
                }
            }
        }
    }
}

// ------------------------------------------------------------- draw tools

fn draw_tool_input(
    _ui: &mut Ui,
    project: &mut Project,
    state: &mut AppState,
    view: &View,
    response: &egui::Response,
    _shift: bool,
) {
    let tool = state.tool;
    if tool == crate::state::Tool::Line {
        if response.drag_started_by(egui::PointerButton::Primary) {
            let stage = response
                .interact_pointer_pos()
                .map(|pointer| view.to_stage(pointer))
                .unwrap_or(Pos2::ZERO);
            state.draft = Some(crate::state::DraftStroke {
                tool,
                points: vec![stage],
            });
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pointer) = response.interact_pointer_pos() {
                let stage = view.to_stage(pointer);
                if let Some(draft) = &mut state.draft {
                    if draft.points.len() == 1 {
                        draft.points.push(stage);
                    } else {
                        *draft.points.last_mut().unwrap() = stage;
                    }
                }
            }
        }
        if response.drag_stopped() {
            commit_draft(project, state);
        }
        return;
    }

    if tool == crate::state::Tool::Eraser || tool == crate::state::Tool::Fill {
        if response.clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let stage = view.to_stage(pointer);
                match hit_object(project, state, stage) {
                    Some(object) => {
                        if tool == crate::state::Tool::Eraser {
                            erase_object(project, state, &object);
                        } else {
                            fill_object(project, state, &object);
                        }
                    }
                    None => {
                        if tool == crate::state::Tool::Fill {
                            state.set_status("Kein Objekt unter dem Zeiger");
                        }
                    }
                }
            }
        }
        return;
    }

    // Pencil / Brush: freehand stroke.
    if response.drag_started_by(egui::PointerButton::Primary) {
        let stage = response
            .interact_pointer_pos()
            .map(|pointer| view.to_stage(pointer))
            .unwrap_or(Pos2::ZERO);
        state.draft = Some(crate::state::DraftStroke {
            tool,
            points: vec![stage],
        });
        state.dragging = None;
        state.marquee = None;
    }
    if response.dragged_by(egui::PointerButton::Primary) {
        if let Some(pointer) = response.interact_pointer_pos() {
            let stage = view.to_stage(pointer);
            if let Some(draft) = &mut state.draft {
                if let Some(last) = draft.points.last() {
                    if last.distance(stage) > 1.5 {
                        draft.points.push(stage);
                    }
                }
            }
        }
    }
    if response.drag_stopped() {
        commit_draft(project, state);
    }
}

fn path_edit_input(
    project: &mut Project,
    state: &mut AppState,
    view: &View,
    response: &egui::Response,
) {
    let _ = project;
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pointer) = response.interact_pointer_pos() {
            if let Some(node) = node_at(project, state, view, pointer) {
                state.node_drag = Some(node);
            }
        }
    }
    if state.node_drag.is_some() && response.dragged_by(egui::PointerButton::Primary) {
        if let (Some(pointer), Some(node)) = (response.interact_pointer_pos(), state.node_drag) {
            let stage = view.to_stage(pointer);
            move_node(project, state, node, stage);
            state.dirty = true;
        }
    }
    if response.drag_stopped() {
        state.node_drag = None;
    }
    // Right-click a node deletes it (a MoveTo removes its whole sub-path).
    if response.secondary_clicked()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(node) = node_at(project, state, view, pointer)
    {
        delete_node(project, state, node);
        return;
    }
    // Double-click on a straight segment inserts a new anchor.
    if response.double_clicked()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(hit) = segment_at(project, state, view, pointer)
    {
        insert_node(project, state, hit);
    }
}

// ------------------------------------------------------------------ pen

/// Pen tool input: click adds corner anchors, press-drag adds curve
/// anchors (drag position becomes the quadratic control point), clicking
/// the first anchor closes the path, Enter/double-click finishes.
fn pen_input(
    ui: &mut Ui,
    project: &mut Project,
    state: &mut AppState,
    view: &View,
    response: &egui::Response,
) {
    let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
    if escape {
        state.pen = None;
        state.set_status("Stift-Pfad verworfen");
        return;
    }
    if state.pen.is_none() {
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            state.pen = Some(PenDraft {
                anchors: vec![PenAnchor {
                    pos: view.to_stage(pointer),
                    handle: None,
                }],
                press: Some(view.to_stage(pointer)),
            });
        }
        return;
    }
    if enter || response.double_clicked() {
        commit_pen(project, state, false);
        return;
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pointer) = response.interact_pointer_pos() {
            let stage = view.to_stage(pointer);
            let draft = state.pen.as_mut().expect("draft checked above");
            if draft.press.is_none() {
                draft.anchors.push(PenAnchor {
                    pos: stage,
                    handle: None,
                });
                draft.press = Some(stage);
            }
        }
        return;
    }
    if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let stage = view.to_stage(pointer);
            let draft = state.pen.as_mut().expect("draft checked above");
            if draft.press.is_some() {
                // This press already created its anchor at drag_started.
                draft.press = None;
                return;
            }
            if draft.anchors.len() >= 3
                && view.to_screen(draft.anchors[0].pos).distance(pointer) < 10.0
            {
                commit_pen(project, state, true);
                return;
            }
            draft.anchors.push(PenAnchor {
                pos: stage,
                handle: None,
            });
        }
        return;
    }
    if response.drag_stopped() {
        let draft = state.pen.as_mut().expect("draft checked above");
        if let (Some(start), Some(end)) = (
            draft.press,
            response.interact_pointer_pos().map(|p| view.to_stage(p)),
        ) {
            if start.distance(end) > 6.0
                && let Some(last) = draft.anchors.last_mut()
            {
                last.handle = Some(end);
            }
        }
        draft.press = None;
    }
}

/// Converts the pen draft into a Path object. Anchor positions and handles
/// are stage coordinates; the stored commands are local to the object
/// centre (matching the node editor and the SWF exporter).
fn commit_pen(project: &mut Project, state: &mut AppState, closed: bool) {
    let Some(draft) = state.pen.take() else {
        return;
    };
    let mut anchors = draft.anchors;
    if anchors.len() < 2 {
        state.set_status("Stift: mindestens zwei Anker nötig");
        return;
    }
    if closed && anchors.len() < 3 {
        state.set_status("Stift: geschlossene Pfade brauchen drei Anker");
        return;
    }
    // Bounds over anchors and control points (curve extremes stay inside
    // the anchor/handle hull for quadratic segments).
    let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for anchor in &anchors {
        for p in [anchor.pos].into_iter().chain(anchor.handle) {
            min = Pos2::new(min.x.min(p.x), min.y.min(p.y));
            max = Pos2::new(max.x.max(p.x), max.y.max(p.y));
        }
    }
    let center = Pos2::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);
    let local = |p: Pos2| p - center.to_vec2();
    let mut commands = Vec::with_capacity(anchors.len() + 1);
    for (index, anchor) in anchors.iter().enumerate() {
        let (x, y) = {
            let l = local(anchor.pos);
            (l.x, l.y)
        };
        if index == 0 {
            commands.push(PathCommand::MoveTo { x, y });
            continue;
        }
        match anchor.handle {
            Some(handle) => {
                let c = local(handle);
                commands.push(PathCommand::CurveTo {
                    control_x: c.x,
                    control_y: c.y,
                    x,
                    y,
                });
            }
            None => commands.push(PathCommand::LineTo { x, y }),
        }
    }
    if closed {
        let first = anchors[0];
        let l = local(first.pos);
        match first.handle {
            Some(handle) => {
                let c = local(handle);
                commands.push(PathCommand::CurveTo {
                    control_x: c.x,
                    control_y: c.y,
                    x: l.x,
                    y: l.y,
                });
            }
            None => commands.push(PathCommand::LineTo { x: l.x, y: l.y }),
        }
    }
    let object = ObjectInstance::path(
        "Stift",
        center.x,
        center.y,
        (max.x - min.x).max(1.0),
        (max.y - min.y).max(1.0),
        vec![PathShape {
            fill: state.new_object_color,
            commands,
        }],
        state.new_object_color,
    );
    commit_object(project, state, object);
    state.set_status(if closed {
        "Geschlossener Stift-Pfad erstellt".to_string()
    } else {
        "Stift-Pfad erstellt".to_string()
    });
}

/// Draws the in-progress pen path: anchor polyline plus handle hints.
fn draw_pen_preview(painter: &egui::Painter, view: &View, state: &AppState) {
    let Some(draft) = &state.pen else {
        return;
    };
    if draft.anchors.is_empty() {
        return;
    }
    let screen = |p: Pos2| view.to_screen(p);
    let color = theme::ACCENT;
    let mut prev = draft.anchors[0].pos;
    for anchor in draft.anchors.iter().skip(1) {
        painter.line_segment([screen(prev), screen(anchor.pos)], Stroke::new(1.5, color));
        prev = anchor.pos;
    }
    for (index, anchor) in draft.anchors.iter().enumerate() {
        let center = screen(anchor.pos);
        painter.circle_filled(center, 3.5, color);
        if index == 0 {
            painter.circle_stroke(center, 6.0, Stroke::new(1.0, color));
        }
        if let Some(handle) = anchor.handle {
            painter.line_segment(
                [center, screen(handle)],
                Stroke::new(1.0, theme::TEXT_FAINT),
            );
            painter.circle_stroke(screen(handle), 3.0, Stroke::new(1.0, theme::TEXT_FAINT));
        }
    }
}

// ------------------------------------------------------------- commits

/// Inserts a drawn object into the currently edited timeline, mirroring the
/// keyframe carry-over behaviour of the app-layer shape tools.
fn commit_object(project: &mut Project, state: &mut AppState, object: ObjectInstance) {
    let Some(target) = target_layer_id(project, state) else {
        state.set_status("Keine Ebene zum Einfügen vorhanden");
        return;
    };
    let frame = state.frame;
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
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
    let id = object.id;
    layer.frames[frame].objects.push(object);
    project.touch();
    state.dirty = true;
    state.selection = Selection::object(id);
    state.mode = crate::state::Mode::Animation;
}

fn target_layer_id(project: &Project, state: &AppState) -> Option<Uuid> {
    match &state.selection {
        Selection::Layer(id) => Some(*id),
        Selection::Objects(ids) => ids
            .first()
            .and_then(|id| state.layer_of_object(project, *id)),
        _ => project
            .edit_scene(state.edit_symbol)
            .and_then(|scene| scene.layers.last().map(|layer| layer.id)),
    }
}

fn commit_draft(project: &mut Project, state: &mut AppState) {
    let Some(draft) = state.draft.take() else {
        return;
    };
    match draft.tool {
        crate::state::Tool::Line => {
            if draft.points.len() < 2 {
                return;
            }
            let a = draft.points[0];
            let b = draft.points[draft.points.len() - 1];
            let delta = b - a;
            let length = delta.length();
            if length < 2.0 {
                return;
            }
            let mut object = ObjectInstance::rect(
                "Linie",
                (a.x + b.x) / 2.0,
                (a.y + b.y) / 2.0,
                length,
                2.0,
                state.new_object_color,
            );
            object.transform.rotation = delta.y.atan2(delta.x).to_degrees();
            commit_object(project, state, object);
        }
        crate::state::Tool::Pencil | crate::state::Tool::Brush => {
            if draft.points.len() < 2 {
                return;
            }
            let width = if draft.tool == crate::state::Tool::Brush {
                12.0
            } else {
                2.0
            };
            let (min, max) = draft.points.iter().fold(
                (
                    Pos2::new(f32::INFINITY, f32::INFINITY),
                    Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
                ),
                |(lo, hi), p| {
                    (
                        Pos2::new(lo.x.min(p.x), lo.y.min(p.y)),
                        Pos2::new(hi.x.max(p.x), hi.y.max(p.y)),
                    )
                },
            );
            // Path commands are stored in object-local space (centred on
            // the transform origin); draft points are stage coordinates.
            let center = Pos2::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);
            let local_points: Vec<Pos2> =
                draft.points.iter().map(|p| *p - center.to_vec2()).collect();
            let commands = stroke_polygon(&local_points, width);
            if commands.len() < 3 {
                return;
            }
            let object = ObjectInstance::path(
                if draft.tool == crate::state::Tool::Brush {
                    "Pinsel"
                } else {
                    "Bleistift"
                },
                (min.x + max.x) / 2.0,
                (min.y + max.y) / 2.0,
                (max.x - min.x).max(1.0),
                (max.y - min.y).max(1.0),
                vec![PathShape {
                    fill: state.new_object_color,
                    commands,
                }],
                state.new_object_color,
            );
            commit_object(project, state, object);
        }
        _ => {}
    }
}

/// Builds a closed filled polygon around a polyline (pencil/brush strokes are
/// stored as filled shapes because SWF shapes have no stroke width).
fn stroke_polygon(points: &[Pos2], width: f32) -> Vec<PathCommand> {
    if points.len() < 2 {
        return Vec::new();
    }
    let half = width / 2.0;
    let mut left = Vec::with_capacity(points.len());
    let mut right = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let prev = points[index.saturating_sub(1)];
        let next = points[(index + 1).min(points.len() - 1)];
        let dir = next - prev;
        let len = dir.length().max(0.001);
        let normal = Vec2::new(-dir.y / len, dir.x / len) * half;
        left.push(*point + normal);
        right.push(*point - normal);
    }
    let mut commands = Vec::with_capacity(points.len() * 2 + 1);
    for (index, point) in left.iter().enumerate() {
        if index == 0 {
            commands.push(PathCommand::MoveTo {
                x: point.x,
                y: point.y,
            });
        } else {
            commands.push(PathCommand::LineTo {
                x: point.x,
                y: point.y,
            });
        }
    }
    for point in right.iter().rev() {
        commands.push(PathCommand::LineTo {
            x: point.x,
            y: point.y,
        });
    }
    commands.push(PathCommand::LineTo {
        x: left[0].x,
        y: left[0].y,
    });
    commands
}

fn erase_object(project: &mut Project, state: &mut AppState, object: &nova_core::ResolvedObject) {
    let frame = state.frame;
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let Some(layer) = scene
        .layers
        .iter_mut()
        .find(|layer| layer.id == object.layer_id)
    else {
        return;
    };
    let Some(key_index) = layer.last_keyframe_at_or_before(frame) else {
        return;
    };
    let before = layer.frames[key_index].objects.len();
    layer.frames[key_index]
        .objects
        .retain(|candidate| candidate.id != object.instance.id);
    if layer.frames[key_index].objects.len() != before {
        project.touch();
        state.dirty = true;
        if state.selection.contains_object(object.instance.id) {
            state.selection = Selection::None;
        }
        state.set_status(format!("{} gelöscht", object.instance.name));
    }
}

fn fill_object(project: &mut Project, state: &mut AppState, object: &nova_core::ResolvedObject) {
    let color = state.new_object_color;
    let frame = state.frame;
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let Some(layer) = scene
        .layers
        .iter_mut()
        .find(|layer| layer.id == object.layer_id)
    else {
        return;
    };
    let Some(key_index) = layer.last_keyframe_at_or_before(frame) else {
        return;
    };
    let Some(target) = layer.frames[key_index]
        .objects
        .iter_mut()
        .find(|candidate| candidate.id == object.instance.id)
    else {
        return;
    };
    match &mut target.kind {
        ObjectKind::Path { paths, .. } => {
            for path in paths.iter_mut() {
                path.fill = color;
            }
        }
        _ => target.fill = color,
    }
    let name = target.name.clone();
    project.touch();
    state.dirty = true;
    state.set_status(format!("{name} gefüllt"));
}

// ------------------------------------------------------------ bone overlay

/// World pose of the edited symbol's skeleton at the current frame, or
/// `None` when no rigged symbol is being edited.
fn bone_pose<'a>(
    project: &'a Project,
    state: &AppState,
) -> Option<(&'a nova_core::Skeleton, Vec<(Uuid, nova_core::BonePose)>)> {
    let symbol = project.symbol(state.edit_symbol?)?;
    let skeleton = symbol.skeleton.as_ref()?;
    let clip = symbol
        .bone_clips
        .iter()
        .find(|c| Some(c.name.as_str()) == symbol.active_clip.as_deref());
    Some((skeleton, skeleton.pose_at(clip, state.frame as u32)))
}

/// Draws bones as joint→tip lines plus parent links while a rigged symbol
/// timeline is edited. Skeleton space equals stage space here because the
/// symbol timeline is shown unscaled.
fn draw_bone_overlay(painter: &egui::Painter, view: &View, project: &Project, state: &AppState) {
    let Some((skeleton, pose)) = bone_pose(project, state) else {
        return;
    };
    let pos_of = |id: Uuid| pose.iter().find(|(bid, _)| *bid == id).map(|(_, p)| *p);
    for bone in &skeleton.bones {
        let Some(p) = pos_of(bone.id) else {
            continue;
        };
        let joint = view.to_screen(Pos2::new(p.x, p.y));
        let rad = p.rotation.to_radians();
        let tip = view.to_screen(Pos2::new(
            p.x + rad.cos() * bone.length,
            p.y + rad.sin() * bone.length,
        ));
        let color = if state.bone_drag == Some(bone.id) {
            theme::RED
        } else {
            theme::ACCENT
        };
        painter.line_segment([joint, tip], Stroke::new(3.0, color.gamma_multiply(0.5)));
        if let Some(parent) = bone.parent.and_then(|pid| pos_of(pid)) {
            painter.line_segment(
                [view.to_screen(Pos2::new(parent.x, parent.y)), joint],
                Stroke::new(1.0, theme::TEXT_FAINT),
            );
        }
        painter.circle_filled(joint, 4.0, color);
        painter.text(
            joint + Vec2::new(6.0, -6.0),
            Align2::LEFT_BOTTOM,
            &bone.name,
            FontId::proportional(10.0),
            theme::TEXT_DIM,
        );
    }
}

/// Drag a bone joint to move it (updates `bone.x`/`bone.y` in skeleton
/// space). Returns `true` when the pointer is over a joint so the select
/// tool does not also hit-test objects.
fn bone_joint_input(
    project: &mut Project,
    state: &mut AppState,
    view: &View,
    response: &egui::Response,
) -> bool {
    if state.edit_symbol.is_none() || bone_pose(project, state).is_none() {
        return false;
    }
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let Some((skeleton, pose)) = bone_pose(project, state) else {
            return false;
        };
        let mut best: Option<(f32, Uuid)> = None;
        for (id, p) in &pose {
            let screen = view.to_screen(Pos2::new(p.x, p.y));
            let dist = screen.distance(pointer);
            if dist <= 9.0 && best.as_ref().is_none_or(|(d, _)| dist < *d) {
                best = Some((dist, *id));
            }
        }
        if let Some((_, id)) = best {
            let _ = skeleton;
            state.bone_drag = Some(id);
            return true;
        }
    }
    if state.bone_drag.is_some() {
        if response.dragged_by(egui::PointerButton::Primary)
            && let (Some(pointer), Some(id)) = (response.interact_pointer_pos(), state.bone_drag)
        {
            let stage = view.to_stage(pointer);
            // bone.x/y is the joint offset in parent space; convert the
            // stage-space joint target back through the parent world pose.
            let parent_frame = bone_pose(project, state).and_then(|(skel, pose)| {
                let bone = skel.bones.iter().find(|b| b.id == id)?;
                bone.parent
                    .and_then(|pid| pose.iter().find(|(bid, _)| *bid == pid).map(|(_, p)| *p))
            });
            if let Some(symbol) = project
                .symbols
                .iter_mut()
                .find(|s| Some(s.id) == state.edit_symbol)
                && let Some(skeleton) = symbol.skeleton.as_mut()
                && let Some(bone) = skeleton.bones.iter_mut().find(|b| b.id == id)
            {
                match parent_frame {
                    Some(parent) => {
                        let dx = stage.x - parent.x;
                        let dy = stage.y - parent.y;
                        let rad = (-parent.rotation).to_radians();
                        let (s, c) = rad.sin_cos();
                        bone.x = dx * c - dy * s;
                        bone.y = dx * s + dy * c;
                    }
                    None => {
                        bone.x = stage.x;
                        bone.y = stage.y;
                    }
                }
                project.touch();
                state.dirty = true;
            }
        }
        if response.drag_stopped() {
            state.bone_drag = None;
        }
        return true;
    }
    false
}

// ------------------------------------------------------------- node edit

/// Collects editable path nodes of the single selected Path object.
fn path_nodes(
    project: &Project,
    state: &AppState,
) -> Option<(nova_core::ResolvedObject, Vec<(usize, usize, usize, Pos2)>)> {
    let ids = state.selection.object_ids();
    if ids.len() != 1 {
        return None;
    }
    let resolved = state.resolved_object(project, ids[0])?;
    let ObjectKind::Path { paths, .. } = &resolved.instance.kind else {
        return None;
    };
    let mut nodes = Vec::new();
    for (path_index, path) in paths.iter().enumerate() {
        for (command_index, command) in path.commands.iter().enumerate() {
            match *command {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                    nodes.push((path_index, command_index, 0, Pos2::new(x, y)));
                }
                PathCommand::CurveTo {
                    control_x,
                    control_y,
                    x,
                    y,
                } => {
                    nodes.push((
                        path_index,
                        command_index,
                        0,
                        Pos2::new(control_x, control_y),
                    ));
                    nodes.push((path_index, command_index, 1, Pos2::new(x, y)));
                }
            }
        }
    }
    Some((resolved, nodes))
}

fn node_at(
    project: &Project,
    state: &AppState,
    view: &View,
    pointer: Pos2,
) -> Option<crate::state::NodeDrag> {
    let (resolved, nodes) = path_nodes(project, state)?;
    let mut best: Option<(f32, crate::state::NodeDrag)> = None;
    for (path_index, command_index, slot, local) in nodes {
        let stage = util::stage_point(&resolved.instance, local.to_vec2());
        let screen = view.to_screen(stage);
        let distance = screen.distance(pointer);
        if distance <= 8.0 && best.as_ref().is_none_or(|(d, _)| distance < *d) {
            best = Some((
                distance,
                crate::state::NodeDrag {
                    object: resolved.instance.id,
                    layer: resolved.layer_id,
                    path: path_index,
                    command: command_index,
                    slot,
                },
            ));
        }
    }
    best.map(|(_, node)| node)
}

fn move_node(project: &mut Project, state: &AppState, node: crate::state::NodeDrag, stage: Pos2) {
    let Some(resolved) = state.resolved_object(project, node.object) else {
        return;
    };
    let local = util::local_point(&resolved.instance, stage);
    let (x, y) = (local.x, local.y);
    let frame = state.frame;
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == node.layer) else {
        return;
    };
    let Some(key_index) = layer.last_keyframe_at_or_before(frame) else {
        return;
    };
    let Some(object) = layer.frames[key_index]
        .objects
        .iter_mut()
        .find(|object| object.id == node.object)
    else {
        return;
    };
    let ObjectKind::Path { paths, .. } = &mut object.kind else {
        return;
    };
    let Some(path) = paths.get_mut(node.path) else {
        return;
    };
    let Some(command) = path.commands.get_mut(node.command) else {
        return;
    };
    match command {
        PathCommand::MoveTo { x: px, y: py } | PathCommand::LineTo { x: px, y: py } => {
            *px = x;
            *py = y;
        }
        PathCommand::CurveTo {
            control_x,
            control_y,
            x: px,
            y: py,
        } => {
            if node.slot == 0 {
                *control_x = x;
                *control_y = y;
            } else {
                *px = x;
                *py = y;
            }
        }
    }
}

/// A straight segment hit for node insertion: path/command indices plus the
/// local-space insertion point.
#[derive(Debug, Clone, Copy)]
struct SegmentHit {
    object: Uuid,
    layer: Uuid,
    path: usize,
    /// Index of the command that ends the segment; the new node is
    /// inserted directly before it.
    command: usize,
    point: egui::Vec2,
}

/// Finds the nearest straight (LineTo) segment of the selected path object
/// within ~8 screen px of the pointer. CurveTo segments are skipped —
/// splitting a quadratic needs de Casteljau and is left to the node drag.
fn segment_at(
    project: &Project,
    state: &AppState,
    view: &View,
    pointer: Pos2,
) -> Option<SegmentHit> {
    let ids = state.selection.object_ids();
    if ids.len() != 1 {
        return None;
    }
    let resolved = state.resolved_object(project, ids[0])?;
    let ObjectKind::Path { paths, .. } = &resolved.instance.kind else {
        return None;
    };
    let mut best: Option<(f32, SegmentHit)> = None;
    for (path_index, path) in paths.iter().enumerate() {
        let mut cursor = egui::Vec2::ZERO;
        for (command_index, command) in path.commands.iter().enumerate() {
            let end = match *command {
                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => egui::vec2(x, y),
                PathCommand::CurveTo { x, y, .. } => egui::vec2(x, y),
            };
            let is_line = matches!(command, PathCommand::LineTo { .. });
            if is_line {
                // Distance from pointer (in local space) to the segment.
                let a = util::stage_point(&resolved.instance, cursor);
                let b = util::stage_point(&resolved.instance, end);
                let ab = b - a;
                let len_sq = ab.length_sq().max(1e-6);
                let t = (((pointer - a).dot(ab)) / len_sq).clamp(0.0, 1.0);
                let closest = a + ab * t;
                let dist = view.to_screen(closest).distance(pointer);
                if dist <= 8.0 && best.as_ref().is_none_or(|(d, _)| dist < *d) {
                    // Insertion point in local space.
                    let local = util::local_point(&resolved.instance, closest);
                    best = Some((
                        dist,
                        SegmentHit {
                            object: resolved.instance.id,
                            layer: resolved.layer_id,
                            path: path_index,
                            command: command_index,
                            point: local,
                        },
                    ));
                }
            }
            cursor = end;
        }
    }
    best.map(|(_, hit)| hit)
}

/// Inserts a LineTo node before `hit.command`, splitting the segment.
fn insert_node(project: &mut Project, state: &mut AppState, hit: SegmentHit) {
    let frame = state.frame;
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == hit.layer) else {
        return;
    };
    let Some(key_index) = layer.last_keyframe_at_or_before(frame) else {
        return;
    };
    let Some(object) = layer.frames[key_index]
        .objects
        .iter_mut()
        .find(|object| object.id == hit.object)
    else {
        return;
    };
    let ObjectKind::Path { paths, .. } = &mut object.kind else {
        return;
    };
    let Some(path) = paths.get_mut(hit.path) else {
        return;
    };
    path.commands.insert(
        hit.command,
        PathCommand::LineTo {
            x: hit.point.x,
            y: hit.point.y,
        },
    );
    project.touch();
    state.dirty = true;
    state.set_status("Pfadknoten eingefügt".to_string());
}

/// Deletes the command at `node`. Deleting a MoveTo removes its whole
/// sub-path (all commands up to the next MoveTo). Empty paths/objects are
/// cleaned up so validation stays green.
fn delete_node(project: &mut Project, state: &mut AppState, node: crate::state::NodeDrag) {
    let frame = state.frame;
    let Some(scene) = project.edit_scene_mut(state.edit_symbol) else {
        return;
    };
    let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == node.layer) else {
        return;
    };
    let Some(key_index) = layer.last_keyframe_at_or_before(frame) else {
        return;
    };
    let Some(object) = layer.frames[key_index]
        .objects
        .iter_mut()
        .find(|object| object.id == node.object)
    else {
        return;
    };
    let ObjectKind::Path { paths, .. } = &mut object.kind else {
        return;
    };
    let Some(path) = paths.get_mut(node.path) else {
        return;
    };
    let is_moveto = matches!(
        path.commands.get(node.command),
        Some(PathCommand::MoveTo { .. })
    );
    if is_moveto {
        // Drop this MoveTo and everything until the next MoveTo.
        let mut end = node.command + 1;
        while end < path.commands.len() && !matches!(path.commands[end], PathCommand::MoveTo { .. })
        {
            end += 1;
        }
        path.commands.drain(node.command..end);
    } else if node.command < path.commands.len() {
        path.commands.remove(node.command);
    }
    // A path with no MoveTo cannot render; drop it, and drop the object
    // when no paths remain.
    let has_moveto = path
        .commands
        .iter()
        .any(|c| matches!(c, PathCommand::MoveTo { .. }));
    if !has_moveto {
        paths.remove(node.path);
    }
    let object_empty = paths.is_empty();
    if object_empty {
        layer.frames[key_index]
            .objects
            .retain(|o| o.id != node.object);
        if state.selection.contains_object(node.object) {
            state.selection = Selection::None;
        }
    }
    project.touch();
    state.dirty = true;
    state.set_status("Pfadknoten gelöscht".to_string());
}

fn draw_node_handles(painter: &egui::Painter, view: &View, project: &Project, state: &AppState) {
    let Some((resolved, nodes)) = path_nodes(project, state) else {
        return;
    };
    for (path_index, command_index, slot, local) in nodes {
        let stage = util::stage_point(&resolved.instance, local.to_vec2());
        let screen = view.to_screen(stage);
        let dragging = state.node_drag.is_some_and(|node| {
            node.object == resolved.instance.id
                && node.path == path_index
                && node.command == command_index
                && node.slot == slot
        });
        let rect = Rect::from_center_size(screen, Vec2::splat(7.0));
        painter.rect_filled(
            rect,
            1.0,
            if dragging {
                theme::ACCENT
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 220)
            },
        );
        painter.rect_stroke(
            rect,
            1.0,
            Stroke::new(1.0, theme::BG_DEEPEST),
            StrokeKind::Inside,
        );
    }
}

// ------------------------------------------------------- handle painting

fn draw_transform_handles(
    painter: &egui::Painter,
    view: &View,
    project: &Project,
    state: &AppState,
) {
    let Some((min, max, _)) = selection_bounds(project, state) else {
        return;
    };
    let tl = view.to_screen(min);
    let br = view.to_screen(max);
    let rect = Rect::from_min_max(tl, br);

    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 120)),
        StrokeKind::Outside,
    );
    let handle_color = theme::ACCENT;
    for corner in [
        rect.left_top(),
        rect.right_top(),
        rect.left_bottom(),
        rect.right_bottom(),
    ] {
        let handle = Rect::from_center_size(corner, Vec2::splat(7.0));
        painter.rect_filled(handle, 1.0, handle_color);
        painter.rect_stroke(
            handle,
            1.0,
            Stroke::new(1.0, theme::BG_DEEPEST),
            StrokeKind::Outside,
        );
    }
    let rotate_pos = Pos2::new(rect.center().x, rect.top() - 26.0);
    painter.circle_stroke(rotate_pos, 6.0, Stroke::new(1.5, handle_color));
    painter.line_segment(
        [
            Pos2::new(rect.center().x, rect.top()),
            rotate_pos + Vec2::new(0.0, 6.0),
        ],
        Stroke::new(1.0, handle_color),
    );
}

fn draw_draft(
    painter: &egui::Painter,
    view: &View,
    draft: &crate::state::DraftStroke,
    color: Rgba,
) {
    let points: Vec<Pos2> = draft
        .points
        .iter()
        .map(|point| view.to_screen(*point))
        .collect();
    if points.len() < 2 {
        return;
    }
    let stroke_color = util::to_color32(color);
    if draft.tool == crate::state::Tool::Line {
        painter.line_segment(
            [points[0], *points.last().unwrap()],
            Stroke::new(2.0, stroke_color),
        );
    } else {
        painter.add(egui::Shape::line(points, Stroke::new(2.0, stroke_color)));
    }
}

/// Draws a bitmap asset as a textured quad honouring the object transform.
fn draw_bitmap(
    painter: &egui::Painter,
    view: &View,
    instance: &nova_core::ObjectInstance,
    size: (f32, f32),
    texture: &egui::TextureHandle,
) {
    let corners = util::corners(instance, size);
    let screen: Vec<Pos2> = corners.iter().map(|point| view.to_screen(*point)).collect();
    let mut mesh = egui::Mesh::with_texture(texture.id());
    let uvs = [
        egui::pos2(0.0, 0.0),
        egui::pos2(1.0, 0.0),
        egui::pos2(1.0, 1.0),
        egui::pos2(0.0, 1.0),
    ];
    for (index, point) in screen.iter().enumerate() {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *point,
            uv: uvs[index],
            color: util::to_color32(instance.fill),
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
    if let ObjectKind::Bitmap {
        crop: Some(crop), ..
    } = &instance.kind
        && !crop.is_full()
    {
        painter.rect_stroke(
            Rect::from_points(&[screen[0], screen[2]]),
            0.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 60, 200)),
            StrokeKind::Inside,
        );
    }
}

/// Draws resolved particle views (shared by game preview and edit preview).
fn draw_particle_views(
    painter: &egui::Painter,
    view: &View,
    views: &[nova_core::particles::ParticleView],
) {
    for particle in views {
        if particle.size <= 0.0 || particle.opacity <= 0.0 {
            continue;
        }
        let center = view.to_screen(Pos2::new(particle.x, particle.y));
        let radius = (particle.size * view.zoom / 2.0).max(0.5);
        let alpha = (particle.color.a as f32 * particle.opacity).round() as u8;
        let color = Color32::from_rgba_unmultiplied(
            particle.color.r,
            particle.color.g,
            particle.color.b,
            alpha,
        );
        let radians = particle.rotation.to_radians();
        let (sin, cos) = radians.sin_cos();
        let quad = [
            center + Vec2::new(-radius * cos + radius * sin, -radius * sin - radius * cos),
            center + Vec2::new(radius * cos + radius * sin, radius * sin - radius * cos),
            center + Vec2::new(radius * cos - radius * sin, radius * sin + radius * cos),
            center + Vec2::new(-radius * cos - radius * sin, -radius * sin + radius * cos),
        ];
        painter.add(Shape::convex_polygon(quad.to_vec(), color, Stroke::NONE));
    }
}

/// Steps the edit-mode particle systems for every resolved object carrying
/// an emitter. Systems are recreated when the spec changes; removed objects
/// are pruned so restarts never accumulate particles.
fn step_edit_particles(state: &mut AppState, objects: &[nova_core::ResolvedObject], dt: f32) {
    use std::collections::HashSet;
    let mut live: HashSet<uuid::Uuid> = HashSet::new();
    for resolved in objects {
        let instance = &resolved.instance;
        let Some(spec) = &instance.particles else {
            continue;
        };
        live.insert(instance.id);
        let origin = (instance.transform.x, instance.transform.y);
        let entry = state.edit_particles.entry(instance.id).or_insert_with(|| {
            (
                spec.clone(),
                nova_core::particles::ParticleSystem::new(spec.clone()),
                origin,
            )
        });
        if entry.0 != *spec {
            entry.0 = spec.clone();
            entry.1 = nova_core::particles::ParticleSystem::new(spec.clone());
        }
        entry.2 = origin;
        entry.1.update(origin, dt);
    }
    state.edit_particles.retain(|id, _| live.contains(id));
}

/// Render-ready views of all live edit-mode particle systems.
fn edit_particle_views(state: &AppState) -> Vec<Vec<nova_core::particles::ParticleView>> {
    state
        .edit_particles
        .values()
        .map(|(_, system, origin)| system.view(*origin))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, Marquee, ObjectDragStart};
    use egui::pos2;
    use nova_core::{ObjectInstance, Rgba};

    fn view() -> View {
        View {
            rect: Rect::from_min_size(pos2(10.0, 20.0), Vec2::new(400.0, 300.0)),
            center: pos2(275.0, 200.0),
            zoom: 2.0,
        }
    }

    #[test]
    fn stage_and_screen_roundtrip() {
        let view = view();
        let stage = pos2(42.0, 17.0);
        let back = view.to_stage(view.to_screen(stage));
        assert!((back.x - stage.x).abs() < 1e-4);
        assert!((back.y - stage.y).abs() < 1e-4);
    }

    #[test]
    fn camera_center_maps_to_panel_center() {
        let view = view();
        assert_eq!(view.to_screen(view.center), view.rect.center());
    }

    #[test]
    fn zoom_scales_stage_offsets() {
        let view = view();
        let screen = view.to_screen(pos2(view.center.x + 10.0, view.center.y - 5.0));
        assert_eq!(
            screen,
            pos2(view.rect.center().x + 20.0, view.rect.center().y - 10.0)
        );
    }

    #[test]
    fn pan_changes_mapping_inversely() {
        let view = view();
        let stage = pos2(300.0, 220.0);
        let first = view.to_screen(stage);
        let panned = View {
            center: view.center + Vec2::new(20.0, 0.0),
            ..view
        };
        assert_eq!(panned.to_stage(first), stage + Vec2::new(20.0, 0.0));
    }

    fn two_object_project() -> (Project, Uuid, ObjectInstance, ObjectInstance) {
        let mut project = Project::new_empty("Selection test");
        let scene = project.edit_scene_mut(None).expect("scene");
        let layer_id = scene.layers[0].id;
        let first = ObjectInstance::rect("A", 100.0, 100.0, 40.0, 20.0, Rgba::rgb(1, 2, 3));
        let second = ObjectInstance::rect("B", 300.0, 250.0, 20.0, 20.0, Rgba::rgb(4, 5, 6));
        scene.layers[0].frames[0].keyframe = true;
        scene.layers[0].frames[0].objects.push(first.clone());
        scene.layers[0].frames[0].objects.push(second.clone());
        (project, layer_id, first, second)
    }

    #[test]
    fn marquee_selects_only_intersecting_objects() {
        let (project, _, first, _) = two_object_project();
        let marquee = Marquee {
            start: pos2(70.0, 70.0),
            current: pos2(140.0, 140.0),
            base: Vec::new(),
        };
        assert_eq!(marquee_hits(&project, None, 0, &marquee), vec![first.id]);
    }

    #[test]
    fn shared_drag_uses_captured_start_transforms() {
        let (mut project, layer_id, first, second) = two_object_project();
        let state = AppState::new(&project);
        let drag = ObjectDragStart {
            frame: 0,
            objects: vec![
                (first.id, layer_id, first.transform),
                (second.id, layer_id, second.transform),
            ],
        };
        move_objects(&mut project, &state, &drag, Vec2::new(25.0, -10.0));
        move_objects(&mut project, &state, &drag, Vec2::new(5.0, 3.0));
        let scene = project.edit_scene(state.edit_symbol).expect("scene");
        let objects = &scene.layers[0].frames[0].objects;
        assert_eq!(objects[0].transform.x, 105.0);
        assert_eq!(objects[0].transform.y, 103.0);
        assert_eq!(objects[1].transform.x, 305.0);
        assert_eq!(objects[1].transform.y, 253.0);
    }
}
