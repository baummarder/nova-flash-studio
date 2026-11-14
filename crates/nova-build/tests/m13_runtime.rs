//! M13 runtime-parity export tests: tile-grid collider specs, concave
//! polygon hull approximation and the generated Math shim.

use nova_build::build_swf;
use nova_core::Project;
use std::path::PathBuf;

fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples")
        .join(name)
}

fn do_abc2_count(bytes: &[u8]) -> usize {
    let buf = swf::decompress_swf(&mut std::io::Cursor::new(bytes)).expect("swf parses");
    let parsed = swf::parse_swf(&buf).expect("swf parses");
    parsed
        .tags
        .iter()
        .filter(|t| matches!(t, swf::Tag::DoAbc2(_)))
        .count()
}

#[test]
fn m13_demo_builds_with_runtime_data_and_math() {
    let project = Project::load(&sample("m13-runtime-features.nova")).expect("demo loads");
    // Main + 6 gamekit + Math shim = 8 project scripts.
    assert_eq!(project.scripts.len(), 8);
    assert!(project.scripts.iter().any(|s| s.path == "scripts/Math.as"));
    let output = build_swf(&project).expect("demo builds");
    // +1 for the generated game.NovaGameData class.
    assert_eq!(do_abc2_count(&output.bytes), 9);
}

#[test]
fn tilemap_solid_cells_emit_grid_colliders() {
    let project = Project::load(&sample("m13-runtime-features.nova")).expect("demo loads");
    let output = build_swf(&project).expect("demo builds");
    // The demo has a tilemap with solid cells but no per-object collider on
    // it — NovaGameData must still be emitted (grid spec lives inside it).
    assert!(do_abc2_count(&output.bytes) >= 2);
    assert!(
        !output.warnings.iter().any(|w| w.contains("Tile-Kollider")),
        "unexpected tile collider warning: {:?}",
        output.warnings
    );
}

#[test]
fn concave_polygon_is_hulled_with_warning() {
    // L-shaped (concave) polygon collider on a dynamic body.
    let json = r##"{
        "format": "nova-flash-studio",
        "formatVersion": 4,
        "project": {
            "id": "11111111-1111-4111-8111-111111111111",
            "metadata": {"name": "Concave", "author": "", "description": "", "createdUnix": 0, "modifiedUnix": 0},
            "scenes": [{
                "id": "22222222-2222-4222-8222-222222222222",
                "name": "Szene 1", "width": 640.0, "height": 360.0,
                "background": {"r": 0, "g": 0, "b": 0, "a": 255},
                "fps": 30.0,
                "layers": [{
                    "id": "33333333-3333-4333-8333-333333333333",
                    "name": "Ebene 1", "visible": true, "locked": false, "kind": "normal",
                    "frames": [{"keyframe": true, "objects": [{
                        "id": "44444444-4444-4444-8444-444444444444",
                        "name": "LShape",
                        "kind": {"kind": "rect", "width": 40.0, "height": 40.0, "corner_radius": 0.0},
                        "transform": {"x": 100.0, "y": 100.0, "scale_x": 1.0, "scale_y": 1.0, "rotation": 0.0},
                        "fill": {"r": 255, "g": 0, "b": 0, "a": 255},
                        "game": {
                            "collision": {"shape": {"polygon": {"points": [[-20,-20],[20,-20],[20,0],[0,0],[0,20],[-20,20]]}},
                                "width": 40.0, "height": 40.0, "layer": 1, "mask": 4294967295},
                            "physics": {"body": "dynamic", "gravityScale": 1.0, "friction": 0.0, "bounce": 0.0, "controlled": false}
                        }
                    }]}]
                }],
                "game": null
            }],
            "symbols": [], "assets": [], "tilesets": [], "scripts": [],
            "export": {"target": "avm2", "swfVersion": 10, "compression": "zlib",
                       "includeScripts": true, "exportAllScenes": false},
            "activeScene": "22222222-2222-4222-8222-222222222222"
        }
    }"##;
    let project = Project::from_json(json).expect("project parses");
    let output = build_swf(&project).expect("build succeeds");
    assert!(
        output
            .warnings
            .iter()
            .any(|w| w.contains("konvexe Hülle") || w.contains("konvexen Hülle")),
        "expected convex-hull warning, got: {:?}",
        output.warnings
    );
}
