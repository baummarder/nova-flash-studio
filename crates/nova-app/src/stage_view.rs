//! Animation mode: the stage, camera, selection and direct manipulation.
//!
//! The stage draws exactly what the SWF exporter bakes: keyframes resolved by
//! `nova_core::resolve_scene`, with the same centre-based transform origin.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use nova_core::{ObjectKind, PathCommand, PathShape, Project, ResolvedObject, Symbol};
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
    let available = ui.available_size();
    if available.x < 20.0 || available.y < 20.0 {
        return actions;
    }

    let Some(scene) = project.active_scene() else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Keine Szene vorhanden.").color(theme::RED));
        });
        return actions;
    };

    let stage_size = Vec2::new(scene.width.max(1.0), scene.height.max(1.0));
    let background = scene.background;
    let scene_name = scene.name.clone();
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

    let view = View {
        rect,
        center: state.camera_center,
        zoom: state.zoom,
    };

    // ---------------------------------------------------------------- input
    let shift = ui.input(|input| input.modifiers.shift);
    let hit = if response.clicked() || response.drag_started() {
        response.interact_pointer_pos().and_then(|pointer| {
            let stage = view.to_stage(pointer);
            let scene = project.active_scene()?;
            nova_core::resolve_scene(scene, state.frame)
                .iter()
                .rev()
                .find_map(|object| {
                    let size = util::object_size(&object.instance, &project.symbols)?;
                    util::hit_instance(&object.instance, size, stage).then_some(object.instance.id)
                })
        })
    } else {
        None
    };

    if response.clicked() {
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
                    if response.double_clicked() {
                        if let Some(script) = resolved.instance.script {
                            actions.push(UiAction::OpenScript(script));
                        }
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

    if response.drag_started_by(egui::PointerButton::Primary) {
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

    if response.dragged_by(egui::PointerButton::Primary) {
        if let Some(drag) = state.dragging.clone() {
            // Use the total pointer travel from the press position and the
            // transforms captured at drag start. This avoids accumulating
            // rounding drift over a long drag.
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
            let ids = marquee_hits(project, state.frame, &marquee);
            state.selection = if ids.is_empty() {
                Selection::None
            } else {
                Selection::Objects(ids)
            };
            state.marquee = Some(marquee);
        }
    } else if response.dragged_by(egui::PointerButton::Middle) {
        state.camera_center -= response.drag_delta() / state.zoom;
    }

    if response.drag_stopped() {
        state.dragging = None;
        state.marquee = None;
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
        draw_onion(&painter, &view, project, state.frame);
    }

    let scene = project.active_scene().expect("scene checked above");
    let objects = nova_core::resolve_scene(scene, state.frame);
    for object in &objects {
        draw_object(&painter, &view, object, &project.symbols, &state.selection);
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
    let Some(scene) = project.active_scene_mut() else {
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

fn marquee_hits(project: &Project, frame: usize, marquee: &crate::state::Marquee) -> Vec<Uuid> {
    let rect = Rect::from_two_pos(marquee.start, marquee.current);
    let mut ids = marquee.base.clone();
    let Some(scene) = project.active_scene() else {
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

fn draw_onion(painter: &egui::Painter, view: &View, project: &Project, frame: usize) {
    let Some(scene) = project.active_scene() else {
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
    symbols: &[Symbol],
    selection: &Selection,
) {
    let Some(size) = util::object_size(&object.instance, symbols) else {
        return;
    };
    match &object.instance.kind {
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
        let scene = project.active_scene_mut().expect("scene");
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
        assert_eq!(marquee_hits(&project, 0, &marquee), vec![first.id]);
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
        let scene = project.active_scene().expect("scene");
        let objects = &scene.layers[0].frames[0].objects;
        assert_eq!(objects[0].transform.x, 105.0);
        assert_eq!(objects[0].transform.y, 103.0);
        assert_eq!(objects[1].transform.x, 305.0);
        assert_eq!(objects[1].transform.y, 253.0);
    }
}
