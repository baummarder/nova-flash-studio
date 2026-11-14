//! M11 tilemap export coverage: tilemaps bake into lossless bitmap
//! characters; missing tilesets produce warnings instead of silent drops.

use nova_build::build_swf;
use nova_core::model::{Asset, ObjectInstance, ObjectKind, Tileset};
use nova_core::tilemap::{self, PaintTool};
use nova_core::{Layer, Project, Rgba};
use uuid::Uuid;

fn tiny_tileset_png() -> Vec<u8> {
    // 2x2 tiles of 8px each = 16x16 px image.
    let mut image = image::RgbaImage::new(16, 16);
    for y in 0..16u32 {
        for x in 0..16u32 {
            let tile = (x / 8) + (y / 8) * 2;
            let color = match tile {
                0 => image::Rgba([200, 60, 60, 255]),
                1 => image::Rgba([60, 200, 60, 255]),
                2 => image::Rgba([60, 60, 200, 255]),
                _ => image::Rgba([220, 220, 60, 255]),
            };
            image.put_pixel(x, y, color);
        }
    }
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn project_with_tilemap() -> Project {
    let mut project = Project::new_empty("Tilemap");
    project.assets.push(Asset::bitmap(
        "Tiles",
        "assets/tiles.png",
        "image/png",
        16,
        16,
        tiny_tileset_png(),
    ));
    let asset_id = project.assets[0].id;
    project.tilesets.push(Tileset {
        name: "Test".into(),
        asset: asset_id,
        tile_width: 8,
        tile_height: 8,
        columns: 2,
        solid: vec![true, false, false, true],
        names: Vec::new(),
        ..Default::default()
    });
    let tileset_id = project.tilesets[0].id;

    let mut map = tilemap::new_tilemap(tileset_id, 8.0, 8.0, 4, 3, &["Ground"]);
    tilemap::paint(&mut map, 0, PaintTool::Rect, 0, 0, 3, 0, 1, 0, 0).unwrap();
    tilemap::paint(&mut map, 0, PaintTool::Pencil, 1, 1, 1, 1, 3, 0, 0).unwrap();
    let object = ObjectInstance::new(
        "Map",
        ObjectKind::Tilemap(map),
        nova_core::Transform::at(100.0, 100.0),
        Rgba::WHITE,
    );
    let scene = project.active_scene_mut().unwrap();
    let mut layer = Layer::new("Map", 1);
    layer.frames[0].objects.push(object);
    scene.layers.push(layer);
    project
}

#[test]
fn tilemap_bakes_to_lossless_bitmap() {
    let project = project_with_tilemap();
    project.validate().expect("tilemap project valid");
    let output = build_swf(&project).expect("build");
    assert!(
        output.stats.defined_bitmaps > 0 || output.stats.defined_shapes > 0,
        "tilemap must contribute a baked character"
    );
    assert_eq!(output.stats.placed_objects, 1, "tilemap object placed once");
    // The bake must not drop warnings silently.
    for warning in &output.warnings {
        assert!(
            !warning.contains("Kachelkarte"),
            "unexpected warning: {warning}"
        );
    }
}

#[test]
fn missing_tileset_warns_and_skips() {
    let mut project = project_with_tilemap();
    // Break the reference: point the tilemap at a random tileset id.
    if let Some(scene) = project.scenes.first_mut() {
        if let Some(layer) = scene.layers.last_mut() {
            if let Some(object) = layer.frames[0].objects.first_mut() {
                if let ObjectKind::Tilemap(map) = &mut object.kind {
                    map.tileset = Uuid::new_v4();
                }
            }
        }
    }
    let output = build_swf(&project).expect("build still succeeds");
    assert!(
        output
            .warnings
            .iter()
            .any(|w| w.contains("Tileset") || w.contains("Kachelkarte")),
        "missing tileset must produce an explicit warning"
    );
    assert_eq!(output.stats.placed_objects, 0);
}

#[test]
fn empty_tilemap_produces_nothing_but_no_warning_spam() {
    let mut project = Project::new_empty("Leer");
    project.assets.push(Asset::bitmap(
        "Tiles",
        "assets/tiles.png",
        "image/png",
        16,
        16,
        tiny_tileset_png(),
    ));
    let asset_id = project.assets[0].id;
    project.tilesets.push(Tileset {
        name: "Test".into(),
        asset: asset_id,
        tile_width: 8,
        tile_height: 8,
        columns: 2,
        ..Default::default()
    });
    let tileset_id = project.tilesets[0].id;
    let mut map = tilemap::new_tilemap(tileset_id, 8.0, 8.0, 2, 2, &["Ground"]);
    // Leave every cell empty.
    map.layers[0].cells.fill(0);
    let object = ObjectInstance::new(
        "Map",
        ObjectKind::Tilemap(map),
        nova_core::Transform::at(0.0, 0.0),
        Rgba::WHITE,
    );
    let scene = project.active_scene_mut().unwrap();
    let mut layer = Layer::new("Map", 1);
    layer.frames[0].objects.push(object);
    scene.layers.push(layer);
    let output = build_swf(&project).expect("build");
    // Fully transparent bake still defines a bitmap; no tilemap warning.
    assert!(
        !output
            .warnings
            .iter()
            .any(|w| w.contains("Kachelkarte") || w.contains("Tileset")),
        "empty tilemap should bake quietly: {:?}",
        output.warnings
    );
}
