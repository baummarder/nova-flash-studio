//! Generates the Mega-Milestone-11 demo `.nova` projects into `samples/`.
//!
//! Run with `cargo run -p nova-core --example gen_m11`. The tilemap game demos
//! reuse the M11 game templates (real AS3 document classes); the editing demo
//! is a script-free document that exercises tilemaps, polygon collision,
//! camera extensions and particle emitters in the editor/preview.

use nova_core::games;
use nova_core::model::{
    BodyType, CollisionShape, CollisionSpec, GameObjectConfig, ObjectInstance, ObjectKind,
    ParticleEmitter, ParticlePreset, PhysicsSpec, Rgba,
};
use nova_core::tilemap::{self, PaintTool};
use std::path::PathBuf;
use uuid::Uuid;

fn main() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples");
    std::fs::create_dir_all(&out).unwrap();

    for (template_id, file) in [
        ("tilemap-platformer", "m11-platformer.nova"),
        ("tilemap-dungeon", "m11-topdown-adventure.nova"),
        ("particle-showcase", "m11-particle-showcase.nova"),
    ] {
        let project = games::create_project(template_id).expect("template exists");
        project.save(&out.join(file)).expect("save demo");
        println!(" wrote {file}");
    }

    tilemap_editing_demo(&out);
    println!("Demo-Projekte unter {} geschrieben", out.display());
}

/// Script-free editing demo: painted tilemap with solid metadata, a polygon
/// collider, extended camera settings and several particle emitters.
fn tilemap_editing_demo(dir: &std::path::Path) {
    // Reuse the dungeon template as the base (it already has the Terrain
    // tileset + floor tilemap + torches), then enrich it.
    let mut project = games::create_project("tilemap-dungeon").expect("dungeon template");
    project.scripts.retain(|s| s.path != "scripts/Main.as");
    project.scripts.push(nova_core::model::Script::new(
        "scripts/Main.as",
        "package { public dynamic class Main extends flash.display.Sprite {} }",
    ));

    // Mark terrain tiles solid for the preview physics: stone, brick, wood,
    // path and water are solid; decor is not.
    if let Some(tileset) = project.tilesets.iter_mut().find(|t| t.name == "Terrain") {
        for index in [4usize, 6, 8, 9, 10, 11] {
            if index < tileset.solid.len() {
                tileset.solid[index] = true;
            }
        }
    }

    let scene = project.active_scene_mut().unwrap();

    // A polygon-colliders "hero" with camera dead-zone + shake configured.
    let mut hero = ObjectInstance::rect("Held", 120.0, 120.0, 28.0, 40.0, Rgba::rgb(106, 208, 255));
    hero.game = Some(GameObjectConfig {
        collision: Some(CollisionSpec {
            shape: CollisionShape::Polygon {
                points: vec![
                    (-14.0, -20.0),
                    (14.0, -20.0),
                    (16.0, 0.0),
                    (10.0, 20.0),
                    (-10.0, 20.0),
                    (-16.0, 0.0),
                ],
            },
            width: 32.0,
            height: 40.0,
            layer: 1,
            mask: u32::MAX,
        }),
        physics: Some(PhysicsSpec {
            body: BodyType::Dynamic,
            gravity_scale: 0.0,
            friction: 0.2,
            bounce: 0.0,
            controlled: true,
        }),
    });
    hero.particles = Some(ParticleEmitter::preset(ParticlePreset::Magic));
    scene.layers[0].frames[0].objects.push(hero);

    // Crystal obstacle with polygon collision on layer 2.
    let mut crystal = ObjectInstance::rect(
        "Kristall",
        500.0,
        300.0,
        36.0,
        60.0,
        Rgba::rgb(120, 220, 255),
    );
    crystal.game = Some(GameObjectConfig {
        collision: Some(CollisionSpec {
            shape: CollisionShape::Polygon {
                points: vec![(0.0, -30.0), (18.0, 0.0), (0.0, 30.0), (-18.0, 0.0)],
            },
            width: 36.0,
            height: 60.0,
            layer: 2,
            mask: 0b1111,
        }),
        physics: Some(PhysicsSpec {
            body: BodyType::Static,
            ..Default::default()
        }),
    });
    crystal.particles = Some(ParticleEmitter::preset(ParticlePreset::Sparks));
    scene.layers[0].frames[0].objects.push(crystal);

    // Extended camera: follow hero with smoothing, dead-zone and shake.
    let mut config = scene.game.clone().unwrap_or_default();
    config.camera = Some(nova_core::CameraConfig {
        enabled: true,
        follow: "Held".into(),
        left: 0.0,
        top: 0.0,
        right: 1400.0,
        bottom: 900.0,
        zoom: 1.25,
        follow_smoothing: 0.35,
        dead_zone: (48.0, 32.0),
        shake: 6.0,
    });
    scene.game = Some(config);

    // Paint a small lake + path detail onto the floor tilemap's detail layer.
    let floor = scene.layers[0].frames[0]
        .objects
        .iter_mut()
        .find(|o| o.name == "Floor")
        .expect("floor tilemap");
    if let ObjectKind::Tilemap(map) = &mut floor.kind {
        // Tile ids: water = index 10 -> cell 11, path = index 11 -> cell 12.
        tilemap::paint(map, 1, PaintTool::Rect, 30, 30, 44, 38, 11, 0, 0).unwrap();
        tilemap::paint(map, 1, PaintTool::Rect, 46, 30, 60, 33, 12, 0, 0).unwrap();
    }

    project
        .save(&dir.join("m11-tilemap-editing.nova"))
        .expect("save editing demo");
    println!(" wrote m11-tilemap-editing.nova");
}
