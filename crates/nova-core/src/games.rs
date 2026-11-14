//! Built-in game templates shipped as `.nova` projects.
//!
//! Each template is a real project: the document class (`scripts/Main.as`)
//! extends `Sprite`/`MovieClip` so `nova-build` links it as the SWF document
//! class, and the `game.*` helper kit is installed alongside. Games therefore
//! execute inside Ruffle/Adobe like any other AS3 content — nothing here is a
//! fake preview.

use crate::gamekit;
use crate::model::{
    Asset, BodyType, CollisionShape, CollisionSpec, Frame, GameObjectConfig, Layer, ObjectInstance,
    ObjectKind, ParticleEmitter, ParticlePreset, PhysicsSpec, Project, Rgba, Script, TilemapData,
    Tileset,
};
use crate::tilemap::{self, PaintTool};

/// A shippable game template.
pub struct GameTemplate {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    main_as: &'static str,
}

pub const TEMPLATES: &[GameTemplate] = &[
    GameTemplate {
        id: "blank-game",
        title: "Blank Game",
        description: "Minimaler Spielplatz: beweglicher Akteur, leere Stage, Game Kit.",
        main_as: include_str!("../assets/games/blank/Main.as"),
    },
    GameTemplate {
        id: "pong",
        title: "Pong",
        description: "Zwei Schläger, ein Ball, CPU-Gegner, Punkteanzeige.",
        main_as: include_str!("../assets/games/pong/Main.as"),
    },
    GameTemplate {
        id: "breakout",
        title: "Breakout",
        description: "Paddle, Ball und zerstörbare Ziegel mit Leben und Punkten.",
        main_as: include_str!("../assets/games/breakout/Main.as"),
    },
    GameTemplate {
        id: "platformer",
        title: "2D Platformer",
        description: "Scrolling-Level mit Plattformen, Münzen und Springen.",
        main_as: include_str!("../assets/games/platformer/Main.as"),
    },
    GameTemplate {
        id: "topdown",
        title: "Top-Down Adventure",
        description: "Vogelperspektive, Sammelobjekte, Verfolger, Kamera.",
        main_as: include_str!("../assets/games/topdown/Main.as"),
    },
    GameTemplate {
        id: "racing",
        title: "Simple Racing",
        description: "Scrollender Racer auf kurviger Strecke mit Verkehr.",
        main_as: include_str!("../assets/games/racing/Main.as"),
    },
    GameTemplate {
        id: "menu",
        title: "Interactive Menu",
        description: "Tastaturgesteuertes Hauptmenü mit Demo-Spielmodus.",
        main_as: include_str!("../assets/games/menu/Main.as"),
    },
    GameTemplate {
        id: "tilemap-platformer",
        title: "Tilemap Platformer",
        description: "Platformer-Level mit gemalter Tilemap-Landschaft, Kamera und Spielwelt.",
        main_as: include_str!("../assets/games/platformer/Main.as"),
    },
    GameTemplate {
        id: "tilemap-dungeon",
        title: "Tilemap Dungeon",
        description: "Top-Down-Level mit gefliestem Boden, Mauern, Verfolger und Partikel-Deko.",
        main_as: include_str!("../assets/games/topdown/Main.as"),
    },
    GameTemplate {
        id: "particle-showcase",
        title: "Particle Showcase",
        description: "Acht Particle-Emitter-Presets auf Objekten — vollständig in der Spiel-Vorschau.",
        main_as: include_str!("../assets/games/particles/Main.as"),
    },
];

pub fn template_by_id(id: &str) -> Option<&'static GameTemplate> {
    TEMPLATES.iter().find(|t| t.id == id)
}

/// Builds a fresh project for `id`: empty-ish scene, document class script,
/// game kit and (for the platformer) the named level objects the script
/// expects on the timeline.
pub fn create_project(id: &str) -> Option<Project> {
    let template = template_by_id(id)?;
    let mut project = Project::new_empty(template.title);
    project.metadata.description = template.description.into();
    if let Some(scene) = project
        .scenes
        .iter_mut()
        .find(|s| Some(s.id) == project.active_scene)
    {
        scene.width = 640.0;
        scene.height = 360.0;
        scene.fps = 30.0;
    }

    // Replace the stub Main.as with the game document class.
    project.scripts.retain(|s| s.path != "scripts/Main.as");
    project
        .scripts
        .push(Script::new("scripts/Main.as", template.main_as));
    gamekit::install(&mut project);

    if id == "platformer" {
        build_platformer_level(&mut project);
    }
    match id {
        "tilemap-platformer" => build_tilemap_platformer(&mut project),
        "tilemap-dungeon" => build_tilemap_dungeon(&mut project),
        "particle-showcase" => build_particle_showcase(&mut project),
        _ => {}
    }
    Some(project)
}

fn level_object(
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    fill: Rgba,
    game: Option<GameObjectConfig>,
) -> ObjectInstance {
    let mut object = ObjectInstance::rect(name, x, y, w, h, fill);
    object.game = game;
    object
}

fn static_collider(w: f32, h: f32) -> Option<GameObjectConfig> {
    Some(GameObjectConfig {
        collision: Some(CollisionSpec {
            shape: CollisionShape::Rectangle,
            width: w,
            height: h,
            ..Default::default()
        }),
        physics: Some(PhysicsSpec {
            body: BodyType::Static,
            ..Default::default()
        }),
    })
}

/// The platformer level: a 1600x360 world inside a 640x360 stage. Objects are
/// named `Plat1..n` / `Coin1..n` because the document class collects them via
/// `getChildByName`. Platforms also carry static colliders so the editor
/// preview simulates the same level.
fn build_platformer_level(project: &mut Project) {
    let scene = project
        .scenes
        .iter_mut()
        .find(|s| Some(s.id) == project.active_scene)
        .expect("active scene");
    scene.layers.clear();
    let mut level = Layer::new("Level", 1);
    level.frames = vec![Frame {
        keyframe: true,
        objects: Vec::new(),
        tween: None,
        label: None,
        sound: None,
    }];

    let ground = Rgba::rgb(70, 90, 140);
    let plat = Rgba::rgb(120, 160, 220);
    let coin = Rgba::rgb(255, 210, 74);

    // Long ground strip (extends past the right stage edge on purpose).
    level.frames[0].objects.push(level_object(
        "Plat1",
        800.0,
        345.0,
        1600.0,
        30.0,
        ground,
        static_collider(1600.0, 30.0),
    ));
    // Floating platforms.
    let platforms: &[(f32, f32, f32, f32)] = &[
        (300.0, 260.0, 140.0, 18.0),
        (520.0, 210.0, 120.0, 18.0),
        (760.0, 250.0, 150.0, 18.0),
        (1020.0, 190.0, 120.0, 18.0),
        (1260.0, 240.0, 160.0, 18.0),
        (1420.0, 170.0, 110.0, 18.0),
    ];
    for (i, (x, y, w, h)) in platforms.iter().enumerate() {
        level.frames[0].objects.push(level_object(
            &format!("Plat{}", i + 2),
            *x,
            *y,
            *w,
            *h,
            plat,
            static_collider(*w, *h),
        ));
    }
    // Coins hovering above platforms.
    let coins: &[(f32, f32)] = &[
        (300.0, 225.0),
        (520.0, 175.0),
        (760.0, 215.0),
        (1020.0, 155.0),
        (1260.0, 205.0),
        (1420.0, 135.0),
    ];
    for (i, (x, y)) in coins.iter().enumerate() {
        level.frames[0].objects.push(level_object(
            &format!("Coin{}", i + 1),
            *x,
            *y,
            18.0,
            18.0,
            coin,
            Some(GameObjectConfig {
                collision: Some(CollisionSpec {
                    shape: CollisionShape::Rectangle,
                    width: 18.0,
                    height: 18.0,
                    ..Default::default()
                }),
                physics: None,
            }),
        ));
    }
    scene.layers.push(level);
}

// ---------------------------------------------------------------------------
// Mega-Milestone 11: tilemap / particle game templates.
// ---------------------------------------------------------------------------

/// Generates the shared "Terrain" tileset bitmap: 16×16 px tiles, 16 per row.
/// Tile indices (0-based): 0 sky, 1 cloud, 2 hill-far, 3 hill-near, 4 dirt,
/// 5 grass-top, 6 stone, 7 stone-dark, 8 wood, 9 brick, 10 water, 11 path,
/// 12 bush, 13 flower, 14 torch, 15 crystal.
fn terrain_tileset_png() -> Vec<u8> {
    let tile = 16u32;
    let columns = 16u32;
    let mut image = image::RgbaImage::new(columns * tile, tile);
    for t in 0..columns {
        for y in 0..tile {
            for x in 0..tile {
                let (u, v) = (x as f32 / tile as f32, y as f32 / tile as f32);
                let px = match t {
                    0 => image::Rgba([86, 140, 220, 255]), // sky
                    1 => {
                        // cloud: soft white blob
                        let d = ((u - 0.5).powi(2) + (v - 0.45).powi(2) * 1.6).sqrt();
                        if d < 0.42 {
                            image::Rgba([245, 250, 255, 255])
                        } else {
                            image::Rgba([86, 140, 220, 255])
                        }
                    }
                    2 => image::Rgba([96, 160, 110, 255]), // hill far
                    3 => {
                        // hill near: darker with a highlight arc
                        let arc = ((u - 0.5).abs() - 0.28).abs() < 0.06 && v < 0.6;
                        if arc {
                            image::Rgba([140, 200, 130, 255])
                        } else {
                            image::Rgba([70, 130, 80, 255])
                        }
                    }
                    4 => image::Rgba([122, 84, 52, 255]), // dirt
                    5 => {
                        // grass top: green cap over dirt
                        if v < 0.45 {
                            image::Rgba([96, 190, 80, 255])
                        } else {
                            image::Rgba([122, 84, 52, 255])
                        }
                    }
                    6 => image::Rgba([140, 140, 155, 255]), // stone
                    7 => image::Rgba([105, 105, 120, 255]), // stone dark
                    8 => {
                        // wood planks
                        let seam = (v * 4.0).fract() < 0.12;
                        if seam {
                            image::Rgba([90, 60, 34, 255])
                        } else {
                            image::Rgba([160, 110, 60, 255])
                        }
                    }
                    9 => {
                        // brick
                        let row = (v * 4.0) as u32;
                        let off = if row % 2 == 0 { 0.0 } else { 0.5 };
                        let seam = (u * 4.0 + off).fract() < 0.14 || (v * 4.0).fract() < 0.14;
                        if seam {
                            image::Rgba([70, 60, 66, 255])
                        } else {
                            image::Rgba([190, 90, 70, 255])
                        }
                    }
                    10 => {
                        // water with moving-looking stripes
                        let wave = ((u * 3.0 + v * 1.5).sin() * 0.5 + 0.5) as f32;
                        image::Rgba([
                            (30.0 + wave * 30.0) as u8,
                            (110.0 + wave * 40.0) as u8,
                            220,
                            255,
                        ])
                    }
                    11 => image::Rgba([170, 150, 110, 255]), // path
                    12 => {
                        // bush
                        let d = ((u - 0.5).powi(2) + (v - 0.6).powi(2)).sqrt();
                        if d < 0.4 {
                            image::Rgba([60, 140, 60, 255])
                        } else {
                            image::Rgba([0, 0, 0, 0])
                        }
                    }
                    13 => {
                        // flower on transparent
                        let cx = (u - 0.5).abs() < 0.08 && v > 0.45;
                        let head = ((u - 0.5).powi(2) + (v - 0.3).powi(2)).sqrt() < 0.18;
                        if head {
                            image::Rgba([255, 120, 160, 255])
                        } else if cx {
                            image::Rgba([50, 140, 50, 255])
                        } else {
                            image::Rgba([0, 0, 0, 0])
                        }
                    }
                    14 => {
                        // torch: stick + flame
                        if (u - 0.5).abs() < 0.12 && v > 0.55 {
                            image::Rgba([110, 70, 40, 255])
                        } else {
                            let d = ((u - 0.5).powi(2) + (v - 0.35).powi(2)).sqrt();
                            if d < 0.3 {
                                image::Rgba([255, 190, 60, 255])
                            } else if d < 0.42 {
                                image::Rgba([255, 110, 30, 255])
                            } else {
                                image::Rgba([0, 0, 0, 0])
                            }
                        }
                    }
                    _ => {
                        // crystal: diamond
                        let d = ((u - 0.5).abs() * 1.4 + (v - 0.5).abs() * 0.9).abs();
                        if d < 0.3 {
                            image::Rgba([120, 220, 255, 255])
                        } else if d < 0.38 {
                            image::Rgba([200, 245, 255, 255])
                        } else {
                            image::Rgba([0, 0, 0, 0])
                        }
                    }
                };
                image.put_pixel(t * tile + x, y, px);
            }
        }
    }
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("tileset png encodes");
    out
}

/// Registers the shared Terrain tileset (bitmap asset + tileset record).
/// Returns the tileset id. Tile index i maps to cell value i+1.
fn add_terrain_tileset(project: &mut Project) -> uuid::Uuid {
    let name = "Terrain";
    if let Some(existing) = project.tilesets.iter().find(|t| t.name == name) {
        return existing.id;
    }
    if !project.assets.iter().any(|a| a.name == "TerrainBitmap") {
        let png = terrain_tileset_png();
        project.assets.push(Asset::bitmap(
            "TerrainBitmap",
            "assets/terrain.png",
            "image/png",
            256,
            16,
            png,
        ));
    }
    let asset_id = project
        .assets
        .iter()
        .find(|a| a.name == "TerrainBitmap")
        .expect("terrain asset")
        .id;
    let tileset = Tileset {
        name: name.into(),
        asset: asset_id,
        tile_width: 16,
        tile_height: 16,
        columns: 16,
        solid: vec![false; 16],
        names: [
            "Himmel",
            "Wolke",
            "Hügel fern",
            "Hügel nah",
            "Erde",
            "Gras",
            "Stein",
            "Stein dunkel",
            "Holz",
            "Ziegel",
            "Wasser",
            "Weg",
            "Busch",
            "Blume",
            "Fackel",
            "Kristall",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        ..Default::default()
    };
    project.tilesets.push(tileset);
    project.tilesets.last().unwrap().id
}

const T_GRASS: u16 = 5 + 1;
const T_DIRT: u16 = 4 + 1;
const T_STONE: u16 = 6 + 1;
const T_PATH: u16 = 11 + 1;
const T_BUSH: u16 = 12 + 1;
const T_FLOWER: u16 = 13 + 1;
const T_HILL_FAR: u16 = 2 + 1;
const T_HILL_NEAR: u16 = 3 + 1;
const T_SKY: u16 = 0 + 1;
const T_WATER: u16 = 10 + 1;
const T_BRICK: u16 = 9 + 1;
const T_CRYSTAL: u16 = 15 + 1;

fn tilemap_object(
    name: &str,
    tileset: uuid::Uuid,
    columns: usize,
    rows: usize,
    x: f32,
    y: f32,
) -> ObjectInstance {
    let map = tilemap::new_tilemap(tileset, 16.0, 16.0, columns, rows, &["Terrain", "Details"]);
    let mut object = ObjectInstance::new(
        name,
        ObjectKind::Tilemap(map),
        crate::model::Transform::at(x, y),
        Rgba::WHITE,
    );
    // Terrain layer: opaque; Details layer: slightly transparent decor.
    if let ObjectKind::Tilemap(map) = &mut object.kind {
        if map.layers.len() > 1 {
            map.layers[1].opacity = 0.9;
        }
    }
    object
}

fn paint(
    map: &mut TilemapData,
    layer: usize,
    tool: PaintTool,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    tile: u16,
) {
    tilemap::paint(map, layer, tool, x0, y0, x1, y1, tile, 0, 0).expect("template paint");
}

fn tilemap_mut<'a>(object: &'a mut ObjectInstance) -> &'a mut TilemapData {
    match &mut object.kind {
        ObjectKind::Tilemap(map) => map,
        _ => panic!("expected tilemap"),
    }
}

/// Platformer with a painted parallax tilemap background (sky/hills/bushes)
/// plus the proven platformer document class. World bounds are configured so
/// the editor preview camera clamps correctly.
fn build_tilemap_platformer(project: &mut Project) {
    build_platformer_level(project);
    let tileset = add_terrain_tileset(project);
    let scene = project
        .scenes
        .iter_mut()
        .find(|s| Some(s.id) == project.active_scene)
        .expect("active scene");

    // World + spawn for the preview runtime.
    let mut config = scene.game.clone().unwrap_or_default();
    config.world = Some(crate::model::WorldConfig {
        width: 1600.0,
        height: 360.0,
        spawn: (80.0, 300.0),
        ..Default::default()
    });
    scene.game = Some(config);

    // Background tilemap: 100x23 cells covering the 1600x368 world, centred.
    let mut object = tilemap_object("Landschaft", tileset, 100, 23, 800.0, 184.0);
    let map = tilemap_mut(&mut object);
    // Layer 0 (Terrain): sky base + hill bands + ground strip.
    paint(map, 0, PaintTool::Rect, 0, 0, 99, 13, T_SKY);
    paint(map, 0, PaintTool::Rect, 0, 14, 99, 15, T_HILL_FAR);
    paint(map, 0, PaintTool::Rect, 0, 16, 99, 18, T_HILL_NEAR);
    paint(map, 0, PaintTool::Rect, 0, 19, 99, 21, T_DIRT);
    paint(map, 0, PaintTool::Rect, 0, 22, 99, 22, T_GRASS);
    // Scattered bushes/flowers on the grass row.
    for x in (3..96).step_by(7) {
        paint(
            map,
            0,
            PaintTool::Pencil,
            x,
            21,
            x,
            21,
            if x % 14 == 3 { T_BUSH } else { T_FLOWER },
        );
    }
    // Layer 1 (Details): water pond + path.
    paint(map, 1, PaintTool::Rect, 62, 20, 72, 21, T_WATER);
    paint(map, 1, PaintTool::Rect, 74, 20, 90, 21, T_PATH);

    // Put the tilemap on its own bottom layer so platforms draw on top.
    let mut terrain_layer = Layer::new("Terrain-Map", 1);
    terrain_layer.frames[0].objects.push(object);
    scene.layers.insert(0, terrain_layer);
}

/// Top-down dungeon: the proven topdown document class plus a fully tiled
/// floor (stone/brick/path), wall accents and particle-emitter decorations.
fn build_tilemap_dungeon(project: &mut Project) {
    let tileset = add_terrain_tileset(project);
    let scene = project
        .scenes
        .iter_mut()
        .find(|s| Some(s.id) == project.active_scene)
        .expect("active scene");
    scene.width = 640.0;
    scene.height = 360.0;

    // World bounds matching the topdown script (1400x900).
    let mut config = scene.game.clone().unwrap_or_default();
    config.world = Some(crate::model::WorldConfig {
        width: 1400.0,
        height: 900.0,
        spawn: (100.0, 100.0),
        ..Default::default()
    });
    scene.game = Some(config);

    // Floor tilemap: 88x57 cells of 16px = 1408x912 covering the world.
    let mut floor = tilemap_object("Floor", tileset, 88, 57, 700.0, 450.0);
    let map = tilemap_mut(&mut floor);
    paint(map, 0, PaintTool::Rect, 0, 0, 87, 56, T_STONE);
    // Border walls (brick) — visual only; collision lives on script objects.
    paint(map, 0, PaintTool::Rect, 0, 0, 87, 1, T_BRICK);
    paint(map, 0, PaintTool::Rect, 0, 55, 87, 55, T_BRICK);
    paint(map, 0, PaintTool::Rect, 0, 0, 1, 56, T_BRICK);
    paint(map, 0, PaintTool::Rect, 86, 0, 86, 56, T_BRICK);
    // Scattered detail: crystals and paths.
    for y in (4..52).step_by(9) {
        for x in (4..84).step_by(11) {
            paint(
                map,
                1,
                PaintTool::Pencil,
                x,
                y,
                x,
                y,
                if (x + y) % 3 == 0 { T_CRYSTAL } else { T_PATH },
            );
        }
    }

    let mut floor_layer = Layer::new("Floor-Map", 1);
    floor_layer.frames[0].objects.push(floor);
    scene.layers.clear();
    scene.layers.push(floor_layer);

    // Torch emitters for the preview (particles are preview-only).
    let mut torch =
        ObjectInstance::rect("Fackel", 200.0, 200.0, 12.0, 20.0, Rgba::rgb(120, 80, 40));
    torch.particles = Some(ParticleEmitter::preset(ParticlePreset::Fire));
    scene.layers[0].frames[0].objects.push(torch.clone());
    let mut torch2 = torch;
    torch2.id = uuid::Uuid::new_v4();
    torch2.name = "Fackel 2".into();
    torch2.transform.x = 1200.0;
    torch2.transform.y = 700.0;
    scene.layers[0].frames[0].objects.push(torch2);
}

/// Eight emitter presets on labelled pedestals — a preview-runtime demo.
fn build_particle_showcase(project: &mut Project) {
    let scene = project
        .scenes
        .iter_mut()
        .find(|s| Some(s.id) == project.active_scene)
        .expect("active scene");
    scene.layers.clear();
    let mut layer = Layer::new("Emitter", 60);
    layer.frames[0].keyframe = true;

    let presets = [
        (
            "Rauch",
            ParticlePreset::Smoke,
            80.0,
            200.0,
            Rgba::rgb(120, 120, 130),
        ),
        (
            "Funken",
            ParticlePreset::Sparks,
            180.0,
            200.0,
            Rgba::rgb(255, 200, 80),
        ),
        (
            "Feuer",
            ParticlePreset::Fire,
            280.0,
            200.0,
            Rgba::rgb(255, 120, 40),
        ),
        (
            "Regen",
            ParticlePreset::Rain,
            380.0,
            200.0,
            Rgba::rgb(90, 160, 255),
        ),
        (
            "Schnee",
            ParticlePreset::Snow,
            480.0,
            200.0,
            Rgba::rgb(240, 245, 255),
        ),
        (
            "Staub",
            ParticlePreset::Dust,
            80.0,
            320.0,
            Rgba::rgb(170, 150, 110),
        ),
        (
            "Magie",
            ParticlePreset::Magic,
            280.0,
            320.0,
            Rgba::rgb(190, 120, 255),
        ),
        (
            "Explosion",
            ParticlePreset::Explosion,
            480.0,
            320.0,
            Rgba::rgb(255, 160, 60),
        ),
    ];
    for (name, preset, x, y, color) in presets {
        let mut pedestal = ObjectInstance::rect(name, x, y, 24.0, 24.0, color);
        pedestal.particles = Some(ParticleEmitter::preset(preset));
        layer.frames[0].objects.push(pedestal);
    }
    scene.layers.push(layer);
}
