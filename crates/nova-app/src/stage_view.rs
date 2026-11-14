//! Animation mode: the stage, camera, selection and direct manipulation.
//!
//! The stage draws exactly what the SWF exporter bakes: keyframes resolved by
//! `nova_core::resolve_scene`, with the same centre-based transform origin.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use nova_core::{
    ObjectInstance, ObjectKind, PathCommand, PathShape, Project, ResolvedObject, Rgba, Symbol,
};
use uuid::Uuid;

use crate::state::{AppState, Selection, UiAction};
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
            select_input(ui, project, state, &view, &response, shift, &mut actions);
        }
        crate::state::Tool::PathEdit => {
            path_edit_input(project, state, &view, &response);
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
    let mut objects = nova_core::resolve_scene(scene, state.frame);

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
            let (w, h, circle) = match body.collision {
                Some(spec) => (spec.width, spec.height, spec.shape == nova_core::CollisionShape::Circle),
                None => (body.width, body.height, false),
            };
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let center = view.to_screen(Pos2::new(body_state.x, body_state.y));
            let half = Vec2::new(w / 2.0, h / 2.0) * view.zoom;
            if circle {
                painter.circle_stroke(
                    center,
                    half.x.max(half.y),
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(80, 220, 120, 220)),
                );
            } else {
                let rect = Rect::from_center_size(center, half * 2.0);
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(80, 220, 120, 220)),
                    StrokeKind::Inside,
                );
            }
        }
    }

    if let Some(draft) = &state.draft {
        draw_draft(&painter, &view, draft, state.new_object_color);
    }
    if state.tool == crate::state::Tool::Select {
        draw_transform_handles(&painter, &view, project, state);
    }
    if state.tool == crate::state::Tool::PathEdit {
        draw_node_handles(&painter, &view, project, state);
    }

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
            let commands = stroke_polygon(&draft.points, width);
            if commands.len() < 3 {
                return;
            }
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
