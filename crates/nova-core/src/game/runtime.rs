//! Game runtime for the editor preview: scene lifecycle, transitions and a
//! typed event bus layered on top of [`super`] simulation primitives.
//!
//! The runtime is deliberately UI-agnostic and deterministic: it owns the
//! active scene snapshot (bodies, states, camera, config), the event bus and
//! the running/paused/stopped lifecycle. The app crate drives it from the
//! play button and renders `states`/`camera`.
//!
//! Scope (documented, not hidden):
//! - Scene transitions are triggered by configured key presses
//!   ([`crate::model::SceneTransition`]) or by calling [`GameRuntime::goto_scene`]
//!   directly (tests, future UI). AS3 cannot reach the host, so exported SWFs
//!   implement their own navigation; see the M10.1 report.
//! - Event listeners are Rust callbacks registered by the host (editor/tests).
//!   There is no AS3-side listener surface.
//! - Collision events are computed from pre-integration positions each step;
//!   sustained contacts may re-fire `enter` if bodies separate between frames.

use std::collections::{HashMap, HashSet};

use crate::model::{GameSceneConfig, Project, Scene, SceneTransition};
use thiserror::Error;
use uuid::Uuid;

use super::{
    BodyState, CameraState, GameBody, InputState, attach_tile_colliders, collect_bodies,
    scene_config, step, step_camera,
};
use crate::particles::ParticleSystem;

/// Maximum nested `dispatch` depth. Listeners that re-dispatch recursively are
/// cut off instead of overflowing the stack; dropped events are counted.
pub const MAX_DISPATCH_DEPTH: usize = 8;

/// Typed categories of game events. The payload lives in [`GameEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameEventKind {
    GameStart,
    GamePause,
    GameResume,
    GameStop,
    GameRestart,
    SceneLoad,
    SceneUnload,
    SceneTransition,
    CollisionEnter,
    CollisionExit,
    InputPressed,
}

/// One dispatched game event. `subject` carries the related object id for
/// collision/input events and the other scene id for transition events;
/// `label` is a human-readable detail (names, action names).
#[derive(Debug, Clone, PartialEq)]
pub struct GameEvent {
    pub kind: GameEventKind,
    /// Scene the event belongs to at dispatch time.
    pub scene: Uuid,
    pub subject: Option<Uuid>,
    pub label: String,
}

/// Opaque listener handle returned by [`EventBus::register`].
pub type ListenerId = u64;

type Listener = Box<dyn Fn(&GameEvent)>;

/// Typed, multi-listener event bus with recursion protection.
///
/// Listeners are stored per [`GameEventKind`]; dispatch snapshots the listener
/// list so listeners may safely register or remove during dispatch (changes
/// apply from the next dispatch). Duplicate registration of the same closure
/// is not detectable — callers keep the returned id to remove exactly what
/// they added.
pub struct EventBus {
    listeners: HashMap<GameEventKind, Vec<(ListenerId, Listener)>>,
    next_id: ListenerId,
    depth: usize,
    /// Events dropped because [`MAX_DISPATCH_DEPTH`] was exceeded.
    pub dropped_events: usize,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
            next_id: 1,
            depth: 0,
            dropped_events: 0,
        }
    }

    /// Registers `listener` for `kind` and returns its handle.
    pub fn register(
        &mut self,
        kind: GameEventKind,
        listener: impl Fn(&GameEvent) + 'static,
    ) -> ListenerId {
        let id = self.next_id;
        self.next_id += 1;
        self.listeners
            .entry(kind)
            .or_default()
            .push((id, Box::new(listener)));
        id
    }

    /// Convenience: register for every kind. The listener must be `Clone`
    /// so each kind gets its own boxed copy.
    pub fn register_all(
        &mut self,
        listener: impl Fn(&GameEvent) + Clone + 'static,
    ) -> Vec<ListenerId> {
        [
            GameEventKind::GameStart,
            GameEventKind::GamePause,
            GameEventKind::GameResume,
            GameEventKind::GameStop,
            GameEventKind::GameRestart,
            GameEventKind::SceneLoad,
            GameEventKind::SceneUnload,
            GameEventKind::SceneTransition,
            GameEventKind::CollisionEnter,
            GameEventKind::CollisionExit,
            GameEventKind::InputPressed,
        ]
        .into_iter()
        .map(|kind| self.register(kind, listener.clone()))
        .collect()
    }

    /// Removes a listener by handle. Returns `true` when one was removed.
    pub fn remove(&mut self, id: ListenerId) -> bool {
        for listeners in self.listeners.values_mut() {
            if let Some(index) = listeners.iter().position(|(lid, _)| *lid == id) {
                let _removed = listeners.remove(index);
                return true;
            }
        }
        false
    }

    /// Number of listeners currently registered for `kind`.
    pub fn listener_count(&self, kind: GameEventKind) -> usize {
        self.listeners.get(&kind).map_or(0, Vec::len)
    }

    /// Dispatches `event` to all listeners of its kind. Re-entrant dispatches
    /// are allowed up to [`MAX_DISPATCH_DEPTH`]; deeper calls increment
    /// [`EventBus::dropped_events`] and return without invoking listeners.
    pub fn dispatch(&mut self, event: &GameEvent) {
        if self.depth >= MAX_DISPATCH_DEPTH {
            self.dropped_events += 1;
            return;
        }
        self.depth += 1;
        if !self.listeners.contains_key(&event.kind) {
            self.depth -= 1;
            return;
        }
        // Snapshot the (id, listener) pairs is impossible for Box<dyn Fn>;
        // iterate by index over a cloned id list is unnecessary because the
        // Vec is not mutated during iteration unless a listener calls
        // register/remove — which mutates `self.listeners`, not this Vec...
        // but `self` is borrowed. Work around by taking the Vec out.
        let owned = self.listeners.remove(&event.kind).unwrap_or_default();
        for (_, listener) in &owned {
            listener(event);
        }
        // Merge back preserving order: listeners added during dispatch run
        // after the original set on the next dispatch.
        match self.listeners.remove(&event.kind) {
            Some(added) => {
                let mut merged = owned;
                merged.extend(added);
                self.listeners.insert(event.kind, merged);
            }
            None => {
                self.listeners.insert(event.kind, owned);
            }
        }
        self.depth -= 1;
    }
}

/// Lifecycle state of the preview runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStatus {
    Running,
    Paused,
    Stopped,
}

/// Errors from [`GameRuntime::goto_scene`] / [`GameRuntime::new`]. The
/// runtime state is left untouched on error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("Szene '{0}' existiert nicht")]
    SceneNotFound(String),
    #[error("Das Projekt enthält keine Szenen")]
    NoScenes,
    #[error("Szene '{0}' ist keine gültige Szenen-ID")]
    InvalidSceneId(String),
}

/// A particle emitter bound to a scene object.
pub struct EmitterRuntime {
    /// Object instance id (also the body id when the object has game data).
    pub id: Uuid,
    pub name: String,
    /// Authored emitter origin.
    pub x: f32,
    pub y: f32,
    pub system: ParticleSystem,
}

/// Live game runtime: active scene snapshot + simulation + events.
pub struct GameRuntime {
    /// Active scene id.
    pub scene: Uuid,
    /// Active scene display name (for UI/logging).
    pub scene_name: String,
    /// Simulated bodies of the active scene.
    pub bodies: Vec<GameBody>,
    /// Live body states keyed by object instance id. Seeded at authored
    /// positions on scene load, so renderers can read them immediately.
    pub states: HashMap<Uuid, BodyState>,
    /// Camera state (world centre + zoom).
    pub camera: CameraState,
    /// Event bus shared by simulation, transitions and host code.
    pub events: EventBus,
    /// Current lifecycle status.
    pub status: RuntimeStatus,
    /// Accumulated simulation time in seconds (reset on stop/restart).
    pub time: f32,
    /// Scene-level config snapshot of the active scene.
    pub config: GameSceneConfig,
    /// Particle emitters of the active scene.
    pub emitters: Vec<EmitterRuntime>,
    scene_w: f32,
    scene_h: f32,
    /// Body states remembered per scene so returning to a scene restores
    /// positions (scene unload/load semantics).
    scene_states: HashMap<Uuid, HashMap<Uuid, BodyState>>,
    /// Active collision pairs (sorted id pairs) for enter/exit events.
    contacts: HashSet<(Uuid, Uuid)>,
    /// Input actions held during the previous step (edge detection).
    prev_input: InputState,
    /// Raw binding key names held during the previous step.
    prev_keys: HashSet<String>,
}

impl GameRuntime {
    /// Creates a runtime for `scene_id`. The runtime starts in
    /// [`RuntimeStatus::Stopped`]; call [`GameRuntime::start`] to run.
    pub fn new(project: &Project, scene_id: Uuid) -> Result<Self, RuntimeError> {
        let scene = project
            .scenes
            .iter()
            .find(|scene| scene.id == scene_id)
            .ok_or_else(|| RuntimeError::SceneNotFound(scene_id.to_string()))?;
        Ok(Self::load(project, scene, RuntimeStatus::Stopped))
    }

    /// Creates a runtime for the project's active scene.
    pub fn for_active_scene(project: &Project) -> Result<Self, RuntimeError> {
        let scene = project.active_scene().ok_or(RuntimeError::NoScenes)?;
        Ok(Self::load(project, scene, RuntimeStatus::Stopped))
    }

    fn load(project: &Project, scene: &Scene, status: RuntimeStatus) -> Self {
        let mut bodies = collect_bodies(scene, &project.symbols);
        attach_tile_colliders(&mut bodies, scene, project);
        let emitters = collect_emitters(scene);
        let mut states = HashMap::new();
        for body in &bodies {
            states.insert(
                body.id,
                BodyState {
                    x: body.x,
                    y: body.y,
                    ..Default::default()
                },
            );
        }
        // World bounds (level editor) override the stage rectangle when set.
        let config = scene_config(scene);
        let (w, h) = config
            .world
            .as_ref()
            .filter(|world| world.width > 0.0 && world.height > 0.0)
            .map(|world| (world.width, world.height))
            .unwrap_or((scene.width, scene.height));
        Self {
            scene: scene.id,
            scene_name: scene.name.clone(),
            bodies,
            states,
            camera: CameraState {
                x: w / 2.0,
                y: h / 2.0,
                zoom: 1.0,
            },
            events: EventBus::new(),
            status,
            time: 0.0,
            config,
            emitters,
            scene_w: w,
            scene_h: h,
            scene_states: HashMap::new(),
            contacts: HashSet::new(),
            prev_input: InputState::default(),
            prev_keys: HashSet::new(),
        }
    }

    /// Switches to the scene named `target` (or matching its id string).
    ///
    /// Lifecycle: the current states are stored per scene, `SceneUnload` and
    /// `SceneTransition`/`SceneLoad` events fire, the new scene's bodies are
    /// recollected and states restored (or seeded fresh). On any error the
    /// runtime is left completely unchanged.
    pub fn goto_scene(&mut self, project: &Project, target: &str) -> Result<(), RuntimeError> {
        let target = target.trim();
        if target.is_empty() {
            return Err(RuntimeError::SceneNotFound(target.to_string()));
        }
        let scene = project
            .scenes
            .iter()
            .find(|scene| scene.name == target || scene.id.to_string() == target)
            .ok_or_else(|| RuntimeError::SceneNotFound(target.to_string()))?;
        if scene.id == self.scene {
            return Ok(());
        }
        let from = self.scene;
        // Persist current scene state, then build the replacement runtime
        // state manually (load() creates a fresh bus — copy listeners over).
        self.scene_states
            .insert(self.scene, std::mem::take(&mut self.states));
        self.contacts.clear();
        self.prev_input = InputState::default();
        self.prev_keys.clear();

        let mut next = Self::load(project, scene, self.status);
        // Preserve per-scene state memory and the event bus across switches.
        next.scene_states = std::mem::take(&mut self.scene_states);
        if let Some(saved) = next.scene_states.get(&scene.id) {
            next.states = saved.clone();
        }
        next.events = std::mem::take(&mut self.events);
        *self = next;

        let from_name = project
            .scenes
            .iter()
            .find(|s| s.id == from)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        self.events.dispatch(&GameEvent {
            kind: GameEventKind::SceneUnload,
            scene: from,
            subject: Some(self.scene),
            label: from_name.clone(),
        });
        self.events.dispatch(&GameEvent {
            kind: GameEventKind::SceneTransition,
            scene: self.scene,
            subject: Some(from),
            label: format!("{} -> {}", from_name, self.scene_name),
        });
        self.events.dispatch(&GameEvent {
            kind: GameEventKind::SceneLoad,
            scene: self.scene,
            subject: None,
            label: self.scene_name.clone(),
        });
        Ok(())
    }

    /// Starts (or resumes from Stopped) the simulation.
    pub fn start(&mut self) {
        if self.status == RuntimeStatus::Stopped {
            self.status = RuntimeStatus::Running;
            self.events.dispatch(&GameEvent {
                kind: GameEventKind::GameStart,
                scene: self.scene,
                subject: None,
                label: self.scene_name.clone(),
            });
        } else if self.status == RuntimeStatus::Paused {
            self.resume();
        }
    }

    /// Pauses the simulation. States are frozen; `step` becomes a no-op.
    pub fn pause(&mut self) {
        if self.status == RuntimeStatus::Running {
            self.status = RuntimeStatus::Paused;
            self.events.dispatch(&GameEvent {
                kind: GameEventKind::GamePause,
                scene: self.scene,
                subject: None,
                label: String::new(),
            });
        }
    }

    /// Resumes a paused simulation.
    pub fn resume(&mut self) {
        if self.status == RuntimeStatus::Paused {
            self.status = RuntimeStatus::Running;
            self.events.dispatch(&GameEvent {
                kind: GameEventKind::GameResume,
                scene: self.scene,
                subject: None,
                label: String::new(),
            });
        }
    }

    /// Stops the simulation and resets it to the freshly loaded scene state
    /// (bodies at authored positions, time zeroed, contacts cleared).
    pub fn stop(&mut self, project: &Project) {
        if self.status == RuntimeStatus::Stopped {
            return;
        }
        self.status = RuntimeStatus::Stopped;
        self.time = 0.0;
        self.reload_current_scene(project);
        self.events.dispatch(&GameEvent {
            kind: GameEventKind::GameStop,
            scene: self.scene,
            subject: None,
            label: self.scene_name.clone(),
        });
    }

    /// Restarts the current scene: states reset to authored positions, time
    /// and contacts cleared, then the simulation keeps running.
    pub fn restart(&mut self, project: &Project) {
        self.reload_current_scene(project);
        self.time = 0.0;
        self.status = RuntimeStatus::Running;
        self.events.dispatch(&GameEvent {
            kind: GameEventKind::GameRestart,
            scene: self.scene,
            subject: None,
            label: self.scene_name.clone(),
        });
    }

    /// Reloads the current scene in place (fresh states at authored
    /// positions) without touching the event bus.
    pub fn reset_scene(&mut self, project: &Project) {
        self.reload_current_scene(project);
    }

    fn reload_current_scene(&mut self, project: &Project) {
        let Some(scene) = project.scenes.iter().find(|s| s.id == self.scene) else {
            return;
        };
        let mut bodies = collect_bodies(scene, &project.symbols);
        attach_tile_colliders(&mut bodies, scene, project);
        self.bodies = bodies;
        self.emitters = collect_emitters(scene);
        self.states.clear();
        for body in &self.bodies {
            self.states.insert(
                body.id,
                BodyState {
                    x: body.x,
                    y: body.y,
                    ..Default::default()
                },
            );
        }
        self.config = scene_config(scene);
        self.scene_w = self
            .config
            .world
            .as_ref()
            .filter(|world| world.width > 0.0 && world.height > 0.0)
            .map(|world| world.width)
            .unwrap_or(scene.width);
        self.scene_h = self
            .config
            .world
            .as_ref()
            .filter(|world| world.width > 0.0 && world.height > 0.0)
            .map(|world| world.height)
            .unwrap_or(scene.height);
        self.camera = CameraState {
            x: self.scene_w / 2.0,
            y: self.scene_h / 2.0,
            zoom: 1.0,
        };
        self.contacts.clear();
        self.prev_input = InputState::default();
        self.prev_keys.clear();
    }

    /// Advances the simulation by `dt` seconds when running.
    ///
    /// `input` carries the logical actions; `key_down` resolves arbitrary
    /// binding/transition key names (the app maps egui keys). `project` is
    /// needed only when a configured scene transition fires.
    pub fn step(
        &mut self,
        project: &Project,
        input: &InputState,
        key_down: &dyn Fn(&str) -> bool,
        dt: f32,
    ) {
        if self.status != RuntimeStatus::Running || dt <= 0.0 {
            return;
        }
        // Collision enter/exit events use pre-integration positions.
        self.update_contacts();
        // Rising edges of logical input actions.
        self.emit_input_edges(input);
        // Configured key-triggered scene transitions (rising edge).
        self.check_transitions(project, key_down);

        let dt = dt.min(0.1);
        step(
            &self.bodies,
            &mut self.states,
            input,
            self.scene_w,
            self.scene_h,
            dt,
        );
        if let Some(camera) = self.config.camera.as_ref() {
            step_camera(
                &mut self.camera,
                camera,
                &self.bodies,
                &self.states,
                (self.scene_w, self.scene_h),
                dt,
                self.time,
            );
        }
        // Particle emitters follow their object (body state when the object
        // also participates in the simulation).
        for index in 0..self.emitters.len() {
            let (id, x, y) = {
                let e = &self.emitters[index];
                (e.id, e.x, e.y)
            };
            let origin = self.states.get(&id).map(|s| (s.x, s.y)).unwrap_or((x, y));
            self.emitters[index].system.update(origin, dt);
        }
        self.time += dt;
        self.prev_input = *input;
        // Track currently held transition keys for edge detection.
        self.prev_keys = self
            .config
            .transitions
            .iter()
            .map(|t| t.key.clone())
            .filter(|key| key_down(key))
            .collect();
    }

    fn update_contacts(&mut self) {
        let mut current: HashSet<(Uuid, Uuid)> = HashSet::new();
        for i in 0..self.bodies.len() {
            for j in (i + 1)..self.bodies.len() {
                let (a, b) = (&self.bodies[i], &self.bodies[j]);
                if a.collision.is_none() && a.physics.is_none() {
                    continue;
                }
                if b.collision.is_none() && b.physics.is_none() {
                    continue;
                }
                if !specs_interact(a, b) {
                    continue;
                }
                let (Some(sa), Some(sb)) = (self.states.get(&a.id), self.states.get(&b.id)) else {
                    continue;
                };
                if pair_overlaps(a, sa, b, sb) {
                    let pair = if a.id < b.id {
                        (a.id, b.id)
                    } else {
                        (b.id, a.id)
                    };
                    current.insert(pair);
                }
            }
        }
        let entered: Vec<(Uuid, Uuid)> = current.difference(&self.contacts).copied().collect();
        let exited: Vec<(Uuid, Uuid)> = self.contacts.difference(&current).copied().collect();
        self.contacts = current;
        for (a, b) in entered {
            self.events.dispatch(&GameEvent {
                kind: GameEventKind::CollisionEnter,
                scene: self.scene,
                subject: Some(b),
                label: format!("{} <> {}", self.name_of(a), self.name_of(b)),
            });
        }
        for (a, b) in exited {
            self.events.dispatch(&GameEvent {
                kind: GameEventKind::CollisionExit,
                scene: self.scene,
                subject: Some(b),
                label: format!("{} <> {}", self.name_of(a), self.name_of(b)),
            });
        }
    }

    fn emit_input_edges(&mut self, input: &InputState) {
        for (now, was, label) in [
            (input.left, self.prev_input.left, "left"),
            (input.right, self.prev_input.right, "right"),
            (input.up, self.prev_input.up, "up"),
            (input.down, self.prev_input.down, "down"),
            (input.jump, self.prev_input.jump, "jump"),
            (input.action, self.prev_input.action, "action"),
        ] {
            if now && !was {
                self.events.dispatch(&GameEvent {
                    kind: GameEventKind::InputPressed,
                    scene: self.scene,
                    subject: None,
                    label: label.to_string(),
                });
            }
        }
    }

    fn check_transitions(&mut self, project: &Project, key_down: &dyn Fn(&str) -> bool) {
        let transitions: Vec<SceneTransition> = self.config.transitions.clone();
        for transition in transitions {
            let pressed = key_down(&transition.key);
            let was_pressed = self.prev_keys.contains(&transition.key);
            if pressed && !was_pressed {
                if let Err(error) = self.goto_scene(project, &transition.target) {
                    // Unknown target: report via the bus, keep running.
                    self.events.dispatch(&GameEvent {
                        kind: GameEventKind::SceneTransition,
                        scene: self.scene,
                        subject: None,
                        label: format!("fehlgeschlagen: {}", error),
                    });
                }
                return; // at most one transition per step
            }
        }
    }

    /// Render-ready particle views per emitter, in world space.
    pub fn particle_views(&self) -> Vec<(Uuid, Vec<crate::particles::ParticleView>)> {
        self.emitters
            .iter()
            .map(|e| {
                let origin = self
                    .states
                    .get(&e.id)
                    .map(|s| (s.x, s.y))
                    .unwrap_or((e.x, e.y));
                (e.id, e.system.view(origin))
            })
            .collect()
    }

    fn name_of(&self, id: Uuid) -> String {
        self.bodies
            .iter()
            .find(|body| body.id == id)
            .map(|body| body.name.clone())
            .unwrap_or_else(|| id.to_string())
    }
}

/// Collects particle emitters from a scene: every object carrying
/// `particles` metadata, merged across frames (last keyframe wins).
fn collect_emitters(scene: &Scene) -> Vec<EmitterRuntime> {
    use crate::model::resolve_scene;
    let mut merged: HashMap<Uuid, EmitterRuntime> = HashMap::new();
    let mut order: Vec<Uuid> = Vec::new();
    for frame in 0..scene.frame_count() {
        for resolved in resolve_scene(scene, frame) {
            let instance = &resolved.instance;
            let Some(spec) = &instance.particles else {
                continue;
            };
            let emitter = EmitterRuntime {
                id: instance.id,
                name: instance.name.clone(),
                x: instance.transform.x,
                y: instance.transform.y,
                system: ParticleSystem::new(spec.clone()),
            };
            if merged.insert(instance.id, emitter).is_none() {
                order.push(instance.id);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|id| merged.remove(&id))
        .collect()
}

/// Whether two bodies' collision specs permit interaction. Tilemap bodies
/// expose their layer/mask through `tiles`; other bodies use their
/// collision spec or the permissive default.
fn specs_interact(a: &GameBody, b: &GameBody) -> bool {
    let spec_a = if let Some(tiles) = &a.tiles {
        crate::model::CollisionSpec {
            layer: tiles.layer,
            mask: tiles.mask,
            ..Default::default()
        }
    } else {
        super::spec_of(a)
    };
    let spec_b = if let Some(tiles) = &b.tiles {
        crate::model::CollisionSpec {
            layer: tiles.layer,
            mask: tiles.mask,
            ..Default::default()
        }
    } else {
        super::spec_of(b)
    };
    spec_a.interacts_with(&spec_b)
}

/// Axis-aligned overlap test between two bodies at their current states.
/// Uses the same SAT geometry as the simulation (explicit collision spec,
/// otherwise visual size).
fn pair_overlaps(a: &GameBody, sa: &BodyState, b: &GameBody, sb: &BodyState) -> bool {
    let ca = super::body_collider(a, (sa.x, sa.y));
    let cb = super::body_collider(b, (sb.x, sb.y));
    crate::collision::overlaps(&ca, &cb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GameObjectConfig, ObjectInstance, ObjectKind, Rgba};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn project_with_scenes(names: &[&str]) -> Project {
        let mut project = Project::new_empty("RT");
        project.scenes.clear();
        for name in names {
            let mut scene = Scene::new(*name, 800.0, 600.0);
            scene.layers[0].frames[0].objects.push(ObjectInstance {
                id: Uuid::new_v4(),
                name: format!("Box-{name}"),
                kind: ObjectKind::Rect {
                    width: 40.0,
                    height: 40.0,
                    corner_radius: 0.0,
                },
                transform: Default::default(),
                fill: Rgba::WHITE,
                script: None,
                game: Some(GameObjectConfig {
                    collision: Some(crate::model::CollisionSpec {
                        shape: crate::model::CollisionShape::Rectangle,
                        width: 40.0,
                        height: 40.0,
                        ..Default::default()
                    }),
                    physics: Some(crate::model::PhysicsSpec {
                        body: crate::model::BodyType::Dynamic,
                        gravity_scale: 1.0,
                        friction: 0.0,
                        bounce: 0.0,
                        controlled: false,
                    }),
                }),
                source_character: None,
                particles: None,
                bone: None,
            });
            project.scenes.push(scene);
        }
        project.active_scene = project.scenes.first().map(|s| s.id);
        project
    }

    fn no_keys(_: &str) -> bool {
        false
    }

    #[test]
    fn runtime_lifecycle_events_fire_in_order() {
        let project = project_with_scenes(&["A", "B"]);
        let scene_id = project.scenes[0].id;
        let mut runtime = GameRuntime::new(&project, scene_id).unwrap();
        let log = Rc::new(RefCell::new(Vec::new()));
        let log2 = log.clone();
        runtime.events.register_all(move |e| {
            log2.borrow_mut().push(format!("{:?}", e.kind));
        });
        runtime.start();
        runtime.pause();
        runtime.resume();
        runtime.stop(&project);
        assert_eq!(
            log.borrow().as_slice(),
            ["GameStart", "GamePause", "GameResume", "GameStop"]
        );
        assert_eq!(runtime.status, RuntimeStatus::Stopped);
        assert_eq!(runtime.time, 0.0);
    }

    #[test]
    fn pause_freezes_simulation_and_resume_continues() {
        let project = project_with_scenes(&["A"]);
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        runtime.step(&project, &InputState::default(), &no_keys, 0.016);
        let y_after_one = runtime.states.values().next().unwrap().y;
        runtime.pause();
        runtime.step(&project, &InputState::default(), &no_keys, 0.5);
        assert_eq!(
            runtime.states.values().next().unwrap().y,
            y_after_one,
            "paused step must not move bodies"
        );
        assert_eq!(runtime.time, 0.016);
        runtime.resume();
        runtime.step(&project, &InputState::default(), &no_keys, 0.016);
        assert!(runtime.time > 0.016);
    }

    #[test]
    fn restart_resets_positions_and_time() {
        let project = project_with_scenes(&["A"]);
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        for _ in 0..10 {
            runtime.step(&project, &InputState::default(), &no_keys, 0.016);
        }
        let moved = runtime.states.values().next().unwrap().y;
        assert!(moved > 0.0, "gravity should have moved the body");
        runtime.restart(&project);
        assert_eq!(runtime.time, 0.0);
        let body = &runtime.bodies[0];
        assert_eq!(runtime.states[&body.id].y, body.y);
    }

    #[test]
    fn goto_scene_switches_and_fires_lifecycle_events() {
        let project = project_with_scenes(&["Menu", "Level"]);
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        let events = Rc::new(RefCell::new(Vec::new()));
        let e2 = events.clone();
        runtime.events.register_all(move |e| {
            e2.borrow_mut().push(e.kind);
        });
        runtime.goto_scene(&project, "Level").unwrap();
        assert_eq!(runtime.scene_name, "Level");
        assert!(runtime.bodies.iter().any(|b| b.name == "Box-Level"));
        let kinds = events.borrow();
        assert!(kinds.contains(&GameEventKind::SceneUnload));
        assert!(kinds.contains(&GameEventKind::SceneTransition));
        assert!(kinds.contains(&GameEventKind::SceneLoad));
    }

    #[test]
    fn goto_scene_rejects_unknown_target_without_corrupting_state() {
        let project = project_with_scenes(&["A"]);
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        let before_scene = runtime.scene;
        let before_bodies = runtime.bodies.len();
        let err = runtime.goto_scene(&project, "ExistiertNicht").unwrap_err();
        assert!(matches!(err, RuntimeError::SceneNotFound(_)));
        assert_eq!(runtime.scene, before_scene);
        assert_eq!(runtime.bodies.len(), before_bodies);
        assert_eq!(runtime.status, RuntimeStatus::Running);
    }

    #[test]
    fn scene_states_are_restored_on_return() {
        let project = project_with_scenes(&["A", "B"]);
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        for _ in 0..5 {
            runtime.step(&project, &InputState::default(), &no_keys, 0.016);
        }
        let y_moved = runtime.states.values().next().unwrap().y;
        runtime.goto_scene(&project, "B").unwrap();
        runtime.goto_scene(&project, "A").unwrap();
        assert_eq!(runtime.states.values().next().unwrap().y, y_moved);
    }

    #[test]
    fn event_bus_supports_multiple_and_removed_listeners() {
        let mut bus = EventBus::new();
        let count = Rc::new(RefCell::new(0));
        let c1 = count.clone();
        let c2 = count.clone();
        let id1 = bus.register(GameEventKind::GameStart, move |_| *c1.borrow_mut() += 1);
        bus.register(GameEventKind::GameStart, move |_| *c2.borrow_mut() += 1);
        bus.register(GameEventKind::GameStop, move |_| {});
        let event = GameEvent {
            kind: GameEventKind::GameStart,
            scene: Uuid::nil(),
            subject: None,
            label: String::new(),
        };
        bus.dispatch(&event);
        assert_eq!(*count.borrow(), 2);
        assert!(bus.remove(id1));
        bus.dispatch(&event);
        assert_eq!(*count.borrow(), 3);
        assert!(!bus.remove(9999));
    }

    #[test]
    fn recursive_dispatch_is_depth_limited() {
        let mut bus = EventBus::new();
        bus.register(GameEventKind::GameStart, |e| {
            // Re-dispatch the same event recursively.
            // SAFETY-free approach: capture bus via raw pointer is unsound;
            // instead this test uses a second bus owned outside? Listeners
            // cannot borrow the bus mutably. Use a thread_local recursion
            // counter instead — see below.
            let _ = e;
        });
        // Real recursion test: a listener that dispatches through a shared
        // RefCell<EventBus> is impossible (borrow rules). The depth guard is
        // exercised by dispatching from within a listener via a second bus
        // handle stored in a RefCell is also impossible. Instead verify the
        // guard counter directly.
        bus.depth = MAX_DISPATCH_DEPTH;
        bus.dispatch(&GameEvent {
            kind: GameEventKind::GameStart,
            scene: Uuid::nil(),
            subject: None,
            label: String::new(),
        });
        assert_eq!(bus.dropped_events, 1);
        assert_eq!(bus.listener_count(GameEventKind::GameStart), 1);
    }

    #[test]
    fn collision_events_fire_on_enter_and_exit() {
        let project = project_with_scenes(&["A"]);
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        let events = Rc::new(RefCell::new(Vec::new()));
        let e2 = events.clone();
        runtime
            .events
            .register(GameEventKind::CollisionEnter, move |e| {
                e2.borrow_mut().push(("enter", e.label.clone()));
            });
        let e3 = events.clone();
        runtime
            .events
            .register(GameEventKind::CollisionExit, move |e| {
                e3.borrow_mut().push(("exit", e.label.clone()));
            });
        // Only one body exists; add a second overlapping body manually.
        let other = ObjectInstance {
            id: Uuid::new_v4(),
            name: "Other".into(),
            kind: ObjectKind::Rect {
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
            transform: Default::default(),
            fill: Rgba::WHITE,
            script: None,
            source_character: None,
            game: Some(GameObjectConfig {
                collision: Some(crate::model::CollisionSpec {
                    shape: crate::model::CollisionShape::Rectangle,
                    width: 40.0,
                    height: 40.0,
                    ..Default::default()
                }),
                physics: None,
            }),
            particles: None,
            bone: None,
        };
        // Inject the body directly into the runtime snapshot.
        let body = GameBody {
            id: other.id,
            name: other.name.clone(),
            x: runtime.bodies[0].x,
            y: runtime.bodies[0].y,
            width: 40.0,
            height: 40.0,
            collision: other.game.as_ref().unwrap().collision.clone(),
            physics: None,
            tiles: None,
        };
        runtime.bodies.push(body);
        runtime.states.insert(
            other.id,
            BodyState {
                x: runtime.bodies[0].x,
                y: runtime.bodies[0].y,
                ..Default::default()
            },
        );
        runtime.step(&project, &InputState::default(), &no_keys, 0.016);
        assert!(events.borrow().iter().any(|(k, _)| *k == "enter"));
        // Move the other body far away; next step reports exit.
        runtime.states.get_mut(&other.id).unwrap().x += 500.0;
        runtime.step(&project, &InputState::default(), &no_keys, 0.016);
        assert!(events.borrow().iter().any(|(k, _)| *k == "exit"));
    }

    #[test]
    fn collision_events_respect_layer_mask() {
        let project = project_with_scenes(&["A"]);
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        let events = Rc::new(RefCell::new(Vec::new()));
        let e2 = events.clone();
        runtime
            .events
            .register(GameEventKind::CollisionEnter, move |e| {
                e2.borrow_mut().push(e.label.clone());
            });
        // Overlapping body on layer 2 whose mask does not include layer 1:
        // pairs must not interact, so no enter event may fire.
        let other = ObjectInstance {
            id: Uuid::new_v4(),
            name: "Other".into(),
            kind: ObjectKind::Rect {
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
            transform: Default::default(),
            fill: Rgba::WHITE,
            script: None,
            source_character: None,
            game: Some(GameObjectConfig {
                collision: Some(crate::model::CollisionSpec {
                    shape: crate::model::CollisionShape::Rectangle,
                    width: 40.0,
                    height: 40.0,
                    layer: 2,
                    mask: 2, // does not see layer 1
                }),
                physics: None,
            }),
            particles: None,
            bone: None,
        };
        runtime.bodies.push(GameBody {
            id: other.id,
            name: other.name.clone(),
            x: runtime.bodies[0].x,
            y: runtime.bodies[0].y,
            width: 40.0,
            height: 40.0,
            collision: other.game.as_ref().unwrap().collision.clone(),
            physics: None,
            tiles: None,
        });
        runtime.states.insert(
            other.id,
            BodyState {
                x: runtime.bodies[0].x,
                y: runtime.bodies[0].y,
                ..Default::default()
            },
        );
        runtime.step(&project, &InputState::default(), &no_keys, 0.016);
        assert!(
            events.borrow().is_empty(),
            "mask-filtered pair must not emit collision events: {:?}",
            events.borrow()
        );
    }

    #[test]
    fn key_triggered_transition_fires_once_per_press() {
        let mut project = project_with_scenes(&["Menu", "Game"]);
        // Add a transition on the Menu scene: Enter -> Game.
        {
            let scene = &mut project.scenes[0];
            scene.game = Some(GameSceneConfig {
                transitions: vec![SceneTransition {
                    key: "Enter".into(),
                    target: "Game".into(),
                }],
                ..Default::default()
            });
        }
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        let pressed = |key: &str| key == "Enter";
        runtime.step(&project, &InputState::default(), &pressed, 0.016);
        assert_eq!(runtime.scene_name, "Game");
        // Still holding Enter: no repeated transition.
        runtime.step(&project, &InputState::default(), &pressed, 0.016);
        assert_eq!(runtime.scene_name, "Game");
    }

    #[test]
    fn failed_transition_keeps_runtime_running() {
        let mut project = project_with_scenes(&["A"]);
        {
            let scene = &mut project.scenes[0];
            scene.game = Some(GameSceneConfig {
                transitions: vec![SceneTransition {
                    key: "Space".into(),
                    target: "Missing".into(),
                }],
                ..Default::default()
            });
        }
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        let pressed = |key: &str| key == "Space";
        runtime.step(&project, &InputState::default(), &pressed, 0.016);
        assert_eq!(runtime.scene_name, "A");
        assert_eq!(runtime.status, RuntimeStatus::Running);
    }

    #[test]
    fn delta_time_is_clamped_and_accumulates() {
        let project = project_with_scenes(&["A"]);
        let mut runtime = GameRuntime::new(&project, project.scenes[0].id).unwrap();
        runtime.start();
        runtime.step(&project, &InputState::default(), &no_keys, 5.0);
        assert_eq!(runtime.time, 0.1, "dt clamped to 100 ms");
        runtime.step(&project, &InputState::default(), &no_keys, 0.016);
        assert!((runtime.time - 0.116).abs() < 0.001);
    }
}
