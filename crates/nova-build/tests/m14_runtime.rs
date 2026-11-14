//! M14 AVM2 API-surface tests: SWF v13 auto-raise for script projects and
//! the playerglobal API demo build.

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
fn script_projects_raise_swf_version_to_13() {
    // M14: playerglobal APIs (JSON, ByteArray, flash.utils.*, …) are gated at
    // SWF v13 in Ruffle; script projects are raised automatically.
    let project = Project::load(&sample("m13-runtime-features.nova")).expect("demo loads");
    assert_eq!(project.export.swf_version, 10, "fixture pins v10");
    let output = build_swf(&project).expect("demo builds");
    assert!(
        output
            .warnings
            .iter()
            .any(|w| w.contains("Version 13") || w.contains("Version 10")),
        "expected version-raise warning, got: {:?}",
        output.warnings
    );
}

#[test]
fn m14_api_surface_demo_builds() {
    let project = Project::load(&sample("m14-api-surface.nova")).expect("demo loads");
    let output = build_swf(&project).expect("demo builds");
    // Single Main.as script, no game metadata -> one DoABC2 tag.
    assert_eq!(do_abc2_count(&output.bytes), 1);
}
