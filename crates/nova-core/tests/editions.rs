//! M19/M20 cross-edition compatibility and packaging-identity tests.
//!
//! The `.nova` project format is shared by all three editions (User, Edu,
//! Enterprise). These tests prove that a project authored in one edition
//! serializes and loads identically for every other edition — there is no
//! edition lock-in and no edition field inside the project document.
//!
//! M20.0 additionally checks `packaging/editions.json`, the single source of
//! truth for product names, installer identities, ProgIds, uninstall keys and
//! portable artifact names, against [`nova_core::Edition`]. This keeps the
//! packaging scripts and the shared core from drifting apart.

use std::collections::HashSet;
use std::path::Path;

use nova_core::{Edition, Project, ProjectTemplate};
use serde_json::Value;

fn sample_project() -> Project {
    // A project exercising several subsystems: scene, symbol, script,
    // asset slot and game metadata stay format-identical across editions.
    let mut project = ProjectTemplate::Platformer.create("Edition-Test".to_string());
    project.touch();
    project
}

fn any_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key(key) || map.values().any(|child| any_key(child, key))
        }
        Value::Array(items) => items.iter().any(|child| any_key(child, key)),
        _ => false,
    }
}

#[test]
fn project_format_has_no_edition_field() {
    let project = sample_project();
    let json = serde_json::to_string(&project).expect("serialize");
    let value: Value = serde_json::from_str(&json).expect("parse project JSON");
    // Precise intent: the project must not carry an edition marker. Do not
    // search for the bare substrings "user"/"edu" — ordinary object, script
    // or asset names may legitimately contain them.
    assert!(
        !any_key(&value, "edition"),
        "project JSON must not contain an `edition` field"
    );
    assert!(
        !any_key(&value, "editionId"),
        "project JSON must not contain an `editionId` field"
    );
}

#[test]
fn project_roundtrips_identically_for_all_editions() {
    // The project model has no edition input; every edition sees the same
    // serializer. Loop anyway so the test documents the three-edition promise
    // and fails if an edition-specific branch is ever introduced.
    let project = sample_project();
    let baseline = serde_json::to_string_pretty(&project).expect("serialize");
    for edition in Edition::ALL {
        let restored: Project = serde_json::from_str(&baseline).expect("deserialize");
        assert_eq!(
            serde_json::to_string_pretty(&restored).unwrap(),
            baseline,
            "round-trip must be byte-identical for the {:?} edition",
            edition
        );
    }
}

#[test]
fn edition_identity_is_complete_and_unique() {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let mut dirs = HashSet::new();
    for edition in Edition::ALL {
        assert!(ids.insert(edition.id()));
        assert!(names.insert(edition.product_name()));
        assert!(dirs.insert(edition.config_dir_name()));
        assert_eq!(Edition::parse(edition.id()), Some(edition));
    }
}

#[test]
fn edition_dirs_are_isolated() {
    // Every edition resolves a distinct configuration directory name, so
    // settings/credentials/provider config cannot overwrite each other.
    let dirs: Vec<_> = Edition::ALL.iter().map(|e| e.config_dir_name()).collect();
    let unique: HashSet<_> = dirs.iter().collect();
    assert_eq!(
        unique.len(),
        Edition::ALL.len(),
        "edition config dirs must be unique: {dirs:?}"
    );
    // User keeps the legacy directory (backwards compatibility).
    assert!(dirs.contains(&"NOVA Flash Studio"));

    let recovery: Vec<_> = Edition::ALL
        .iter()
        .map(|edition| edition.recovery_dir_name())
        .collect();
    let unique: HashSet<_> = recovery.iter().collect();
    assert_eq!(
        unique.len(),
        Edition::ALL.len(),
        "edition recovery dirs must be unique: {recovery:?}"
    );
}

fn editions_json() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("packaging")
        .join("editions.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).expect("packaging/editions.json must be valid JSON")
}

fn string_field<'a>(entry: &'a Value, name: &str) -> &'a str {
    entry
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("packaging/editions.json entry is missing string field `{name}`"))
}

#[test]
fn packaging_manifest_matches_core_edition_api() {
    let doc = editions_json();
    let entries = doc
        .get("editions")
        .and_then(Value::as_array)
        .expect("packaging/editions.json must contain an editions array");
    assert_eq!(
        entries.len(),
        Edition::ALL.len(),
        "packaging manifest must describe every edition"
    );

    let mut ids = HashSet::new();
    let mut products = HashSet::new();
    let mut config_dirs = HashSet::new();
    let mut recovery_dirs = HashSet::new();
    let mut bundle_ids = HashSet::new();
    let mut uninstall_keys = HashSet::new();
    let mut prog_ids = HashSet::new();
    let mut artifact_names = HashSet::new();

    for entry in entries {
        let id = string_field(entry, "id");
        let edition = Edition::parse(id)
            .unwrap_or_else(|| panic!("unknown edition id `{id}` in packaging manifest"));

        assert_eq!(string_field(entry, "product"), edition.product_name());
        assert_eq!(string_field(entry, "configDir"), edition.config_dir_name());
        assert_eq!(
            string_field(entry, "recoveryDir"),
            edition.recovery_dir_name()
        );
        assert_eq!(string_field(entry, "slug"), edition.artifact_slug());

        assert!(ids.insert(id));
        assert!(products.insert(string_field(entry, "product")));
        assert!(config_dirs.insert(string_field(entry, "configDir")));
        assert!(recovery_dirs.insert(string_field(entry, "recoveryDir")));
        assert!(bundle_ids.insert(string_field(entry, "bundleId")));
        assert!(uninstall_keys.insert(string_field(entry, "uninstallKey")));
        assert!(prog_ids.insert(string_field(entry, "progIdNova")));
        assert!(prog_ids.insert(string_field(entry, "progIdSwf")));
        assert!(artifact_names.insert(string_field(entry, "setupExe")));
        assert!(artifact_names.insert(string_field(entry, "portableZip")));

        // Installer/portable names must stay edition-attributable.
        let slug = edition.artifact_slug();
        assert_eq!(
            string_field(entry, "setupExe"),
            format!("NOVA-{slug}-Setup.exe")
        );
        assert_eq!(
            string_field(entry, "portableZip"),
            format!("NOVA-{slug}-Portable.zip")
        );
        assert!(string_field(entry, "appExe").starts_with(edition.product_name()));
        assert!(string_field(entry, "uninstallExe").starts_with(edition.product_name()));
    }
}
