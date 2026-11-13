//! Serializable data model of a `.nova` project.

mod color;
mod object;
mod project;
mod scene;
mod timeline;
mod transform;

pub use color::Rgba;
pub use object::{ObjectInstance, ObjectKind, PathCommand, PathShape, ScriptId, Symbol, SymbolId};
pub use project::{
    Asset, AssetKind, CURRENT_FORMAT_VERSION, CodeProvenance, ExportConfig, Metadata, NovaFile,
    Project, ProjectId, Script, SourceFormat, SourceInfo, SwfCompression, SymbolClassLink,
    TargetRuntime,
};
pub use scene::{Layer, LayerKind, Scene};
pub use timeline::{Easing, Frame, ResolvedObject, Tween, resolve_scene};
pub use transform::Transform;
