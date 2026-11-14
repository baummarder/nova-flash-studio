//! Repeatable performance measurements for the M11.1 world-building systems.
//!
//! These tests are `#[ignore]`d so the normal suite stays fast. Run them with:
//!
//! ```text
//! cargo test -p nova-core --release --test perf -- --ignored --nocapture
//! ```
//!
//! Numbers are machine-dependent; they exist to catch order-of-magnitude
//! regressions, not to certify absolute performance.

use nova_core::model::{Tileset, Transform};
use nova_core::tilemap::{self, PaintTool};
use nova_core::{CollisionShape, CollisionSpec, ObjectInstance, Project, Rgba, Scene};
use std::time::Instant;

fn bench_tileset() -> Tileset {
    Tileset {
        name: "Terrain".into(),
        tile_width: 16,
        tile_height: 16,
        columns: 8,
        solid: vec![true; 64],
        ..Default::default()
    }
}

fn bench_map(columns: usize, rows: usize) -> nova_core::model::TilemapData {
    tilemap::new_tilemap(
        uuid::Uuid::nil(),
        16.0,
        16.0,
        columns,
        rows,
        &["Ground", "Details"],
    )
}

#[test]
#[ignore]
fn perf_large_tilemap_operations() {
    let tileset = bench_tileset();
    let _ = tileset;
    let mut map = bench_map(512, 512);

    let t = Instant::now();
    tilemap::paint(&mut map, 0, PaintTool::Rect, 0, 0, 511, 511, 1, 0, 0).unwrap();
    println!("paint rect 512x512: {:?}", t.elapsed());

    let t = Instant::now();
    tilemap::paint(&mut map, 1, PaintTool::Flood, 200, 200, 200, 200, 7, 0, 0).unwrap();
    println!("flood fill 512x512 layer1: {:?}", t.elapsed());

    let t = Instant::now();
    let region = tilemap::copy_region(&map, 0, 100, 100, 355, 355).unwrap();
    println!(
        "copy region 256x256: {:?} ({} cells)",
        t.elapsed(),
        region.width * region.height
    );

    let t = Instant::now();
    tilemap::clear_region(&mut map, 0, 100, 100, 355, 355);
    tilemap::paste_region(&mut map, 0, 150, 150, &region);
    println!("clear+paste 256x256: {:?}", t.elapsed());

    let t = Instant::now();
    tilemap::flip_region(&mut map, 0, 0, 0, 511, 511, true, true);
    println!("flip region 512x512: {:?}", t.elapsed());

    let t = Instant::now();
    let _ = tilemap::validate_tileset(&tileset, 128, 128);
    println!("validate_tileset: {:?}", t.elapsed());
}

#[test]
#[ignore]
fn perf_collision_many_bodies() {
    // 200 dynamic circles falling onto a static polygon floor + each other.
    let mut scene = Scene::new("Perf", 1280.0, 720.0);
    let mut objects = Vec::new();
    let mut floor_game = nova_core::GameObjectConfig::default();
    floor_game.collision = Some(CollisionSpec {
        shape: CollisionShape::Rectangle,
        width: 1280.0,
        height: 40.0,
        ..Default::default()
    });
    floor_game.physics = Some(nova_core::PhysicsSpec {
        body: nova_core::BodyType::Static,
        gravity_scale: 0.0,
        friction: 0.0,
        bounce: 0.0,
        controlled: false,
    });
    let mut floor = ObjectInstance::rect("Floor", 640.0, 700.0, 1280.0, 40.0, Rgba::WHITE);
    floor.game = Some(floor_game);
    objects.push(floor);
    for i in 0..199 {
        let mut game = nova_core::GameObjectConfig::default();
        game.collision = Some(CollisionSpec {
            shape: CollisionShape::Circle,
            width: 24.0,
            height: 24.0,
            ..Default::default()
        });
        game.physics = Some(nova_core::PhysicsSpec {
            body: nova_core::BodyType::Dynamic,
            gravity_scale: 1.0,
            friction: 0.0,
            bounce: 0.2,
            controlled: false,
        });
        let mut object = ObjectInstance::ellipse(
            &format!("B{i}"),
            100.0 + (i % 20) as f32 * 55.0,
            100.0 + (i / 20) as f32 * 60.0,
            24.0,
            24.0,
            Rgba::WHITE,
        );
        object.game = Some(game);
        objects.push(object);
    }
    scene.layers[0].frames[0].objects = objects;
    let project = Project {
        scenes: vec![scene],
        ..Project::new_empty("Perf")
    };
    let scene_ref = project.active_scene().unwrap();
    let bodies = nova_core::game::collect_bodies(scene_ref, &[]);
    let mut states = std::collections::HashMap::new();
    let input = nova_core::game::InputState::default();
    let t = Instant::now();
    for _ in 0..120 {
        nova_core::game::step(&bodies, &mut states, &input, 1280.0, 720.0, 1.0 / 60.0);
    }
    let elapsed = t.elapsed();
    println!(
        "collision: {} bodies x 120 steps = {:?} ({:?}/step)",
        bodies.len(),
        elapsed,
        elapsed / 120
    );
}

#[test]
#[ignore]
fn perf_particles_many_emitters() {
    use nova_core::particles::ParticleSystem;
    let mut systems: Vec<ParticleSystem> = (0..50)
        .map(|i| {
            ParticleSystem::new(nova_core::model::ParticleEmitter {
                rate: 40.0 + i as f32,
                burst: 10,
                lifetime: (0.8, 1.6),
                speed: (40.0, 120.0),
                size_start: (4.0, 8.0),
                size_end: (0.0, 1.0),
                opacity_start: (1.0, 1.0),
                opacity_end: (0.0, 0.0),
                color_start: Rgba::rgba(255, 200, 40, 255),
                color_end: Rgba::rgba(255, 40, 0, 0),
                spread: 180.0,
                direction: -90.0,
                gravity: 300.0,
                drag: 0.5,
                max_particles: 2000,
                ..Default::default()
            })
        })
        .collect();
    let t = Instant::now();
    for _ in 0..600 {
        for (i, system) in systems.iter_mut().enumerate() {
            system.update((i as f32 * 50.0, 0.0), 1.0 / 60.0);
        }
    }
    let elapsed = t.elapsed();
    let live: usize = systems.iter().map(|s| s.view((0.0, 0.0)).len()).sum();
    println!(
        "particles: {} emitters x 600 steps = {:?} ({live} live particles)",
        systems.len(),
        elapsed
    );
}

#[test]
#[ignore]
fn perf_undo_history_snapshots() {
    // History stores full project clones; measure the cost of one record on
    // a project with a large tilemap.
    let mut project = Project::new_empty("Perf");
    let mut scene = Scene::new("S", 800.0, 600.0);
    let mut map = bench_map(512, 512);
    tilemap::paint(&mut map, 0, PaintTool::Rect, 0, 0, 511, 511, 1, 0, 0).unwrap();
    scene.layers[0].frames[0].objects.push(ObjectInstance::new(
        "Map",
        nova_core::ObjectKind::Tilemap(map),
        Transform::at(400.0, 300.0),
        Rgba::WHITE,
    ));
    project.scenes = vec![scene];
    let before = project.clone();
    project.touch();
    let t = Instant::now();
    for _ in 0..20 {
        let snapshot = project.clone();
        std::hint::black_box(&snapshot);
    }
    println!("project clone (512x512 tilemap): {:?}", t.elapsed() / 20);
    drop(before);
}
