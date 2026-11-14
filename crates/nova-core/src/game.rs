//! Editor-side game preview simulation.
//!
//! This is a deliberately small, deterministic 2D simulation used by the
//! editor's game preview and exercised by unit tests. It is NOT a Flash
//! runtime: AS3 game logic runs only inside SWF players (Ruffle/Adobe). The
//! simulation consumes the optional game metadata stored in the document
//! (`ObjectInstance.game`, `Scene.game`) and nothing else.
//!
//! Scope and simplifications (documented, not hidden):
//! - Collision pairs are dynamic-vs-static/kinematic only; two dynamic bodies
//!   never resolve against each other.
//! - Circle-vs-rectangle pairs are approximated by the circle's bounding box.
//! - Rotation is ignored; all volumes are axis-aligned.
//! - Only objects carrying game metadata participate at all.

pub mod runtime;

use std::collections::HashMap;

use crate::model::{
    BodyType, CollisionShape, CollisionSpec, GameSceneConfig, PhysicsSpec, Scene, Symbol,
    resolve_scene,
};
use uuid::Uuid;

/// Pixels per second squared applied as base gravity.
pub const GRAVITY: f32 = 2200.0;
/// Horizontal acceleration applied to controlled dynamic bodies.
const MOVE_ACCEL: f32 = 2600.0;
/// Jump impulse applied to controlled dynamic bodies.
const JUMP_VELOCITY: f32 = -560.0;
/// Maximum horizontal speed of controlled bodies.
const MAX_SPEED: f32 = 320.0;
/// Horizontal ground friction applied while a controlled body is on ground.
const GROUND_FRICTION: f32 = 10.0;
/// Camera follow lerp factor per second.
const CAMERA_LERP: f32 = 6.0;

/// Logical input state for one simulation step.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InputState {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub jump: bool,
    pub action: bool,
}

/// Persistent per-body simulation state, keyed by object instance id.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BodyState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
}

/// Camera state in world coordinates (centre point + zoom).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraState {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

/// A game object collected from the scene for simulation.
#[derive(Debug, Clone)]
pub struct GameBody {
    pub id: Uuid,
    pub name: String,
    /// Authored world position (object transform is centre-based).
    pub x: f32,
    pub y: f32,
    /// Visual extents in pixels, used when no collision spec exists.
    pub width: f32,
    pub height: f32,
    pub collision: Option<CollisionSpec>,
    pub physics: Option<PhysicsSpec>,
    /// Solid-tile collider for tilemap objects (filled by
    /// [`attach_tile_colliders`]); `None` for regular objects.
    pub tiles: Option<TileCollider>,
}

/// Solid-cell grid extracted from a tilemap object for collision tests.
/// Cell coordinates are in tile units; world position is derived from the
/// owning body's state minus half the grid size (objects are centre-based).
#[derive(Debug, Clone)]
pub struct TileCollider {
    pub columns: usize,
    pub rows: usize,
    pub tile_width: f32,
    pub tile_height: f32,
    /// Row-major solidity flags (true = solid), length columns*rows.
    pub solid: Vec<bool>,
}

impl TileCollider {
    /// World-space rect of cell (cx, cy) given the body centre (x, y).
    fn cell_rect(&self, cx: usize, cy: usize, x: f32, y: f32) -> (f32, f32, f32, f32) {
        let left = x - self.columns as f32 * self.tile_width / 2.0 + cx as f32 * self.tile_width;
        let top = y - self.rows as f32 * self.tile_height / 2.0 + cy as f32 * self.tile_height;
        (left, top, self.tile_width, self.tile_height)
    }
}

/// Collects the effective game objects of a scene: every object that carries
/// game metadata, merged across all frames (the last keyframe wins, matching
/// how later placements override earlier ones).
pub fn collect_bodies(scene: &Scene, symbols: &[Symbol]) -> Vec<GameBody> {
    let mut merged: HashMap<Uuid, GameBody> = HashMap::new();
    let mut order: Vec<Uuid> = Vec::new();
    for frame in 0..scene.frame_count() {
        for resolved in resolve_scene(scene, frame) {
            let instance = &resolved.instance;
            // Tilemap objects participate implicitly: they become static
            // bodies so `attach_tile_colliders` can turn their solid cells
            // into colliders — no game metadata required.
            if instance.game.is_none() {
                if let crate::model::ObjectKind::Tilemap(map) = &instance.kind {
                    let body = GameBody {
                        id: instance.id,
                        name: instance.name.clone(),
                        x: instance.transform.x,
                        y: instance.transform.y,
                        width: map.width(),
                        height: map.height(),
                        collision: None,
                        physics: None,
                        tiles: None,
                    };
                    if merged.insert(instance.id, body).is_none() {
                        order.push(instance.id);
                    }
                }
                continue;
            }
            let game = instance.game.as_ref().unwrap();
            if game.collision.is_none() && game.physics.is_none() {
                continue;
            }
            let fallback = game
                .collision
                .as_ref()
                .map(|c| (c.width, c.height))
                .unwrap_or((32.0, 32.0));
            let (width, height) = resolved.size_hint(symbols).unwrap_or(fallback);
            let body = GameBody {
                id: instance.id,
                name: instance.name.clone(),
                x: instance.transform.x,
                y: instance.transform.y,
                width,
                height,
                collision: game.collision.clone(),
                physics: game.physics,
                tiles: None,
            };
            if merged.insert(instance.id, body).is_none() {
                order.push(instance.id);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|id| merged.remove(&id))
        .collect()
}

/// Collision extents: explicit spec wins, otherwise the visual size.
/// Polygon colliders use their bounding box in the preview (documented).
fn extent(body: &GameBody) -> (f32, f32, CollisionShape) {
    match &body.collision {
        Some(c) => (c.width.max(1.0), c.height.max(1.0), c.shape.clone()),
        None => (
            body.width.max(1.0),
            body.height.max(1.0),
            CollisionShape::Rectangle,
        ),
    }
}

/// One simulation step. Mutates `states` in place; `dt` is seconds (clamped
/// to 100 ms so tab-switches cannot tunnel bodies through walls).
pub fn step(
    bodies: &[GameBody],
    states: &mut HashMap<Uuid, BodyState>,
    input: &InputState,
    scene_w: f32,
    scene_h: f32,
    dt: f32,
) {
    let dt = dt.clamp(0.0, 0.1);
    if dt <= 0.0 {
        return;
    }

    // Integrate forces, damping and controlled input.
    for body in bodies {
        let state = states.entry(body.id).or_insert(BodyState {
            x: body.x,
            y: body.y,
            ..Default::default()
        });
        let Some(physics) = body.physics else {
            continue;
        };
        match physics.body {
            BodyType::Static => continue,
            BodyType::Kinematic => {}
            BodyType::Dynamic => {
                state.vy += GRAVITY * physics.gravity_scale * dt;
                let damping = (1.0 - physics.friction * dt).clamp(0.0, 1.0);
                state.vx *= damping;
                state.vy *= damping;
                if physics.controlled {
                    if input.left {
                        state.vx -= MOVE_ACCEL * dt;
                    }
                    if input.right {
                        state.vx += MOVE_ACCEL * dt;
                    }
                    state.vx = state.vx.clamp(-MAX_SPEED, MAX_SPEED);
                    if input.jump && state.grounded {
                        state.vy = JUMP_VELOCITY;
                        state.grounded = false;
                    }
                    if state.grounded {
                        state.vx *= (1.0 - GROUND_FRICTION * dt).clamp(0.0, 1.0);
                    }
                }
            }
        }
        state.x += state.vx * dt;
        state.y += state.vy * dt;
        state.grounded = false;
    }

    // Collisions: dynamic bodies vs static/kinematic colliders.
    for i in 0..bodies.len() {
        let Some(physics_i) = bodies[i].physics else {
            continue;
        };
        if physics_i.body != BodyType::Dynamic || bodies[i].collision.is_none() {
            continue;
        }
        for j in 0..bodies.len() {
            if i == j {
                continue;
            }
            if let Some(tiles) = &bodies[j].tiles {
                resolve_tiles(bodies, states, i, j, tiles);
                continue;
            }
            let (Some(ci), Some(cj)) = (&bodies[i].collision, &bodies[j].collision) else {
                continue;
            };
            if !ci.interacts_with(cj) {
                continue;
            }
            if bodies[j]
                .physics
                .is_some_and(|p| p.body == BodyType::Dynamic)
            {
                continue;
            }
            resolve_pair(bodies, states, i, j);
        }
    }

    // Keep dynamic bodies inside the stage rectangle.
    for body in bodies {
        if body.physics.map(|p| p.body) != Some(BodyType::Dynamic) {
            continue;
        }
        let (w, h, _) = extent(body);
        let state = states.get_mut(&body.id).expect("state created above");
        state.x = state.x.clamp(w / 2.0, (scene_w - w / 2.0).max(w / 2.0));
        state.y = state.y.clamp(h / 2.0, (scene_h - h / 2.0).max(h / 2.0));
    }
}

fn resolve_pair(bodies: &[GameBody], states: &mut HashMap<Uuid, BodyState>, i: usize, j: usize) {
    let a = &bodies[i];
    let b = &bodies[j];
    // BodyState is Copy; take both out to satisfy the borrow checker.
    let mut sa = states.remove(&a.id).unwrap_or(BodyState {
        x: a.x,
        y: a.y,
        ..Default::default()
    });
    let sb = states.remove(&b.id).unwrap_or(BodyState {
        x: b.x,
        y: b.y,
        ..Default::default()
    });
    let (ax, ay) = (sa.x, sa.y);
    let (bx, by) = (sb.x, sb.y);

    let (aw, ah, ashape) = extent(a);
    let (bw, bh, bshape) = extent(b);

    let contact = match (ashape, bshape) {
        (CollisionShape::Circle, CollisionShape::Circle) => {
            let ra = aw / 2.0;
            let rb = bw / 2.0;
            let dx = ax - bx;
            let dy = ay - by;
            let dist = (dx * dx + dy * dy).sqrt();
            let overlap = ra + rb - dist;
            if overlap <= 0.0 {
                states.insert(a.id, sa);
                states.insert(b.id, sb);
                return;
            }
            if dist < 0.001 {
                Some((0.0, -1.0, overlap))
            } else {
                Some((dx / dist, dy / dist, overlap))
            }
        }
        // Mixed circle/rect pairs use the bounding-box approximation.
        _ => rect_overlap(ax, ay, aw, ah, bx, by, bw, bh),
    };
    let Some((nx, ny, depth)) = contact else {
        states.insert(a.id, sa);
        states.insert(b.id, sb);
        return;
    };

    let bounce = a.physics.map(|p| p.bounce).unwrap_or(0.0);
    let friction = a.physics.map(|p| p.friction).unwrap_or(0.0);

    // Separate along the contact normal, then reflect the normal velocity.
    sa.x += nx * depth;
    sa.y += ny * depth;
    let vn = sa.vx * nx + sa.vy * ny;
    if vn < 0.0 {
        sa.vx -= (1.0 + bounce) * vn * nx;
        sa.vy -= (1.0 + bounce) * vn * ny;
        let tx = -ny;
        let ty = nx;
        let vt = sa.vx * tx + sa.vy * ty;
        sa.vx -= vt * friction.min(1.0) * tx;
        sa.vy -= vt * friction.min(1.0) * ty;
        if ny < -0.5 {
            sa.grounded = true;
        }
    }
    states.insert(a.id, sa);
    states.insert(b.id, sb);
}

/// Resolves dynamic body `i` against the solid cells of tilemap body `j`.
/// Only cells overlapping the body's AABB are tested; resolution uses the
/// same min-axis push-out + restitution as [`resolve_pair`]. Tile collision
/// ignores layer/mask (tileset solidity is binary) — documented limitation.
fn resolve_tiles(
    bodies: &[GameBody],
    states: &mut HashMap<Uuid, BodyState>,
    i: usize,
    j: usize,
    tiles: &TileCollider,
) {
    let a = &bodies[i];
    let b = &bodies[j];
    let mut sa = states.remove(&a.id).unwrap_or(BodyState {
        x: a.x,
        y: a.y,
        ..Default::default()
    });
    let sb = states.remove(&b.id).unwrap_or(BodyState {
        x: b.x,
        y: b.y,
        ..Default::default()
    });
    let (aw, ah, _) = extent(a);
    let left = sa.x - aw / 2.0;
    let top = sa.y - ah / 2.0;
    let right = sa.x + aw / 2.0;
    let bottom = sa.y + ah / 2.0;

    // Grid origin in world space: tilemap body centre minus half grid size.
    let grid_left = sb.x - tiles.columns as f32 * tiles.tile_width / 2.0;
    let grid_top = sb.y - tiles.rows as f32 * tiles.tile_height / 2.0;
    let min_cx = ((left - grid_left) / tiles.tile_width).floor().max(0.0) as usize;
    let max_cx = (((right - grid_left) / tiles.tile_width).ceil() as usize).min(tiles.columns);
    let min_cy = ((top - grid_top) / tiles.tile_height).floor().max(0.0) as usize;
    let max_cy = (((bottom - grid_top) / tiles.tile_height).ceil() as usize).min(tiles.rows);

    let bounce = a.physics.map(|p| p.bounce).unwrap_or(0.0);
    let friction = a.physics.map(|p| p.friction).unwrap_or(0.0);

    for cy in min_cy..max_cy {
        for cx in min_cx..max_cx {
            if !tiles
                .solid
                .get(cy * tiles.columns + cx)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let (cl, ct, cw, ch) = tiles.cell_rect(cx, cy, sb.x, sb.y);
            let contact = rect_overlap(sa.x, sa.y, aw, ah, cl + cw / 2.0, ct + ch / 2.0, cw, ch);
            let Some((nx, ny, depth)) = contact else {
                continue;
            };
            sa.x += nx * depth;
            sa.y += ny * depth;
            let vn = sa.vx * nx + sa.vy * ny;
            if vn < 0.0 {
                sa.vx -= (1.0 + bounce) * vn * nx;
                sa.vy -= (1.0 + bounce) * vn * ny;
                let tx = -ny;
                let ty = nx;
                let vt = sa.vx * tx + sa.vy * ty;
                sa.vx -= vt * friction.min(1.0) * tx;
                sa.vy -= vt * friction.min(1.0) * ty;
                if ny < -0.5 {
                    sa.grounded = true;
                }
            }
        }
    }
    states.insert(a.id, sa);
    states.insert(b.id, sb);
}

/// Fills `GameBody::tiles` for every tilemap object in `bodies` that uses a
/// tileset with at least one solid tile. Needs the full project because the
/// tileset (not the tilemap) stores the solidity flags.
pub fn attach_tile_colliders(
    bodies: &mut [GameBody],
    scene: &Scene,
    project: &crate::model::Project,
) {
    use crate::model::ObjectKind;
    // Map object id -> TilemapData for every tilemap instance on the scene.
    let mut maps: HashMap<Uuid, crate::model::TilemapData> = HashMap::new();
    for frame in 0..scene.frame_count() {
        for resolved in resolve_scene(scene, frame) {
            if let ObjectKind::Tilemap(data) = &resolved.instance.kind {
                maps.insert(resolved.instance.id, data.clone());
            }
        }
    }
    if maps.is_empty() {
        return;
    }
    for body in bodies.iter_mut() {
        let Some(map) = maps.get(&body.id) else {
            continue;
        };
        let Some(tileset) = project.tilesets.iter().find(|t| t.id == map.tileset) else {
            continue;
        };
        if tileset.solid.iter().all(|solid| !solid) {
            continue;
        }
        let mut solid = vec![false; map.columns * map.rows];
        for layer in &map.layers {
            for (index, cell) in layer.cells.iter().enumerate() {
                if *cell > 0 && tileset.is_solid(*cell as usize - 1) {
                    solid[index] = true;
                }
            }
        }
        if solid.iter().any(|s| *s) {
            body.tiles = Some(TileCollider {
                columns: map.columns,
                rows: map.rows,
                tile_width: map.tile_width,
                tile_height: map.tile_height,
                solid,
            });
        }
    }
}

/// AABB overlap; returns the contact normal pointing from b to a plus depth.
fn rect_overlap(
    ax: f32,
    ay: f32,
    aw: f32,
    ah: f32,
    bx: f32,
    by: f32,
    bw: f32,
    bh: f32,
) -> Option<(f32, f32, f32)> {
    let dx = ax - bx;
    let px = (aw + bw) / 2.0 - dx.abs();
    if px <= 0.0 {
        return None;
    }
    let dy = ay - by;
    let py = (ah + bh) / 2.0 - dy.abs();
    if py <= 0.0 {
        return None;
    }
    if px < py {
        Some((if dx >= 0.0 { 1.0 } else { -1.0 }, 0.0, px))
    } else {
        Some((0.0, if dy >= 0.0 { 1.0 } else { -1.0 }, py))
    }
}

/// Advances the camera one step: follows `follow` (object name) or holds the
/// stage centre, applies dead-zone and smoothing, adds deterministic shake
/// and clamps inside the configured world bounds. `time` is the accumulated
/// simulation time used for the shake phase.
pub fn step_camera(
    camera: &mut CameraState,
    config: &crate::model::CameraConfig,
    bodies: &[GameBody],
    states: &HashMap<Uuid, BodyState>,
    scene_w: f32,
    scene_h: f32,
    dt: f32,
    time: f32,
) {
    if !config.enabled {
        *camera = CameraState {
            x: scene_w / 2.0,
            y: scene_h / 2.0,
            zoom: 1.0,
        };
        return;
    }
    let target = if config.follow.is_empty() {
        (scene_w / 2.0, scene_h / 2.0)
    } else {
        bodies
            .iter()
            .find(|b| b.name == config.follow)
            .map(|b| {
                let s = states.get(&b.id);
                (s.map(|s| s.x).unwrap_or(b.x), s.map(|s| s.y).unwrap_or(b.y))
            })
            .unwrap_or((scene_w / 2.0, scene_h / 2.0))
    };
    // Dead-zone: the camera only tracks axes on which the target leaves the
    // configured box around the current centre.
    let (dz_x, dz_y) = config.dead_zone;
    let mut tx = target.0;
    let mut ty = target.1;
    if dz_x > 0.0 && (tx - camera.x).abs() <= dz_x {
        tx = camera.x;
    }
    if dz_y > 0.0 && (ty - camera.y).abs() <= dz_y {
        ty = camera.y;
    }
    // follow_smoothing 0 = instant (previous behaviour), approaching 1 = laggy.
    let smooth = config.follow_smoothing.clamp(0.0, 0.95);
    let t = (CAMERA_LERP * dt * (1.0 - smooth)).min(1.0);
    camera.x += (tx - camera.x) * t;
    camera.y += (ty - camera.y) * t;
    camera.zoom = config.zoom.max(0.1);

    let half_w = scene_w / (2.0 * camera.zoom);
    let half_h = scene_h / (2.0 * camera.zoom);
    if config.right > config.left {
        let lo = config.left + half_w;
        let hi = config.right - half_w;
        if lo <= hi {
            camera.x = camera.x.clamp(lo, hi);
        }
    }
    if config.bottom > config.top {
        let lo = config.top + half_h;
        let hi = config.bottom - half_h;
        if lo <= hi {
            camera.y = camera.y.clamp(lo, hi);
        }
    }

    // Deterministic shake: two incommensurate sines give a stable jitter that
    // does not depend on wall-clock time or RNG state.
    if config.shake > 0.0 {
        let s = config.shake;
        camera.x += (time * 39.7).sin() * s;
        camera.y += (time * 47.3 + 1.7).sin() * s * 0.8;
    }
}

/// Scene game config accessor kept separate so callers handle `None`.
pub fn scene_config(scene: &Scene) -> GameSceneConfig {
    scene.game.clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CollisionSpec, Frame, GameObjectConfig, Layer, ObjectInstance, ObjectKind, Rgba, Scene,
        Transform,
    };

    fn game_object(
        name: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        game: GameObjectConfig,
    ) -> ObjectInstance {
        let mut object = ObjectInstance::rect(name, x, y, w, h, Rgba::WHITE);
        object.game = Some(game);
        object
    }

    fn physics(body: BodyType, controlled: bool) -> Option<PhysicsSpec> {
        Some(PhysicsSpec {
            body,
            controlled,
            ..Default::default()
        })
    }

    fn scene_with(objects: Vec<ObjectInstance>) -> Scene {
        let mut scene = Scene::new("Game", 400.0, 300.0);
        scene.fps = 60.0;
        let layer = &mut scene.layers[0];
        layer.frames = vec![Frame {
            keyframe: true,
            objects,
            tween: None,
            label: None,
            sound: None,
        }];
        scene
    }

    fn dynamic_player() -> GameObjectConfig {
        GameObjectConfig {
            collision: Some(CollisionSpec {
                shape: CollisionShape::Rectangle,
                width: 20.0,
                height: 20.0,
                ..Default::default()
            }),
            physics: physics(BodyType::Dynamic, true),
        }
    }

    fn static_floor(y: f32) -> GameObjectConfig {
        GameObjectConfig {
            collision: Some(CollisionSpec {
                shape: CollisionShape::Rectangle,
                width: 400.0,
                height: 20.0,
                ..Default::default()
            }),
            physics: physics(BodyType::Static, false),
        }
    }

    #[test]
    fn dynamic_body_falls_and_lands_on_static_floor() {
        let scene = scene_with(vec![
            game_object("Spieler", 200.0, 50.0, 20.0, 20.0, dynamic_player()),
            game_object("Boden", 200.0, 290.0, 400.0, 20.0, static_floor(290.0)),
        ]);
        let bodies = collect_bodies(&scene, &[]);
        assert_eq!(bodies.len(), 2);
        let mut states = HashMap::new();
        for _ in 0..120 {
            step(
                &bodies,
                &mut states,
                &InputState::default(),
                400.0,
                300.0,
                1.0 / 60.0,
            );
        }
        let player = bodies.iter().find(|b| b.name == "Spieler").unwrap();
        let state = states.get(&player.id).unwrap();
        // Floor top is at y=280; player half-height 10 -> rest y = 270.
        assert!((state.y - 270.0).abs() < 2.0, "y={}", state.y);
        assert!(state.grounded);
        assert!(state.vy.abs() < 1.0, "vy={}", state.vy);
    }

    #[test]
    fn controlled_body_moves_right_with_input() {
        let scene = scene_with(vec![game_object(
            "Spieler",
            100.0,
            100.0,
            20.0,
            20.0,
            dynamic_player(),
        )]);
        let bodies = collect_bodies(&scene, &[]);
        let mut states = HashMap::new();
        let input = InputState {
            right: true,
            ..Default::default()
        };
        for _ in 0..30 {
            step(&bodies, &mut states, &input, 400.0, 300.0, 1.0 / 60.0);
        }
        let player = &bodies[0];
        let state = states.get(&player.id).unwrap();
        assert!(state.x > 140.0, "x={}", state.x);
    }

    #[test]
    fn objects_without_game_metadata_are_ignored() {
        let scene = scene_with(vec![ObjectInstance::rect(
            "Deko",
            10.0,
            10.0,
            50.0,
            50.0,
            Rgba::WHITE,
        )]);
        assert!(collect_bodies(&scene, &[]).is_empty());
    }

    #[test]
    fn circle_collision_resolves_overlap() {
        let mut mover_game = dynamic_player();
        mover_game.collision = Some(CollisionSpec {
            shape: CollisionShape::Circle,
            width: 20.0,
            height: 20.0,
            ..Default::default()
        });
        let mut target_game = static_floor(200.0);
        target_game.collision = Some(CollisionSpec {
            shape: CollisionShape::Circle,
            width: 20.0,
            height: 20.0,
            ..Default::default()
        });
        let scene = scene_with(vec![
            game_object("Mover", 100.0, 100.0, 20.0, 20.0, mover_game),
            game_object("Target", 105.0, 100.0, 20.0, 20.0, target_game),
        ]);
        let bodies = collect_bodies(&scene, &[]);
        let mover = bodies.iter().find(|b| b.name == "Mover").unwrap();
        let target = bodies.iter().find(|b| b.name == "Target").unwrap();
        let mut states = HashMap::new();
        step(
            &bodies,
            &mut states,
            &InputState::default(),
            400.0,
            300.0,
            1.0 / 60.0,
        );
        let sm = states.get(&mover.id).unwrap();
        let st = states.get(&target.id).unwrap();
        // Centres must be separated by at least r1+r2 = 20 after resolution.
        let dist = ((sm.x - st.x).powi(2) + (sm.y - st.y).powi(2)).sqrt();
        assert!(dist >= 19.9, "dist={dist}");
    }

    #[test]
    fn camera_follows_target_and_respects_bounds() {
        let scene = scene_with(vec![game_object(
            "Helden",
            300.0,
            100.0,
            20.0,
            20.0,
            dynamic_player(),
        )]);
        let bodies = collect_bodies(&scene, &[]);
        let mut states = HashMap::new();
        let mut camera = CameraState {
            x: 200.0,
            y: 150.0,
            zoom: 1.0,
        };
        let config = crate::model::CameraConfig {
            enabled: true,
            follow: "Helden".into(),
            left: 0.0,
            top: 0.0,
            right: 800.0,
            bottom: 600.0,
            zoom: 1.0,
            follow_smoothing: 0.0,
            dead_zone: (0.0, 0.0),
            shake: 0.0,
        };
        for _ in 0..60 {
            step(
                &bodies,
                &mut states,
                &InputState::default(),
                400.0,
                300.0,
                1.0 / 60.0,
            );
            step_camera(
                &mut camera,
                &config,
                &bodies,
                &states,
                400.0,
                300.0,
                1.0 / 60.0,
                0.0,
            );
        }
        let hero = &bodies[0];
        let sx = states.get(&hero.id).unwrap().x;
        assert!((camera.x - sx).abs() < 30.0, "cam={} hero={}", camera.x, sx);
    }
}
