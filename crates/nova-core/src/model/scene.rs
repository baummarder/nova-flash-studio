use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Frame, Rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    #[default]
    Normal,
    Folder,
    Guide,
    Mask,
}

impl LayerKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Folder => "Ordner",
            Self::Guide => "Hilfsebene",
            Self::Mask => "Maske",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Uuid,
    pub name: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub kind: LayerKind,
    /// Contiguous frames, starting at frame 0. Index == frame number.
    pub frames: Vec<Frame>,
}

fn default_true() -> bool {
    true
}

impl Layer {
    pub fn new(name: impl Into<String>, frame_count: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            visible: true,
            locked: false,
            kind: LayerKind::Normal,
            frames: (0..frame_count)
                .map(|i| Frame {
                    keyframe: i == 0,
                    ..Default::default()
                })
                .collect(),
        }
    }

    /// Ensures at least `count` frames exist.
    pub fn ensure_frames(&mut self, count: usize) {
        while self.frames.len() < count {
            self.frames.push(Frame::default());
        }
    }

    /// Number of frames this layer contributes to the scene duration.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn last_keyframe_at_or_before(&self, frame: usize) -> Option<usize> {
        (0..=frame.min(self.frames.len().saturating_sub(1)))
            .rev()
            .find(|&i| self.frames[i].keyframe)
    }

    pub fn next_keyframe_after(&self, frame: usize) -> Option<usize> {
        ((frame + 1)..self.frames.len()).find(|&i| self.frames[i].keyframe)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: Uuid,
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub background: Rgba,
    /// Frames per second.
    pub fps: f32,
    pub layers: Vec<Layer>,
}

impl Scene {
    pub fn new(name: impl Into<String>, width: f32, height: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            width,
            height,
            background: Rgba::rgb(18, 20, 26),
            fps: 24.0,
            layers: vec![Layer::new("Ebene 1", 1)],
        }
    }

    /// Longest layer determines the timeline length.
    pub fn frame_count(&self) -> usize {
        self.layers
            .iter()
            .map(Layer::frame_count)
            .max()
            .unwrap_or(1)
            .max(1)
    }

    pub fn ensure_frame_count(&mut self, count: usize) {
        for layer in &mut self.layers {
            layer.ensure_frames(count);
        }
    }

    pub fn layer(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }

    pub fn layer_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1.0..=8192.0).contains(&self.width) || !(1.0..=8192.0).contains(&self.height) {
            return Err(format!(
                "Szene '{}': Bühnenmaße müssen zwischen 1 und 8192 px liegen",
                self.name
            ));
        }
        if !(1.0..=120.0).contains(&self.fps) {
            return Err(format!(
                "Szene '{}': FPS muss zwischen 1 und 120 liegen",
                self.name
            ));
        }
        if self.layers.is_empty() {
            return Err(format!(
                "Szene '{}': mindestens eine Ebene nötig",
                self.name
            ));
        }
        for layer in &self.layers {
            if layer.frames.is_empty() {
                return Err(format!("Ebene '{}' hat keine Frames", layer.name));
            }
            for frame in &layer.frames {
                for object in &frame.objects {
                    if !object.transform.x.is_finite()
                        || !object.transform.y.is_finite()
                        || !object.transform.scale_x.is_finite()
                        || !object.transform.scale_y.is_finite()
                        || !object.transform.rotation.is_finite()
                    {
                        return Err(format!(
                            "Objekt '{}' hat ungültige Transformwerte",
                            object.name
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
