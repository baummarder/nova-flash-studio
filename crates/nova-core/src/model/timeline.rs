use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ObjectInstance, Rgba, Scene, Transform};

/// One frame slot of a layer.
///
/// Only keyframes carry objects. Held frames are resolved from the closest
/// preceding keyframe. An optional motion tween interpolates towards the next
/// keyframe that contains an object with the same id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Frame {
    #[serde(default)]
    pub keyframe: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tween: Option<Tween>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tween {
    #[serde(default)]
    pub easing: Easing,
}

impl Default for Tween {
    fn default() -> Self {
        Self {
            easing: Easing::EaseInOut,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    #[default]
    EaseInOut,
}

impl Easing {
    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::EaseIn => "Ease In",
            Self::EaseOut => "Ease Out",
            Self::EaseInOut => "Ease In/Out",
        }
    }

    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

/// An object after keyframes and tweens have been evaluated for one frame.
#[derive(Debug, Clone)]
pub struct ResolvedObject {
    pub layer_index: usize,
    pub layer_id: Uuid,
    pub instance: ObjectInstance,
}

impl ResolvedObject {
    pub fn size_hint(&self, symbols: &[super::Symbol]) -> Option<(f32, f32)> {
        match &self.instance.kind {
            super::ObjectKind::Rect { width, height, .. }
            | super::ObjectKind::Ellipse { width, height }
            | super::ObjectKind::Path { width, height, .. } => Some((*width, *height)),
            super::ObjectKind::Text(text) => Some(text.metrics()),
            super::ObjectKind::Symbol { symbol } => symbols
                .iter()
                .find(|s| s.id == *symbol)
                .map(|s| (s.width, s.height)),
        }
    }
}

/// Evaluates all visible layers of a scene at `frame`.
///
/// Layers are returned bottom-to-top, which is also the SWF depth order.
pub fn resolve_scene(scene: &Scene, frame: usize) -> Vec<ResolvedObject> {
    let mut out = Vec::new();
    for (layer_index, layer) in scene.layers.iter().enumerate() {
        if !scene.layer_effectively_visible(layer) || layer.kind == super::LayerKind::Folder {
            continue;
        }
        let Some(key_index) = layer.last_keyframe_at_or_before(frame) else {
            continue;
        };
        let key = &layer.frames[key_index];
        let next_index = layer.next_keyframe_after(key_index);
        let t = match (key.tween.as_ref(), next_index) {
            (Some(_), Some(next)) => {
                let span = (next - key_index).max(1) as f32;
                Some(((frame - key_index) as f32 / span).clamp(0.0, 1.0))
            }
            _ => None,
        };
        let eased = t.map(|t| key.tween.as_ref().map_or(t, |tw| tw.easing.apply(t)));
        let next_frame = next_index.and_then(|i| layer.frames.get(i));

        for object in &key.objects {
            let mut resolved = object.clone();
            if let (Some(t), Some(next_frame)) = (eased, next_frame) {
                if let Some(target) = next_frame.objects.iter().find(|o| o.id == object.id) {
                    resolved.transform = lerp_transform(object.transform, target.transform, t);
                    resolved.fill = lerp_color(object.fill, target.fill, t);
                }
            }
            out.push(ResolvedObject {
                layer_index,
                layer_id: layer.id,
                instance: resolved,
            });
        }
    }
    out
}

fn lerp_transform(a: Transform, b: Transform, t: f32) -> Transform {
    let mut out = a.lerp(b, t);
    if (out.scale_x - 0.0).abs() < 1e-4 {
        out.scale_x = 1e-4;
    }
    if (out.scale_y - 0.0).abs() < 1e-4 {
        out.scale_y = 1e-4;
    }
    out
}

fn lerp_color(a: Rgba, b: Rgba, t: f32) -> Rgba {
    a.lerp(b, t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, ObjectInstance, Rgba, Scene};

    fn scene_with_tween() -> (Scene, Uuid) {
        let mut scene = Scene::new("Test", 100.0, 100.0);
        let mut layer = Layer::new("Bewegt", 10);
        let obj = ObjectInstance::rect("Block", 0.0, 0.0, 10.0, 10.0, Rgba::WHITE);
        let id = obj.id;
        layer.frames[0].keyframe = true;
        layer.frames[0].objects.push(obj.clone());
        layer.frames[0].tween = Some(Tween {
            easing: Easing::Linear,
        });
        layer.frames[9].keyframe = true;
        layer.frames[9].objects.push(ObjectInstance {
            transform: Transform::at(90.0, 90.0),
            ..obj
        });
        scene.layers = vec![layer];
        (scene, id)
    }

    #[test]
    fn tween_interpolates_position_at_midpoint() {
        let (scene, id) = scene_with_tween();
        let mid = resolve_scene(&scene, 4);
        let obj = mid.iter().find(|o| o.instance.id == id).unwrap();
        assert!((obj.instance.transform.x - 40.0).abs() < 0.01);
        assert!((obj.instance.transform.y - 40.0).abs() < 0.01);
    }

    #[test]
    fn held_frames_repeat_last_keyframe() {
        let (scene, _) = scene_with_tween();
        let held = resolve_scene(&scene, 2);
        assert_eq!(held.len(), 1);
    }

    #[test]
    fn hidden_layers_are_not_resolved() {
        let (mut scene, _) = scene_with_tween();
        scene.layers[0].visible = false;
        assert!(resolve_scene(&scene, 0).is_empty());
    }

    #[test]
    fn easing_endpoints() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert!((easing.apply(0.0) - 0.0).abs() < 1e-6);
            assert!((easing.apply(1.0) - 1.0).abs() < 1e-6);
        }
    }
}
