//! The built-in AS3 game kit must compile with the `nova-as` subset.
//!
//! These tests prove the helper sources are valid input for the compiler;
//! runtime behaviour inside Ruffle/Adobe is verified per game sample.

use nova_core::gamekit;

#[test]
fn every_gamekit_script_compiles_without_errors() {
    for (path, source) in gamekit::FILES {
        let unit = nova_as::compile(source);
        let errors: Vec<_> = unit
            .diagnostics
            .iter()
            .filter(|d| d.severity == nova_as::Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{path} produced errors: {errors:?}");
        assert!(!unit.abc.is_empty(), "{path} produced no ABC");
    }
}

#[test]
fn gamekit_install_is_idempotent() {
    let mut project = nova_core::Project::new_empty("Kit");
    let before = project.scripts.len();
    gamekit::install(&mut project);
    let after_first = project.scripts.len();
    assert_eq!(after_first, before + gamekit::FILES.len());
    gamekit::install(&mut project);
    assert_eq!(
        project.scripts.len(),
        after_first,
        "second install must not duplicate"
    );
    for (path, _) in gamekit::FILES {
        assert!(project.scripts.iter().any(|s| s.path == *path));
    }
}

/// Every shipped game template must produce a project whose scripts all
/// compile cleanly — a template that fails here can never build a SWF.
#[test]
fn every_game_template_compiles() {
    for template in nova_core::games::TEMPLATES {
        let project = nova_core::games::create_project(template.id)
            .unwrap_or_else(|| panic!("template {} missing", template.id));
        for script in &project.scripts {
            let unit = nova_as::compile(&script.source);
            let errors: Vec<_> = unit
                .diagnostics
                .iter()
                .filter(|d| d.severity == nova_as::Severity::Error)
                .collect();
            assert!(
                errors.is_empty(),
                "{}/{} produced errors: {errors:?}",
                template.id,
                script.path
            );
        }
    }
}

/// Game metadata on template objects must survive a save/load roundtrip.
#[test]
fn game_templates_roundtrip_with_metadata() {
    for template in nova_core::games::TEMPLATES {
        let project = nova_core::games::create_project(template.id).unwrap();
        let json = project.to_json().unwrap();
        let loaded = nova_core::Project::from_json(&json).unwrap();
        for scene in &loaded.scenes {
            for layer in &scene.layers {
                for frame in &layer.frames {
                    for object in &frame.objects {
                        if let Some(game) = &object.game {
                            assert!(
                                game.collision.is_some() || game.physics.is_some(),
                                "{}/{}: empty game config",
                                template.id,
                                object.name
                            );
                        }
                    }
                }
            }
        }
    }
}
