//! M16 tests: multi-bone FABRIK chain IK end-to-end (AI ops -> model ->
//! baked SWF frames) and the new IK-key operation.

use nova_ai::{ObjectShape, ProjectOperation, apply_operations};
use nova_build::build_swf;
use nova_core::{Project, SymbolKind, resolve_symbol_frame};

fn rig_ops() -> Vec<ProjectOperation> {
    vec![
        ProjectOperation::AddSymbol {
            name: "Rig".into(),
            width: 200.0,
            height: 200.0,
            fill: None,
            corner_radius: None,
            kind: Some(SymbolKind::MovieClip),
        },
        ProjectOperation::AddObject {
            name: "RootPart".into(),
            scene: None,
            symbol: Some("Rig".into()),
            layer: None,
            shape: ObjectShape::Rect,
            x: 0.0,
            y: 0.0,
            width: 30.0,
            height: 30.0,
            fill: None,
            text: String::new(),
            font_size: None,
        },
        ProjectOperation::AddObject {
            name: "MidPart".into(),
            scene: None,
            symbol: Some("Rig".into()),
            layer: None,
            shape: ObjectShape::Rect,
            x: 30.0,
            y: 0.0,
            width: 30.0,
            height: 30.0,
            fill: None,
            text: String::new(),
            font_size: None,
        },
        ProjectOperation::AddObject {
            name: "EndPart".into(),
            scene: None,
            symbol: Some("Rig".into()),
            layer: None,
            shape: ObjectShape::Rect,
            x: 60.0,
            y: 0.0,
            width: 30.0,
            height: 30.0,
            fill: None,
            text: String::new(),
            font_size: None,
        },
        ProjectOperation::AddBone {
            symbol: "Rig".into(),
            name: "Root".into(),
            x: 0.0,
            y: 0.0,
            length: 30.0,
            parent: None,
            rotation: None,
        },
        ProjectOperation::AddBone {
            symbol: "Rig".into(),
            name: "Mid".into(),
            x: 30.0,
            y: 0.0,
            length: 30.0,
            parent: Some("Root".into()),
            rotation: Some(0.0),
        },
        ProjectOperation::AddBone {
            symbol: "Rig".into(),
            name: "End".into(),
            x: 30.0,
            y: 0.0,
            length: 30.0,
            parent: Some("Mid".into()),
            rotation: Some(0.0),
        },
        ProjectOperation::AddBoneClip {
            symbol: "Rig".into(),
            name: "Reach".into(),
            frames: Some(12),
            looping: Some(true),
        },
        ProjectOperation::AddIkKey {
            symbol: "Rig".into(),
            clip: "Reach".into(),
            bone: "End".into(),
            frame: 0,
            x: 60.0,
            y: -60.0,
            pole_x: Some(60.0),
            pole_y: Some(60.0),
        },
        ProjectOperation::AddIkKey {
            symbol: "Rig".into(),
            clip: "Reach".into(),
            bone: "End".into(),
            frame: 12,
            x: 60.0,
            y: 60.0,
            pole_x: Some(60.0),
            pole_y: Some(-60.0),
        },
        ProjectOperation::BindObjectToBone {
            symbol: "Rig".into(),
            object: "RootPart".into(),
            bone: "Root".into(),
        },
        ProjectOperation::BindObjectToBone {
            symbol: "Rig".into(),
            object: "MidPart".into(),
            bone: "Mid".into(),
        },
        ProjectOperation::BindObjectToBone {
            symbol: "Rig".into(),
            object: "EndPart".into(),
            bone: "End".into(),
        },
        ProjectOperation::SetSymbolClip {
            symbol: "Rig".into(),
            clip: Some("Reach".into()),
        },
    ]
}

#[test]
fn chain_ik_moves_bound_parts_deterministically() {
    let mut project = Project::new_empty("IK-Test");
    let summaries = apply_operations(&mut project, &rig_ops()).expect("ops apply");
    assert_eq!(summaries.len(), 14);
    let symbol = project.symbols.iter().find(|s| s.name == "Rig").unwrap();
    assert_eq!(symbol.active_clip.as_deref(), Some("Reach"));
    let f0 = resolve_symbol_frame(symbol, 0);
    let f6 = resolve_symbol_frame(symbol, 6);
    let end0 = f0.iter().find(|o| o.instance.name == "EndPart").unwrap();
    let end6 = f6.iter().find(|o| o.instance.name == "EndPart").unwrap();
    assert!(
        (end0.instance.transform.y - end6.instance.transform.y).abs() > 5.0,
        "IK target moves the bound end part between frames"
    );
    // Deterministic: same frame twice -> identical transforms.
    let f0b = resolve_symbol_frame(symbol, 0);
    let end0b = f0b.iter().find(|o| o.instance.name == "EndPart").unwrap();
    assert_eq!(end0.instance.transform, end0b.instance.transform);
}

#[test]
fn chain_ik_bakes_into_sprite_frames() {
    let mut project = Project::new_empty("IK-Test");
    apply_operations(&mut project, &rig_ops()).expect("ops apply");
    // Place the rig on the main scene so it exports.
    apply_operations(
        &mut project,
        &[ProjectOperation::PlaceSymbol {
            symbol: "Rig".into(),
            name: "RigInst".into(),
            x: 100.0,
            y: 100.0,
            scene: None,
            symbol_target: None,
            layer: None,
            fill: None,
        }],
    )
    .expect("place");
    let output = build_swf(&project).expect("builds");
    let buf = swf::decompress_swf(&mut std::io::Cursor::new(&output.bytes)).expect("swf parses");
    let parsed = swf::parse_swf(&buf).expect("swf parses");
    let mut frames: Vec<Vec<swf::Matrix>> = Vec::new();
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
                    swf::Tag::ShowFrame => frames.push(std::mem::take(&mut current)),
                    _ => {}
                }
            }
        }
    }
    assert!(
        frames.len() >= 10,
        "clip frames baked, got {}",
        frames.len()
    );
    let differs = frames[0].iter().zip(&frames[6]).any(|(a, b)| {
        (a.a.to_f64() - b.a.to_f64()).abs() > 1e-4
            || (a.b.to_f64() - b.b.to_f64()).abs() > 1e-4
            || (a.tx.to_pixels() - b.tx.to_pixels()).abs() > 0.5
            || (a.ty.to_pixels() - b.ty.to_pixels()).abs() > 0.5
    });
    assert!(
        differs,
        "FABRIK-animated matrices must differ between frames"
    );
}
