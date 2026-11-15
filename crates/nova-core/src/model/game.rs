//! Game-development metadata stored inside the `.nova` document model.
//!
//! Everything in this module is optional authoring data. It never changes how
//! the timeline or the SWF exporter behaves; it drives the editor's game
//! preview simulation, the inspector UI and the AI operations. AS3 game logic
//! lives in project scripts and is compiled to ABC exactly like before.

use serde::{Deserialize, Serialize};

/// Shape used for collision tests in the editor preview simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CollisionShape {
    #[default]
    Rectangle,
    Circle,
    /// Convex polygon, points relative to the object origin in pixels.
    /// The preview approximates polygon colliders by their bounding box;
    /// full polygon resolution is editor metadata only (documented).
    Polygon {
        points: Vec<(f32, f32)>,
    },
}

impl CollisionShape {
    pub fn is_circle(&self) -> bool {
        matches!(self, Self::Circle)
    }
}

/// Collision volume attached to a game object. Extents are in pixels and are
/// centred on the object's transform origin (polygon points are relative).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CollisionSpec {
    pub shape: CollisionShape,
    /// Rectangle width / circle diameter / polygon bounding width.
    pub width: f32,
    /// Rectangle height (ignored for circles).
    pub height: f32,
    /// Collision category bit (1..=31). Two bodies interact only when each
    /// category bit is present in the other's `mask`.
    pub layer: u32,
    /// Collision mask bitfield; bit `layer` of the other body must be set.
    pub mask: u32,
}

impl Default for CollisionSpec {
    fn default() -> Self {
        Self {
            shape: CollisionShape::Rectangle,
            width: 32.0,
            height: 32.0,
            layer: 1,
            mask: u32::MAX,
        }
    }
}

impl CollisionSpec {
    /// Whether two specs can possibly interact under layer/mask rules.
    pub fn interacts_with(&self, other: &Self) -> bool {
        self.layer & other.mask != 0 && other.layer & self.mask != 0
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
    /// Follow smoothing 0.0 (instant) ..= 1.0 (very laggy). Implemented as an
    /// exponential approach factor per second.
    #[serde(default = "default_smoothing")]
    pub follow_smoothing: f32,
    /// Dead-zone half extents in pixels around the follow target; the camera
    /// only moves once the target leaves this box. (0,0) disables it.
    #[serde(default)]
    pub dead_zone: (f32, f32),
    /// Camera shake intensity in pixels (0 = off). The preview decays it
    /// exponentially; there is no runtime API to trigger it yet.
    #[serde(default)]
    pub shake: f32,
}

fn default_smoothing() -> f32 {
    0.0
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
            follow_smoothing: 0.0,
            dead_zone: (0.0, 0.0),
            shake: 0.0,
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

/// World/level bounds for one scene. Optional: when absent the stage
/// rectangle is the world. `spawn` is the suggested player start position
/// used by templates and the AI level workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorldConfig {
    pub width: f32,
    pub height: f32,
    /// Suggested spawn position in world pixels.
    pub spawn: (f32, f32),
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            spawn: (0.0, 0.0),
        }
    }
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
    /// Optional world bounds and spawn point (level editor data).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world: Option<WorldConfig>,
}

/// Simulation space of a particle emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParticleSpace {
    /// Particles move with the emitter object.
    #[default]
    Local,
    /// Particles stay in world space once emitted.
    World,
}

/// Built-in particle effect presets. `Custom` means every field is authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParticlePreset {
    #[default]
    Custom,
    Smoke,
    Sparks,
    Fire,
    Rain,
    Snow,
    Dust,
    Magic,
    Explosion,
}

impl ParticlePreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Custom => "Benutzerdefiniert",
            Self::Smoke => "Rauch",
            Self::Sparks => "Funken",
            Self::Fire => "Feuer",
            Self::Rain => "Regen",
            Self::Snow => "Schnee",
            Self::Dust => "Staub",
            Self::Magic => "Magie",
            Self::Explosion => "Explosion",
        }
    }

    pub const ALL: [ParticlePreset; 9] = [
        Self::Custom,
        Self::Smoke,
        Self::Sparks,
        Self::Fire,
        Self::Rain,
        Self::Snow,
        Self::Dust,
        Self::Magic,
        Self::Explosion,
    ];
}

/// Configurable 2D particle emitter attached to an object. The emitter
/// position is the object's transform origin; `local` space drags emitted
/// particles along with the object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParticleEmitter {
    pub preset: ParticlePreset,
    /// Continuous emission rate in particles per second (0 = burst only).
    pub rate: f32,
    /// Particles emitted once when the emitter (re)starts.
    pub burst: u32,
    /// Particle lifetime range in seconds (start..=end).
    pub lifetime: (f32, f32),
    /// Size range in pixels at birth and death.
    pub size_start: (f32, f32),
    pub size_end: (f32, f32),
    /// Opacity range 0..=1 at birth and death.
    pub opacity_start: (f32, f32),
    pub opacity_end: (f32, f32),
    /// Colour ranges (linear RGBA bytes).
    pub color_start: crate::model::Rgba,
    pub color_end: crate::model::Rgba,
    /// Initial speed px/s along `direction` ± `spread` degrees.
    pub speed: (f32, f32),
    pub direction: f32,
    pub spread: f32,
    /// Downward acceleration px/s².
    pub gravity: f32,
    /// Linear damping per second.
    pub drag: f32,
    /// Rotation speed range in degrees per second.
    pub rotation_speed: (f32, f32),
    /// Hard cap on live particles.
    pub max_particles: u32,
    pub space: ParticleSpace,
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        Self {
            preset: ParticlePreset::Custom,
            rate: 20.0,
            burst: 0,
            lifetime: (0.5, 1.0),
            size_start: (4.0, 8.0),
            size_end: (0.0, 2.0),
            opacity_start: (1.0, 1.0),
            opacity_end: (0.0, 0.0),
            color_start: crate::model::Rgba::WHITE,
            color_end: crate::model::Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
            speed: (40.0, 80.0),
            direction: -90.0,
            spread: 30.0,
            gravity: 0.0,
            drag: 0.0,
            rotation_speed: (0.0, 0.0),
            max_particles: 256,
            space: ParticleSpace::Local,
        }
    }
}

impl ParticleEmitter {
    /// Returns the preset configuration for `preset`.
    pub fn preset(preset: ParticlePreset) -> Self {
        let mut emitter = Self {
            preset,
            ..Default::default()
        };
        match preset {
            ParticlePreset::Smoke => {
                emitter.rate = 12.0;
                emitter.lifetime = (1.5, 3.0);
                emitter.size_start = (8.0, 16.0);
                emitter.size_end = (24.0, 40.0);
                emitter.opacity_start = (0.5, 0.7);
                emitter.opacity_end = (0.0, 0.0);
                emitter.color_start = crate::model::Rgba::rgb(140, 140, 150);
                emitter.color_end = crate::model::Rgba::rgb(90, 90, 100);
                emitter.speed = (10.0, 30.0);
                emitter.direction = -90.0;
                emitter.spread = 40.0;
                emitter.gravity = -20.0;
                emitter.drag = 0.6;
            }
            ParticlePreset::Sparks => {
                emitter.rate = 30.0;
                emitter.lifetime = (0.2, 0.6);
                emitter.size_start = (1.5, 3.0);
                emitter.size_end = (0.0, 0.5);
                emitter.opacity_start = (1.0, 1.0);
                emitter.opacity_end = (0.0, 0.0);
                emitter.color_start = crate::model::Rgba::rgb(255, 220, 120);
                emitter.color_end = crate::model::Rgba::rgb(255, 80, 20);
                emitter.speed = (120.0, 260.0);
                emitter.direction = -90.0;
                emitter.spread = 60.0;
                emitter.gravity = 600.0;
                emitter.drag = 0.4;
            }
            ParticlePreset::Fire => {
                emitter.rate = 40.0;
                emitter.lifetime = (0.3, 0.8);
                emitter.size_start = (6.0, 14.0);
                emitter.size_end = (0.0, 4.0);
                emitter.opacity_start = (0.9, 1.0);
                emitter.opacity_end = (0.0, 0.0);
                emitter.color_start = crate::model::Rgba::rgb(255, 160, 40);
                emitter.color_end = crate::model::Rgba::rgb(200, 30, 10);
                emitter.speed = (20.0, 60.0);
                emitter.direction = -90.0;
                emitter.spread = 25.0;
                emitter.gravity = -120.0;
                emitter.drag = 1.2;
            }
            ParticlePreset::Rain => {
                emitter.rate = 120.0;
                emitter.lifetime = (0.8, 1.4);
                emitter.size_start = (1.0, 2.0);
                emitter.size_end = (1.0, 2.0);
                emitter.opacity_start = (0.5, 0.8);
                emitter.opacity_end = (0.3, 0.5);
                emitter.color_start = crate::model::Rgba::rgb(140, 170, 220);
                emitter.color_end = crate::model::Rgba::rgb(140, 170, 220);
                emitter.speed = (0.0, 20.0);
                emitter.direction = 90.0;
                emitter.spread = 8.0;
                emitter.gravity = 900.0;
                emitter.max_particles = 512;
            }
            ParticlePreset::Snow => {
                emitter.rate = 30.0;
                emitter.lifetime = (2.0, 4.0);
                emitter.size_start = (2.0, 5.0);
                emitter.size_end = (1.0, 3.0);
                emitter.opacity_start = (0.8, 1.0);
                emitter.opacity_end = (0.4, 0.8);
                emitter.color_start = crate::model::Rgba::rgb(240, 245, 255);
                emitter.color_end = crate::model::Rgba::rgb(220, 230, 250);
                emitter.speed = (10.0, 40.0);
                emitter.direction = 90.0;
                emitter.spread = 20.0;
                emitter.gravity = 60.0;
                emitter.drag = 0.8;
                emitter.rotation_speed = (-90.0, 90.0);
            }
            ParticlePreset::Dust => {
                emitter.rate = 8.0;
                emitter.lifetime = (0.8, 1.6);
                emitter.size_start = (3.0, 8.0);
                emitter.size_end = (10.0, 20.0);
                emitter.opacity_start = (0.4, 0.6);
                emitter.opacity_end = (0.0, 0.0);
                emitter.color_start = crate::model::Rgba::rgb(160, 150, 130);
                emitter.color_end = crate::model::Rgba::rgb(140, 130, 110);
                emitter.speed = (10.0, 40.0);
                emitter.direction = -90.0;
                emitter.spread = 60.0;
                emitter.gravity = -10.0;
                emitter.drag = 0.8;
            }
            ParticlePreset::Magic => {
                emitter.rate = 25.0;
                emitter.lifetime = (0.6, 1.4);
                emitter.size_start = (3.0, 7.0);
                emitter.size_end = (0.0, 2.0);
                emitter.opacity_start = (0.9, 1.0);
                emitter.opacity_end = (0.0, 0.0);
                emitter.color_start = crate::model::Rgba::rgb(170, 120, 255);
                emitter.color_end = crate::model::Rgba::rgb(80, 200, 255);
                emitter.speed = (30.0, 90.0);
                emitter.direction = -90.0;
                emitter.spread = 360.0;
                emitter.gravity = -40.0;
                emitter.drag = 0.5;
                emitter.rotation_speed = (-180.0, 180.0);
            }
            ParticlePreset::Explosion => {
                emitter.rate = 0.0;
                emitter.burst = 60;
                emitter.lifetime = (0.3, 0.9);
                emitter.size_start = (3.0, 8.0);
                emitter.size_end = (0.0, 2.0);
                emitter.opacity_start = (1.0, 1.0);
                emitter.opacity_end = (0.0, 0.0);
                emitter.color_start = crate::model::Rgba::rgb(255, 200, 80);
                emitter.color_end = crate::model::Rgba::rgb(255, 60, 20);
                emitter.speed = (150.0, 380.0);
                emitter.direction = -90.0;
                emitter.spread = 360.0;
                emitter.gravity = 300.0;
                emitter.drag = 1.5;
            }
            ParticlePreset::Custom => {}
        }
        emitter
    }
}
