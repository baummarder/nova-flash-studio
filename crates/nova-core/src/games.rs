//! Built-in game templates shipped as `.nova` projects.
//!
//! Each template is a real project: the document class (`scripts/Main.as`)
//! extends `Sprite`/`MovieClip` so `nova-build` links it as the SWF document
//! class, and the `game.*` helper kit is installed alongside. Games therefore
//! execute inside Ruffle/Adobe like any other AS3 content — nothing here is a
//! fake preview.

use crate::gamekit;
use crate::model::{
    BodyType, CollisionShape, CollisionSpec, Frame, GameObjectConfig, Layer, ObjectInstance,
    PhysicsSpec, Project, Rgba, Script,
};

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
                }),
                physics: None,
            }),
        ));
    }
    scene.layers.push(level);
}
