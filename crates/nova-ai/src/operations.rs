//! Safe, validated project operations proposed by the AI agent.
//!
//! File diffs are deliberately separate from project edits. A diff changes a
//! script file on disk; an operation changes the in-memory `.nova` document
//! (scenes, layers, objects, symbols, scripts). Operations only add or update
//! content, never delete it, and are applied transactionally: either the whole
//! batch succeeds or the project is left untouched.
//!
//! The UI previews the batch before applying it. This module is UI-independent
//! and has no network access.

use nova_core::{
    Asset, AssetKind, Easing, LayerKind, ObjectInstance, ObjectKind, Project, Rgba, Script,
    SoundCue, Symbol, SymbolKind, Tween,
};
use serde::{Deserialize, Serialize};

/// Upper bound for one agent-proposed batch. Keeps previews and apply work
/// bounded and prevents accidental project floods.
pub const MAX_OPERATIONS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ObjectShape {
    #[default]
    Rect,
    Ellipse,
    Text,
}

impl ObjectShape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rect => "Rechteck",
            Self::Ellipse => "Ellipse",
            Self::Text => "Text",
        }
    }
}

/// One structured edit to the project model.
///
/// The JSON representation is tagged with `op`, for example:
/// `{"op":"add_layer","name":"Hintergrund"}`. Scene and layer selectors are
/// names; omitted scene selectors use the active scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProjectOperation {
    RenameProject {
        name: String,
    },
    SetStage {
        width: f32,
        height: f32,
        #[serde(default)]
        fps: Option<f32>,
        #[serde(default)]
        background: Option<String>,
        #[serde(default)]
        scene: Option<String>,
    },
    AddScene {
        name: String,
    },
    AddLayer {
        name: String,
        #[serde(default)]
        scene: Option<String>,
        /// When set, the layer is added to this library symbol's timeline
        /// instead of a scene.
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        frames: Option<usize>,
    },
    AddObject {
        name: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        shape: ObjectShape,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        text: String,
        #[serde(default)]
        font_size: Option<f32>,
    },
    UpdateObject {
        name: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(default)]
        rotation: Option<f32>,
        #[serde(default)]
        scale_x: Option<f32>,
        #[serde(default)]
        scale_y: Option<f32>,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        font_size: Option<f32>,
    },
    AddKeyframe {
        layer: String,
        /// 1-based frame number, like the timeline UI.
        frame: usize,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        label: Option<String>,
        /// Copy the objects of the preceding keyframe into the new one. This
        /// is what makes a following `add_tween` useful.
        #[serde(default)]
        duplicate_previous: bool,
    },
    SetFrameLabel {
        layer: String,
        /// 1-based frame number, like the timeline UI.
        frame: usize,
        label: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
    },
    AddTween {
        layer: String,
        /// 1-based frame number of the first keyframe.
        frame: usize,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        easing: Easing,
        #[serde(default)]
        kind: Option<nova_core::TweenKind>,
    },
    AddSymbol {
        name: String,
        width: f32,
        height: f32,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        corner_radius: Option<f32>,
        #[serde(default)]
        kind: Option<SymbolKind>,
    },
    SetSymbolKind {
        symbol: String,
        kind: SymbolKind,
    },
    PlaceSymbol {
        symbol: String,
        name: String,
        x: f32,
        y: f32,
        #[serde(default)]
        scene: Option<String>,
        /// Destination timeline: a library symbol name. When set, the
        /// instance is placed inside that symbol's timeline.
        #[serde(default)]
        symbol_target: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        fill: Option<String>,
    },
    /// Registers a bitmap asset. `path` is resolved by the caller-provided
    /// workspace-confined reader; the operation never touches the filesystem
    /// itself.
    ImportBitmapAsset {
        name: String,
        path: String,
    },
    /// Registers an audio asset (WAV or MP3) from a workspace-relative path.
    ImportAudioAsset {
        name: String,
        path: String,
    },
    /// Places an embedded bitmap asset as an object on a timeline.
    PlaceBitmap {
        asset: String,
        name: String,
        x: f32,
        y: f32,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        layer: Option<String>,
    },
    /// Attaches a sound cue to a frame of a sound layer (created if needed).
    PlaceSoundCue {
        asset: String,
        /// 1-based frame number.
        frame: usize,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        event: bool,
    },
    SetLayerKind {
        layer: String,
        kind: LayerKind,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
    },
    AddScript {
        path: String,
        source: String,
    },
    /// Sets or clears the game components (collision, physics) of a placed
    /// object. `clear` removes all game metadata; otherwise provided specs
    /// replace the matching component and `no_collision`/`no_physics` remove
    /// a single component.
    SetGameObject {
        object: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        collision: Option<nova_core::CollisionSpec>,
        #[serde(default)]
        no_collision: bool,
        #[serde(default)]
        physics: Option<nova_core::PhysicsSpec>,
        #[serde(default)]
        no_physics: bool,
        #[serde(default)]
        clear: bool,
    },
    /// Configures scene-level game settings (camera, input bindings).
    /// `clear` removes the whole game configuration; otherwise provided
    /// sub-configs replace the existing ones.
    SetSceneGame {
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        camera: Option<nova_core::CameraConfig>,
        #[serde(default)]
        input: Option<nova_core::InputBindings>,
        /// Optional world bounds/spawn (level editor data, format v4).
        #[serde(default)]
        world: Option<nova_core::WorldConfig>,
        #[serde(default)]
        clear: bool,
    },
    /// Installs the reusable AS3 game-kit scripts (`game.Input`,
    /// `game.Collision`, `game.Camera`, `game.Random`, `game.Physics`).
    /// Existing scripts are never overwritten.
    InstallGameKit,
    /// Adds, updates or removes a key-triggered scene transition on a
    /// scene's game config. `key` uses the same naming space as the input
    /// bindings ("Space", "Enter", "A", ...); `target` must name an existing
    /// scene. `remove` deletes the transition for `key`.
    SetSceneTransition {
        key: String,
        target: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        remove: bool,
    },
    /// Creates a tileset over an existing bitmap asset (format v4).
    CreateTileset {
        name: String,
        /// Name of an existing embedded bitmap asset.
        asset: String,
        tile_width: u32,
        tile_height: u32,
        /// 0-based tile indices that act as solid collision.
        #[serde(default)]
        solid_tiles: Vec<usize>,
        /// Optional display names per tile index.
        #[serde(default)]
        tile_names: Vec<String>,
        /// Pixel margin around the tileset grid (spritesheet padding).
        #[serde(default)]
        margin_x: u32,
        #[serde(default)]
        margin_y: u32,
        /// Pixel gap between adjacent tiles in the spritesheet.
        #[serde(default)]
        spacing_x: u32,
        #[serde(default)]
        spacing_y: u32,
    },
    /// Places a tilemap object on a scene layer.
    CreateTilemap {
        name: String,
        /// Tileset name from `create_tileset`.
        tileset: String,
        columns: usize,
        rows: usize,
        x: f32,
        y: f32,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        layer: Option<String>,
    },
    /// Resizes/retiles an existing tilemap object.
    ModifyTilemap {
        object: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        columns: Option<usize>,
        #[serde(default)]
        rows: Option<usize>,
        #[serde(default)]
        tile_width: Option<f32>,
        #[serde(default)]
        tile_height: Option<f32>,
        #[serde(default)]
        tileset: Option<String>,
    },
    /// Paints cells on one tile layer of a tilemap object.
    PaintTiles {
        object: String,
        /// Tile layer index (0-based).
        layer: usize,
        tool: nova_core::tilemap::PaintTool,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        /// 1-based tile index (0 = empty).
        #[serde(default)]
        tile: u16,
        /// Replace tool: source tile (1-based, 0 = empty).
        #[serde(default)]
        from: u16,
        /// Replace tool: target tile (1-based, 0 = empty).
        #[serde(default)]
        to: u16,
        #[serde(default)]
        scene: Option<String>,
    },
    /// Adds an empty named tile layer to a tilemap object.
    AddTileLayer {
        object: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        scene: Option<String>,
    },
    /// Configures visibility/lock/opacity/name of one tile layer.
    ConfigureTileLayer {
        object: String,
        /// Tile layer index (0-based).
        layer: usize,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        visible: Option<bool>,
        #[serde(default)]
        locked: Option<bool>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        scene: Option<String>,
    },
    /// Flips a rectangular cell region in place on one tile layer.
    FlipTileRegion {
        object: String,
        layer: usize,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        #[serde(default)]
        flip_h: bool,
        #[serde(default)]
        flip_v: bool,
        #[serde(default)]
        scene: Option<String>,
    },
    /// Moves a rectangular cell region by (dx, dy) on one tile layer.
    MoveTileRegion {
        object: String,
        layer: usize,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        dx: i32,
        dy: i32,
        #[serde(default)]
        scene: Option<String>,
    },
    /// Sets, updates or clears an object's collision shape (rect, circle or
    /// validated polygon) including layer/mask bits.
    CreateCollisionShape {
        object: String,
        #[serde(default)]
        scene: Option<String>,
        shape: nova_core::CollisionShape,
        #[serde(default)]
        width: Option<f32>,
        #[serde(default)]
        height: Option<f32>,
        #[serde(default)]
        layer: Option<u32>,
        #[serde(default)]
        mask: Option<u32>,
        #[serde(default)]
        remove: bool,
    },
    /// Merges camera settings into a scene's game configuration.
    ModifyCamera {
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        enabled: Option<bool>,
        #[serde(default)]
        follow: Option<String>,
        #[serde(default)]
        left: Option<f32>,
        #[serde(default)]
        top: Option<f32>,
        #[serde(default)]
        right: Option<f32>,
        #[serde(default)]
        bottom: Option<f32>,
        #[serde(default)]
        zoom: Option<f32>,
        #[serde(default)]
        smoothing: Option<f32>,
        #[serde(default)]
        dead_zone: Option<(f32, f32)>,
        #[serde(default)]
        shake: Option<f32>,
    },
    /// Creates (or replaces) a particle emitter on an object.
    CreateParticleEmitter {
        object: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        preset: Option<nova_core::ParticlePreset>,
        #[serde(default)]
        rate: Option<f32>,
        #[serde(default)]
        burst: Option<u32>,
        #[serde(default)]
        lifetime: Option<(f32, f32)>,
        #[serde(default)]
        speed: Option<(f32, f32)>,
        #[serde(default)]
        direction: Option<f32>,
        #[serde(default)]
        spread: Option<f32>,
        #[serde(default)]
        gravity: Option<f32>,
        #[serde(default)]
        drag: Option<f32>,
        #[serde(default)]
        max_particles: Option<u32>,
        #[serde(default)]
        space: Option<nova_core::ParticleSpace>,
        #[serde(default)]
        remove: bool,
    },
    /// Modifies an existing particle emitter; fails when none exists.
    ConfigureParticleEmitter {
        object: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        preset: Option<nova_core::ParticlePreset>,
        #[serde(default)]
        rate: Option<f32>,
        #[serde(default)]
        burst: Option<u32>,
        #[serde(default)]
        lifetime: Option<(f32, f32)>,
        #[serde(default)]
        speed: Option<(f32, f32)>,
        #[serde(default)]
        direction: Option<f32>,
        #[serde(default)]
        spread: Option<f32>,
        #[serde(default)]
        gravity: Option<f32>,
        #[serde(default)]
        drag: Option<f32>,
        #[serde(default)]
        max_particles: Option<u32>,
        #[serde(default)]
        space: Option<nova_core::ParticleSpace>,
    },
    /// Adds a folder layer for grouping level objects (layer-level groups;
    /// persistent nested object groups are not part of the format).
    CreateLevelGroup {
        name: String,
        #[serde(default)]
        scene: Option<String>,
    },
    /// Places a rectangle game object with optional collision/physics in one
    /// step (convenience over `add_object` + `set_game_object`).
    PlaceGameObject {
        name: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        collision: Option<nova_core::CollisionSpec>,
        #[serde(default)]
        physics: Option<nova_core::PhysicsSpec>,
    },
    /// Imports a workspace asset, detecting bitmap vs audio by extension.
    ImportProjectAsset {
        name: Option<String>,
        path: String,
    },
    /// Adds a bone to a library symbol's skeleton (created when missing).
    AddBone {
        symbol: String,
        name: String,
        x: f32,
        y: f32,
        length: f32,
        #[serde(default)]
        parent: Option<String>,
        #[serde(default)]
        rotation: Option<f32>,
    },
    /// Sets or clears the parent of a bone inside a symbol skeleton.
    SetBoneParent {
        symbol: String,
        bone: String,
        #[serde(default)]
        parent: Option<String>,
    },
    /// Adds a bone animation clip (created when missing) to a symbol.
    AddBoneClip {
        symbol: String,
        name: String,
        #[serde(default)]
        frames: Option<u32>,
        #[serde(default)]
        looping: Option<bool>,
    },
    /// Adds or replaces an FK rotation key on a bone track.
    AddBoneKey {
        symbol: String,
        clip: String,
        bone: String,
        frame: u32,
        rotation: f32,
    },
    /// Adds or replaces an IK target key on a bone track. Chains of two
    /// bones use the analytic solver; longer chains use FABRIK.
    AddIkKey {
        symbol: String,
        clip: String,
        bone: String,
        frame: u32,
        x: f32,
        y: f32,
        #[serde(default)]
        pole_x: Option<f32>,
        #[serde(default)]
        pole_y: Option<f32>,
    },
    /// Binds an object inside a symbol timeline to a bone.
    BindObjectToBone {
        symbol: String,
        object: String,
        bone: String,
    },
    /// Selects the bone clip baked into exports/preview for a symbol.
    SetSymbolClip {
        symbol: String,
        #[serde(default)]
        clip: Option<String>,
    },
    /// Duplicates a bone clip under a new name (M17 clip management).
    DuplicateBoneClip {
        symbol: String,
        clip: String,
        name: String,
    },
    /// Renames a bone clip; `active_clip` follows the rename.
    RenameBoneClip {
        symbol: String,
        clip: String,
        name: String,
    },
    /// Applies or updates a gradient fill on every path of a Path object.
    /// Omitted fields keep their current values; `stops` replaces the ramp
    /// with `[position, "#rrggbb"]` pairs.
    SetPathGradient {
        name: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        kind: Option<nova_core::GradientKind>,
        #[serde(default)]
        angle: Option<f32>,
        #[serde(default)]
        center_x: Option<f32>,
        #[serde(default)]
        center_y: Option<f32>,
        #[serde(default)]
        radius: Option<f32>,
        #[serde(default)]
        stops: Option<Vec<(f32, String)>>,
    },
    /// Real polygon boolean operation between two Path objects. The
    /// result replaces the target's paths; the `with` object is kept.
    BooleanPathOperation {
        target: String,
        with: String,
        operation: nova_core::BoolOp,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
    },
    /// Auto-binds every vertex of a Path object to its nearest bone
    /// (weight 1.0) so the object becomes a skinned mesh.
    BindMeshToSkeleton {
        symbol: String,
        object: String,
    },
    /// Sets one vertex weight and renormalizes the vertex. `path` selects
    /// the sub-path index (default 0); `vertex` is the flat skinning-point
    /// index (anchors + curve control points, see PathShape::skinning_points).
    SetVertexWeight {
        symbol: String,
        object: String,
        #[serde(default)]
        path: Option<usize>,
        vertex: usize,
        bone: String,
        weight: f32,
    },
    /// Builds a small playable level in one transaction: scene (created when
    /// missing), world bounds + spawn, tileset, tilemap with painted strokes,
    /// placed game objects and camera follow.
    CreateGameLevel {
        scene: String,
        #[serde(default)]
        world_width: Option<f32>,
        #[serde(default)]
        world_height: Option<f32>,
        #[serde(default)]
        spawn: Option<(f32, f32)>,
        #[serde(default)]
        tileset: Option<LevelTileset>,
        #[serde(default)]
        tilemap: Option<LevelTilemap>,
        #[serde(default)]
        objects: Vec<LevelObject>,
        #[serde(default)]
        camera_follow: Option<String>,
    },
}

/// Tileset description inside [`ProjectOperation::CreateGameLevel`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelTileset {
    /// Existing bitmap asset name.
    pub asset: String,
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    #[serde(default)]
    pub solid_tiles: Vec<usize>,
}

/// Tilemap description inside [`ProjectOperation::CreateGameLevel`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelTilemap {
    pub name: String,
    /// Tileset name created by the same operation.
    pub tileset: String,
    pub columns: usize,
    pub rows: usize,
    pub x: f32,
    pub y: f32,
    /// Painted strokes applied after creation (pencil/rect/flood/replace).
    #[serde(default)]
    pub strokes: Vec<LevelStroke>,
}

/// One paint stroke inside [`LevelTilemap`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelStroke {
    pub tool: nova_core::tilemap::PaintTool,
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    #[serde(default)]
    pub tile: u16,
    #[serde(default)]
    pub layer: usize,
}

/// One placed object inside [`ProjectOperation::CreateGameLevel`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelObject {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default)]
    pub collision: Option<nova_core::CollisionSpec>,
    #[serde(default)]
    pub physics: Option<nova_core::PhysicsSpec>,
    #[serde(default)]
    pub particle_preset: Option<nova_core::ParticlePreset>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum OperationError {
    #[error("Ungültige Projektoperation: {0}")]
    Invalid(String),
    #[error("Nicht unterstützte Projektoperation: {0}")]
    Unsupported(String),
}

impl OperationError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationPlan {
    /// Human-readable summaries in execution order.
    pub summaries: Vec<String>,
    /// Errors that would block the batch. Empty means the batch is applicable.
    pub errors: Vec<String>,
}

impl OperationPlan {
    pub fn is_applicable(&self) -> bool {
        self.errors.is_empty() && !self.summaries.is_empty()
    }
}

/// Validates a batch against a clone of the project without touching the
/// caller's project. This is what the preview pane renders.
pub fn preview_operations(project: &Project, operations: &[ProjectOperation]) -> OperationPlan {
    preview_operations_with(project, operations, &|_| {
        Err("Dateizugriff ist ohne Workspace nicht konfiguriert.".to_string())
    })
}

/// Like [`preview_operations`], but asset import operations resolve their
/// `path` through `read_asset`. The reader is expected to enforce workspace
/// confinement; this module never touches the filesystem itself.
pub fn preview_operations_with(
    project: &Project,
    operations: &[ProjectOperation],
    read_asset: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> OperationPlan {
    let mut preview = project.clone();
    let mut summaries = Vec::new();
    if operations.len() > MAX_OPERATIONS {
        return OperationPlan {
            summaries,
            errors: vec![format!(
                "Zu viele Projektoperationen: {} (Maximum {MAX_OPERATIONS}).",
                operations.len()
            )],
        };
    }
    for (index, operation) in operations.iter().enumerate() {
        match apply_one(&mut preview, operation, read_asset) {
            Ok(summary) => summaries.push(format!("{}. {summary}", index + 1)),
            Err(error) => {
                return OperationPlan {
                    summaries,
                    errors: vec![format!("Operation {}: {error}", index + 1)],
                };
            }
        }
    }
    if let Err(error) = preview.validate() {
        return OperationPlan {
            summaries,
            errors: vec![format!("Projektvalidierung: {error}")],
        };
    }
    OperationPlan {
        summaries,
        errors: Vec::new(),
    }
}

/// Applies a whole batch transactionally. On error the project is unchanged.
pub fn apply_operations(
    project: &mut Project,
    operations: &[ProjectOperation],
) -> Result<Vec<String>, OperationError> {
    apply_operations_with(project, operations, &|_| {
        Err("Dateizugriff ist ohne Workspace nicht konfiguriert.".to_string())
    })
}

/// Like [`apply_operations`], but asset import operations resolve `path`
/// through the workspace-confined `read_asset` callback.
pub fn apply_operations_with(
    project: &mut Project,
    operations: &[ProjectOperation],
    read_asset: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<String>, OperationError> {
    let plan = preview_operations_with(project, operations, read_asset);
    if !plan.errors.is_empty() {
        return Err(OperationError::Invalid(plan.errors.join(" ")));
    }
    let mut next = project.clone();
    let mut summaries = Vec::new();
    for operation in operations {
        summaries.push(apply_one(&mut next, operation, read_asset)?);
    }
    next.validate()
        .map_err(|error| OperationError::Invalid(error.to_string()))?;
    *project = next;
    Ok(summaries)
}

fn apply_one(
    project: &mut Project,
    operation: &ProjectOperation,
    read_asset: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<String, OperationError> {
    match operation {
        ProjectOperation::RenameProject { name } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Projektname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            project.metadata.name = name.to_string();
            project.touch();
            Ok(format!("Projekt umbenennen in '{name}'"))
        }
        ProjectOperation::SetStage {
            width,
            height,
            fps,
            background,
            scene,
        } => {
            check_size(*width, *height, "Bühne")?;
            if let Some(fps) = fps {
                if !fps.is_finite() || !(1.0..=120.0).contains(fps) {
                    return Err(OperationError::invalid(
                        "Die Bildrate muss zwischen 1 und 120 liegen.",
                    ));
                }
            }
            let background = background.as_deref().map(parse_color).transpose()?;
            let scene = scene_mut(project, scene.as_deref())?;
            scene.width = *width;
            scene.height = *height;
            if let Some(fps) = fps {
                scene.fps = *fps;
            }
            if let Some(background) = background {
                scene.background = background;
            }
            let name = scene.name.clone();
            project.touch();
            Ok(format!("Bühne '{name}' auf {width}×{height} setzen"))
        }
        ProjectOperation::AddScene { name } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Szenenname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            if project.scenes.iter().any(|scene| scene.name == name) {
                return Err(OperationError::invalid(format!(
                    "Eine Szene namens '{name}' existiert bereits."
                )));
            }
            if project.scenes.len() >= 256 {
                return Err(OperationError::invalid(
                    "Ein Projekt kann höchstens 256 Szenen enthalten.",
                ));
            }
            project.add_scene(name);
            Ok(format!("Szene '{name}' hinzufügen"))
        }
        ProjectOperation::AddLayer {
            name,
            scene,
            symbol,
            frames,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Ebenenname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            let frames = frames.unwrap_or(1);
            if !(1..=100_000).contains(&frames) {
                return Err(OperationError::invalid(
                    "Eine neue Ebene muss 1 bis 100000 Frames haben.",
                ));
            }
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            if scene.layers.iter().any(|layer| layer.name == name) {
                return Err(OperationError::invalid(format!(
                    "Eine Ebene namens '{name}' existiert in dieser Timeline bereits."
                )));
            }
            if scene.layers.len() >= 1_000 {
                return Err(OperationError::invalid(
                    "Eine Timeline kann höchstens 1000 Ebenen enthalten.",
                ));
            }
            let scene_name = scene.name.clone();
            scene.layers.push(nova_core::Layer::new(name, frames));
            project.touch();
            Ok(format!("Ebene '{name}' in '{scene_name}' hinzufügen"))
        }
        ProjectOperation::AddObject {
            name,
            scene,
            symbol,
            layer,
            shape,
            x,
            y,
            width,
            height,
            fill,
            text,
            font_size,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Objektname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            check_size(*width, *height, "Objekt")?;
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid(
                    "Objektposition muss eine endliche Zahl sein.",
                ));
            }
            let fill = fill
                .as_deref()
                .map(parse_color)
                .transpose()?
                .unwrap_or_else(|| match shape {
                    ObjectShape::Text => Rgba::WHITE,
                    _ => Rgba::rgb(78, 161, 255),
                });
            let object = match shape {
                ObjectShape::Rect => ObjectInstance::rect(name, *x, *y, *width, *height, fill),
                ObjectShape::Ellipse => {
                    ObjectInstance::ellipse(name, *x, *y, *width, *height, fill)
                }
                ObjectShape::Text => {
                    if text.trim().is_empty() {
                        return Err(OperationError::invalid(
                            "Ein Textobjekt benötigt nichtleeren Text.",
                        ));
                    }
                    let size = font_size.unwrap_or(24.0);
                    if !size.is_finite() || !(1.0..=512.0).contains(&size) {
                        return Err(OperationError::invalid(
                            "Die Textgröße muss zwischen 1 und 512 liegen.",
                        ));
                    }
                    ObjectInstance::text(name, *x, *y, text, size, fill)
                }
            };
            let layer_name = insert_object(
                project,
                scene.as_deref(),
                symbol.as_deref(),
                layer.as_deref(),
                object,
            )?;
            Ok(format!(
                "{} '{name}' auf Ebene '{layer_name}' hinzufügen",
                shape.label()
            ))
        }
        ProjectOperation::UpdateObject {
            name,
            scene,
            layer,
            x,
            y,
            rotation,
            scale_x,
            scale_y,
            fill,
            text,
            font_size,
        } => {
            if x.is_none()
                && y.is_none()
                && rotation.is_none()
                && scale_x.is_none()
                && scale_y.is_none()
                && fill.is_none()
                && text.is_none()
                && font_size.is_none()
            {
                return Err(OperationError::invalid(
                    "UpdateObject benötigt mindestens eine zu ändernde Eigenschaft.",
                ));
            }
            for (value, label) in [(x, "x"), (y, "y"), (rotation, "rotation")] {
                if let Some(value) = value {
                    if !value.is_finite() {
                        return Err(OperationError::invalid(format!(
                            "Objekt-Eigenschaft '{label}' muss eine endliche Zahl sein."
                        )));
                    }
                }
            }
            for (value, label) in [(scale_x, "scale_x"), (scale_y, "scale_y")] {
                if let Some(value) = value {
                    if !value.is_finite() || *value <= 0.0 || *value > 1_000.0 {
                        return Err(OperationError::invalid(format!(
                            "Objekt-Eigenschaft '{label}' muss zwischen 0 und 1000 liegen."
                        )));
                    }
                }
            }
            if let Some(size) = font_size {
                if !size.is_finite() || !(1.0..=512.0).contains(size) {
                    return Err(OperationError::invalid(
                        "Die Textgröße muss zwischen 1 und 512 liegen.",
                    ));
                }
            }
            let fill = fill.as_deref().map(parse_color).transpose()?;
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            let mut matches = Vec::new();
            for (layer_index, candidate) in scene.layers.iter().enumerate() {
                if let Some(selector) = layer.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                    if candidate.name != selector {
                        continue;
                    }
                }
                for (frame_index, frame) in candidate.frames.iter().enumerate() {
                    for (object_index, object) in frame.objects.iter().enumerate() {
                        if object.name == name.trim() {
                            matches.push((layer_index, frame_index, object_index));
                        }
                    }
                }
            }
            match matches.len() {
                0 => {
                    return Err(OperationError::invalid(format!(
                        "Objekt '{name}' existiert in Szene '{scene_name}' nicht."
                    )));
                }
                1 => {}
                count => {
                    return Err(OperationError::invalid(format!(
                        "Objektname '{name}' ist {count}-mal vorhanden. Gib eine Ebene an, um das Ziel eindeutig zu wählen."
                    )));
                }
            }
            let (layer_index, frame_index, object_index) = matches[0];
            let object = &mut scene.layers[layer_index].frames[frame_index].objects[object_index];
            if (text.is_some() || font_size.is_some())
                && !matches!(object.kind, nova_core::ObjectKind::Text(_))
            {
                return Err(OperationError::invalid(format!(
                    "Objekt '{name}' ist kein Textobjekt."
                )));
            }
            if let Some(x) = x {
                object.transform.x = *x;
            }
            if let Some(y) = y {
                object.transform.y = *y;
            }
            if let Some(rotation) = rotation {
                object.transform.rotation = *rotation;
            }
            if let Some(scale_x) = scale_x {
                object.transform.scale_x = *scale_x;
            }
            if let Some(scale_y) = scale_y {
                object.transform.scale_y = *scale_y;
            }
            if let Some(fill) = fill {
                object.fill = fill;
            }
            if let Some(text) = text {
                if let nova_core::ObjectKind::Text(text_object) = &mut object.kind {
                    text_object.text = text.clone();
                }
            }
            if let Some(size) = font_size {
                if let nova_core::ObjectKind::Text(text_object) = &mut object.kind {
                    text_object.size = *size;
                }
            }
            project.touch();
            Ok(format!(
                "Objekt '{name}' in Szene '{scene_name}' aktualisieren"
            ))
        }
        ProjectOperation::AddKeyframe {
            layer,
            frame,
            scene,
            symbol,
            label,
            duplicate_previous,
        } => {
            let index = checked_frame(*frame)?;
            if let Some(label) = label {
                validate_label(label)?;
            }
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            let layer = find_layer_mut(scene, layer)?;
            let layer_name = layer.name.clone();
            let duplicated = if *duplicate_previous {
                index
                    .checked_sub(1)
                    .and_then(|previous| layer.last_keyframe_at_or_before(previous))
                    .and_then(|previous| layer.frames.get(previous))
                    .map(|frame| frame.objects.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            layer.ensure_frames(index + 1);
            let frame = &mut layer.frames[index];
            frame.keyframe = true;
            if *duplicate_previous {
                frame.objects = duplicated;
            }
            if let Some(label) = label {
                frame.label = Some(label.trim().to_string());
            }
            project.touch();
            Ok(format!(
                "Schlüsselbild {} in Ebene '{layer_name}' (Szene '{scene_name}') anlegen",
                index + 1
            ))
        }
        ProjectOperation::SetFrameLabel {
            layer,
            frame,
            label,
            scene,
            symbol,
        } => {
            let index = checked_frame(*frame)?;
            validate_label(label)?;
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            let layer = find_layer_mut(scene, layer)?;
            let layer_name = layer.name.clone();
            if index >= layer.frames.len() {
                return Err(OperationError::invalid(format!(
                    "Frame {} existiert in Ebene '{layer_name}' nicht.",
                    index + 1
                )));
            }
            layer.frames[index].label = Some(label.trim().to_string());
            project.touch();
            Ok(format!(
                "Frame {} in Ebene '{layer_name}' (Szene '{scene_name}') als '{}' beschriften",
                index + 1,
                label.trim()
            ))
        }
        ProjectOperation::AddTween {
            layer,
            frame,
            scene,
            symbol,
            easing,
            kind,
        } => {
            let index = checked_frame(*frame)?;
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            let layer = find_layer_mut(scene, layer)?;
            let layer_name = layer.name.clone();
            if index >= layer.frames.len() || !layer.frames[index].keyframe {
                return Err(OperationError::invalid(format!(
                    "Frame {} in Ebene '{layer_name}' ist kein vorhandenes Schlüsselbild.",
                    index + 1
                )));
            }
            let ids: Vec<_> = layer.frames[index]
                .objects
                .iter()
                .map(|object| object.id)
                .collect();
            if ids.is_empty() {
                return Err(OperationError::invalid(format!(
                    "Schlüsselbild {} in Ebene '{layer_name}' enthält keine Objekte für einen Tween.",
                    index + 1
                )));
            }
            let next = layer
                .frames
                .iter()
                .enumerate()
                .skip(index + 1)
                .find(|(_, frame)| {
                    frame.keyframe && frame.objects.iter().any(|object| ids.contains(&object.id))
                });
            if next.is_none() {
                return Err(OperationError::invalid(format!(
                    "Für den Tween ab Frame {} fehlt ein späteres Schlüsselbild mit denselben Objekt-IDs.",
                    index + 1
                )));
            }
            layer.frames[index].tween = Some(Tween {
                easing: *easing,
                kind: kind.unwrap_or_default(),
            });
            project.touch();
            Ok(format!(
                "{}-Tween ab Frame {} in Ebene '{layer_name}' (Szene '{scene_name}') setzen",
                kind.unwrap_or_default().label(),
                index + 1
            ))
        }
        ProjectOperation::AddSymbol {
            name,
            width,
            height,
            fill,
            corner_radius,
            kind,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Symbolname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            check_size(*width, *height, "Symbol")?;
            if project.symbols.iter().any(|symbol| symbol.name == name) {
                return Err(OperationError::invalid(format!(
                    "Ein Symbol namens '{name}' existiert bereits."
                )));
            }
            if project.symbols.len() >= 1_000 {
                return Err(OperationError::invalid(
                    "Die Bibliothek kann höchstens 1000 Symbole enthalten.",
                ));
            }
            let fill = fill
                .as_deref()
                .map(parse_color)
                .transpose()?
                .unwrap_or_else(|| Rgba::rgb(78, 161, 255));
            let corner_radius = corner_radius.unwrap_or(0.0);
            if !corner_radius.is_finite() || corner_radius < 0.0 {
                return Err(OperationError::invalid(
                    "Der Eckenradius muss eine nichtnegative Zahl sein.",
                ));
            }
            let mut symbol =
                Symbol::with_kind(name, kind.unwrap_or_default(), *width, *height, fill);
            symbol.corner_radius = corner_radius.min(width.min(*height) / 2.0);
            project.symbols.push(symbol);
            project.touch();
            Ok(format!(
                "{}-Symbol '{name}' zur Bibliothek hinzufügen",
                kind.unwrap_or_default().label()
            ))
        }
        ProjectOperation::SetSymbolKind { symbol, kind } => {
            let name = symbol.trim();
            let target = project
                .symbols
                .iter_mut()
                .find(|candidate| candidate.name == name)
                .ok_or_else(|| {
                    OperationError::invalid(format!("Symbol '{name}' existiert nicht."))
                })?;
            if target.kind == *kind {
                return Err(OperationError::invalid(format!(
                    "Symbol '{name}' ist bereits vom Typ '{}'.",
                    kind.label()
                )));
            }
            target.kind = *kind;
            project.touch();
            Ok(format!("Symbol '{name}' als '{}' setzen", kind.label()))
        }
        ProjectOperation::PlaceSymbol {
            symbol,
            name,
            x,
            y,
            scene,
            symbol_target,
            layer,
            fill,
        } => {
            let symbol = project
                .symbols
                .iter()
                .find(|candidate| candidate.name == symbol.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Symbol '{}' existiert nicht.", symbol.trim()))
                })?;
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Instanzname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid(
                    "Symbolposition muss eine endliche Zahl sein.",
                ));
            }
            let fill = fill
                .as_deref()
                .map(parse_color)
                .transpose()?
                .unwrap_or(symbol.fill);
            let object = ObjectInstance::symbol(name, *x, *y, symbol.id, fill);
            let layer_name = insert_object(
                project,
                scene.as_deref(),
                symbol_target.as_deref(),
                layer.as_deref(),
                object,
            )?;
            Ok(format!(
                "Symbolinstanz '{name}' auf Ebene '{layer_name}' platzieren"
            ))
        }
        ProjectOperation::ImportBitmapAsset { name, path } => {
            let name = validate_asset_name(name)?;
            let path = validate_asset_path(path, &["png", "jpg", "jpeg", "gif"])?;
            if project.assets.iter().any(|asset| asset.name == name) {
                return Err(OperationError::invalid(format!(
                    "Ein Asset namens '{name}' existiert bereits."
                )));
            }
            if project.assets.len() >= 2_000 {
                return Err(OperationError::invalid(
                    "Ein Projekt kann höchstens 2000 Assets enthalten.",
                ));
            }
            let data = read_asset(&path)
                .map_err(|error| OperationError::invalid(format!("Asset '{path}': {error}")))?;
            if data.is_empty() || data.len() > 64 * 1024 * 1024 {
                return Err(OperationError::invalid(format!(
                    "Asset '{path}' ist leer oder überschreitet 64 MB."
                )));
            }
            let image = image::load_from_memory(&data).map_err(|error| {
                OperationError::invalid(format!("Asset '{path}' ist kein gültiges Bild: {error}"))
            })?;
            let (width, height) = (image.width(), image.height());
            let mime = match path.rsplit('.').next().unwrap_or_default() {
                "png" => "image/png",
                "gif" => "image/gif",
                _ => "image/jpeg",
            };
            project.assets.push(Asset::bitmap(
                name.clone(),
                path.clone(),
                mime,
                width,
                height,
                data,
            ));
            project.touch();
            Ok(format!(
                "Bitmap-Asset '{name}' importieren ({width}×{height})"
            ))
        }
        ProjectOperation::ImportAudioAsset { name, path } => {
            let name = validate_asset_name(name)?;
            let path = validate_asset_path(path, &["wav", "mp3"])?;
            if project.assets.iter().any(|asset| asset.name == name) {
                return Err(OperationError::invalid(format!(
                    "Ein Asset namens '{name}' existiert bereits."
                )));
            }
            if project.assets.len() >= 2_000 {
                return Err(OperationError::invalid(
                    "Ein Projekt kann höchstens 2000 Assets enthalten.",
                ));
            }
            let data = read_asset(&path)
                .map_err(|error| OperationError::invalid(format!("Asset '{path}': {error}")))?;
            if data.is_empty() || data.len() > 256 * 1024 * 1024 {
                return Err(OperationError::invalid(format!(
                    "Asset '{path}' ist leer oder überschreitet 256 MB."
                )));
            }
            let is_wav = path.ends_with(".wav");
            let (mime, sample_rate, channels, duration) = if is_wav {
                match wav_info(&data) {
                    Some(info) => ("audio/wav", info.0, info.1, info.2),
                    None => {
                        return Err(OperationError::invalid(format!(
                            "Asset '{path}' ist keine gültige PCM-WAV-Datei."
                        )));
                    }
                }
            } else {
                ("audio/mpeg", 0, 0, 0.0)
            };
            project.assets.push(Asset::audio(
                name.clone(),
                path.clone(),
                mime,
                sample_rate,
                channels,
                duration,
                data,
            ));
            project.touch();
            Ok(format!("Audio-Asset '{name}' importieren"))
        }
        ProjectOperation::PlaceBitmap {
            asset,
            name,
            x,
            y,
            scene,
            symbol,
            layer,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Objektname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid(
                    "Bitmapposition muss eine endliche Zahl sein.",
                ));
            }
            let asset = project
                .assets
                .iter()
                .find(|candidate| candidate.name == asset.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Asset '{}' existiert nicht.", asset.trim()))
                })?;
            if asset.kind != AssetKind::Bitmap || !asset.is_embedded() {
                return Err(OperationError::invalid(format!(
                    "Asset '{}' ist kein eingebettetes Bitmap.",
                    asset.name
                )));
            }
            if asset.width == 0 || asset.height == 0 {
                return Err(OperationError::invalid(format!(
                    "Asset '{}' hat keine gültigen Abmessungen.",
                    asset.name
                )));
            }
            let (asset_id, asset_w, asset_h) = (asset.id, asset.width, asset.height);
            let object =
                ObjectInstance::bitmap(name, *x, *y, asset_id, asset_w as f32, asset_h as f32);
            let layer_name = insert_object(
                project,
                scene.as_deref(),
                symbol.as_deref(),
                layer.as_deref(),
                object,
            )?;
            Ok(format!(
                "Bitmap '{name}' auf Ebene '{layer_name}' platzieren"
            ))
        }
        ProjectOperation::PlaceSoundCue {
            asset,
            frame,
            scene,
            symbol,
            layer,
            event,
        } => {
            let index = checked_frame(*frame)?;
            let asset = project
                .assets
                .iter()
                .find(|candidate| candidate.name == asset.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Asset '{}' existiert nicht.", asset.trim()))
                })?;
            if asset.kind != AssetKind::Audio || !asset.is_embedded() {
                return Err(OperationError::invalid(format!(
                    "Asset '{}' ist kein eingebettetes Audio.",
                    asset.name
                )));
            }
            let asset_id = asset.id;
            let asset_name = asset.name.clone();
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            scene.ensure_frame_count(index + 1);
            // Reuse an existing sound layer, honouring an explicit selector,
            // or create a dedicated one.
            let layer_id = match layer.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                Some(name) => scene
                    .layers
                    .iter()
                    .find(|candidate| candidate.name == name)
                    .map(|candidate| candidate.id)
                    .ok_or_else(|| {
                        OperationError::invalid(format!(
                            "Ebene '{name}' existiert in '{scene_name}' nicht."
                        ))
                    })?,
                None => scene
                    .layers
                    .iter()
                    .find(|candidate| candidate.kind == LayerKind::Sound)
                    .map(|candidate| candidate.id)
                    .unwrap_or_else(|| {
                        let mut sound_layer = nova_core::Layer::new("Sound", scene.frame_count());
                        sound_layer.kind = LayerKind::Sound;
                        scene.layers.push(sound_layer);
                        scene.layers.last().map(|layer| layer.id).unwrap()
                    }),
            };
            let target = scene
                .layers
                .iter_mut()
                .find(|candidate| candidate.id == layer_id)
                .ok_or_else(|| OperationError::invalid("Sound-Ebene nicht gefunden."))?;
            if target.kind != LayerKind::Sound {
                return Err(OperationError::invalid(format!(
                    "Ebene '{}' ist keine Sound-Ebene.",
                    target.name
                )));
            }
            target.ensure_frames(index + 1);
            target.frames[index].keyframe = true;
            target.frames[index].sound = Some(if *event {
                SoundCue::event(asset_id)
            } else {
                SoundCue::timeline(asset_id)
            });
            project.touch();
            Ok(format!(
                "Sound '{asset_name}' auf Frame {} in '{scene_name}' setzen",
                index + 1
            ))
        }
        ProjectOperation::SetLayerKind {
            layer,
            kind,
            scene,
            symbol,
        } => {
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            let target = scene
                .layers
                .iter_mut()
                .find(|candidate| candidate.name == layer.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!(
                        "Ebene '{}' existiert in '{scene_name}' nicht.",
                        layer.trim()
                    ))
                })?;
            if target.kind == *kind {
                return Err(OperationError::invalid(format!(
                    "Ebene '{}' ist bereits vom Typ '{}'.",
                    target.name,
                    kind.label()
                )));
            }
            let layer_name = target.name.clone();
            target.kind = *kind;
            project.touch();
            Ok(format!(
                "Ebene '{layer_name}' in '{scene_name}' als '{}' setzen",
                kind.label()
            ))
        }
        ProjectOperation::AddScript { path, source } => {
            let path = normalize_script_path(path)?;
            if project.scripts.iter().any(|script| script.path == path) {
                return Err(OperationError::invalid(format!(
                    "Ein Skript unter '{path}' existiert bereits."
                )));
            }
            if source.chars().count() > 1_000_000 {
                return Err(OperationError::invalid(
                    "Skriptquelle ist auf 1.000.000 Zeichen begrenzt.",
                ));
            }
            if project.scripts.len() >= 1_000 {
                return Err(OperationError::invalid(
                    "Ein Projekt kann höchstens 1000 Skripte enthalten.",
                ));
            }
            project
                .scripts
                .push(Script::new(path.clone(), source.clone()));
            project.touch();
            Ok(format!("Skript '{path}' hinzufügen"))
        }
        ProjectOperation::SetGameObject {
            object,
            scene,
            collision,
            no_collision,
            physics,
            no_physics,
            clear,
        } => {
            if collision.is_none() && physics.is_none() && !no_collision && !no_physics && !clear {
                return Err(OperationError::invalid(
                    "SetGameObject benötigt mindestens eine Änderung (collision, physics, no_*, clear).",
                ));
            }
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            let needle = object.trim();
            let mut matches = Vec::new();
            for (layer_index, layer) in scene.layers.iter().enumerate() {
                for (frame_index, frame) in layer.frames.iter().enumerate() {
                    for (object_index, candidate) in frame.objects.iter().enumerate() {
                        if candidate.name == needle {
                            matches.push((layer_index, frame_index, object_index));
                        }
                    }
                }
            }
            match matches.len() {
                0 => {
                    return Err(OperationError::invalid(format!(
                        "Objekt '{needle}' existiert in Szene '{scene_name}' nicht."
                    )));
                }
                1 => {}
                count => {
                    return Err(OperationError::invalid(format!(
                        "Objektname '{needle}' ist {count}-mal vorhanden."
                    )));
                }
            }
            let (layer_index, frame_index, object_index) = matches[0];
            let target = &mut scene.layers[layer_index].frames[frame_index].objects[object_index];
            if *clear {
                target.game = None;
            } else {
                let mut config = target.game.clone().unwrap_or_default();
                if *no_collision {
                    config.collision = None;
                } else if let Some(spec) = collision {
                    if !spec.width.is_finite()
                        || !spec.height.is_finite()
                        || spec.width <= 0.0
                        || spec.height <= 0.0
                        || spec.width > 100_000.0
                        || spec.height > 100_000.0
                    {
                        return Err(OperationError::invalid(
                            "Kollisionsausmaße müssen positive endliche Zahlen sein.",
                        ));
                    }
                    config.collision = Some(spec.clone());
                }
                if *no_physics {
                    config.physics = None;
                } else if let Some(spec) = physics {
                    if !spec.gravity_scale.is_finite()
                        || !spec.friction.is_finite()
                        || !spec.bounce.is_finite()
                        || spec.friction < 0.0
                        || spec.friction > 1_000.0
                        || !(0.0..=1.0).contains(&spec.bounce)
                    {
                        return Err(OperationError::invalid(
                            "Physikwerte sind ungültig (bounce 0..=1, reibung >= 0).",
                        ));
                    }
                    config.physics = Some(*spec);
                }
                target.game = Some(config);
            }
            project.touch();
            Ok(format!(
                "Game-Metadaten für Objekt '{needle}' in Szene '{scene_name}' gesetzt"
            ))
        }
        ProjectOperation::SetSceneGame {
            scene,
            camera,
            input,
            world,
            clear,
        } => {
            if camera.is_none() && input.is_none() && world.is_none() && !clear {
                return Err(OperationError::invalid(
                    "SetSceneGame benötigt mindestens camera, input, world oder clear.",
                ));
            }
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            if *clear {
                scene.game = None;
            } else {
                let mut config = scene.game.clone().unwrap_or_default();
                if let Some(camera) = camera {
                    if !camera.zoom.is_finite() || camera.zoom <= 0.0 || camera.zoom > 64.0 {
                        return Err(OperationError::invalid(
                            "Kamera-Zoom muss zwischen 0 und 64 liegen.",
                        ));
                    }
                    if camera.follow.chars().count() > 120 {
                        return Err(OperationError::invalid(
                            "Kamera-Folgeobjekt-Name ist zu lang.",
                        ));
                    }
                    config.camera = Some(camera.clone());
                }
                if let Some(input) = input {
                    for (value, label) in [
                        (&input.left, "left"),
                        (&input.right, "right"),
                        (&input.up, "up"),
                        (&input.down, "down"),
                        (&input.jump, "jump"),
                        (&input.action, "action"),
                    ] {
                        if value.trim().is_empty() || value.chars().count() > 40 {
                            return Err(OperationError::invalid(format!(
                                "Eingabe-Bindung '{label}' ist leer oder zu lang."
                            )));
                        }
                    }
                    config.input = Some(input.clone());
                }
                if let Some(world) = world {
                    if !world.width.is_finite()
                        || !world.height.is_finite()
                        || world.width < 0.0
                        || world.height < 0.0
                        || world.width > 1_000_000.0
                        || world.height > 1_000_000.0
                        || !world.spawn.0.is_finite()
                        || !world.spawn.1.is_finite()
                    {
                        return Err(OperationError::invalid(
                            "Weltmaße und Spawn müssen endliche, nicht-negative Werte sein.",
                        ));
                    }
                    config.world = Some(world.clone());
                }
                scene.game = Some(config);
            }
            project.touch();
            Ok(format!(
                "Spielkonfiguration für Szene '{scene_name}' gesetzt"
            ))
        }
        ProjectOperation::AddBone {
            symbol,
            name,
            x,
            y,
            length,
            parent,
            rotation,
        } => {
            check_size(*length, 1.0, "Knochen")?;
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid(
                    "Knochenposition muss endlich sein.",
                ));
            }
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            let skeleton = symbol
                .skeleton
                .get_or_insert_with(nova_core::Skeleton::default);
            if skeleton.bones.iter().any(|b| b.name == name.trim()) {
                return Err(OperationError::invalid(format!(
                    "Knochen '{name}' existiert im Symbol '{symbol_name}' bereits.",
                    symbol_name = symbol.name
                )));
            }
            let mut bone = nova_core::Bone::new(name.trim(), *x, *y, *length);
            if let Some(rot) = rotation {
                if !rot.is_finite() {
                    return Err(OperationError::invalid(
                        "Knochenrotation muss endlich sein.",
                    ));
                }
                bone.rotation = *rot;
            }
            if let Some(parent_name) = parent.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
                let parent_id = skeleton
                    .bones
                    .iter()
                    .find(|b| b.name == parent_name)
                    .map(|b| b.id)
                    .ok_or_else(|| {
                        OperationError::invalid(format!(
                            "Elternknochen '{parent_name}' existiert nicht."
                        ))
                    })?;
                bone.parent = Some(parent_id);
            }
            let bone_name = bone.name.clone();
            skeleton.bones.push(bone);
            project.touch();
            Ok(format!(
                "Knochen '{bone_name}' zum Skelett von Symbol '{symbol_name}' hinzugefügt"
            ))
        }
        ProjectOperation::SetBoneParent {
            symbol,
            bone,
            parent,
        } => {
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            let skeleton = symbol
                .skeleton
                .as_mut()
                .ok_or_else(|| OperationError::invalid("Das Symbol hat kein Skelett."))?;
            let bone_id = skeleton
                .bones
                .iter()
                .find(|b| b.name == bone.trim())
                .map(|b| b.id)
                .ok_or_else(|| {
                    OperationError::invalid(format!("Knochen '{bone}' existiert nicht."))
                })?;
            let parent_id = match parent.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
                Some(parent_name) => Some(
                    skeleton
                        .bones
                        .iter()
                        .find(|b| b.name == parent_name)
                        .map(|b| b.id)
                        .ok_or_else(|| {
                            OperationError::invalid(format!(
                                "Elternknochen '{parent_name}' existiert nicht."
                            ))
                        })?,
                ),
                None => None,
            };
            if parent_id == Some(bone_id) {
                return Err(OperationError::invalid(
                    "Ein Knochen kann nicht sein eigener Elternteil sein.",
                ));
            }
            if let Some(pid) = parent_id {
                // Cycle check: parent must not descend from this bone.
                let mut cursor = Some(pid);
                while let Some(id) = cursor {
                    if id == bone_id {
                        return Err(OperationError::invalid(
                            "Knochenhierarchie würde einen Zyklus erzeugen.",
                        ));
                    }
                    cursor = skeleton
                        .bones
                        .iter()
                        .find(|b| b.id == id)
                        .and_then(|b| b.parent);
                }
            }
            let target = skeleton.bones.iter_mut().find(|b| b.id == bone_id).unwrap();
            target.parent = parent_id;
            project.touch();
            Ok(format!(
                "Elternknochen von '{bone}' in Symbol '{symbol_name}' aktualisiert"
            ))
        }
        ProjectOperation::AddBoneClip {
            symbol,
            name,
            frames,
            looping,
        } => {
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            let clip_name = name.trim();
            if clip_name.is_empty() || clip_name.chars().count() > 80 {
                return Err(OperationError::invalid("Clip-Name ist leer oder zu lang."));
            }
            let clip = match symbol.bone_clips.iter_mut().find(|c| c.name == clip_name) {
                Some(existing) => existing,
                None => {
                    symbol
                        .bone_clips
                        .push(nova_core::BoneClip::new(clip_name, frames.unwrap_or(24)));
                    symbol.bone_clips.last_mut().unwrap()
                }
            };
            if let Some(f) = frames {
                clip.frames = (*f).max(1);
            }
            if let Some(l) = looping {
                clip.looping = *l;
            }
            // Keep the authored timeline at least as long as the clip so the
            // editor and the exporter bake every clip frame.
            symbol.timeline.ensure_frame_count(clip.frames as usize);
            project.touch();
            Ok(format!(
                "Bone-Clip '{clip_name}' in Symbol '{symbol_name}' angelegt"
            ))
        }
        ProjectOperation::AddBoneKey {
            symbol,
            clip,
            bone,
            frame,
            rotation,
        } => {
            if !rotation.is_finite() {
                return Err(OperationError::invalid(
                    "Knochenrotation muss endlich sein.",
                ));
            }
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            let clip_ref = symbol
                .bone_clips
                .iter_mut()
                .find(|c| c.name == clip.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Clip '{clip}' existiert nicht."))
                })?;
            let skeleton = symbol
                .skeleton
                .as_ref()
                .ok_or_else(|| OperationError::invalid("Das Symbol hat kein Skelett."))?;
            let bone_id = skeleton
                .bones
                .iter()
                .find(|b| b.name == bone.trim())
                .map(|b| b.id)
                .ok_or_else(|| {
                    OperationError::invalid(format!("Knochen '{bone}' existiert nicht."))
                })?;
            let track = match clip_ref.tracks.iter_mut().find(|t| t.bone == bone_id) {
                Some(t) => t,
                None => {
                    clip_ref.tracks.push(nova_core::BoneTrack {
                        bone: bone_id,
                        keys: Vec::new(),
                        ik: Vec::new(),
                    });
                    clip_ref.tracks.last_mut().unwrap()
                }
            };
            match track.keys.iter_mut().find(|k| k.frame == *frame) {
                Some(k) => k.rotation = *rotation,
                None => {
                    track.keys.push(nova_core::BoneKey {
                        frame: *frame,
                        rotation: *rotation,
                    });
                    track.keys.sort_by_key(|k| k.frame);
                }
            }
            project.touch();
            Ok(format!(
                "FK-Key Frame {frame} für Knochen '{bone}' in Clip '{clip}' (Symbol '{symbol_name}') gesetzt"
            ))
        }
        ProjectOperation::AddIkKey {
            symbol,
            clip,
            bone,
            frame,
            x,
            y,
            pole_x,
            pole_y,
        } => {
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid("IK-Ziel muss endlich sein."));
            }
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            let clip_ref = symbol
                .bone_clips
                .iter_mut()
                .find(|c| c.name == clip.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Clip '{clip}' existiert nicht."))
                })?;
            let skeleton = symbol
                .skeleton
                .as_ref()
                .ok_or_else(|| OperationError::invalid("Das Symbol hat kein Skelett."))?;
            let bone_id = skeleton
                .bones
                .iter()
                .find(|b| b.name == bone.trim())
                .map(|b| b.id)
                .ok_or_else(|| {
                    OperationError::invalid(format!("Knochen '{bone}' existiert nicht."))
                })?;
            let track = match clip_ref.tracks.iter_mut().find(|t| t.bone == bone_id) {
                Some(t) => t,
                None => {
                    clip_ref.tracks.push(nova_core::BoneTrack {
                        bone: bone_id,
                        keys: Vec::new(),
                        ik: Vec::new(),
                    });
                    clip_ref.tracks.last_mut().unwrap()
                }
            };
            let pole = match (pole_x, pole_y) {
                (Some(px), Some(py)) if px.is_finite() && py.is_finite() => (Some(*px), Some(*py)),
                _ => (None, None),
            };
            let key = nova_core::IkKey {
                frame: *frame,
                x: *x,
                y: *y,
                pole_x: pole.0,
                pole_y: pole.1,
            };
            match track.ik.iter_mut().find(|k| k.frame == *frame) {
                Some(k) => *k = key,
                None => {
                    track.ik.push(key);
                    track.ik.sort_by_key(|k| k.frame);
                }
            }
            project.touch();
            Ok(format!(
                "IK-Key Frame {frame} für Knochen '{bone}' in Clip '{clip}' (Symbol '{symbol_name}') gesetzt"
            ))
        }
        ProjectOperation::BindObjectToBone {
            symbol,
            object,
            bone,
        } => {
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            let skeleton = symbol
                .skeleton
                .as_ref()
                .ok_or_else(|| OperationError::invalid("Das Symbol hat kein Skelett."))?;
            let bone_id = skeleton
                .bones
                .iter()
                .find(|b| b.name == bone.trim())
                .map(|b| b.id)
                .ok_or_else(|| {
                    OperationError::invalid(format!("Knochen '{bone}' existiert nicht."))
                })?;
            let target = find_object_mut_in_scene(&mut symbol.timeline, object)?;
            target.bone = Some(bone_id);
            project.touch();
            Ok(format!(
                "Objekt '{object}' in Symbol '{symbol_name}' an Knochen '{bone}' gebunden"
            ))
        }
        ProjectOperation::SetSymbolClip { symbol, clip } => {
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            match clip.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                Some(name) => {
                    if !symbol.bone_clips.iter().any(|c| c.name == name) {
                        return Err(OperationError::invalid(format!(
                            "Clip '{name}' existiert nicht; lege ihn mit add_bone_clip an."
                        )));
                    }
                    symbol.active_clip = Some(name.to_string());
                }
                None => symbol.active_clip = None,
            }
            project.touch();
            Ok(format!(
                "Aktiver Bone-Clip von Symbol '{symbol_name}' aktualisiert"
            ))
        }
        ProjectOperation::DuplicateBoneClip { symbol, clip, name } => {
            let new_name = name.trim();
            if new_name.is_empty() || new_name.chars().count() > 80 {
                return Err(OperationError::invalid(
                    "Clipname muss 1 bis 80 Zeichen enthalten.",
                ));
            }
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            if symbol.bone_clips.iter().any(|c| c.name == new_name) {
                return Err(OperationError::invalid(format!(
                    "Clip '{new_name}' existiert bereits."
                )));
            }
            let source = symbol
                .bone_clips
                .iter()
                .find(|c| c.name == clip.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Clip '{}' existiert nicht.", clip.trim()))
                })?
                .clone();
            let mut copy = source;
            copy.name = new_name.to_string();
            symbol.bone_clips.push(copy);
            project.touch();
            Ok(format!(
                "Clip '{clip}' als '{new_name}' dupliziert (Symbol '{symbol_name}')"
            ))
        }
        ProjectOperation::RenameBoneClip { symbol, clip, name } => {
            let new_name = name.trim();
            if new_name.is_empty() || new_name.chars().count() > 80 {
                return Err(OperationError::invalid(
                    "Clipname muss 1 bis 80 Zeichen enthalten.",
                ));
            }
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            if symbol.bone_clips.iter().any(|c| c.name == new_name) {
                return Err(OperationError::invalid(format!(
                    "Clip '{new_name}' existiert bereits."
                )));
            }
            let clip_ref = symbol
                .bone_clips
                .iter_mut()
                .find(|c| c.name == clip.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Clip '{}' existiert nicht.", clip.trim()))
                })?;
            let old = std::mem::replace(&mut clip_ref.name, new_name.to_string());
            if symbol.active_clip.as_deref() == Some(old.as_str()) {
                symbol.active_clip = Some(new_name.to_string());
            }
            project.touch();
            Ok(format!(
                "Clip '{old}' in '{new_name}' umbenannt (Symbol '{symbol_name}')"
            ))
        }
        ProjectOperation::SetPathGradient {
            name,
            scene,
            symbol,
            kind,
            angle,
            center_x,
            center_y,
            radius,
            stops,
        } => {
            let timeline = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let object = find_object_mut_in_scene(timeline, name)?;
            let ObjectKind::Path { paths, .. } = &mut object.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{name}' ist kein Pfadobjekt."
                )));
            };
            let parsed_stops: Option<Vec<nova_core::GradientStop>> = stops
                .as_ref()
                .map(|list| {
                    list.iter()
                        .map(|(pos, color)| {
                            Ok(nova_core::GradientStop {
                                position: *pos,
                                color: parse_color(color)?,
                            })
                        })
                        .collect::<Result<Vec<_>, OperationError>>()
                })
                .transpose()?;
            for path in paths.iter_mut() {
                let existing = path.gradient.take();
                let mut gradient = existing.unwrap_or_else(|| nova_core::Gradient {
                    kind: nova_core::GradientKind::Linear,
                    stops: vec![
                        nova_core::GradientStop {
                            position: 0.0,
                            color: path.fill,
                        },
                        nova_core::GradientStop {
                            position: 1.0,
                            color: Rgba::WHITE,
                        },
                    ],
                    angle: 0.0,
                    center_x: 0.0,
                    center_y: 0.0,
                    radius: 50.0,
                });
                if let Some(k) = kind {
                    gradient.kind = *k;
                }
                if let Some(a) = angle {
                    if !a.is_finite() {
                        return Err(OperationError::invalid("Gradientwinkel muss endlich sein."));
                    }
                    gradient.angle = *a;
                }
                for (field, value) in [
                    (&mut gradient.center_x, center_x),
                    (&mut gradient.center_y, center_y),
                    (&mut gradient.radius, radius),
                ] {
                    if let Some(v) = value {
                        if !v.is_finite() {
                            return Err(OperationError::invalid(
                                "Gradientwerte müssen endlich sein.",
                            ));
                        }
                        *field = *v;
                    }
                }
                if let Some(new_stops) = &parsed_stops {
                    if new_stops.len() < 2 {
                        return Err(OperationError::invalid(
                            "Ein Gradient benötigt mindestens zwei Stops.",
                        ));
                    }
                    for stop in new_stops {
                        if !(0.0..=1.0).contains(&stop.position) {
                            return Err(OperationError::invalid(
                                "Stop-Positionen müssen zwischen 0 und 1 liegen.",
                            ));
                        }
                    }
                    gradient.stops = new_stops.clone();
                }
                gradient
                    .stops
                    .sort_by(|a, b| a.position.total_cmp(&b.position));
                path.gradient = Some(gradient);
            }
            project.touch();
            Ok(format!("Verlauf auf '{name}' angewendet"))
        }
        ProjectOperation::BooleanPathOperation {
            target,
            with,
            operation,
            scene,
            symbol,
        } => {
            let timeline = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let (clip_paths, clip_transform) = {
                let other = find_object_mut_in_scene(timeline, with)?;
                let ObjectKind::Path { paths, .. } = &other.kind else {
                    return Err(OperationError::invalid(format!(
                        "Objekt '{with}' ist kein Pfadobjekt."
                    )));
                };
                (paths.clone(), other.transform)
            };
            let target_obj = find_object_mut_in_scene(timeline, target)?;
            let ObjectKind::Path { paths, .. } = &mut target_obj.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{target}' ist kein Pfadobjekt."
                )));
            };
            let target_transform = target_obj.transform;
            *paths = nova_core::boolean_path_objects(
                paths,
                &target_transform,
                &clip_paths,
                &clip_transform,
                *operation,
            );
            project.touch();
            Ok(format!(
                "Boolean-Operation '{}' auf '{target}' mit '{with}' angewendet",
                operation.label()
            ))
        }
        ProjectOperation::BindMeshToSkeleton { symbol, object } => {
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            let Some(skeleton) = symbol.skeleton.clone() else {
                return Err(OperationError::invalid(
                    "Das Symbol hat kein Skelett; lege Knochen mit add_bone an.",
                ));
            };
            let target = find_object_mut_in_scene(&mut symbol.timeline, object)?;
            let ObjectKind::Path { paths, .. } = &mut target.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{object}' ist kein Pfadobjekt."
                )));
            };
            let transform = target.transform;
            for path in paths.iter_mut() {
                let points = path.skinning_points();
                let weights = nova_core::auto_skin_weights(&skeleton, &transform, &points);
                path.skin = Some(nova_core::SkinData { weights });
            }
            project.touch();
            Ok(format!(
                "Mesh-Bindung für '{object}' in Symbol '{symbol_name}' erstellt (nächster Knochen pro Vertex)"
            ))
        }
        ProjectOperation::SetVertexWeight {
            symbol,
            object,
            path,
            vertex,
            bone,
            weight,
        } => {
            if !weight.is_finite() || *weight < 0.0 {
                return Err(OperationError::invalid(
                    "Gewicht muss eine nicht-negative endliche Zahl sein.",
                ));
            }
            let symbol = symbol_mut(project, symbol)?;
            let symbol_name = symbol.name.clone();
            let Some(skeleton) = symbol.skeleton.as_ref() else {
                return Err(OperationError::invalid("Das Symbol hat kein Skelett."));
            };
            let bone_id = skeleton
                .bones
                .iter()
                .find(|b| b.name == bone.trim())
                .map(|b| b.id)
                .ok_or_else(|| {
                    OperationError::invalid(format!("Knochen '{bone}' existiert nicht."))
                })?;
            let target = find_object_mut_in_scene(&mut symbol.timeline, object)?;
            let ObjectKind::Path { paths, .. } = &mut target.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{object}' ist kein Pfadobjekt."
                )));
            };
            let index = path.unwrap_or(0);
            let path_ref = paths.get_mut(index).ok_or_else(|| {
                OperationError::invalid(format!("Pfadindex {index} existiert nicht."))
            })?;
            let count = path_ref.skinning_points().len();
            if *vertex >= count {
                return Err(OperationError::invalid(format!(
                    "Vertex {vertex} existiert nicht (Pfad hat {count} Punkte)."
                )));
            }
            if path_ref.skin.is_none() {
                path_ref.skin = Some(nova_core::SkinData {
                    weights: vec![Vec::new(); count],
                });
            }
            let skin = path_ref.skin.as_mut().unwrap();
            if skin.weights.len() < count {
                skin.weights.resize(count, Vec::new());
            }
            let vertex_weights = &mut skin.weights[*vertex];
            vertex_weights.retain(|w| w.bone != bone_id);
            if *weight > 0.0 {
                vertex_weights.push(nova_core::SkinWeight {
                    bone: bone_id,
                    weight: *weight,
                });
            }
            nova_core::normalize_weights(vertex_weights);
            project.touch();
            Ok(format!(
                "Vertex-{vertex}-Gewicht für Knochen '{bone}' in '{object}' (Symbol '{symbol_name}') gesetzt"
            ))
        }
        ProjectOperation::InstallGameKit => {
            let before = project.scripts.len();
            nova_core::gamekit::install(project);
            let added = project.scripts.len() - before;
            project.touch();
            Ok(format!(
                "Spiel-Kit installiert ({added} Skript(e) hinzugefügt; vorhandene bleiben unverändert)"
            ))
        }
        ProjectOperation::SetSceneTransition {
            key,
            target,
            scene,
            remove,
        } => {
            let key = key.trim();
            if key.is_empty() || key.chars().count() > 40 {
                return Err(OperationError::invalid(
                    "Transition-Taste ist leer oder zu lang.",
                ));
            }
            let scene_names: Vec<String> = project.scenes.iter().map(|s| s.name.clone()).collect();
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            let mut config = scene.game.clone().unwrap_or_default();
            if *remove {
                let before = config.transitions.len();
                config.transitions.retain(|t| t.key != key);
                if config.transitions.len() == before {
                    return Err(OperationError::invalid(format!(
                        "Keine Transition für Taste '{key}' in Szene '{scene_name}'."
                    )));
                }
            } else {
                let target = target.trim();
                if target.is_empty() || target.chars().count() > 120 {
                    return Err(OperationError::invalid(
                        "Transition-Zielszene ist leer oder zu lang.",
                    ));
                }
                if !scene_names.iter().any(|name| name == target) {
                    return Err(OperationError::invalid(format!(
                        "Zielszene '{target}' existiert nicht."
                    )));
                }
                match config.transitions.iter_mut().find(|t| t.key == key) {
                    Some(existing) => existing.target = target.to_string(),
                    None => config.transitions.push(nova_core::SceneTransition {
                        key: key.to_string(),
                        target: target.to_string(),
                    }),
                }
            }
            scene.game = Some(config);
            project.touch();
            Ok(format!(
                "Szene-Transition '{key}' in Szene '{scene_name}' aktualisiert"
            ))
        }
        ProjectOperation::CreateTileset {
            name,
            asset,
            tile_width,
            tile_height,
            solid_tiles,
            tile_names,
            margin_x,
            margin_y,
            spacing_x,
            spacing_y,
        } => {
            let name = validate_asset_name(name)?;
            if project.tilesets.iter().any(|t| t.name == name) {
                return Err(OperationError::invalid(format!(
                    "Ein Tileset namens '{name}' existiert bereits."
                )));
            }
            let asset_ref = project
                .assets
                .iter()
                .find(|a| a.name == asset.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Asset '{}' existiert nicht.", asset.trim()))
                })?;
            if asset_ref.kind != AssetKind::Bitmap || !asset_ref.is_embedded() {
                return Err(OperationError::invalid(format!(
                    "Asset '{}' ist kein eingebettetes Bitmap.",
                    asset_ref.name
                )));
            }
            let (asset_id, asset_name, asset_w, asset_h) = (
                asset_ref.id,
                asset_ref.name.clone(),
                asset_ref.width,
                asset_ref.height,
            );
            if *tile_width == 0 || *tile_height == 0 || *tile_width > 512 || *tile_height > 512 {
                return Err(OperationError::invalid(
                    "Kachelgröße muss zwischen 1 und 512 px liegen.",
                ));
            }
            if *margin_x as u64 + *margin_y as u64 > u32::MAX as u64 {
                return Err(OperationError::invalid("Randwerte zu groß."));
            }
            let probe = nova_core::Tileset {
                tile_width: *tile_width,
                tile_height: *tile_height,
                margin_x: *margin_x,
                margin_y: *margin_y,
                spacing_x: *spacing_x,
                spacing_y: *spacing_y,
                columns: 0,
                ..Default::default()
            };
            let columns = probe.columns_for(asset_w);
            let tile_count = probe.tile_count(asset_w, asset_h);
            if tile_count == 0 {
                return Err(OperationError::invalid(format!(
                    "Asset '{}' ist zu klein für {}×{}-Kacheln (Rand/Abstand).",
                    asset_name, tile_width, tile_height
                )));
            }
            for index in solid_tiles {
                if *index >= tile_count {
                    return Err(OperationError::invalid(format!(
                        "Solid-Kachel {index} liegt außerhalb von 0..{}",
                        tile_count - 1
                    )));
                }
            }
            let mut solid = vec![false; tile_count];
            for index in solid_tiles {
                solid[*index] = true;
            }
            let mut names = vec![String::new(); tile_count];
            for (index, tile_name) in tile_names.iter().enumerate() {
                if index >= tile_count {
                    break;
                }
                names[index] = tile_name.trim().chars().take(80).collect();
            }
            project.tilesets.push(nova_core::Tileset {
                name: name.clone(),
                asset: asset_id,
                tile_width: *tile_width,
                tile_height: *tile_height,
                columns,
                margin_x: *margin_x,
                margin_y: *margin_y,
                spacing_x: *spacing_x,
                spacing_y: *spacing_y,
                solid,
                names,
                ..Default::default()
            });
            project.touch();
            Ok(format!(
                "Tileset '{name}' erstellt ({tile_count} Kacheln aus '{}')",
                asset_name
            ))
        }
        ProjectOperation::CreateTilemap {
            name,
            tileset,
            columns,
            rows,
            x,
            y,
            scene,
            layer,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Kachelkartenname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid("Position muss endlich sein."));
            }
            if *columns == 0 || *rows == 0 || *columns > 512 || *rows > 512 {
                return Err(OperationError::invalid(
                    "Raster muss zwischen 1×1 und 512×512 Kacheln liegen.",
                ));
            }
            let tileset = project
                .tilesets
                .iter()
                .find(|t| t.name == tileset.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!(
                        "Tileset '{}' existiert nicht (create_tileset).",
                        tileset.trim()
                    ))
                })?;
            let (tileset_id, tile_w, tile_h) = (
                tileset.id,
                tileset.tile_width as f32,
                tileset.tile_height as f32,
            );
            let map = nova_core::tilemap::new_tilemap(
                tileset_id,
                tile_w,
                tile_h,
                *columns,
                *rows,
                &["Ebene 1"],
            );
            let object = ObjectInstance::new(
                name,
                nova_core::ObjectKind::Tilemap(map),
                nova_core::Transform::at(*x, *y),
                Rgba::WHITE,
            );
            let layer_name =
                insert_object(project, scene.as_deref(), None, layer.as_deref(), object)?;
            project.touch();
            Ok(format!(
                "Kachelkarte '{name}' ({columns}×{rows}) in Ebene '{layer_name}' platziert"
            ))
        }
        ProjectOperation::ModifyTilemap {
            object,
            scene,
            columns,
            rows,
            tile_width,
            tile_height,
            tileset,
        } => {
            let tileset_id = if let Some(t) = tileset {
                Some(
                    project
                        .tilesets
                        .iter()
                        .find(|ts| ts.name == t.trim())
                        .ok_or_else(|| {
                            OperationError::invalid(format!(
                                "Tileset '{}' existiert nicht.",
                                t.trim()
                            ))
                        })?
                        .id,
                )
            } else {
                None
            };
            let scene = scene_mut(project, scene.as_deref())?;
            let needle = object.trim();
            let (li, fi, oi) = find_object_indices(scene, needle)?;
            let target = &mut scene.layers[li].frames[fi].objects[oi];
            let nova_core::ObjectKind::Tilemap(map) = &mut target.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{needle}' ist keine Kachelkarte."
                )));
            };
            if let Some(id) = tileset_id {
                map.tileset = id;
            }
            if let Some(w) = tile_width {
                if !w.is_finite() || *w <= 0.0 || *w > 512.0 {
                    return Err(OperationError::invalid("Kachelbreite ungültig."));
                }
                map.tile_width = *w;
            }
            if let Some(h) = tile_height {
                if !h.is_finite() || *h <= 0.0 || *h > 512.0 {
                    return Err(OperationError::invalid("Kachelhöhe ungültig."));
                }
                map.tile_height = *h;
            }
            if columns.is_some() || rows.is_some() {
                let new_cols = columns.unwrap_or(map.columns);
                let new_rows = rows.unwrap_or(map.rows);
                if new_cols == 0 || new_rows == 0 || new_cols > 1024 || new_rows > 1024 {
                    return Err(OperationError::invalid(
                        "Raster muss zwischen 1×1 und 1024×1024 liegen.",
                    ));
                }
                nova_core::tilemap::resize(map, new_cols, new_rows);
            }
            map.normalize();
            project.touch();
            Ok(format!("Kachelkarte '{needle}' aktualisiert"))
        }
        ProjectOperation::PaintTiles {
            object,
            layer,
            tool,
            x0,
            y0,
            x1,
            y1,
            tile,
            from,
            to,
            scene,
        } => {
            let scene = scene_mut(project, scene.as_deref())?;
            let needle = object.trim();
            let (li, fi, oi) = find_object_indices(scene, needle)?;
            let target = &mut scene.layers[li].frames[fi].objects[oi];
            let nova_core::ObjectKind::Tilemap(map) = &mut target.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{needle}' ist keine Kachelkarte."
                )));
            };
            if *layer >= map.layers.len() {
                return Err(OperationError::invalid(format!(
                    "Kachelebene {layer} existiert nicht (0..{}).",
                    map.layers.len().saturating_sub(1)
                )));
            }
            let changed = nova_core::tilemap::paint(
                map, *layer, *tool, *x0, *y0, *x1, *y1, *tile, *from, *to,
            )
            .map_err(OperationError::invalid)?;
            project.touch();
            Ok(format!(
                "{changed} Zellen auf '{needle}' Ebene {layer} gemalt"
            ))
        }
        ProjectOperation::AddTileLayer {
            object,
            name,
            scene,
        } => {
            let scene = scene_mut(project, scene.as_deref())?;
            let needle = object.trim();
            let (li, fi, oi) = find_object_indices(scene, needle)?;
            let target = &mut scene.layers[li].frames[fi].objects[oi];
            let nova_core::ObjectKind::Tilemap(map) = &mut target.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{needle}' ist keine Kachelkarte."
                )));
            };
            let layer_name = name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(|n| n.chars().take(80).collect::<String>())
                .unwrap_or_else(|| format!("Ebene {}", map.layers.len() + 1));
            let index = nova_core::tilemap::add_layer(map, &layer_name, None);
            project.touch();
            Ok(format!(
                "Kachelebene '{layer_name}' (#{index}) zu '{needle}' hinzugefügt"
            ))
        }
        ProjectOperation::ConfigureTileLayer {
            object,
            layer,
            name,
            visible,
            locked,
            opacity,
            scene,
        } => {
            let scene = scene_mut(project, scene.as_deref())?;
            let needle = object.trim();
            let (li, fi, oi) = find_object_indices(scene, needle)?;
            let target = &mut scene.layers[li].frames[fi].objects[oi];
            let nova_core::ObjectKind::Tilemap(map) = &mut target.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{needle}' ist keine Kachelkarte."
                )));
            };
            if *layer >= map.layers.len() {
                return Err(OperationError::invalid(format!(
                    "Kachelebene {layer} existiert nicht (0..{}).",
                    map.layers.len().saturating_sub(1)
                )));
            }
            let target_layer = &mut map.layers[*layer];
            if let Some(n) = name {
                let n = n.trim();
                if n.is_empty() || n.chars().count() > 80 {
                    return Err(OperationError::invalid(
                        "Kachelebenenname muss 1 bis 80 Zeichen enthalten.",
                    ));
                }
                target_layer.name = n.to_string();
            }
            if let Some(v) = visible {
                target_layer.visible = *v;
            }
            if let Some(l) = locked {
                target_layer.locked = *l;
            }
            if let Some(o) = opacity {
                if !o.is_finite() || !(0.0..=1.0).contains(o) {
                    return Err(OperationError::invalid(
                        "Opazität muss zwischen 0 und 1 liegen.",
                    ));
                }
                target_layer.opacity = *o;
            }
            project.touch();
            Ok(format!("Kachelebene {layer} von '{needle}' konfiguriert"))
        }
        ProjectOperation::FlipTileRegion {
            object,
            layer,
            x0,
            y0,
            x1,
            y1,
            flip_h,
            flip_v,
            scene,
        } => {
            if !flip_h && !flip_v {
                return Err(OperationError::invalid(
                    "Mindestens eine Spiegelachse wählen.",
                ));
            }
            let scene = scene_mut(project, scene.as_deref())?;
            let needle = object.trim();
            let (li, fi, oi) = find_object_indices(scene, needle)?;
            let target = &mut scene.layers[li].frames[fi].objects[oi];
            let nova_core::ObjectKind::Tilemap(map) = &mut target.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{needle}' ist keine Kachelkarte."
                )));
            };
            if *layer >= map.layers.len() {
                return Err(OperationError::invalid(format!(
                    "Kachelebene {layer} existiert nicht (0..{}).",
                    map.layers.len().saturating_sub(1)
                )));
            }
            if map.layers[*layer].locked {
                return Err(OperationError::invalid(format!(
                    "Kachelebene {layer} ist gesperrt."
                )));
            }
            nova_core::tilemap::flip_region(map, *layer, *x0, *y0, *x1, *y1, *flip_h, *flip_v);
            project.touch();
            Ok(format!(
                "Region ({x0},{y0})-({x1},{y1}) auf '{needle}' Ebene {layer} gespiegelt"
            ))
        }
        ProjectOperation::MoveTileRegion {
            object,
            layer,
            x0,
            y0,
            x1,
            y1,
            dx,
            dy,
            scene,
        } => {
            if *dx == 0 && *dy == 0 {
                return Err(OperationError::invalid("Verschiebung ist (0,0)."));
            }
            let scene = scene_mut(project, scene.as_deref())?;
            let needle = object.trim();
            let (li, fi, oi) = find_object_indices(scene, needle)?;
            let target = &mut scene.layers[li].frames[fi].objects[oi];
            let nova_core::ObjectKind::Tilemap(map) = &mut target.kind else {
                return Err(OperationError::invalid(format!(
                    "Objekt '{needle}' ist keine Kachelkarte."
                )));
            };
            if *layer >= map.layers.len() {
                return Err(OperationError::invalid(format!(
                    "Kachelebene {layer} existiert nicht (0..{}).",
                    map.layers.len().saturating_sub(1)
                )));
            }
            if map.layers[*layer].locked {
                return Err(OperationError::invalid(format!(
                    "Kachelebene {layer} ist gesperrt."
                )));
            }
            let Some(region) = nova_core::tilemap::copy_region(map, *layer, *x0, *y0, *x1, *y1)
            else {
                return Err(OperationError::invalid("Region ist leer oder ungültig."));
            };
            let (ax, ay, bx, by) = (
                (*x0).min(*x1),
                (*y0).min(*y1),
                (*x0).max(*x1),
                (*y0).max(*y1),
            );
            nova_core::tilemap::clear_region(map, *layer, ax, ay, bx, by);
            let dest_x = ax.saturating_add(*dx);
            let dest_y = ay.saturating_add(*dy);
            nova_core::tilemap::paste_region(map, *layer, dest_x, dest_y, &region);
            project.touch();
            Ok(format!(
                "Region ({ax},{ay})-({bx},{by}) auf '{needle}' Ebene {layer} um ({dx},{dy}) verschoben"
            ))
        }
        ProjectOperation::CreateCollisionShape {
            object,
            scene,
            shape,
            width,
            height,
            layer: col_layer,
            mask,
            remove,
        } => {
            let scene = scene_mut(project, scene.as_deref())?;
            let needle = object.trim();
            let (li, fi, oi) = find_object_indices(scene, needle)?;
            let target = &mut scene.layers[li].frames[fi].objects[oi];
            if *remove {
                if let Some(game) = target.game.as_mut() {
                    game.collision = None;
                }
                project.touch();
                return Ok(format!("Kollision von '{needle}' entfernt"));
            }
            // Validate polygon geometry: >=3 points, finite, sane extents.
            if let nova_core::CollisionShape::Polygon { points } = shape {
                if points.len() < 3 || points.len() > 64 {
                    return Err(OperationError::invalid("Polygon braucht 3 bis 64 Punkte."));
                }
                let mut min_x = f32::INFINITY;
                let mut min_y = f32::INFINITY;
                let mut max_x = f32::NEG_INFINITY;
                let mut max_y = f32::NEG_INFINITY;
                for (px, py) in points {
                    if !px.is_finite()
                        || !py.is_finite()
                        || px.abs() > 100_000.0
                        || py.abs() > 100_000.0
                    {
                        return Err(OperationError::invalid(
                            "Polygonpunkte müssen endliche Koordinaten < 100000 haben.",
                        ));
                    }
                    min_x = min_x.min(*px);
                    min_y = min_y.min(*py);
                    max_x = max_x.max(*px);
                    max_y = max_y.max(*py);
                }
                if max_x - min_x < 0.5 || max_y - min_y < 0.5 {
                    return Err(OperationError::invalid(
                        "Polygon ist degeneriert (Bounding-Box zu klein).",
                    ));
                }
            }
            let w = width.unwrap_or_else(|| match shape {
                nova_core::CollisionShape::Polygon { points } => {
                    let xs: Vec<f32> = points.iter().map(|p| p.0).collect();
                    xs.iter().fold(0.0f32, |a, b| a.max(*b))
                        - xs.iter().fold(0.0f32, |a, b| a.min(*b))
                }
                _ => 32.0,
            });
            let h = height.unwrap_or_else(|| match shape {
                nova_core::CollisionShape::Polygon { points } => {
                    let ys: Vec<f32> = points.iter().map(|p| p.1).collect();
                    ys.iter().fold(0.0f32, |a, b| a.max(*b))
                        - ys.iter().fold(0.0f32, |a, b| a.min(*b))
                }
                _ => 32.0,
            });
            if !w.is_finite()
                || !h.is_finite()
                || w <= 0.0
                || h <= 0.0
                || w > 100_000.0
                || h > 100_000.0
            {
                return Err(OperationError::invalid(
                    "Kollisionsausmaße müssen positive endliche Zahlen sein.",
                ));
            }
            if let Some(layer) = col_layer {
                if *layer == 0 || *layer > 31 {
                    return Err(OperationError::invalid(
                        "Kollisions-Layer muss 1..=31 sein.",
                    ));
                }
            }
            let mut config = target.game.clone().unwrap_or_default();
            config.collision = Some(nova_core::CollisionSpec {
                shape: shape.clone(),
                width: w,
                height: h,
                layer: col_layer.unwrap_or(1),
                mask: mask.unwrap_or(u32::MAX),
            });
            target.game = Some(config);
            project.touch();
            Ok(format!("Kollisionsform für '{needle}' gesetzt"))
        }
        ProjectOperation::ModifyCamera {
            scene,
            enabled,
            follow,
            left,
            top,
            right,
            bottom,
            zoom,
            smoothing,
            dead_zone,
            shake,
        } => {
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            let mut config = scene.game.clone().unwrap_or_default();
            let camera = config
                .camera
                .get_or_insert_with(nova_core::CameraConfig::default);
            if let Some(v) = enabled {
                camera.enabled = *v;
            }
            if let Some(v) = follow {
                let v = v.trim();
                if v.chars().count() > 120 {
                    return Err(OperationError::invalid("Follow-Name zu lang."));
                }
                camera.follow = v.to_string();
            }
            for (value, field) in [
                (left, &mut camera.left),
                (top, &mut camera.top),
                (right, &mut camera.right),
                (bottom, &mut camera.bottom),
                (zoom, &mut camera.zoom),
                (shake, &mut camera.shake),
            ] {
                if let Some(v) = value {
                    if !v.is_finite() {
                        return Err(OperationError::invalid("Kamerawert muss endlich sein."));
                    }
                    *field = *v;
                }
            }
            if let Some(v) = smoothing {
                if !v.is_finite() || !(0.0..=1.0).contains(v) {
                    return Err(OperationError::invalid("Smoothing muss 0..=1 sein."));
                }
                camera.follow_smoothing = *v;
            }
            if let Some((dx, dy)) = dead_zone {
                if !dx.is_finite() || !dy.is_finite() || *dx < 0.0 || *dy < 0.0 {
                    return Err(OperationError::invalid("Dead-Zone muss >= 0 sein."));
                }
                camera.dead_zone = (*dx, *dy);
            }
            if let Some(z) = zoom {
                if *z <= 0.0 || *z > 16.0 {
                    return Err(OperationError::invalid("Zoom muss 0 < z <= 16 sein."));
                }
            }
            scene.game = Some(config);
            project.touch();
            Ok(format!("Kamera in Szene '{scene_name}' aktualisiert"))
        }
        ProjectOperation::CreateParticleEmitter {
            object,
            scene,
            preset,
            rate,
            burst,
            lifetime,
            speed,
            direction,
            spread,
            gravity,
            drag,
            max_particles,
            space,
            remove,
        } => apply_particle_op(
            project,
            scene.as_deref(),
            object,
            *remove,
            *preset,
            ParticlePatch {
                rate: *rate,
                burst: *burst,
                lifetime: *lifetime,
                speed: *speed,
                direction: *direction,
                spread: *spread,
                gravity: *gravity,
                drag: *drag,
                max_particles: *max_particles,
                space: *space,
            },
            true,
        ),
        ProjectOperation::ConfigureParticleEmitter {
            object,
            scene,
            preset,
            rate,
            burst,
            lifetime,
            speed,
            direction,
            spread,
            gravity,
            drag,
            max_particles,
            space,
        } => apply_particle_op(
            project,
            scene.as_deref(),
            object,
            false,
            *preset,
            ParticlePatch {
                rate: *rate,
                burst: *burst,
                lifetime: *lifetime,
                speed: *speed,
                direction: *direction,
                spread: *spread,
                gravity: *gravity,
                drag: *drag,
                max_particles: *max_particles,
                space: *space,
            },
            false,
        ),
        ProjectOperation::CreateLevelGroup { name, scene } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Gruppenname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            let scene = scene_mut(project, scene.as_deref())?;
            if scene.layers.iter().any(|l| l.name == name) {
                return Err(OperationError::invalid(format!(
                    "Eine Ebene namens '{name}' existiert bereits."
                )));
            }
            let frame_count = scene.frame_count();
            let mut layer = nova_core::Layer::new(name, frame_count);
            layer.kind = nova_core::LayerKind::Folder;
            scene.layers.push(layer);
            project.touch();
            Ok(format!("Gruppen-Ebene '{name}' erstellt"))
        }
        ProjectOperation::PlaceGameObject {
            name,
            x,
            y,
            width,
            height,
            fill,
            scene,
            layer,
            collision,
            physics,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Objektname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            if !x.is_finite() || !y.is_finite() || !width.is_finite() || !height.is_finite() {
                return Err(OperationError::invalid(
                    "Position und Größe müssen endlich sein.",
                ));
            }
            if *width <= 0.0 || *height <= 0.0 || *width > 100_000.0 || *height > 100_000.0 {
                return Err(OperationError::invalid(
                    "Größe muss positiv und <= 100000 sein.",
                ));
            }
            let rgba = match fill.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
                Some(hex) => parse_color(hex)?,
                None => Rgba::WHITE,
            };
            let mut object = ObjectInstance::rect(name, *x, *y, *width, *height, rgba);
            if collision.is_some() || physics.is_some() {
                object.game = Some(nova_core::GameObjectConfig {
                    collision: collision.clone(),
                    physics: *physics,
                });
            }
            let layer_name =
                insert_object(project, scene.as_deref(), None, layer.as_deref(), object)?;
            project.touch();
            Ok(format!(
                "Game-Objekt '{name}' in Ebene '{layer_name}' platziert"
            ))
        }
        ProjectOperation::ImportProjectAsset { name, path } => {
            let ext = path
                .trim()
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let derived = name.clone().unwrap_or_else(|| {
                path.trim()
                    .rsplit('/')
                    .next()
                    .unwrap_or("asset")
                    .to_string()
            });
            let result = match ext.as_str() {
                "png" | "jpg" | "jpeg" | "gif" => apply_operations_with(
                    project,
                    &[ProjectOperation::ImportBitmapAsset {
                        name: derived,
                        path: path.clone(),
                    }],
                    read_asset,
                ),
                "wav" | "mp3" => apply_operations_with(
                    project,
                    &[ProjectOperation::ImportAudioAsset {
                        name: derived,
                        path: path.clone(),
                    }],
                    read_asset,
                ),
                "ogg" | "flac" => import_audio_asset_op(project, derived, path.clone(), read_asset)
                    .map(|_| vec!["Audio-Asset importiert".to_string()]),
                _ => Err(OperationError::invalid(format!(
                    "Dateityp '.{ext}' wird nicht unterstützt (png/jpg/jpeg/gif/wav/mp3/ogg/flac)."
                ))),
            }?;
            project.touch();
            Ok(result.join("; "))
        }
        ProjectOperation::CreateGameLevel {
            scene,
            world_width,
            world_height,
            spawn,
            tileset,
            tilemap,
            objects,
            camera_follow,
        } => apply_game_level(
            project,
            scene,
            *world_width,
            *world_height,
            *spawn,
            tileset.as_ref(),
            tilemap.as_ref(),
            objects,
            camera_follow.as_deref(),
        )
        .map(|notes| notes.join("; ")),
    }
}

/// Optional overrides for [`apply_particle_op`].
#[derive(Default)]
struct ParticlePatch {
    rate: Option<f32>,
    burst: Option<u32>,
    lifetime: Option<(f32, f32)>,
    speed: Option<(f32, f32)>,
    direction: Option<f32>,
    spread: Option<f32>,
    gravity: Option<f32>,
    drag: Option<f32>,
    max_particles: Option<u32>,
    space: Option<nova_core::ParticleSpace>,
}

fn validate_particle_patch(patch: &ParticlePatch) -> Result<(), OperationError> {
    for (value, lo, hi) in [
        patch.rate.map(|v| (v, 0.0, 10_000.0)),
        patch.direction.map(|v| (v, -3600.0, 3600.0)),
        patch.spread.map(|v| (v, 0.0, 360.0)),
        patch.gravity.map(|v| (v, -100_000.0, 100_000.0)),
        patch.drag.map(|v| (v, 0.0, 100.0)),
    ]
    .into_iter()
    .flatten()
    {
        if !value.is_finite() || value < lo || value > hi {
            return Err(OperationError::invalid("Partikelparameter ungueltig."));
        }
    }
    if let Some((a, b)) = patch.lifetime {
        if !a.is_finite() || !b.is_finite() || a <= 0.0 || b < a || b > 3600.0 {
            return Err(OperationError::invalid("Partikel-Lebensdauer ungueltig."));
        }
    }
    if let Some((a, b)) = patch.speed {
        if !a.is_finite() || !b.is_finite() || a < 0.0 || b < a || b > 100_000.0 {
            return Err(OperationError::invalid(
                "Partikel-Geschwindigkeit ungueltig.",
            ));
        }
    }
    if let Some(max) = patch.max_particles {
        if max == 0 || max > 100_000 {
            return Err(OperationError::invalid(
                "max_particles muss 1..=100000 sein.",
            ));
        }
    }
    Ok(())
}

fn apply_particle_patch(emitter: &mut nova_core::ParticleEmitter, patch: &ParticlePatch) {
    if let Some(v) = patch.rate {
        emitter.rate = v;
    }
    if let Some(v) = patch.burst {
        emitter.burst = v;
    }
    if let Some(v) = patch.lifetime {
        emitter.lifetime = v;
    }
    if let Some(v) = patch.speed {
        emitter.speed = v;
    }
    if let Some(v) = patch.direction {
        emitter.direction = v;
    }
    if let Some(v) = patch.spread {
        emitter.spread = v;
    }
    if let Some(v) = patch.gravity {
        emitter.gravity = v;
    }
    if let Some(v) = patch.drag {
        emitter.drag = v;
    }
    if let Some(v) = patch.max_particles {
        emitter.max_particles = v;
    }
    if let Some(v) = patch.space {
        emitter.space = v;
    }
}

fn apply_particle_op(
    project: &mut Project,
    scene_sel: Option<&str>,
    object_name: &str,
    remove: bool,
    preset: Option<nova_core::ParticlePreset>,
    patch: ParticlePatch,
    allow_create: bool,
) -> Result<String, OperationError> {
    validate_particle_patch(&patch)?;
    let scene = scene_mut(project, scene_sel)?;
    let needle = object_name.trim();
    let (li, fi, oi) = find_object_indices(scene, needle)?;
    let target = &mut scene.layers[li].frames[fi].objects[oi];
    if remove {
        target.particles = None;
        project.touch();
        return Ok(format!("Partikelemitter von '{needle}' entfernt"));
    }
    if target.particles.is_none() && !allow_create {
        return Err(OperationError::invalid(format!(
            "Objekt '{needle}' hat keinen Partikelemitter (create_particle_emitter)."
        )));
    }
    let mut emitter = target.particles.take().unwrap_or_else(|| {
        preset
            .map(nova_core::ParticleEmitter::preset)
            .unwrap_or_default()
    });
    if let Some(p) = preset {
        // Switching preset re-seeds tuned defaults, then applies overrides.
        let fresh = nova_core::ParticleEmitter::preset(p);
        if emitter.preset != p {
            emitter = fresh;
        }
    }
    apply_particle_patch(&mut emitter, &patch);
    target.particles = Some(emitter);
    project.touch();
    Ok(format!("Partikelemitter für '{needle}' konfiguriert"))
}

/// Finds the single object named `needle` across all frames/layers.
fn find_object_indices(
    scene: &nova_core::Scene,
    needle: &str,
) -> Result<(usize, usize, usize), OperationError> {
    let mut matches = Vec::new();
    for (layer_index, layer) in scene.layers.iter().enumerate() {
        for (frame_index, frame) in layer.frames.iter().enumerate() {
            for (object_index, candidate) in frame.objects.iter().enumerate() {
                if candidate.name == needle {
                    matches.push((layer_index, frame_index, object_index));
                }
            }
        }
    }
    match matches.len() {
        0 => Err(OperationError::invalid(format!(
            "Objekt '{needle}' existiert nicht."
        ))),
        1 => Ok(matches[0]),
        count => Err(OperationError::invalid(format!(
            "Objektname '{needle}' ist {count}-mal vorhanden."
        ))),
    }
}

/// Imports an audio asset without going through the enum dispatch
/// (used for ogg/flac which the typed ops don't cover yet).
fn import_audio_asset_op(
    project: &mut Project,
    name: String,
    path: String,
    read_asset: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(), OperationError> {
    let name = validate_asset_name(&name)?;
    if project.assets.iter().any(|a| a.name == name) {
        return Err(OperationError::invalid(format!(
            "Ein Asset namens '{name}' existiert bereits."
        )));
    }
    let path = validate_asset_path(&path, &["ogg", "flac"])?;
    let data = read_asset(&path)
        .map_err(|error| OperationError::invalid(format!("Asset '{path}': {error}")))?;
    if data.is_empty() || data.len() > 256 * 1024 * 1024 {
        return Err(OperationError::invalid(format!(
            "Asset '{path}' ist leer oder überschreitet 256 MB."
        )));
    }
    let mime = if path.ends_with(".ogg") {
        "audio/ogg"
    } else {
        "audio/flac"
    };
    project.assets.push(nova_core::Asset::audio(
        name,
        path.clone(),
        mime,
        0,
        0,
        0.0,
        data,
    ));
    project.touch();
    Ok(())
}

/// Applies the composite [`ProjectOperation::CreateGameLevel`].
fn apply_game_level(
    project: &mut Project,
    scene_name: &str,
    world_width: Option<f32>,
    world_height: Option<f32>,
    spawn: Option<(f32, f32)>,
    tileset: Option<&LevelTileset>,
    tilemap: Option<&LevelTilemap>,
    objects: &[LevelObject],
    camera_follow: Option<&str>,
) -> Result<Vec<String>, OperationError> {
    let mut notes: Vec<String> = Vec::new();
    // 1. Scene: create when missing.
    if !project.scenes.iter().any(|s| s.name == scene_name.trim()) {
        apply_operations(
            project,
            &[ProjectOperation::AddScene {
                name: scene_name.trim().to_string(),
            }],
        )?;
        notes.push("Szene erstellt".to_string());
    }
    let scene_sel = Some(scene_name.trim().to_string());
    // 2. World bounds + spawn.
    if world_width.is_some() || world_height.is_some() || spawn.is_some() {
        let scene_for_world = scene_mut(project, scene_sel.as_deref())?;
        let mut config = scene_for_world.game.clone().unwrap_or_default();
        let mut world_cfg = config.world.clone().unwrap_or_default();
        if let Some(w) = world_width {
            world_cfg.width = w;
        }
        if let Some(h) = world_height {
            world_cfg.height = h;
        }
        if let Some((sx, sy)) = spawn {
            world_cfg.spawn = (sx, sy);
        }
        config.world = Some(world_cfg);
        scene_for_world.game = Some(config);
        project.touch();
        notes.push("Weltgrenzen gesetzt".to_string());
    }
    // 3. Tileset.
    if let Some(ts) = tileset {
        apply_operations(
            project,
            &[ProjectOperation::CreateTileset {
                name: ts.name.clone(),
                asset: ts.asset.clone(),
                tile_width: ts.tile_width,
                tile_height: ts.tile_height,
                solid_tiles: ts.solid_tiles.clone(),
                tile_names: Vec::new(),
                margin_x: 0,
                margin_y: 0,
                spacing_x: 0,
                spacing_y: 0,
            }],
        )?;
        notes.push(format!("Tileset '{}' erstellt", ts.name));
    }
    // 4. Tilemap + strokes.
    if let Some(tm) = tilemap {
        apply_operations(
            project,
            &[ProjectOperation::CreateTilemap {
                name: tm.name.clone(),
                tileset: tm.tileset.clone(),
                columns: tm.columns,
                rows: tm.rows,
                x: tm.x,
                y: tm.y,
                scene: scene_sel.clone(),
                layer: None,
            }],
        )?;
        for stroke in &tm.strokes {
            apply_operations(
                project,
                &[ProjectOperation::PaintTiles {
                    object: tm.name.clone(),
                    layer: stroke.layer,
                    tool: stroke.tool,
                    x0: stroke.x0,
                    y0: stroke.y0,
                    x1: stroke.x1,
                    y1: stroke.y1,
                    tile: stroke.tile,
                    from: 0,
                    to: 0,
                    scene: scene_sel.clone(),
                }],
            )?;
        }
        notes.push(format!(
            "Kachelkarte '{}' mit {} Strichen",
            tm.name,
            tm.strokes.len()
        ));
    }
    // 5. Objects.
    for object in objects {
        apply_operations(
            project,
            &[ProjectOperation::PlaceGameObject {
                name: object.name.clone(),
                x: object.x,
                y: object.y,
                width: object.width,
                height: object.height,
                fill: object.fill.clone(),
                scene: scene_sel.clone(),
                layer: None,
                collision: object.collision.clone(),
                physics: object.physics,
            }],
        )?;
        if let Some(preset) = object.particle_preset {
            let _ = apply_operations(
                project,
                &[ProjectOperation::CreateParticleEmitter {
                    object: object.name.clone(),
                    scene: scene_sel.clone(),
                    preset: Some(preset),
                    rate: None,
                    burst: None,
                    lifetime: None,
                    speed: None,
                    direction: None,
                    spread: None,
                    gravity: None,
                    drag: None,
                    max_particles: None,
                    space: None,
                    remove: false,
                }],
            );
        }
    }
    if !objects.is_empty() {
        notes.push(format!("{} Objekte platziert", objects.len()));
    }
    // 6. Camera follow.
    if let Some(follow) = camera_follow {
        apply_operations(
            project,
            &[ProjectOperation::ModifyCamera {
                scene: scene_sel.clone(),
                enabled: Some(true),
                follow: Some(follow.to_string()),
                left: None,
                top: None,
                right: None,
                bottom: None,
                zoom: None,
                smoothing: None,
                dead_zone: None,
                shake: None,
            }],
        )?;
        notes.push(format!("Kamera folgt '{follow}'"));
    }
    notes.push("Spiel-Update".to_string());
    Ok(notes)
}

fn insert_object(
    project: &mut Project,
    scene: Option<&str>,
    symbol: Option<&str>,
    layer: Option<&str>,
    object: ObjectInstance,
) -> Result<String, OperationError> {
    let scene = timeline_mut(project, scene, symbol)?;
    let layer_name = layer.map(str::trim).filter(|name| !name.is_empty());
    let target = match layer_name {
        Some(name) => scene
            .layers
            .iter_mut()
            .find(|layer| layer.name == name)
            .ok_or_else(|| {
                OperationError::invalid(format!(
                    "Ebene '{name}' existiert in Szene '{}' nicht.",
                    scene.name
                ))
            })?,
        None => scene.layers.first_mut().ok_or_else(|| {
            OperationError::invalid("Die Szene enthält keine Ebene für das Objekt.")
        })?,
    };
    let layer_name = target.name.clone();
    let Some(frame) = target.frames.first_mut() else {
        return Err(OperationError::invalid(format!(
            "Ebene '{layer_name}' enthält keinen Frame."
        )));
    };
    frame.keyframe = true;
    frame.objects.push(object);
    project.touch();
    Ok(layer_name)
}

fn find_layer_mut<'a>(
    scene: &'a mut nova_core::Scene,
    name: &str,
) -> Result<&'a mut nova_core::Layer, OperationError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OperationError::invalid("Ebenenname ist leer."));
    }
    let scene_name = scene.name.clone();
    scene
        .layers
        .iter_mut()
        .find(|layer| layer.name == name)
        .ok_or_else(|| {
            OperationError::invalid(format!(
                "Ebene '{name}' existiert in Szene '{scene_name}' nicht."
            ))
        })
}

fn checked_frame(frame: usize) -> Result<usize, OperationError> {
    if frame == 0 {
        return Err(OperationError::invalid(
            "Frame-Nummern sind 1-basiert; 0 ist ungültig.",
        ));
    }
    if frame > 100_000 {
        return Err(OperationError::invalid(
            "Frame-Nummer ist auf 100000 begrenzt.",
        ));
    }
    Ok(frame - 1)
}

fn validate_label(label: &str) -> Result<(), OperationError> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 64 {
        return Err(OperationError::invalid(
            "Frame-Label muss 1 bis 64 Zeichen enthalten.",
        ));
    }
    if label.chars().any(char::is_control) {
        return Err(OperationError::invalid(
            "Frame-Label darf keine Steuerzeichen enthalten.",
        ));
    }
    Ok(())
}

/// Resolves the edit target: a symbol timeline when `symbol` names a library
/// symbol, otherwise a scene (or the active scene when `scene` is empty).
fn timeline_mut<'a>(
    project: &'a mut Project,
    scene: Option<&str>,
    symbol: Option<&str>,
) -> Result<&'a mut nova_core::Scene, OperationError> {
    let symbol = symbol.map(str::trim).filter(|name| !name.is_empty());
    if let Some(name) = symbol {
        if scene.is_some() {
            return Err(OperationError::invalid(
                "Gib entweder eine Szene oder ein Symbol an, nicht beides.",
            ));
        }
        return project
            .symbols
            .iter_mut()
            .find(|candidate| candidate.name == name)
            .map(|symbol| &mut symbol.timeline)
            .ok_or_else(|| OperationError::invalid(format!("Symbol '{name}' existiert nicht.")));
    }
    scene_mut(project, scene)
}

/// Resolves a library symbol by name for skeleton operations.
fn symbol_mut<'a>(project: &'a mut Project, name: &str) -> Result<&'a mut Symbol, OperationError> {
    let needle = name.trim();
    project
        .symbols
        .iter_mut()
        .find(|candidate| candidate.name == needle)
        .ok_or_else(|| OperationError::invalid(format!("Symbol '{needle}' existiert nicht.")))
}

/// Finds the single object named `needle` inside a (symbol) timeline and
/// returns a mutable reference to it.
fn find_object_mut_in_scene<'a>(
    scene: &'a mut nova_core::Scene,
    needle: &str,
) -> Result<&'a mut ObjectInstance, OperationError> {
    let name = needle.trim();
    let mut found: Option<&mut ObjectInstance> = None;
    for layer in &mut scene.layers {
        for frame in &mut layer.frames {
            for object in &mut frame.objects {
                if object.name == name {
                    if found.is_some() {
                        return Err(OperationError::invalid(format!(
                            "Objektname '{name}' ist nicht eindeutig."
                        )));
                    }
                    found = Some(object);
                }
            }
        }
    }
    found.ok_or_else(|| OperationError::invalid(format!("Objekt '{name}' existiert nicht.")))
}

fn scene_mut<'a>(
    project: &'a mut Project,
    selector: Option<&str>,
) -> Result<&'a mut nova_core::Scene, OperationError> {
    let selector = selector.map(str::trim).filter(|name| !name.is_empty());
    match selector {
        Some(name) => project
            .scenes
            .iter_mut()
            .find(|scene| scene.name == name)
            .ok_or_else(|| OperationError::invalid(format!("Szene '{name}' existiert nicht."))),
        None => project
            .active_scene_mut()
            .ok_or_else(|| OperationError::invalid("Das Projekt enthält keine aktive Szene.")),
    }
}

fn check_size(width: f32, height: f32, label: &str) -> Result<(), OperationError> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(OperationError::invalid(format!(
            "{label}breite und -höhe müssen positive endliche Zahlen sein."
        )));
    }
    if width > 100_000.0 || height > 100_000.0 {
        return Err(OperationError::invalid(format!(
            "{label}abmessungen sind auf 100000 begrenzt."
        )));
    }
    Ok(())
}

fn validate_asset_name(name: &str) -> Result<String, OperationError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(OperationError::invalid(
            "Assetname muss 1 bis 120 Zeichen enthalten.",
        ));
    }
    if name
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(OperationError::invalid(
            "Assetname darf keine Pfad- oder Steuerzeichen enthalten.",
        ));
    }
    Ok(name.to_string())
}

/// Validates a workspace-relative asset path: relative, no traversal, no
/// drive/UNC components, whitelisted extension, bounded length.
fn validate_asset_path(path: &str, extensions: &[&str]) -> Result<String, OperationError> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() || path.chars().count() > 512 {
        return Err(OperationError::invalid("Assetpfad ist leer oder zu lang."));
    }
    if path.starts_with('/')
        || path.starts_with("//")
        || path.contains(':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == ".." || part == ".")
    {
        return Err(OperationError::invalid(format!(
            "Unsicherer Assetpfad '{path}'."
        )));
    }
    let extension = path.rsplit('.').next().unwrap_or_default();
    if !extensions.contains(&extension.to_ascii_lowercase().as_str()) {
        return Err(OperationError::invalid(format!(
            "Assettyp wird nicht unterstützt (erlaubt: {}).",
            extensions.join(", ")
        )));
    }
    Ok(path)
}

/// Extracts `(sample_rate, channels, duration_seconds)` from PCM WAV data.
fn wav_info(data: &[u8]) -> Option<(u32, u16, f32)> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12usize;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut data_size = 0u32;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;
        let body = offset + 8;
        if body + size > data.len() {
            break;
        }
        if id == b"fmt " && size >= 16 {
            let audio_format = u16::from_le_bytes([data[body], data[body + 1]]);
            channels = u16::from_le_bytes([data[body + 2], data[body + 3]]);
            sample_rate = u32::from_le_bytes([
                data[body + 4],
                data[body + 5],
                data[body + 6],
                data[body + 7],
            ]);
            let bits = u16::from_le_bytes([data[body + 14], data[body + 15]]);
            if audio_format != 1 || bits != 16 || channels == 0 || sample_rate == 0 {
                return None;
            }
        } else if id == b"data" {
            data_size = size as u32;
        }
        offset = body + size + (size & 1);
    }
    if sample_rate == 0 || data_size == 0 {
        return None;
    }
    let bytes_per_second = sample_rate as f32 * channels as f32 * 2.0;
    Some((sample_rate, channels, data_size as f32 / bytes_per_second))
}

fn parse_color(value: &str) -> Result<Rgba, OperationError> {
    Rgba::from_hex(value.trim()).ok_or_else(|| {
        OperationError::invalid(format!(
            "Ungültige Farbe '{value}' (erwartet #rrggbb oder #rrggbbaa)."
        ))
    })
}

fn normalize_script_path(path: &str) -> Result<String, OperationError> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == ".." || part == ".")
    {
        return Err(OperationError::invalid(format!(
            "Unsicherer Skriptpfad '{path}'."
        )));
    }
    if !path.starts_with("scripts/") {
        return Err(OperationError::invalid(
            "Neue Skripte müssen unter 'scripts/' liegen.",
        ));
    }
    if !path.ends_with(".as") {
        return Err(OperationError::invalid(
            "Skriptpfade müssen mit '.as' enden.",
        ));
    }
    if path.chars().count() > 240 {
        return Err(OperationError::invalid(
            "Skriptpfad ist auf 240 Zeichen begrenzt.",
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_batch_is_transactional_and_previews_summaries() {
        let mut project = Project::new_empty("Start");
        let operations = vec![
            ProjectOperation::RenameProject { name: "Neu".into() },
            ProjectOperation::AddLayer {
                name: "Hintergrund".into(),
                scene: None,
                symbol: None,
                frames: Some(4),
            },
            ProjectOperation::AddObject {
                name: "Held".into(),
                scene: None,
                symbol: None,
                layer: Some("Hintergrund".into()),
                shape: ObjectShape::Rect,
                x: 10.0,
                y: 20.0,
                width: 32.0,
                height: 48.0,
                fill: Some("#ff8800".into()),
                text: String::new(),
                font_size: None,
            },
        ];
        let plan = preview_operations(&project, &operations);
        assert!(plan.errors.is_empty(), "{:?}", plan.errors);
        assert_eq!(plan.summaries.len(), 3);
        assert_eq!(project.metadata.name, "Start", "preview mutated project");

        let summaries = apply_operations(&mut project, &operations).unwrap();
        assert_eq!(summaries.len(), 3);
        assert_eq!(project.metadata.name, "Neu");
        let scene = project.active_scene().unwrap();
        let layer = scene
            .layers
            .iter()
            .find(|layer| layer.name == "Hintergrund")
            .unwrap();
        assert_eq!(layer.frames.len(), 4);
        assert_eq!(layer.frames[0].objects[0].name, "Held");
    }

    #[test]
    fn invalid_late_operation_leaves_project_untouched() {
        let mut project = Project::demo();
        let before = project.clone();
        let operations = vec![
            ProjectOperation::AddLayer {
                name: "Neu".into(),
                scene: None,
                symbol: None,
                frames: Some(1),
            },
            ProjectOperation::AddObject {
                name: "Fehlt".into(),
                scene: None,
                symbol: None,
                layer: Some("Nicht vorhanden".into()),
                shape: ObjectShape::Ellipse,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                fill: None,
                text: String::new(),
                font_size: None,
            },
        ];
        let error = apply_operations(&mut project, &operations).unwrap_err();
        assert!(error.to_string().contains("Nicht vorhanden"));
        assert_eq!(project, before);
    }

    #[test]
    fn symbols_scripts_and_text_objects_are_real_model_data() {
        let mut project = Project::new_empty("M8");
        let operations = vec![
            ProjectOperation::AddSymbol {
                name: "Kreis".into(),
                width: 40.0,
                height: 40.0,
                fill: Some("#00ff00".into()),
                corner_radius: Some(20.0),
                kind: None,
            },
            ProjectOperation::PlaceSymbol {
                symbol: "Kreis".into(),
                name: "KreisInstanz".into(),
                x: 100.0,
                y: 80.0,
                scene: None,
                symbol_target: None,
                layer: None,
                fill: None,
            },
            ProjectOperation::AddObject {
                name: "Titel".into(),
                scene: None,
                symbol: None,
                layer: None,
                shape: ObjectShape::Text,
                x: 20.0,
                y: 30.0,
                width: 200.0,
                height: 40.0,
                fill: None,
                text: "Hallo NØVA".into(),
                font_size: Some(32.0),
            },
            ProjectOperation::AddScript {
                path: "scripts/Added.as".into(),
                source: "class Added {}\n".into(),
            },
            ProjectOperation::SetStage {
                width: 800.0,
                height: 600.0,
                fps: Some(30.0),
                background: Some("#101010".into()),
                scene: None,
            },
        ];
        apply_operations(&mut project, &operations).unwrap();
        assert_eq!(project.symbols.len(), 1);
        assert!(
            project
                .scripts
                .iter()
                .any(|script| script.path == "scripts/Added.as")
        );
        let scene = project.active_scene().unwrap();
        assert_eq!((scene.width, scene.height, scene.fps), (800.0, 600.0, 30.0));
        let objects = &scene.layers[0].frames[0].objects;
        assert!(
            objects
                .iter()
                .any(|object| matches!(object.kind, nova_core::ObjectKind::Symbol { .. }))
        );
        assert!(
            objects
                .iter()
                .any(|object| matches!(object.kind, nova_core::ObjectKind::Text(_)))
        );
    }

    #[test]
    fn animation_operations_create_labeled_keyframes_and_tweens() {
        let mut project = Project::new_empty("Animation");
        let operations = vec![
            ProjectOperation::AddLayer {
                name: "Anim".into(),
                scene: None,
                symbol: None,
                frames: Some(1),
            },
            ProjectOperation::AddObject {
                name: "Box".into(),
                scene: None,
                symbol: None,
                layer: Some("Anim".into()),
                shape: ObjectShape::Rect,
                x: 10.0,
                y: 10.0,
                width: 40.0,
                height: 40.0,
                fill: Some("#ff0000".into()),
                text: String::new(),
                font_size: None,
            },
            ProjectOperation::UpdateObject {
                name: "Box".into(),
                scene: None,
                layer: Some("Anim".into()),
                x: Some(42.0),
                y: None,
                rotation: Some(15.0),
                scale_x: None,
                scale_y: None,
                fill: None,
                text: None,
                font_size: None,
            },
            ProjectOperation::AddKeyframe {
                layer: "Anim".into(),
                frame: 10,
                scene: None,
                symbol: None,
                label: Some("end".into()),
                duplicate_previous: true,
            },
            ProjectOperation::SetFrameLabel {
                layer: "Anim".into(),
                frame: 1,
                label: "start".into(),
                scene: None,
                symbol: None,
            },
            ProjectOperation::AddTween {
                layer: "Anim".into(),
                frame: 1,
                scene: None,
                symbol: None,
                easing: Easing::Linear,
                kind: None,
            },
        ];
        apply_operations(&mut project, &operations).unwrap();
        let layer = project
            .active_scene()
            .unwrap()
            .layers
            .iter()
            .find(|layer| layer.name == "Anim")
            .unwrap();
        assert_eq!(layer.frames.len(), 10);
        assert_eq!(layer.frames[0].label.as_deref(), Some("start"));
        assert_eq!(layer.frames[9].label.as_deref(), Some("end"));
        assert_eq!(layer.frames[0].objects[0].transform.x, 42.0);
        assert_eq!(layer.frames[0].objects[0].transform.rotation, 15.0);
        assert_eq!(
            layer.frames[0].objects[0].id, layer.frames[9].objects[0].id,
            "duplicated keyframe must preserve object identity for tweening"
        );
        assert_eq!(
            layer.frames[0].tween.as_ref().map(|tween| tween.easing),
            Some(Easing::Linear)
        );
    }

    #[test]
    fn tween_without_a_later_matching_keyframe_is_rejected() {
        let mut project = Project::new_empty("Kein Tween");
        let operations = vec![
            ProjectOperation::AddObject {
                name: "Box".into(),
                scene: None,
                symbol: None,
                layer: None,
                shape: ObjectShape::Rect,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                fill: None,
                text: String::new(),
                font_size: None,
            },
            ProjectOperation::AddTween {
                layer: "Ebene 1".into(),
                frame: 1,
                scene: None,
                symbol: None,
                easing: Easing::EaseIn,
                kind: None,
            },
        ];
        let error = apply_operations(&mut project, &operations).unwrap_err();
        assert!(error.to_string().contains("späteres Schlüsselbild"));
    }

    #[test]
    fn unsafe_script_paths_are_rejected() {
        let mut project = Project::new_empty("Safe");
        for path in [
            "../escape.as",
            "C:/absolute.as",
            "scripts/../escape.as",
            "assets/not-a-script.as",
            "scripts/nope.txt",
        ] {
            let error = apply_operations(
                &mut project,
                &[ProjectOperation::AddScript {
                    path: path.into(),
                    source: String::new(),
                }],
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("Skript") || error.to_string().contains("Unsicher"),
                "{path}: {error}"
            );
        }
        assert!(
            project
                .scripts
                .iter()
                .all(|script| script.path != "../escape.as")
        );
    }

    #[test]
    fn operation_count_is_bounded() {
        let project = Project::new_empty("Limit");
        let operations = vec![ProjectOperation::AddScene { name: "S".into() }; MAX_OPERATIONS + 1];
        let plan = preview_operations(&project, &operations);
        assert!(!plan.errors.is_empty());
        assert!(plan.errors[0].contains("Zu viele"));
    }

    // ------------------------------------------------------------ M9 ops

    /// A 1x1 transparent PNG, enough for the importer to decode dimensions.
    fn tiny_png() -> Vec<u8> {
        // 1x1 RGBA PNG generated once; decode path is what matters here.
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    /// Minimal PCM WAV: 8kHz mono 16-bit, 800 samples (0.1 s).
    fn tiny_wav() -> Vec<u8> {
        let data_size = 1600u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        wav.extend(std::iter::repeat(0u8).take(data_size as usize));
        wav
    }

    #[test]
    fn movieclip_symbol_kind_and_symbol_timeline_targeting() {
        let mut project = Project::new_empty("Sym");
        let operations = vec![
            ProjectOperation::AddSymbol {
                name: "Clip".into(),
                width: 64.0,
                height: 64.0,
                fill: None,
                corner_radius: None,
                kind: Some(SymbolKind::MovieClip),
            },
            ProjectOperation::SetSymbolKind {
                symbol: "Clip".into(),
                kind: SymbolKind::Graphic,
            },
            ProjectOperation::AddLayer {
                name: "Innen".into(),
                scene: None,
                symbol: Some("Clip".into()),
                frames: Some(5),
            },
            ProjectOperation::AddObject {
                name: "Punkt".into(),
                scene: None,
                symbol: Some("Clip".into()),
                layer: Some("Innen".into()),
                shape: ObjectShape::Rect,
                x: 5.0,
                y: 5.0,
                width: 10.0,
                height: 10.0,
                fill: None,
                text: String::new(),
                font_size: None,
            },
            ProjectOperation::AddKeyframe {
                layer: "Innen".into(),
                frame: 5,
                scene: None,
                symbol: Some("Clip".into()),
                label: None,
                duplicate_previous: true,
            },
            ProjectOperation::AddTween {
                layer: "Innen".into(),
                frame: 1,
                scene: None,
                symbol: Some("Clip".into()),
                easing: Easing::Linear,
                kind: Some(nova_core::TweenKind::Shape),
            },
        ];
        apply_operations(&mut project, &operations).unwrap();
        let symbol = project
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Clip")
            .unwrap();
        assert_eq!(symbol.kind, SymbolKind::Graphic);
        let timeline = &symbol.timeline;
        assert!(timeline.layers.iter().any(|layer| layer.name == "Innen"));
        let layer = timeline
            .layers
            .iter()
            .find(|layer| layer.name == "Innen")
            .unwrap();
        assert_eq!(layer.frames.len(), 5);
        assert!(layer.frames[4].keyframe);
        assert_eq!(
            layer.frames[0].tween.as_ref().map(|t| t.kind),
            Some(nova_core::TweenKind::Shape)
        );
        assert!(
            layer.frames[0]
                .objects
                .iter()
                .any(|object| object.name == "Punkt")
        );
    }

    #[test]
    fn asset_import_requires_workspace_reader_and_validates_paths() {
        let mut project = Project::new_empty("Assets");
        // Without a reader the import must fail, never silently succeed.
        let denied = apply_operations(
            &mut project,
            &[ProjectOperation::ImportBitmapAsset {
                name: "BG".into(),
                path: "assets/bg.png".into(),
            }],
        )
        .unwrap_err();
        assert!(denied.to_string().contains("Workspace"));

        // Unsafe paths are rejected before the reader is even consulted.
        for path in ["../x.png", "/abs/x.png", "C:/x.png", "a/../b.png", "x.exe"] {
            let error = apply_operations_with(
                &mut project,
                &[ProjectOperation::ImportBitmapAsset {
                    name: "Bad".into(),
                    path: path.into(),
                }],
                &|_| Ok(tiny_png()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("Asset"), "{path}: {error}");
        }

        let reader = |path: &str| -> Result<Vec<u8>, String> {
            match path {
                "assets/bg.png" => Ok(tiny_png()),
                "assets/tune.wav" => Ok(tiny_wav()),
                _ => Err("not found".into()),
            }
        };
        let operations = vec![
            ProjectOperation::ImportBitmapAsset {
                name: "BG".into(),
                path: "assets/bg.png".into(),
            },
            ProjectOperation::ImportAudioAsset {
                name: "Tune".into(),
                path: "assets/tune.wav".into(),
            },
            ProjectOperation::PlaceBitmap {
                asset: "BG".into(),
                name: "Hintergrund".into(),
                x: 100.0,
                y: 100.0,
                scene: None,
                symbol: None,
                layer: None,
            },
            ProjectOperation::PlaceSoundCue {
                asset: "Tune".into(),
                frame: 1,
                scene: None,
                symbol: None,
                layer: None,
                event: true,
            },
            ProjectOperation::SetLayerKind {
                layer: "Ebene 1".into(),
                kind: LayerKind::Guide,
                scene: None,
                symbol: None,
            },
        ];
        apply_operations_with(&mut project, &operations, &reader).unwrap();
        assert_eq!(project.assets.len(), 2);
        let bitmap = project
            .assets
            .iter()
            .find(|asset| asset.name == "BG")
            .unwrap();
        assert_eq!(bitmap.kind, AssetKind::Bitmap);
        assert_eq!((bitmap.width, bitmap.height), (1, 1));
        let audio = project
            .assets
            .iter()
            .find(|asset| asset.name == "Tune")
            .unwrap();
        assert_eq!(audio.kind, AssetKind::Audio);
        assert_eq!(audio.sample_rate, 8000);
        let scene = project.active_scene().unwrap();
        assert!(scene.layers.iter().any(|layer| {
            layer.kind == LayerKind::Sound
                && layer.frames[0]
                    .sound
                    .is_some_and(|cue| cue.asset == audio.id)
        }));
        assert!(
            scene.layers[0].frames[0]
                .objects
                .iter()
                .any(|object| matches!(object.kind, nova_core::ObjectKind::Bitmap { .. }))
        );
    }

    #[test]
    fn game_operations_set_metadata_and_install_kit() {
        let mut project = Project::new_empty("Game");
        apply_operations(
            &mut project,
            &[
                ProjectOperation::AddObject {
                    name: "Held".into(),
                    scene: None,
                    symbol: None,
                    layer: None,
                    shape: ObjectShape::Rect,
                    x: 10.0,
                    y: 20.0,
                    width: 32.0,
                    height: 48.0,
                    fill: None,
                    text: String::new(),
                    font_size: None,
                },
                ProjectOperation::SetGameObject {
                    object: "Held".into(),
                    scene: None,
                    collision: Some(nova_core::CollisionSpec {
                        shape: nova_core::CollisionShape::Rectangle,
                        width: 32.0,
                        height: 48.0,
                        ..Default::default()
                    }),
                    no_collision: false,
                    physics: Some(nova_core::PhysicsSpec {
                        body: nova_core::BodyType::Dynamic,
                        gravity_scale: 1.0,
                        friction: 0.0,
                        bounce: 0.0,
                        controlled: true,
                    }),
                    no_physics: false,
                    clear: false,
                },
                ProjectOperation::SetSceneGame {
                    scene: None,
                    camera: Some(nova_core::CameraConfig {
                        enabled: true,
                        follow: "Held".into(),
                        left: 0.0,
                        top: 0.0,
                        right: 800.0,
                        bottom: 600.0,
                        zoom: 1.5,
                        follow_smoothing: 0.0,
                        dead_zone: (0.0, 0.0),
                        shake: 0.0,
                    }),
                    input: Some(nova_core::InputBindings {
                        left: "A".into(),
                        right: "D".into(),
                        up: "W".into(),
                        down: "S".into(),
                        jump: "Space".into(),
                        action: "E".into(),
                    }),
                    world: None,
                    clear: false,
                },
                ProjectOperation::InstallGameKit,
            ],
        )
        .unwrap();

        let scene = project.active_scene().unwrap();
        let object = scene.layers[0].frames[0]
            .objects
            .iter()
            .find(|object| object.name == "Held")
            .unwrap();
        let game = object.game.as_ref().unwrap();
        assert_eq!(game.collision.as_ref().unwrap().width, 32.0);
        assert!(game.physics.unwrap().controlled);
        let scene_game = scene.game.as_ref().unwrap();
        assert!(scene_game.camera.as_ref().unwrap().enabled);
        assert_eq!(scene_game.input.as_ref().unwrap().left, "A");
        for path in [
            "scripts/game/Input.as",
            "scripts/game/Collision.as",
            "scripts/game/Camera.as",
            "scripts/game/Random.as",
        ] {
            assert!(
                project.scripts.iter().any(|script| script.path == path),
                "missing {path}"
            );
        }

        // Idempotent: a second install adds nothing.
        let script_count = project.scripts.len();
        apply_operations(&mut project, &[ProjectOperation::InstallGameKit]).unwrap();
        assert_eq!(project.scripts.len(), script_count);

        // `clear` removes object game metadata.
        apply_operations(
            &mut project,
            &[ProjectOperation::SetGameObject {
                object: "Held".into(),
                scene: None,
                collision: None,
                no_collision: false,
                physics: None,
                no_physics: false,
                clear: true,
            }],
        )
        .unwrap();
        let scene = project.active_scene().unwrap();
        let object = scene.layers[0].frames[0]
            .objects
            .iter()
            .find(|object| object.name == "Held")
            .unwrap();
        assert!(object.game.is_none());
    }

    #[test]
    fn scene_transition_operation_validates_and_updates() {
        let mut project = Project::new_empty("Scenes");
        let first_scene = project.scenes[0].name.clone();
        apply_operations(
            &mut project,
            &[
                ProjectOperation::AddScene {
                    name: "Level".into(),
                },
                ProjectOperation::SetSceneTransition {
                    key: "Enter".into(),
                    target: "Level".into(),
                    scene: None,
                    remove: false,
                },
            ],
        )
        .unwrap();
        let scene = project.active_scene().unwrap();
        let transitions = &scene.game.as_ref().unwrap().transitions;
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].target, "Level");

        // Same key updates the target instead of duplicating.
        apply_operations(
            &mut project,
            &[ProjectOperation::SetSceneTransition {
                key: "Enter".into(),
                target: first_scene.clone(),
                scene: None,
                remove: false,
            }],
        )
        .unwrap();
        let scene = project.active_scene().unwrap();
        let transitions = &scene.game.as_ref().unwrap().transitions;
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].target, first_scene);

        // Unknown target is rejected and leaves state untouched.
        let before = project.clone();
        let err = apply_operations(
            &mut project,
            &[ProjectOperation::SetSceneTransition {
                key: "Space".into(),
                target: "Missing".into(),
                scene: None,
                remove: false,
            }],
        )
        .unwrap_err();
        assert!(err.to_string().contains("Missing"));
        assert_eq!(project, before);

        // Remove works.
        apply_operations(
            &mut project,
            &[ProjectOperation::SetSceneTransition {
                key: "Enter".into(),
                target: String::new(),
                scene: None,
                remove: true,
            }],
        )
        .unwrap();
        let scene = project.active_scene().unwrap();
        assert!(scene.game.as_ref().unwrap().transitions.is_empty());
    }

    #[test]
    fn ai_builds_playable_tilemap_level() {
        let mut project = Project::new_empty("Level");
        let reader = |path: &str| -> Result<Vec<u8>, String> {
            match path {
                "assets/tiles.png" => Ok(tiny_png()),
                _ => Err("not found".into()),
            }
        };
        // One composite op: scene + world + tileset + tilemap + strokes +
        // objects + camera.
        apply_operations_with(
            &mut project,
            &[ProjectOperation::ImportProjectAsset {
                name: Some("Terrain".into()),
                path: "assets/tiles.png".into(),
            }],
            &reader,
        )
        .unwrap();
        apply_operations_with(
            &mut project,
            &[ProjectOperation::CreateGameLevel {
                scene: "Level 1".into(),
                world_width: Some(640.0),
                world_height: Some(360.0),
                spawn: Some((32.0, 32.0)),
                tileset: Some(LevelTileset {
                    asset: "Terrain".into(),
                    name: "Terrain".into(),
                    tile_width: 1,
                    tile_height: 1,
                    solid_tiles: vec![0],
                }),
                tilemap: Some(LevelTilemap {
                    name: "Ground".into(),
                    tileset: "Terrain".into(),
                    columns: 20,
                    rows: 12,
                    x: 320.0,
                    y: 180.0,
                    strokes: vec![
                        LevelStroke {
                            tool: nova_core::tilemap::PaintTool::Rect,
                            x0: 0,
                            y0: 10,
                            x1: 19,
                            y1: 11,
                            tile: 1,
                            layer: 0,
                        },
                        LevelStroke {
                            tool: nova_core::tilemap::PaintTool::Pencil,
                            x0: 5,
                            y0: 5,
                            x1: 5,
                            y1: 5,
                            tile: 1,
                            layer: 0,
                        },
                    ],
                }),
                objects: vec![LevelObject {
                    name: "Spieler".into(),
                    x: 32.0,
                    y: 32.0,
                    width: 16.0,
                    height: 24.0,
                    fill: Some("#4da3ff".into()),
                    collision: Some(nova_core::CollisionSpec {
                        shape: nova_core::CollisionShape::Rectangle,
                        width: 16.0,
                        height: 24.0,
                        ..Default::default()
                    }),
                    physics: Some(nova_core::PhysicsSpec {
                        body: nova_core::BodyType::Dynamic,
                        gravity_scale: 1.0,
                        friction: 0.0,
                        bounce: 0.0,
                        controlled: true,
                    }),
                    particle_preset: Some(nova_core::ParticlePreset::Sparks),
                }],
                camera_follow: Some("Spieler".into()),
            }],
            &reader,
        )
        .unwrap();

        let scene = project
            .scenes
            .iter()
            .find(|s| s.name == "Level 1")
            .expect("level scene created");
        // World + camera.
        let game = scene.game.as_ref().expect("game config");
        let world = game.world.as_ref().expect("world config");
        assert_eq!((world.width, world.height), (640.0, 360.0));
        assert_eq!(world.spawn, (32.0, 32.0));
        let camera = game.camera.as_ref().expect("camera");
        assert!(camera.enabled);
        assert_eq!(camera.follow, "Spieler");
        // Tileset + tilemap data.
        assert_eq!(project.tilesets.len(), 1);
        let tileset = &project.tilesets[0];
        assert!(tileset.solid[0]);
        let tilemap_obj = scene
            .layers
            .iter()
            .flat_map(|l| l.frames.iter())
            .flat_map(|f| f.objects.iter())
            .find(|o| o.name == "Ground")
            .expect("tilemap object");
        let nova_core::ObjectKind::Tilemap(map) = &tilemap_obj.kind else {
            panic!("Ground is not a tilemap");
        };
        assert_eq!((map.columns, map.rows), (20, 12));
        let layer = &map.layers[0];
        // Rect stroke filled rows 10-11 across all columns with tile 1.
        assert_eq!(layer.cells[10 * 20], 1);
        assert_eq!(layer.cells[11 * 20 + 19], 1);
        // Pencil stroke painted tile 1 at (5,5).
        assert_eq!(layer.cells[5 * 20 + 5], 1);
        // Player object with collision + physics + particles.
        let player = scene
            .layers
            .iter()
            .flat_map(|l| l.frames.iter())
            .flat_map(|f| f.objects.iter())
            .find(|o| o.name == "Spieler")
            .expect("player object");
        let game_obj = player.game.as_ref().expect("game object config");
        assert!(game_obj.collision.is_some());
        assert!(game_obj.physics.is_some());
        assert_eq!(
            player.particles.as_ref().map(|p| p.preset),
            Some(nova_core::ParticlePreset::Sparks)
        );
        project.validate().expect("level project valid");
    }

    #[test]
    fn ai_rejects_invalid_tilemap_ops_transactionally() {
        let mut project = Project::new_empty("T");
        let before = project.clone();
        // Painting a non-existent tilemap fails without touching the project.
        let err = apply_operations(
            &mut project,
            &[ProjectOperation::PaintTiles {
                object: "Missing".into(),
                layer: 0,
                tool: nova_core::tilemap::PaintTool::Pencil,
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
                tile: 1,
                from: 0,
                to: 0,
                scene: None,
            }],
        )
        .unwrap_err();
        assert!(err.to_string().contains("Missing"));
        assert_eq!(project, before);
        // Degenerate polygon collision is rejected.
        apply_operations(
            &mut project,
            &[ProjectOperation::PlaceGameObject {
                name: "Obj".into(),
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                fill: None,
                scene: None,
                layer: None,
                collision: None,
                physics: None,
            }],
        )
        .unwrap();
        let err = apply_operations(
            &mut project,
            &[ProjectOperation::CreateCollisionShape {
                object: "Obj".into(),
                scene: None,
                shape: nova_core::CollisionShape::Polygon {
                    points: vec![(0.0, 0.0), (1.0, 0.0)],
                },
                width: None,
                height: None,
                layer: None,
                mask: None,
                remove: false,
            }],
        )
        .unwrap_err();
        assert!(err.to_string().contains("Polygon"));
    }

    #[test]
    fn tile_layer_region_and_slicing_operations() {
        let mut project = Project::new_empty("Level");
        // 32x16 image so the 8x8 + margin/spacing slicing yields 3 tiles.
        let tiles_png = {
            let img = image::RgbaImage::from_pixel(32, 16, image::Rgba([200, 80, 40, 255]));
            let mut buf = std::io::Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
            buf.into_inner()
        };
        let reader = move |path: &str| -> Result<Vec<u8>, String> {
            match path {
                "assets/tiles.png" => Ok(tiles_png.clone()),
                _ => Err("not found".into()),
            }
        };
        apply_operations_with(
            &mut project,
            &[
                ProjectOperation::ImportProjectAsset {
                    name: Some("Terrain".into()),
                    path: "assets/tiles.png".into(),
                },
                ProjectOperation::CreateTileset {
                    name: "Terrain".into(),
                    asset: "Terrain".into(),
                    tile_width: 8,
                    tile_height: 8,
                    solid_tiles: vec![0],
                    tile_names: Vec::new(),
                    margin_x: 2,
                    margin_y: 2,
                    spacing_x: 1,
                    spacing_y: 1,
                },
                ProjectOperation::CreateTilemap {
                    name: "Map".into(),
                    tileset: "Terrain".into(),
                    columns: 8,
                    rows: 8,
                    x: 0.0,
                    y: 0.0,
                    scene: None,
                    layer: None,
                },
            ],
            &reader,
        )
        .unwrap();

        // Slicing margins/spacing were stored on the tileset.
        let tileset = &project.tilesets[0];
        assert_eq!((tileset.margin_x, tileset.margin_y), (2, 2));
        assert_eq!((tileset.spacing_x, tileset.spacing_y), (1, 1));

        // Paint an L-shape, add a layer, configure it, then flip+move.
        apply_operations(
            &mut project,
            &[
                ProjectOperation::PaintTiles {
                    object: "Map".into(),
                    layer: 0,
                    tool: nova_core::tilemap::PaintTool::Rect,
                    x0: 0,
                    y0: 0,
                    x1: 3,
                    y1: 0,
                    tile: 1,
                    from: 0,
                    to: 0,
                    scene: None,
                },
                ProjectOperation::AddTileLayer {
                    object: "Map".into(),
                    name: Some("Details".into()),
                    scene: None,
                },
                ProjectOperation::ConfigureTileLayer {
                    object: "Map".into(),
                    layer: 1,
                    name: None,
                    visible: Some(true),
                    locked: Some(false),
                    opacity: Some(0.5),
                    scene: None,
                },
                ProjectOperation::FlipTileRegion {
                    object: "Map".into(),
                    layer: 0,
                    x0: 0,
                    y0: 0,
                    x1: 3,
                    y1: 0,
                    flip_h: true,
                    flip_v: false,
                    scene: None,
                },
                ProjectOperation::MoveTileRegion {
                    object: "Map".into(),
                    layer: 0,
                    x0: 0,
                    y0: 0,
                    x1: 3,
                    y1: 0,
                    dx: 2,
                    dy: 1,
                    scene: None,
                },
            ],
        )
        .unwrap();

        let scene = project.active_scene().unwrap();
        let tilemap_obj = scene
            .layers
            .iter()
            .flat_map(|l| l.frames.iter())
            .flat_map(|f| f.objects.iter())
            .find(|o| o.name == "Map")
            .unwrap();
        let nova_core::ObjectKind::Tilemap(map) = &tilemap_obj.kind else {
            panic!("Map is not a tilemap");
        };
        assert_eq!(map.layers.len(), 2);
        assert_eq!(map.layers[1].name, "Details");
        assert_eq!(map.layers[1].opacity, 0.5);
        // Moved region now occupies cells x=2..5, y=1 on layer 0.
        for x in 2..=5 {
            assert_eq!(map.layers[0].cells[1 * 8 + x], 1, "cell ({x},1)");
        }
        assert_eq!(map.layers[0].cells[0], 0, "source cleared");
        project.validate().expect("project valid");
    }

    #[test]
    fn bone_operations_build_a_working_rig() {
        let mut project = Project::new_empty("Rig-Test");
        let symbol_id = project.add_symbol("Char", nova_core::SymbolKind::MovieClip);
        let ops = vec![
            ProjectOperation::AddBone {
                symbol: "Char".into(),
                name: "Root".into(),
                x: 0.0,
                y: 0.0,
                length: 30.0,
                parent: None,
                rotation: None,
            },
            ProjectOperation::AddBone {
                symbol: "Char".into(),
                name: "Arm".into(),
                x: 30.0,
                y: 0.0,
                length: 40.0,
                parent: Some("Root".into()),
                rotation: Some(0.0),
            },
            ProjectOperation::AddBoneClip {
                symbol: "Char".into(),
                name: "Wave".into(),
                frames: Some(12),
                looping: Some(true),
            },
            ProjectOperation::AddBoneKey {
                symbol: "Char".into(),
                clip: "Wave".into(),
                bone: "Arm".into(),
                frame: 0,
                rotation: 0.0,
            },
            ProjectOperation::AddBoneKey {
                symbol: "Char".into(),
                clip: "Wave".into(),
                bone: "Arm".into(),
                frame: 12,
                rotation: -90.0,
            },
            ProjectOperation::SetSymbolClip {
                symbol: "Char".into(),
                clip: Some("Wave".into()),
            },
        ];
        let summaries = apply_operations(&mut project, &ops).expect("ops apply");
        assert_eq!(summaries.len(), 6);
        let symbol = project.symbol(symbol_id).unwrap();
        assert_eq!(symbol.skeleton.as_ref().unwrap().bones.len(), 2);
        assert_eq!(symbol.bone_clips[0].tracks[0].keys.len(), 2);
        assert_eq!(symbol.active_clip.as_deref(), Some("Wave"));
        // The clip ensured the timeline is at least 12 frames long.
        assert!(symbol.timeline.frame_count() >= 12);
        // Root -> Arm -> Root would create a cycle and must be rejected.
        let bad = vec![ProjectOperation::SetBoneParent {
            symbol: "Char".into(),
            bone: "Root".into(),
            parent: Some("Arm".into()),
        }];
        assert!(apply_operations(&mut project, &bad).is_err());
        // Unknown clip for set_symbol_clip must fail cleanly.
        let bad_clip = vec![ProjectOperation::SetSymbolClip {
            symbol: "Char".into(),
            clip: Some("Missing".into()),
        }];
        assert!(apply_operations(&mut project, &bad_clip).is_err());
    }
}
