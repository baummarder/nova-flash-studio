//! M19 cross-edition compatibility tests.
//!
//! The `.nova` project format is shared by all three editions (User, Edu,
//! Enterprise). These tests prove that a project authored in one edition
//! serializes and loads identically for every other edition — there is no
//! edition lock-in and no edition field inside the project document.

use nova_core::{Project, ProjectTemplate};

fn sample_project() -> Project {
    // A project exercising several subsystems: scene, symbol, script,
    // asset slot and game metadata stay format-identical across editions.
    let mut project = ProjectTemplate::Platformer.create("Edition-Test".to_string());
    project.touch();
    project
}

#[test]
fn project_format_has_no_edition_field() {
    let project = sample_project();
    let json = serde_json::to_string(&project).expect("serialize");
    // The edition must never leak into the project document: projects are
    // freely interchangeable between User, Edu and Enterprise.
    for forbidden in ["\"edition\"", "\"user\"", "\"edu\"", "\"enterprise\""] {
        assert!(
            !json.contains(forbidden),
            "project JSON must not contain edition marker {forbidden}"
        );
    }
}

#[test]
fn project_roundtrips_identically_for_all_editions() {
    let project = sample_project();
    let json = serde_json::to_string_pretty(&project).expect("serialize");
    let restored: Project = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        serde_json::to_string_pretty(&restored).unwrap(),
        json,
        "round-trip must be byte-identical regardless of edition"
    );
}

#[test]
fn edition_identity_is_complete_and_unique() {
    use nova_core::Edition;
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    let mut dirs = std::collections::HashSet::new();
    for edition in Edition::ALL {
        assert!(ids.insert(edition.id()));
        assert!(names.insert(edition.product_name()));
        assert!(dirs.insert(edition.config_dir_name()));
        assert!(Edition::parse(edition.id()) == Some(edition));
    }
}

#[test]
fn edition_dirs_are_isolated() {
    use nova_core::Edition;
    // Every edition resolves a distinct configuration directory name, so
    // settings/credentials/provider config cannot overwrite each other.
    let dirs: Vec<_> = Edition::ALL.iter().map(|e| e.config_dir_name()).collect();
    let unique: std::collections::HashSet<_> = dirs.iter().collect();
    assert_eq!(
        unique.len(),
        3,
        "edition config dirs must be unique: {dirs:?}"
    );
    // User keeps the legacy directory (backwards compatibility).
    assert!(dirs.contains(&"NOVA Flash Studio"));
}
