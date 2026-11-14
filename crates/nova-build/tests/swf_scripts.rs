//! End-to-end checks for ActionScript embedding.
//!
//! These tests cross-check the complete project -> SWF path with the Ruffle
//! `swf` parser and its independent AVM2 ABC reader. They prove structural
//! validity only; projector/Ruffle runtime execution is not available here.

use nova_build::{BuildError, build_swf};
use nova_core::{Project, Script, TargetRuntime};
use swf::PlaceObjectAction;
use swf::avm2::read::Reader as AbcReader;

fn parse(buffer: &swf::SwfBuf) -> swf::Swf<'_> {
    swf::parse_swf(buffer).expect("Ruffle parser accepts tag stream")
}

fn do_abc2<'a>(parsed: &'a swf::Swf<'a>) -> &'a swf::DoAbc2<'a> {
    parsed
        .tags
        .iter()
        .find_map(|tag| match tag {
            swf::Tag::DoAbc2(do_abc) => Some(do_abc),
            _ => None,
        })
        .expect("DoAbc2 tag")
}

#[test]
fn demo_script_is_embedded_and_linked_to_its_object() {
    let project = Project::demo();
    let output = build_swf(&project).expect("demo build");
    assert_eq!(output.stats.embedded_scripts, 1);
    assert!(output.stats.abc_bytes > 0);

    let buffer = swf::decompress_swf(output.bytes.as_slice()).expect("decompress");
    let parsed = parse(&buffer);
    assert_eq!(parsed.header.version(), 10);
    assert!(parsed.header.is_action_script_3());

    let do_abc = do_abc2(&parsed);
    assert_eq!(do_abc.name.as_bytes(), b"scripts/Main.as");
    assert!(!do_abc.data.is_empty());
    let abc = AbcReader::new(do_abc.data)
        .read()
        .expect("embedded ABC parses independently");
    assert_eq!(abc.instances.len(), 1, "Main class was compiled");
    assert_eq!(abc.classes.len(), 1);

    let links: Vec<_> = parsed
        .tags
        .iter()
        .filter_map(|tag| match tag {
            swf::Tag::SymbolClass(links) => Some(links.as_slice()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(links.len(), 1, "one scripted object is linked");
    assert_ne!(links[0].id, 0, "object link is not the document class");
    assert_eq!(links[0].class_name.as_bytes(), b"Main");

    let sprite_ids: Vec<u16> = parsed
        .tags
        .iter()
        .filter_map(|tag| match tag {
            swf::Tag::DefineSprite(sprite) => Some(sprite.id),
            _ => None,
        })
        .collect();
    assert!(
        sprite_ids.contains(&links[0].id),
        "linked character must be a defined sprite wrapper: {sprite_ids:?}"
    );

    // The scripted object is placed once and then owned by ActionScript. The
    // demo timeline has 48 frames; re-placing it would reset scripted
    // transforms every frame.
    let scripted_placements: Vec<u16> = parsed
        .tags
        .iter()
        .filter_map(|tag| match tag {
            swf::Tag::PlaceObject(place) => match place.action {
                PlaceObjectAction::Place(id) if id == links[0].id => Some(place.depth),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        scripted_placements.len(),
        1,
        "scripted object must be placed once, got {scripted_placements:?}"
    );
    assert!(
        scripted_placements[0] > 1,
        "scripted object should sit above baked timeline depths"
    );
}

#[test]
fn unlinked_movieclip_script_becomes_the_document_class() {
    let mut project = Project::new_empty("Dokumentklasse");
    project.export.target = TargetRuntime::Avm2;
    project.export.swf_version = 9;
    project.export.include_scripts = true;
    project.scripts = vec![Script::new(
        "scripts/Main.as",
        "package {\n    import flash.display.MovieClip;\n    public class Main extends MovieClip {\n        public function Main() {\n        }\n    }\n}\n",
    )];

    let output = build_swf(&project).expect("document class build");
    assert!(
        output
            .warnings
            .iter()
            .any(|warning| warning.contains("Version 10")),
        "version bump must be reported: {:?}",
        output.warnings
    );
    let buffer = swf::decompress_swf(output.bytes.as_slice()).expect("decompress");
    let parsed = parse(&buffer);
    assert_eq!(parsed.header.version(), 10);
    let links: Vec<_> = parsed
        .tags
        .iter()
        .filter_map(|tag| match tag {
            swf::Tag::SymbolClass(links) => Some(links.as_slice()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].id, 0);
    assert_eq!(links[0].class_name.as_bytes(), b"Main");
}

#[test]
fn disabled_script_export_does_not_embed_abc() {
    let mut project = Project::demo();
    project.export.include_scripts = false;
    let output = build_swf(&project).expect("build without scripts");
    let buffer = swf::decompress_swf(output.bytes.as_slice()).expect("decompress");
    let parsed = parse(&buffer);
    assert!(
        !parsed
            .tags
            .iter()
            .any(|tag| matches!(tag, swf::Tag::DoAbc2(_)))
    );
    assert!(
        output
            .warnings
            .iter()
            .any(|warning| warning.contains("ActionScript-Export")),
        "disabled script export must be reported: {:?}",
        output.warnings
    );
}

#[test]
fn bundled_as3_example_project_builds_and_links_its_class() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples/as3-example.nova");
    let project = Project::load(&path).expect("checked-in AS3 example loads");
    let output = build_swf(&project).expect("checked-in AS3 example builds");

    let buffer = swf::decompress_swf(output.bytes.as_slice()).expect("decompress");
    let parsed = parse(&buffer);
    let links: Vec<_> = parsed
        .tags
        .iter()
        .filter_map(|tag| match tag {
            swf::Tag::SymbolClass(links) => Some(links.as_slice()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].class_name.as_bytes(), b"Box");

    let abc = AbcReader::new(do_abc2(&parsed).data)
        .read()
        .expect("example ABC parses");
    assert_eq!(abc.instances.len(), 1);
}

#[test]
fn avm1_target_never_sets_the_as3_flag() {
    let mut project = Project::new_empty("AVM1");
    project.export.target = TargetRuntime::Avm1;
    let output = build_swf(&project).expect("avm1 build");
    let buffer = swf::decompress_swf(output.bytes.as_slice()).expect("decompress");
    let parsed = parse(&buffer);
    assert!(!parsed.header.is_action_script_3());
    assert!(
        !parsed
            .tags
            .iter()
            .any(|tag| matches!(tag, swf::Tag::DoAbc2(_))),
        "AVM1 output must not contain AVM2 bytecode"
    );
}

#[test]
fn unlinked_avm2_project_downgrades_to_avm1_flags() {
    // Regression for the M9.1 defect: the default `public class Main {}` stub
    // compiles to ABC that no SymbolClass link can instantiate. Emitting it as
    // AVM2 made Ruffle 0.7.1 skip bitmap characters entirely.
    let project = Project::new_empty("Unverklinkt");
    assert_eq!(project.export.target, TargetRuntime::Avm2);
    let output = build_swf(&project).expect("unlinked build");

    assert!(
        output
            .warnings
            .iter()
            .any(|warning| warning.contains("ohne AS3-Flag")),
        "downgrade must be reported: {:?}",
        output.warnings
    );
    assert_eq!(output.stats.embedded_scripts, 0);
    assert_eq!(output.stats.abc_bytes, 0);

    let buffer = swf::decompress_swf(output.bytes.as_slice()).expect("decompress");
    let parsed = parse(&buffer);
    assert!(
        !parsed.header.is_action_script_3(),
        "inert AVM2 content must not keep the AS3 flag"
    );
    assert!(
        !parsed
            .tags
            .iter()
            .any(|tag| matches!(tag, swf::Tag::DoAbc2(_) | swf::Tag::SymbolClass(_))),
        "unlinked bytecode must not be embedded"
    );
}

#[test]
fn linked_avm2_project_keeps_abc_and_as3_flag() {
    // The counterpart case: a document class extending MovieClip produces a
    // SymbolClass link, so AVM2 output (and bitmap-hostile flag) is required.
    let mut project = Project::new_empty("Verknüpft");
    project.export.target = TargetRuntime::Avm2;
    project.scripts = vec![Script::new(
        "scripts/Main.as",
        "package {\n    import flash.display.MovieClip;\n    public class Main extends MovieClip {\n        public function Main() {\n        }\n    }\n}\n",
    )];
    let output = build_swf(&project).expect("linked build");
    assert_eq!(output.stats.embedded_scripts, 1);
    let buffer = swf::decompress_swf(output.bytes.as_slice()).expect("decompress");
    let parsed = parse(&buffer);
    assert!(parsed.header.is_action_script_3());
    assert!(
        parsed
            .tags
            .iter()
            .any(|tag| matches!(tag, swf::Tag::DoAbc2(_)))
    );
    assert!(
        parsed
            .tags
            .iter()
            .any(|tag| matches!(tag, swf::Tag::SymbolClass(_)))
    );
}

#[test]
fn compiler_errors_abort_the_build_with_source_location() {
    let mut project = Project::new_empty("Fehler");
    project.scripts[0].source =
        "package {\n    public class Main {\n        public function Main() {\n            var x:int = \"nope\";\n        }\n    }\n}\n"
            .into();
    let error = build_swf(&project).expect_err("type error must abort the build");
    let BuildError::ActionScript(message) = error else {
        panic!("expected ActionScript error, got {error:?}");
    };
    assert!(message.contains("scripts/Main.as:"), "{message}");
    assert!(message.contains("Fehler"), "{message}");
    assert!(message.contains("^"), "{message}");
}
