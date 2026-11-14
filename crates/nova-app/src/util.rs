//! Shared geometry and keyframe helpers.
//!
//! The stage, the properties panel and the timeline all need the same answers
//! to a few questions: how big is an object, which keyframe governs the current
//! frame, and where is the object's geometry on the stage. Keeping those rules
//! in one place avoids the panel and the renderer disagreeing with the SWF
//! exporter.

use egui::{Pos2, Vec2};
use nova_core::{ObjectInstance, ObjectKind, Rgba, Scene, Symbol, Transform};
use uuid::Uuid;

pub fn to_color32(color: Rgba) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

pub fn from_color32(color: egui::Color32) -> Rgba {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Rgba::rgba(r, g, b, a)
}

/// Width and height of an instance, resolving symbol dimensions from the
/// library.
pub fn object_size(instance: &ObjectInstance, symbols: &[Symbol]) -> Option<(f32, f32)> {
    match &instance.kind {
        ObjectKind::Rect { width, height, .. }
        | ObjectKind::Ellipse { width, height }
        | ObjectKind::Path { width, height, .. }
        | ObjectKind::Bitmap { width, height, .. } => Some((*width, *height)),
        ObjectKind::Text(text) => Some(text.metrics()),
        ObjectKind::Symbol { symbol } => symbols
            .iter()
            .find(|candidate| candidate.id == *symbol)
            .map(|symbol| (symbol.width, symbol.height)),
    }
}

/// Maps a point from an object's local, unscaled space to the stage.
pub fn stage_point(instance: &ObjectInstance, local: Vec2) -> Pos2 {
    let x = local.x * instance.transform.scale_x;
    let y = local.y * instance.transform.scale_y;
    let radians = instance.transform.rotation.to_radians();
    let (sin, cos) = radians.sin_cos();
    Pos2::new(instance.transform.x, instance.transform.y)
        + Vec2::new(x * cos - y * sin, x * sin + y * cos)
}

/// Converts a stage point into the object's local, unscaled space. The origin
/// is the object centre, matching the export matrix.
pub fn local_point(instance: &ObjectInstance, stage: Pos2) -> Vec2 {
    let delta = stage - Pos2::new(instance.transform.x, instance.transform.y);
    let radians = (-instance.transform.rotation).to_radians();
    let (sin, cos) = radians.sin_cos();
    let rotated = Vec2::new(delta.x * cos - delta.y * sin, delta.x * sin + delta.y * cos);
    Vec2::new(
        rotated.x / safe_scale(instance.transform.scale_x),
        rotated.y / safe_scale(instance.transform.scale_y),
    )
}

fn safe_scale(value: f32) -> f32 {
    if value.abs() < 1e-4 {
        if value < 0.0 { -1e-4 } else { 1e-4 }
    } else {
        value
    }
}

/// Hit test in stage coordinates. Ellipses use the normalised ellipse equation,
/// everything else is treated as an axis-aligned rectangle before rotation.
pub fn hit_instance(instance: &ObjectInstance, size: (f32, f32), stage: Pos2) -> bool {
    let (width, height) = size;
    let half_width = (width.abs() / 2.0).max(0.5);
    let half_height = (height.abs() / 2.0).max(0.5);
    let point = local_point(instance, stage);
    match instance.kind {
        ObjectKind::Ellipse { .. } => {
            let nx = point.x / half_width;
            let ny = point.y / half_height;
            nx * nx + ny * ny <= 1.0
        }
        _ => point.x.abs() <= half_width && point.y.abs() <= half_height,
    }
}

/// The four corners of an instance's bounds, already rotated and scaled, in
/// stage coordinates.
pub fn corners(instance: &ObjectInstance, size: (f32, f32)) -> [Pos2; 4] {
    let (width, height) = size;
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let radians = instance.transform.rotation.to_radians();
    let (sin, cos) = radians.sin_cos();
    let center = Pos2::new(instance.transform.x, instance.transform.y);
    let map = |x: f32, y: f32| {
        let x = x * instance.transform.scale_x;
        let y = y * instance.transform.scale_y;
        center + Vec2::new(x * cos - y * sin, x * sin + y * cos)
    };
    [
        map(-half_width, -half_height),
        map(half_width, -half_height),
        map(half_width, half_height),
        map(-half_width, half_height),
    ]
}

/// Outline of an ellipse in stage coordinates, used by the stage painter.
pub fn ellipse_outline(instance: &ObjectInstance, size: (f32, f32), segments: usize) -> Vec<Pos2> {
    let (width, height) = size;
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let radians = instance.transform.rotation.to_radians();
    let (sin, cos) = radians.sin_cos();
    let center = Pos2::new(instance.transform.x, instance.transform.y);
    (0..segments)
        .map(|index| {
            let angle = index as f32 / segments as f32 * std::f32::consts::TAU;
            let x = angle.cos() * half_width * instance.transform.scale_x;
            let y = angle.sin() * half_height * instance.transform.scale_y;
            center + Vec2::new(x * cos - y * sin, x * sin + y * cos)
        })
        .collect()
}

/// Applies a shared translate/scale/rotate operation around a group center.
///
/// All steps operate on the same start transform, so the result is independent
/// of the order in which the inspector controls were touched. Rotation happens
/// around the group center, matching the multi-selection handles.
pub fn shared_transform(
    start: Transform,
    center: Pos2,
    translate: Vec2,
    factor: f32,
    angle_degrees: f32,
) -> Transform {
    let mut transform = start;
    transform.x += translate.x;
    transform.y += translate.y;
    if (factor - 1.0).abs() > f32::EPSILON {
        transform.x = center.x + (transform.x - center.x) * factor;
        transform.y = center.y + (transform.y - center.y) * factor;
        transform.scale_x *= factor;
        transform.scale_y *= factor;
    }
    if angle_degrees.abs() > f32::EPSILON {
        let radians = angle_degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        let offset_x = transform.x - center.x;
        let offset_y = transform.y - center.y;
        transform.x = center.x + offset_x * cos - offset_y * sin;
        transform.y = center.y + offset_x * sin + offset_y * cos;
        transform.rotation += angle_degrees;
    }
    transform
}

/// Returns the mutable instance in the keyframe that governs `frame`.
///
/// Important: the same object id may appear in several keyframes (tween start
/// and end). Editing the *resolved* object would only change a temporary copy,
/// so property edits must always go through this function.
pub fn governing_object_mut<'a>(
    scene: &'a mut Scene,
    layer_id: Uuid,
    frame: usize,
    object_id: Uuid,
) -> Option<&'a mut ObjectInstance> {
    let layer = scene.layers.iter_mut().find(|layer| layer.id == layer_id)?;
    let keyframe = layer.last_keyframe_at_or_before(frame)?;
    layer
        .frames
        .get_mut(keyframe)?
        .objects
        .iter_mut()
        .find(|object| object.id == object_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_core::{Frame, Layer};

    fn red() -> Rgba {
        Rgba::rgb(220, 60, 60)
    }

    #[test]
    fn hit_testing_respects_rotation() {
        let mut instance = ObjectInstance::rect("Balken", 100.0, 50.0, 80.0, 20.0, red());
        assert!(hit_instance(
            &instance,
            (80.0, 20.0),
            Pos2::new(100.0, 50.0)
        ));
        assert!(hit_instance(
            &instance,
            (80.0, 20.0),
            Pos2::new(135.0, 50.0)
        ));
        assert!(!hit_instance(
            &instance,
            (80.0, 20.0),
            Pos2::new(135.0, 65.0)
        ));

        instance.transform.rotation = 90.0;
        // The long axis is now vertical.
        assert!(hit_instance(
            &instance,
            (80.0, 20.0),
            Pos2::new(100.0, 85.0)
        ));
        assert!(!hit_instance(
            &instance,
            (80.0, 20.0),
            Pos2::new(135.0, 50.0)
        ));
    }

    #[test]
    fn hit_testing_uses_ellipse_equation() {
        let instance = ObjectInstance::ellipse("Kreis", 0.0, 0.0, 100.0, 50.0, red());
        assert!(hit_instance(
            &instance,
            (100.0, 50.0),
            Pos2::new(35.0, 15.0)
        ));
        // Inside the bounding box corner, but outside the ellipse.
        assert!(!hit_instance(
            &instance,
            (100.0, 50.0),
            Pos2::new(45.0, 22.0)
        ));
        assert!(!hit_instance(
            &instance,
            (100.0, 50.0),
            Pos2::new(40.0, 30.0)
        ));
    }

    #[test]
    fn governing_keyframe_receives_property_edits() {
        let mut scene = Scene::new("Test", 100.0, 100.0);
        scene.ensure_frame_count(10);
        let layer_id = scene.layers[0].id;

        let start = ObjectInstance::rect("Kiste", 10.0, 10.0, 10.0, 10.0, red());
        let object_id = start.id;
        let mut end = start.clone();
        end.transform.x = 90.0;

        scene.layers[0].frames[0].objects.push(start);
        scene.layers[0].frames[5] = Frame {
            keyframe: true,
            objects: vec![end],
            ..Default::default()
        };

        // Frame 7 is held by keyframe 5.
        governing_object_mut(&mut scene, layer_id, 7, object_id)
            .expect("object at keyframe 5")
            .transform
            .x = 42.0;
        assert_eq!(scene.layers[0].frames[5].objects[0].transform.x, 42.0);
        assert_eq!(scene.layers[0].frames[0].objects[0].transform.x, 10.0);

        // Frame 2 is still governed by keyframe 0.
        governing_object_mut(&mut scene, layer_id, 2, object_id)
            .expect("object at keyframe 0")
            .transform
            .x = 12.0;
        assert_eq!(scene.layers[0].frames[0].objects[0].transform.x, 12.0);
        assert_eq!(scene.layers[0].frames[5].objects[0].transform.x, 42.0);

        assert!(governing_object_mut(&mut scene, layer_id, 7, Uuid::new_v4()).is_none());
        assert!(governing_object_mut(&mut scene, Uuid::new_v4(), 7, object_id).is_none());
    }

    #[test]
    fn layer_without_keyframes_has_no_governing_object() {
        let mut scene = Scene::new("Test", 100.0, 100.0);
        let layer_id = scene.layers[0].id;
        let object = ObjectInstance::rect("Kiste", 0.0, 0.0, 10.0, 10.0, red());
        scene.layers[0] = Layer {
            id: layer_id,
            name: "Leer".into(),
            visible: true,
            locked: false,
            kind: Default::default(),
            parent: None,
            expanded: true,
            frames: vec![Frame::default()],
        };
        assert!(governing_object_mut(&mut scene, layer_id, 0, object.id).is_none());
    }

    #[test]
    fn shared_transform_moves_scales_and_rotates_around_group_center() {
        let start = Transform {
            x: 20.0,
            y: 10.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        };
        let center = Pos2::new(10.0, 10.0);

        let translated = shared_transform(start, center, Vec2::new(5.0, -3.0), 1.0, 0.0);
        assert!((translated.x - 25.0).abs() < 1e-5);
        assert!((translated.y - 7.0).abs() < 1e-5);

        let scaled = shared_transform(start, center, Vec2::ZERO, 2.0, 0.0);
        assert!((scaled.x - 30.0).abs() < 1e-5);
        assert!((scaled.scale_x - 2.0).abs() < 1e-5);

        // 180 degrees around (10,10) maps (20,10) to (0,10).
        let rotated = shared_transform(start, center, Vec2::ZERO, 1.0, 180.0);
        assert!((rotated.x - 0.0).abs() < 1e-4);
        assert!((rotated.y - 10.0).abs() < 1e-4);
        assert!((rotated.rotation - 180.0).abs() < 1e-4);
    }
}
