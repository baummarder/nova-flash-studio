use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{LayerKind, ObjectInstance, ObjectKind, Rgba, Scene, Transform};

/// One frame slot of a layer.
///
/// Only keyframes carry objects. Held frames are resolved from the closest
/// preceding keyframe. An optional tween interpolates towards the next keyframe
/// that contains an object with the same id. Sound layers use `sound` instead
/// of visual objects.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<SoundCue>,
}

/// A sound event attached to a sound-layer keyframe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundCue {
    pub asset: Uuid,
    /// Offset into the source audio, in seconds.
    #[serde(default)]
    pub offset: f32,
    /// Playback length in seconds. Zero means "until the asset ends".
    #[serde(default)]
    pub duration: f32,
    /// Event sounds are triggered once. Timeline sounds are intended to follow
    /// the playhead; streaming export is implemented separately where reliable.
    #[serde(default)]
    pub event: bool,
    #[serde(default = "default_loop_count")]
    pub loop_count: u16,
}

fn default_loop_count() -> u16 {
    1
}

impl SoundCue {
    pub fn event(asset: Uuid) -> Self {
        Self {
            asset,
            offset: 0.0,
            duration: 0.0,
            event: true,
            loop_count: 1,
        }
    }

    pub fn timeline(asset: Uuid) -> Self {
        Self {
            event: false,
            ..Self::event(asset)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TweenKind {
    /// Transform/color interpolation between keyframes.
    #[default]
    Motion,
    /// Geometry interpolation between compatible shapes. The SWF exporter bakes
    /// the interpolated frames as real shapes.
    Shape,
}

impl TweenKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Motion => "Motion",
            Self::Shape => "Form",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tween {
    #[serde(default)]
    pub easing: Easing,
    #[serde(default)]
    pub kind: TweenKind,
}

impl Default for Tween {
    fn default() -> Self {
        Self {
            easing: Easing::EaseInOut,
            kind: TweenKind::Motion,
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
    pub layer_kind: LayerKind,
    pub instance: ObjectInstance,
}

impl ResolvedObject {
    pub fn size_hint(&self, symbols: &[super::Symbol]) -> Option<(f32, f32)> {
        match &self.instance.kind {
            super::ObjectKind::Rect { width, height, .. }
            | super::ObjectKind::Ellipse { width, height }
            | super::ObjectKind::Path { width, height, .. }
            | super::ObjectKind::Bitmap { width, height, .. } => Some((*width, *height)),
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
/// Folder, guide and sound layers never contribute visual objects. Mask layers
/// are included so the editor can show their outline and the exporter can emit
/// a clip depth; clipping itself is applied by the consumer.
pub fn resolve_scene(scene: &Scene, frame: usize) -> Vec<ResolvedObject> {
    let mut out = Vec::new();
    for (layer_index, layer) in scene.layers.iter().enumerate() {
        if !scene.layer_effectively_visible(layer)
            || matches!(
                layer.kind,
                LayerKind::Folder | LayerKind::Guide | LayerKind::Sound
            )
        {
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
            if let (Some(t), Some(next_frame)) = (eased, next_frame)
                && let Some(target) = next_frame.objects.iter().find(|o| o.id == object.id)
            {
                resolved.transform = lerp_transform(object.transform, target.transform, t);
                resolved.fill = lerp_color(object.fill, target.fill, t);
                if key
                    .tween
                    .as_ref()
                    .is_some_and(|tween| tween.kind == TweenKind::Shape)
                {
                    resolved.kind = lerp_shape_kind(&object.kind, &target.kind, t)
                        .unwrap_or_else(|| object.kind.clone());
                }
            }
            out.push(ResolvedObject {
                layer_index,
                layer_id: layer.id,
                layer_kind: layer.kind,
                instance: resolved,
            });
        }
    }
    out
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Whether two object kinds can be interpolated by a shape tween.
pub fn shapes_compatible(a: &ObjectKind, b: &ObjectKind) -> bool {
    match (a, b) {
        (ObjectKind::Rect { .. }, ObjectKind::Rect { .. })
        | (ObjectKind::Ellipse { .. }, ObjectKind::Ellipse { .. }) => true,
        (ObjectKind::Path { paths: a, .. }, ObjectKind::Path { paths: b, .. }) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|(left, right)| {
                    left.commands.len() == right.commands.len()
                        && left
                            .commands
                            .iter()
                            .zip(&right.commands)
                            .all(|(left, right)| {
                                std::mem::discriminant(left) == std::mem::discriminant(right)
                            })
                })
        }
        _ => false,
    }
}

fn lerp_shape_kind(a: &ObjectKind, b: &ObjectKind, t: f32) -> Option<ObjectKind> {
    if !shapes_compatible(a, b) {
        return None;
    }
    match (a, b) {
        (
            ObjectKind::Rect {
                width: aw,
                height: ah,
                corner_radius: ar,
            },
            ObjectKind::Rect {
                width: bw,
                height: bh,
                corner_radius: br,
            },
        ) => Some(ObjectKind::Rect {
            width: lerp_f32(*aw, *bw, t),
            height: lerp_f32(*ah, *bh, t),
            corner_radius: lerp_f32(*ar, *br, t),
        }),
        (
            ObjectKind::Ellipse {
                width: aw,
                height: ah,
            },
            ObjectKind::Ellipse {
                width: bw,
                height: bh,
            },
        ) => Some(ObjectKind::Ellipse {
            width: lerp_f32(*aw, *bw, t),
            height: lerp_f32(*ah, *bh, t),
        }),
        (
            ObjectKind::Path {
                width: aw,
                height: ah,
                paths: ap,
            },
            ObjectKind::Path {
                width: bw,
                height: bh,
                paths: bp,
            },
        ) => {
            let mut paths = Vec::with_capacity(ap.len());
            for (left, right) in ap.iter().zip(bp) {
                let commands = left
                    .commands
                    .iter()
                    .zip(&right.commands)
                    .map(|(left, right)| match (*left, *right) {
                        (
                            super::PathCommand::MoveTo { x: ax, y: ay },
                            super::PathCommand::MoveTo { x: bx, y: by },
                        ) => super::PathCommand::MoveTo {
                            x: lerp_f32(ax, bx, t),
                            y: lerp_f32(ay, by, t),
                        },
                        (
                            super::PathCommand::LineTo { x: ax, y: ay },
                            super::PathCommand::LineTo { x: bx, y: by },
                        ) => super::PathCommand::LineTo {
                            x: lerp_f32(ax, bx, t),
                            y: lerp_f32(ay, by, t),
                        },
                        (
                            super::PathCommand::CurveTo {
                                control_x: acx,
                                control_y: acy,
                                x: ax,
                                y: ay,
                            },
                            super::PathCommand::CurveTo {
                                control_x: bcx,
                                control_y: bcy,
                                x: bx,
                                y: by,
                            },
                        ) => super::PathCommand::CurveTo {
                            control_x: lerp_f32(acx, bcx, t),
                            control_y: lerp_f32(acy, bcy, t),
                            x: lerp_f32(ax, bx, t),
                            y: lerp_f32(ay, by, t),
                        },
                        _ => *left,
                    })
                    .collect();
                paths.push(super::PathShape {
                    fill: lerp_color(left.fill, right.fill, t),
                    commands,
                });
            }
            Some(ObjectKind::Path {
                width: lerp_f32(*aw, *bw, t),
                height: lerp_f32(*ah, *bh, t),
                paths,
            })
        }
        _ => None,
    }
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
            ..Tween::default()
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
    fn shape_tween_interpolates_rect_geometry() {
        let mut scene = Scene::new("Morph", 100.0, 100.0);
        let mut layer = Layer::new("Form", 10);
        let object = ObjectInstance::rect("Block", 0.0, 0.0, 10.0, 10.0, Rgba::WHITE);
        layer.frames[0].keyframe = true;
        layer.frames[0].objects.push(object.clone());
        layer.frames[0].tween = Some(Tween {
            easing: Easing::Linear,
            kind: TweenKind::Shape,
        });
        layer.frames[9].keyframe = true;
        layer.frames[9].objects.push(ObjectInstance {
            kind: ObjectKind::Rect {
                width: 90.0,
                height: 50.0,
                corner_radius: 4.0,
            },
            ..object
        });
        scene.layers = vec![layer];
        let resolved = resolve_scene(&scene, 5);
        assert_eq!(resolved.len(), 1);
        match resolved[0].instance.kind {
            ObjectKind::Rect {
                width,
                height,
                corner_radius,
            } => {
                assert!((width - 54.444).abs() < 0.02);
                assert!((height - 32.222).abs() < 0.02);
                assert!(corner_radius > 0.0);
            }
            _ => panic!("expected interpolated rect"),
        }
    }

    #[test]
    fn incompatible_shape_tween_is_reported_by_scene_validation() {
        let mut scene = Scene::new("Morph", 100.0, 100.0);
        let mut layer = Layer::new("Form", 10);
        let object = ObjectInstance::rect("Block", 0.0, 0.0, 10.0, 10.0, Rgba::WHITE);
        layer.frames[0].keyframe = true;
        layer.frames[0].objects.push(object.clone());
        layer.frames[0].tween = Some(Tween {
            kind: TweenKind::Shape,
            ..Tween::default()
        });
        layer.frames[9].keyframe = true;
        layer.frames[9].objects.push(ObjectInstance {
            kind: ObjectKind::Ellipse {
                width: 10.0,
                height: 10.0,
            },
            ..object.clone()
        });
        scene.layers = vec![layer];
        assert!(scene.validate().is_err());
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
