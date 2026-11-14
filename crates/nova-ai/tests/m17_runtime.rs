//! M17 integration tests: gradients, boolean ops, mesh skinning and clip
//! management through the AI operation layer.

use nova_ai::{ProjectOperation, apply_operations};
use nova_core::{Project, Rgba, SymbolKind};

fn path_project() -> Project {
    // Two overlapping rectangles as Path objects on the main scene.
    let mut project = Project::new_empty("Vector-Test");
    let scene = project.active_scene_mut().unwrap();
    let layer_id = scene.layers[0].id;
    let frame = &mut scene.layers[0].frames[0];
    for (name, x, y, w, h) in [
        ("A", 100.0, 100.0, 80.0, 80.0),
        ("B", 140.0, 140.0, 80.0, 80.0),
    ] {
        let (hw, hh) = (w / 2.0, h / 2.0);
        frame.objects.push(nova_core::ObjectInstance::path(
            name,
            x,
            y,
            w,
            h,
            vec![nova_core::PathShape::new(
                Rgba::WHITE,
                vec![
                    nova_core::PathCommand::MoveTo { x: -hw, y: -hh },
                    nova_core::PathCommand::LineTo { x: hw, y: -hh },
                    nova_core::PathCommand::LineTo { x: hw, y: hh },
                    nova_core::PathCommand::LineTo { x: -hw, y: hh },
                    nova_core::PathCommand::LineTo { x: -hw, y: -hh },
                ],
            )],
            Rgba::WHITE,
        ));
    }
    let _ = layer_id;
    project
}

#[test]
fn boolean_union_via_ai_op() {
    let mut project = path_project();
    let summaries = apply_operations(
        &mut project,
        &[ProjectOperation::BooleanPathOperation {
            target: "A".into(),
            with: "B".into(),
            operation: nova_core::BoolOp::Union,
            scene: None,
            symbol: None,
        }],
    )
    .expect("op applies");
    assert_eq!(summaries.len(), 1);
    let scene = project.active_scene().unwrap();
    let object = scene.layers[0].frames[0]
        .objects
        .iter()
        .find(|o| o.name == "A")
        .unwrap();
    let nova_core::ObjectKind::Path { paths, .. } = &object.kind else {
        panic!("A must stay a path object");
    };
    // Union of two overlapping rects -> single L-shaped outline (>=8 pts).
    let points = paths[0]
        .commands
        .iter()
        .filter(|c| matches!(c, nova_core::PathCommand::LineTo { .. }))
        .count();
    assert!(points >= 8, "union outline too small: {points}");
}

#[test]
fn gradient_via_ai_op_roundtrips() {
    let mut project = path_project();
    apply_operations(
        &mut project,
        &[ProjectOperation::SetPathGradient {
            name: "A".into(),
            scene: None,
            symbol: None,
            kind: Some(nova_core::GradientKind::Linear),
            angle: Some(90.0),
            center_x: None,
            center_y: None,
            radius: None,
            stops: Some(vec![(0.0, "#ff0000".into()), (1.0, "#0000ff".into())]),
        }],
    )
    .expect("gradient applies");
    let scene = project.active_scene().unwrap();
    let object = scene.layers[0].frames[0]
        .objects
        .iter()
        .find(|o| o.name == "A")
        .unwrap();
    let nova_core::ObjectKind::Path { paths, .. } = &object.kind else {
        panic!("A must stay a path object");
    };
    let gradient = paths[0].gradient.as_ref().expect("gradient set");
    assert_eq!(gradient.kind, nova_core::GradientKind::Linear);
    assert!((gradient.angle - 90.0).abs() < 1e-4);
    assert_eq!(gradient.stops.len(), 2);
    assert_eq!(gradient.stops[0].color, Rgba::rgb(255, 0, 0));
    assert_eq!(gradient.stops[1].color, Rgba::rgb(0, 0, 255));
    // Serialization round-trip preserves the gradient.
    let json = serde_json::to_string(&project).unwrap();
    let loaded: Project = serde_json::from_str(&json).unwrap();
    let reloaded = loaded.active_scene().unwrap().layers[0].frames[0]
        .objects
        .iter()
        .find(|o| o.name == "A")
        .unwrap();
    let nova_core::ObjectKind::Path { paths, .. } = &reloaded.kind else {
        panic!("path kind lost");
    };
    assert!(paths[0].gradient.is_some());
}

#[test]
fn mesh_binding_and_vertex_weight_via_ai_ops() {
    let mut project = Project::new_empty("Skin-Test");
    let ops = vec![
        ProjectOperation::AddSymbol {
            name: "Rig".into(),
            width: 200.0,
            height: 200.0,
            fill: None,
            corner_radius: None,
            kind: Some(SymbolKind::MovieClip),
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
            name: "Tip".into(),
            x: 30.0,
            y: 0.0,
            length: 30.0,
            parent: Some("Root".into()),
            rotation: Some(0.0),
        },
    ];
    apply_operations(&mut project, &ops).expect("rig ops apply");
    // Inject a real Path object into the symbol timeline (pen-tool paths
    // cannot be created by AI ops yet).
    {
        let symbol = project
            .symbols
            .iter_mut()
            .find(|s| s.name == "Rig")
            .unwrap();
        let (hw, hh) = (20.0f32, 20.0f32);
        let mut commands = vec![nova_core::PathCommand::MoveTo { x: -hw, y: -hh }];
        for (x, y) in [(hw, -hh), (hw, hh), (-hw, hh), (-hw, -hh)] {
            commands.push(nova_core::PathCommand::LineTo { x, y });
        }
        let object = nova_core::ObjectInstance::path(
            "Mesh",
            50.0,
            50.0,
            40.0,
            40.0,
            vec![nova_core::PathShape::new(Rgba::WHITE, commands)],
            Rgba::WHITE,
        );
        symbol.timeline.layers[0].frames[0].objects.push(object);
    }
    let summaries = apply_operations(
        &mut project,
        &[
            ProjectOperation::BindMeshToSkeleton {
                symbol: "Rig".into(),
                object: "Mesh".into(),
            },
            ProjectOperation::SetVertexWeight {
                symbol: "Rig".into(),
                object: "Mesh".into(),
                path: None,
                vertex: 0,
                bone: "Tip".into(),
                weight: 0.5,
            },
        ],
    )
    .expect("skin ops apply");
    assert_eq!(summaries.len(), 2);
    let symbol = project.symbols.iter().find(|s| s.name == "Rig").unwrap();
    let object = symbol.timeline.layers[0].frames[0]
        .objects
        .iter()
        .find(|o| o.name == "Mesh")
        .unwrap();
    let nova_core::ObjectKind::Path { paths, .. } = &object.kind else {
        panic!("Mesh must be a path object");
    };
    let skin = paths[0].skin.as_ref().expect("skin bound");
    assert_eq!(skin.weights.len(), paths[0].skinning_points().len());
    // Vertex 0 was re-weighted to Tip at 0.5 and normalized.
    let weights = &skin.weights[0];
    let tip_weight = weights.iter().find(|w| {
        symbol
            .skeleton
            .as_ref()
            .unwrap()
            .bones
            .iter()
            .any(|b| b.id == w.bone && b.name == "Tip")
    });
    assert!(tip_weight.is_some(), "vertex 0 must reference Tip bone");
    let sum: f32 = weights.iter().map(|w| w.weight).sum();
    assert!((sum - 1.0).abs() < 1e-4, "weights normalized, sum={sum}");
}

#[test]
fn clip_management_via_ai_ops() {
    let mut project = Project::new_empty("Clip-Test");
    apply_operations(
        &mut project,
        &[
            ProjectOperation::AddSymbol {
                name: "Rig".into(),
                width: 100.0,
                height: 100.0,
                fill: None,
                corner_radius: None,
                kind: Some(SymbolKind::MovieClip),
            },
            ProjectOperation::AddBone {
                symbol: "Rig".into(),
                name: "Root".into(),
                x: 0.0,
                y: 0.0,
                length: 20.0,
                parent: None,
                rotation: None,
            },
            ProjectOperation::AddBoneClip {
                symbol: "Rig".into(),
                name: "Idle".into(),
                frames: Some(12),
                looping: Some(true),
            },
            ProjectOperation::DuplicateBoneClip {
                symbol: "Rig".into(),
                clip: "Idle".into(),
                name: "Walk".into(),
            },
            ProjectOperation::RenameBoneClip {
                symbol: "Rig".into(),
                clip: "Walk".into(),
                name: "Lauf".into(),
            },
        ],
    )
    .expect("clip ops apply");
    let symbol = project.symbols.iter().find(|s| s.name == "Rig").unwrap();
    assert!(symbol.bone_clips.iter().any(|c| c.name == "Idle"));
    assert!(symbol.bone_clips.iter().any(|c| c.name == "Lauf"));
    assert!(!symbol.bone_clips.iter().any(|c| c.name == "Walk"));
}
