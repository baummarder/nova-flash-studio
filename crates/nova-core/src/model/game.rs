//! Game-development metadata stored inside the `.nova` document model.
//!
//! Everything in this module is optional authoring data. It never changes how
//! the timeline or the SWF exporter behaves; it drives the editor's game
//! preview simulation, the inspector UI and the AI operations. AS3 game logic
//! lives in project scripts and is compiled to ABC exactly like before.

use serde::{Deserialize, Serialize};

/// Shape used for collision tests in the editor preview simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CollisionShape {
    #[default]
    Rectangle,
    Circle,
}

/// Collision volume attached to a game object. Extents are in pixels and are
/// centred on the object's transform origin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionSpec {
    pub shape: CollisionShape,
    /// Rectangle width / circle diameter.
    pub width: f32,
    /// Rectangle height (ignored for circles).
    pub height: f32,
}

impl Default for CollisionSpec {
    fn default() -> Self {
        Self {
            shape: CollisionShape::Rectangle,
            width: 32.0,
            height: 32.0,
        }
    }
}

/// Physics body classification used by the preview simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BodyType {
    /// Immovable collider (ground, walls).
    Static,
    /// Moves under velocity, gravity and collisions.
    #[default]
    Dynamic,
    /// Moves only through scripted/preview-driven velocity, ignores forces.
    Kinematic,
}

/// Tunable physics parameters for one game object.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicsSpec {
    pub body: BodyType,
    /// Multiplies the scene gravity for this body (0 = no gravity).
    pub gravity_scale: f32,
    /// Linear damping applied per second (0 = none, ~4 = strong).
    pub friction: f32,
    /// Restitution on collision (0 = dead stop, 1 = perfect bounce).
    pub bounce: f32,
    /// When true the preview simulation applies the scene input bindings
    /// (move/jump) to this body. Only meaningful for dynamic bodies.
    #[serde(default)]
    pub controlled: bool,
}

impl Default for PhysicsSpec {
    fn default() -> Self {
        Self {
            body: BodyType::Dynamic,
            gravity_scale: 1.0,
            friction: 0.0,
            bounce: 0.0,
            controlled: false,
        }
    }
}

/// Optional game components of one placed object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GameObjectConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision: Option<CollisionSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physics: Option<PhysicsSpec>,
}

/// Logical input actions understood by the preview simulation. Key names are
/// egui-style names ("A", "ArrowLeft", "Space", ...); the UI editor maps them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputBindings {
    pub left: String,
    pub right: String,
    pub up: String,
    pub down: String,
    pub jump: String,
    pub action: String,
}

impl Default for InputBindings {
    fn default() -> Self {
        Self {
            left: "ArrowLeft".into(),
            right: "ArrowRight".into(),
            up: "ArrowUp".into(),
            down: "ArrowDown".into(),
            jump: "Space".into(),
            action: "KeyE".into(),
        }
    }
}

/// Camera settings for one scene, used by the preview simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CameraConfig {
    pub enabled: bool,
    /// Name of the object the camera follows (empty = centre of stage).
    pub follow: String,
    /// World-space bounds the camera centre may not leave.
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    /// Zoom factor applied to the view (1.0 = native).
    pub zoom: f32,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            follow: String::new(),
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            zoom: 1.0,
        }
    }
}

/// A key-triggered scene transition evaluated by the game preview runtime.
/// `key` uses the same naming space as [`InputBindings`] values
/// ("Space", "Enter", "A", "ArrowLeft", ...). The transition fires on the
/// rising edge of the key while the scene is active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneTransition {
    pub key: String,
    /// Name of the target scene. Validated when the transition fires; an
    /// unknown target is reported and ignored without corrupting state.
    pub target: String,
}

/// Scene-level game configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GameSceneConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputBindings>,
    /// Key-triggered scene transitions for the preview runtime.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub transitions: Vec<SceneTransition>,
}
