use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Rgba, TextObject, Transform};

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
    /// Instance of a library graphic symbol.
    Symbol {
        symbol: SymbolId,
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
            Self::Text(_) => "Text",
        }
    }

    pub fn size(&self) -> Option<(f32, f32)> {
        match self {
            Self::Rect { width, height, .. }
            | Self::Ellipse { width, height }
            | Self::Path { width, height, .. } => Some((*width, *height)),
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

/// A reusable graphic in the library.
///
/// Milestone 1 models a symbol as a single filled shape. Timeline symbols
/// (nested animation) are a later milestone; the docs and UI say so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub fill: Rgba,
    #[serde(default)]
    pub corner_radius: f32,
}

impl Symbol {
    pub fn new(name: impl Into<String>, width: f32, height: f32, fill: Rgba) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            width,
            height,
            fill,
            corner_radius: 0.0,
        }
    }
}
