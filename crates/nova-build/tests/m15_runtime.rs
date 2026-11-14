//! M15 skeletal-animation tests: rigged demo model and baked SWF frames.

use nova_build::build_swf;
use nova_core::{Project, resolve_symbol_frame};
use std::path::PathBuf;

fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples")
        .join(name)
}

#[test]
fn bone_model_serializes_and_resolves() {
    let project = Project::load(&sample("m15-skeletal-animation.nova")).expect("demo loads");
    let symbol = project
        .symbols
        .iter()
        .find(|s| s.name == "Held")
        .expect("rigged symbol");
    let skeleton = symbol.skeleton.as_ref().expect("skeleton present");
    assert_eq!(skeleton.bones.len(), 3);
    assert_eq!(symbol.active_clip.as_deref(), Some("Schwung"));
    let bound: Vec<_> = symbol
        .timeline
        .layers
        .iter()
        .flat_map(|l| l.frames.iter())
        .flat_map(|f| f.objects.iter())
        .filter(|o| o.bone.is_some())
        .collect();
    assert_eq!(bound.len(), 3);
}

#[test]
fn skeletal_pose_changes_bound_object_transforms() {
    let project = Project::load(&sample("m15-skeletal-animation.nova")).expect("demo loads");
    let symbol = project.symbols.iter().find(|s| s.name == "Held").unwrap();
    let frame0 = resolve_symbol_frame(symbol, 0);
    let frame12 = resolve_symbol_frame(symbol, 12);
    let upper0 = frame0
        .iter()
        .find(|o| o.instance.name == "OberarmSegment")
        .unwrap();
    let upper12 = frame12
        .iter()
        .find(|o| o.instance.name == "OberarmSegment")
        .unwrap();
    assert!(
        (upper0.instance.transform.x - upper12.instance.transform.x).abs() > 1.0
            || (upper0.instance.transform.y - upper12.instance.transform.y).abs() > 1.0
            || (upper0.instance.transform.rotation - upper12.instance.transform.rotation).abs()
                > 1.0,
        "bound part must move with the bone clip"
    );
}

#[test]
fn m15_demo_bakes_skeletal_frames_into_sprite() {
    let project = Project::load(&sample("m15-skeletal-animation.nova")).expect("demo loads");
    let output = build_swf(&project).expect("demo builds");
    let buf = swf::decompress_swf(&mut std::io::Cursor::new(&output.bytes)).expect("swf parses");
    let parsed = swf::parse_swf(&buf).expect("swf parses");

    // Collect per-frame matrices inside every DefineSprite.
    let mut sprite_frames: Vec<Vec<swf::Matrix>> = Vec::new();
    let mut current: Vec<swf::Matrix> = Vec::new();
    for tag in &parsed.tags {
        if let swf::Tag::DefineSprite(sprite) = tag {
            for inner in &sprite.tags {
                match inner {
                    swf::Tag::PlaceObject(place) => {
                        if let Some(m) = place.matrix {
                            current.push(m);
                        }
                    }
                    swf::Tag::ShowFrame => {
                        sprite_frames.push(std::mem::take(&mut current));
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(
        sprite_frames.len() >= 20,
        "expected >=20 baked frames, got {}",
        sprite_frames.len()
    );
    let differs = sprite_frames[0]
        .iter()
        .zip(&sprite_frames[12])
        .any(|(a, b)| {
            (a.a.to_f64() - b.a.to_f64()).abs() > 1e-4
                || (a.b.to_f64() - b.b.to_f64()).abs() > 1e-4
                || (a.c.to_f64() - b.c.to_f64()).abs() > 1e-4
                || (a.d.to_f64() - b.d.to_f64()).abs() > 1e-4
                || (a.tx.to_pixels() - b.tx.to_pixels()).abs() > 0.5
                || (a.ty.to_pixels() - b.ty.to_pixels()).abs() > 0.5
        });
    assert!(
        differs,
        "baked matrices must differ between frames 0 and 12"
    );
}
