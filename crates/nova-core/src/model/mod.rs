//! Serializable data model of a `.nova` project.

mod bones;
mod boolop;
mod color;
mod game;
mod object;
mod project;
mod scene;
mod text;
mod tilemap;
mod timeline;
mod transform;

pub use bones::{
    Bone, BoneClip, BoneKey, BonePose, BoneTrack, IkKey, Skeleton, auto_skin_weights,
    bind_transform, normalize_weights, skin_vertex, solve_two_bone,
};
pub use boolop::{BoolOp, boolean_path_objects, boolean_paths};
pub use color::Rgba;
pub use game::{
    BodyType, CameraConfig, CollisionShape, CollisionSpec, GameObjectConfig, GameSceneConfig,
    InputBindings, ParticleEmitter, ParticlePreset, ParticleSpace, PhysicsSpec, SceneTransition,
    WorldConfig,
};
pub use object::{
    BitmapCrop, Gradient, GradientKind, GradientStop, ObjectInstance, ObjectKind, PathCommand,
    PathShape, ScriptId, SkinData, SkinWeight, Symbol, SymbolId, SymbolKind,
};
pub use project::{
    Asset, AssetKind, CURRENT_FORMAT_VERSION, CodeProvenance, ExportConfig, Metadata, NovaFile,
    Project, ProjectId, Script, SourceFormat, SourceInfo, SwfCompression, SymbolClassLink,
    TargetRuntime,
};
pub use scene::{Layer, LayerKind, Scene};
pub use text::{TextAlign, TextFont, TextObject};
pub use tilemap::{TileLayer, TilemapData, Tileset};
pub use timeline::{
    Easing, Frame, ResolvedObject, SoundCue, Tween, TweenKind, resolve_scene, resolve_symbol_frame,
    shapes_compatible,
};
pub use transform::Transform;
