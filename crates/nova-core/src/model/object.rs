use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Layer, Rgba, Scene, TextObject, Transform};

pub type SymbolId = Uuid;
pub type ScriptId = Uuid;

/// One path command in an imported vector shape.
///
/// Coordinates are relative to the object's centre, in stage pixels. The
/// editor only creates rectangles and ellipses itself; imported FLA/SWF
/// geometry is represented with these commands so it can be displayed and
/// exported again without silently replacing it with a bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathCommand {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    CurveTo {
        control_x: f32,
        control_y: f32,
        x: f32,
        y: f32,
    },
}

/// A filled sub-path of an imported vector shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathShape {
    pub fill: Rgba,
    pub commands: Vec<PathCommand>,
}

/// Normalised crop rectangle for a bitmap instance.
///
/// Values are fractions of the source image (`0.0..=1.0`). Keeping the crop in
/// normalised space means it survives image replacement with different pixel
/// dimensions, while the exporter converts it back to pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BitmapCrop {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BitmapCrop {
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };

    pub fn is_full(self) -> bool {
        self.x.abs() < 1e-5
            && self.y.abs() < 1e-5
            && (self.width - 1.0).abs() < 1e-5
            && (self.height - 1.0).abs() < 1e-5
    }

    pub fn normalized(self) -> Self {
        let x = self.x.clamp(0.0, 1.0);
        let y = self.y.clamp(0.0, 1.0);
        let width = self.width.clamp(0.0, 1.0 - x);
        let height = self.height.clamp(0.0, 1.0 - y);
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

impl PathShape {
    pub fn new(fill: Rgba, commands: Vec<PathCommand>) -> Self {
        Self { fill, commands }
    }
}

/// The primitive shapes the stage and the SWF exporter understand.
///
/// Text is baked into ordinary vector paths by the exporter; it is not
/// `DefineText` and does not provide runtime text editing. Bitmaps and audio
/// remain intentionally absent. `Path` exists for imported geometry and
/// preserves multiple filled sub-paths where the source format provides them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectKind {
    Rect {
        width: f32,
        height: f32,
        #[serde(default)]
        corner_radius: f32,
    },
    Ellipse {
        width: f32,
        height: f32,
    },
    /// Imported vector geometry. `width`/`height` are the local bounds used by
    /// selection, hit-testing and the properties panel.
    Path {
        width: f32,
        height: f32,
        #[serde(default)]
        paths: Vec<PathShape>,
    },
    /// Instance of a library symbol.
    Symbol {
        symbol: SymbolId,
    },
    /// Embedded bitmap asset. The asset bytes live in the project asset table,
    /// so a `.nova` file is self-contained once an image has been imported.
    Bitmap {
        asset: Uuid,
        width: f32,
        height: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crop: Option<BitmapCrop>,
    },
    /// Editable text baked to vector paths on export.
    Text(TextObject),
}

impl ObjectKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Rect { .. } => "Rechteck",
            Self::Ellipse { .. } => "Ellipse",
            Self::Path { .. } => "Importierte Vektorgrafik",
            Self::Symbol { .. } => "Symbol",
            Self::Bitmap { .. } => "Bitmap",
            Self::Text(_) => "Text",
        }
    }

    pub fn size(&self) -> Option<(f32, f32)> {
        match self {
            Self::Rect { width, height, .. }
            | Self::Ellipse { width, height }
            | Self::Path { width, height, .. }
            | Self::Bitmap { width, height, .. } => Some((*width, *height)),
            Self::Text(text) => Some(text.metrics()),
            Self::Symbol { .. } => None,
        }
    }
}

/// A single object placed on the stage / timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectInstance {
    pub id: Uuid,
    pub name: String,
    pub kind: ObjectKind,
    pub transform: Transform,
    pub fill: Rgba,
    /// Optional link to a script. This is what connects the animation mode to
    /// the code mode: the script is a first-class project file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<ScriptId>,
    /// Original SWF character id this object was reconstructed from. It is used
    /// to rebuild `SymbolClass` links without pretending that the original
    /// character table survived unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_character: Option<u16>,
}

impl ObjectInstance {
    pub fn new(
        name: impl Into<String>,
        kind: ObjectKind,
        transform: Transform,
        fill: Rgba,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            transform,
            fill,
            script: None,
            source_character: None,
        }
    }

    pub fn rect(name: impl Into<String>, x: f32, y: f32, w: f32, h: f32, fill: Rgba) -> Self {
        Self::new(
            name,
            ObjectKind::Rect {
                width: w,
                height: h,
                corner_radius: 0.0,
            },
            Transform::at(x, y),
            fill,
        )
    }

    pub fn ellipse(name: impl Into<String>, x: f32, y: f32, w: f32, h: f32, fill: Rgba) -> Self {
        Self::new(
            name,
            ObjectKind::Ellipse {
                width: w,
                height: h,
            },
            Transform::at(x, y),
            fill,
        )
    }

    pub fn symbol(name: impl Into<String>, x: f32, y: f32, symbol: SymbolId, fill: Rgba) -> Self {
        Self::new(
            name,
            ObjectKind::Symbol { symbol },
            Transform::at(x, y),
            fill,
        )
    }

    pub fn bitmap(
        name: impl Into<String>,
        x: f32,
        y: f32,
        asset: Uuid,
        width: f32,
        height: f32,
    ) -> Self {
        Self::new(
            name,
            ObjectKind::Bitmap {
                asset,
                width,
                height,
                crop: None,
            },
            Transform::at(x, y),
            Rgba::WHITE,
        )
    }

    pub fn path(
        name: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        paths: Vec<PathShape>,
        fill: Rgba,
    ) -> Self {
        Self::new(
            name,
            ObjectKind::Path {
                width,
                height,
                paths,
            },
            Transform::at(x, y),
            fill,
        )
    }

    pub fn text(
        name: impl Into<String>,
        x: f32,
        y: f32,
        text: impl Into<String>,
        size: f32,
        fill: Rgba,
    ) -> Self {
        Self::new(
            name,
            ObjectKind::Text(TextObject::new(text, size)),
            Transform::at(x, y),
            fill,
        )
    }
}

/// Behaviour class of a library symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    #[default]
    Graphic,
    MovieClip,
    Button,
}

impl SymbolKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Graphic => "Grafik",
            Self::MovieClip => "MovieClip",
            Self::Button => "Button",
        }
    }
}

fn default_symbol_timeline() -> Scene {
    Scene::new("Symbol", 64.0, 64.0)
}

/// A reusable library symbol with its own real timeline.
///
/// The legacy fields (`width`, `height`, `fill`, `corner_radius`) are retained
/// for library icons and for projects created before format version 2. The
/// authoritative content is `timeline`: layers, keyframes, tweens, nested
/// instances and frame labels are all edited and exported from there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    #[serde(default)]
    pub kind: SymbolKind,
    pub width: f32,
    pub height: f32,
    pub fill: Rgba,
    #[serde(default)]
    pub corner_radius: f32,
    #[serde(default = "default_symbol_timeline")]
    pub timeline: Scene,
}

impl Symbol {
    /// Creates a Graphic symbol containing one filled rectangle. This keeps the
    /// historical symbol behaviour and is also what the library "+" button
    /// creates unless the user picks MovieClip.
    pub fn new(name: impl Into<String>, width: f32, height: f32, fill: Rgba) -> Self {
        Self::with_kind(name, SymbolKind::Graphic, width, height, fill)
    }

    pub fn with_kind(
        name: impl Into<String>,
        kind: SymbolKind,
        width: f32,
        height: f32,
        fill: Rgba,
    ) -> Self {
        let name = name.into();
        let mut timeline = Scene::new(name.clone(), width, height);
        timeline.background = Rgba::rgba(0, 0, 0, 0);
        timeline.layers.clear();
        let mut layer = Layer::new("Inhalt", 1);
        layer.frames[0].keyframe = true;
        if kind == SymbolKind::Graphic {
            layer.frames[0].objects.push(ObjectInstance::rect(
                name.clone(),
                0.0,
                0.0,
                width,
                height,
                fill,
            ));
        }
        timeline.layers.push(layer);
        Self {
            id: Uuid::new_v4(),
            name,
            kind,
            width,
            height,
            fill,
            corner_radius: 0.0,
            timeline,
        }
    }

    /// Creates an empty MovieClip or Graphic with one keyframe.
    pub fn empty(name: impl Into<String>, kind: SymbolKind, width: f32, height: f32) -> Self {
        Self::with_kind(name, kind, width, height, Rgba::WHITE)
    }

    /// Rebuilds the timeline for a legacy (format version 1) shape symbol.
    pub fn from_legacy_shape(&mut self) {
        let mut shape = ObjectInstance::rect(
            self.name.clone(),
            0.0,
            0.0,
            self.width,
            self.height,
            self.fill,
        );
        if self.corner_radius > 0.0 {
            shape.kind = ObjectKind::Rect {
                width: self.width,
                height: self.height,
                corner_radius: self.corner_radius,
            };
        }
        let background = Rgba::rgba(0, 0, 0, 0);
        self.timeline = Scene::new(self.name.clone(), self.width, self.height);
        self.timeline.background = background;
        self.timeline.layers = vec![Layer::new("Inhalt", 1)];
        self.timeline.layers[0].frames[0].keyframe = true;
        self.timeline.layers[0].frames[0].objects.push(shape);
    }

    /// Makes deserialised and programmatically edited symbols internally
    /// consistent without discarding timeline content.
    pub fn normalize(&mut self) {
        if self.name.trim().is_empty() {
            self.name = "Symbol".into();
        }
        if self.timeline.layers.is_empty() {
            let make_shape = self.kind == SymbolKind::Graphic;
            self.timeline.background = Rgba::rgba(0, 0, 0, 0);
            self.timeline.layers = vec![Layer::new("Inhalt", 1)];
            self.timeline.layers[0].frames[0].keyframe = true;
            if make_shape {
                self.timeline.layers[0].frames[0]
                    .objects
                    .push(ObjectInstance::rect(
                        self.name.clone(),
                        0.0,
                        0.0,
                        self.width.max(1.0),
                        self.height.max(1.0),
                        self.fill,
                    ));
            }
        }
        if !(1.0..=8192.0).contains(&self.width) {
            self.width = self.timeline.width.clamp(1.0, 8192.0);
        }
        if !(1.0..=8192.0).contains(&self.height) {
            self.height = self.timeline.height.clamp(1.0, 8192.0);
        }
        self.timeline.name = self.name.clone();
        self.timeline.width = self.width;
        self.timeline.height = self.height;
        if !(1.0..=120.0).contains(&self.timeline.fps) {
            self.timeline.fps = 24.0;
        }
        // Keep every layer at least one frame long so a newly created symbol can
        // be opened and edited immediately.
        for layer in &mut self.timeline.layers {
            if layer.frames.is_empty() {
                layer.ensure_frames(1);
            }
        }
    }

    pub fn frame_count(&self) -> usize {
        self.timeline.frame_count()
    }

    pub fn ensure_frame_count(&mut self, count: usize) {
        self.timeline.ensure_frame_count(count);
    }

    /// Direct and transitive dependencies, excluding this symbol itself.
    pub fn dependency_ids(&self) -> std::collections::HashSet<SymbolId> {
        let mut out = std::collections::HashSet::new();
        for layer in &self.timeline.layers {
            for frame in &layer.frames {
                for object in &frame.objects {
                    if let ObjectKind::Symbol { symbol } = object.kind
                        && symbol != self.id
                    {
                        out.insert(symbol);
                    }
                }
            }
        }
        out
    }
}
