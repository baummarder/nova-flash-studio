//! Serializable data model of a `.nova` project.

mod color;
mod object;
mod project;
mod scene;
mod text;
mod timeline;
mod transform;

pub use color::Rgba;
pub use object::{
    BitmapCrop, ObjectInstance, ObjectKind, PathCommand, PathShape, ScriptId, Symbol, SymbolId,
    SymbolKind,
};
pub use project::{
    Asset, AssetKind, CURRENT_FORMAT_VERSION, CodeProvenance, ExportConfig, Metadata, NovaFile,
    Project, ProjectId, Script, SourceFormat, SourceInfo, SwfCompression, SymbolClassLink,
    TargetRuntime,
};
pub use scene::{Layer, LayerKind, Scene};
pub use text::{TextAlign, TextFont, TextObject};
pub use timeline::{
    Easing, Frame, ResolvedObject, SoundCue, Tween, TweenKind, resolve_scene, shapes_compatible,
};
pub use transform::Transform;
