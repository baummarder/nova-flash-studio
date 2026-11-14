//! M8 feature coverage: vector text, multi-scene timelines, folder visibility
//! and every built-in project template. These tests parse the generated SWF
//! with Ruffle's independent parser; they do not claim runtime playback.

use nova_build::build_swf;
use nova_core::{Layer, LayerKind, ObjectInstance, Project, ProjectTemplate, Rgba};

fn parsed_tags(bytes: &[u8]) -> Vec<swf::Tag<'static>> {
    // `SwfBuf` owns the decompressed bytes. Leak it for the duration of the
    // test; the process is short-lived and every test has its own buffer.
    let buffer = swf::decompress_swf(bytes).expect("decompress SWF");
    let leaked: &'static swf::SwfBuf = Box::leak(Box::new(buffer));
    let movie = swf::parse_swf(leaked).expect("parse SWF");
    movie.tags.into_iter().collect()
}

#[test]
fn text_is_baked_to_shapes_not_definetext() {
    let mut project = Project::new_empty("Text");
    let scene = project.active_scene_mut().expect("scene");
    scene.layers[0].frames[0].objects.push(ObjectInstance::text(
        "Überschrift",
        275.0,
        200.0,
        "NØVA Text",
        64.0,
        Rgba::rgb(255, 209, 84),
    ));
    let output = build_swf(&project).expect("build");
    assert!(output.stats.defined_shapes > 0, "text contributes shapes");
    assert_eq!(output.stats.placed_objects, 1);
    let tags = parsed_tags(&output.bytes);
    assert!(
        !tags
            .iter()
            .any(|tag| matches!(tag, swf::Tag::DefineText(_))),
        "text must not pretend to be DefineText"
    );
}

#[test]
fn all_scenes_are_concatenated_with_boundary_labels() {
    let mut project = Project::new_empty("Szenen");
    project.export.export_all_scenes = true;
    {
        let scene = project.active_scene_mut().expect("scene");
        scene.name = "Intro".into();
        scene.ensure_frame_count(2);
        scene.layers[0].frames[0].label = Some("Start".into());
    }
    project.add_scene("Hauptteil");
    {
        let scene = project.active_scene_mut().expect("scene");
        scene.ensure_frame_count(3);
        scene.layers[0].frames[0].objects.push(ObjectInstance::rect(
            "Kasten",
            50.0,
            50.0,
            40.0,
            30.0,
            Rgba::rgb(1, 2, 3),
        ));
    }

    let output = build_swf(&project).expect("build");
    assert_eq!(output.stats.frames, 5);
    let tags = parsed_tags(&output.bytes);
    let labels: Vec<String> = tags
        .iter()
        .filter_map(|tag| match tag {
            swf::Tag::FrameLabel(label) => {
                Some(String::from_utf8_lossy(label.label.as_bytes()).into_owned())
            }
            _ => None,
        })
        .collect();
    assert!(labels.iter().any(|label| label == "Start"));
    assert!(labels.iter().any(|label| label == "Hauptteil"));
}

#[test]
fn hidden_folder_hides_its_children_from_export() {
    let mut project = Project::new_empty("Ordner");
    let scene = project.active_scene_mut().expect("scene");
    let mut folder = Layer::new("Gruppe", 1);
    folder.kind = LayerKind::Folder;
    folder.visible = false;
    let folder_id = folder.id;
    let mut child = Layer::new("Kind", 1);
    child.parent = Some(folder_id);
    child.frames[0].objects.push(ObjectInstance::rect(
        "Versteckt",
        100.0,
        100.0,
        20.0,
        20.0,
        Rgba::rgb(1, 2, 3),
    ));
    scene.layers.push(folder);
    scene.layers.push(child);
    project.validate().expect("folder project validates");

    let output = build_swf(&project).expect("build");
    assert_eq!(output.stats.placed_objects, 0);
}

#[test]
fn every_template_builds() {
    for template in ProjectTemplate::ALL {
        let project = template.create(format!("Build {}", template.id()));
        let output =
            build_swf(&project).unwrap_or_else(|error| panic!("{}: {error}", template.id()));
        assert!(!output.bytes.is_empty(), "{} produced bytes", template.id());
        assert!(output.stats.frames > 0, "{} has frames", template.id());
    }
}
