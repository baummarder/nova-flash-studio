//! Encoding regression guard (M21 remediation).
//!
//! Several M21 files were corrupted by a UTF-8→CP1252→UTF-8 round-trip
//! (PowerShell `Get-Content`/`Set-Content`), producing mojibake in
//! German strings and stray UTF-8 BOMs. This test scans
//! every Rust source file in the workspace and fails on:
//!
//! - invalid UTF-8
//! - a UTF-8 BOM
//! - typical mojibake marker sequences
//!
//! It is intentionally repo-wide so the whole workspace stays clean.

use std::path::{Path, PathBuf};

/// Mojibake marker sequences produced by decoding UTF-8 as CP1252/Latin-1.
/// Built from char codes so this file does not contain the markers itself.
fn mojibake_markers() -> Vec<String> {
    let s =
        |codes: &[u32]| -> String { codes.iter().map(|&c| char::from_u32(c).unwrap()).collect() };
    vec![
        s(&[0x00E2, 0x20AC]), // dash/ellipsis/quote family
        s(&[0x00C3, 0x00A4]), // a-umlaut family
        s(&[0x00C3, 0x00B6]),
        s(&[0x00C3, 0x00BC]),
        s(&[0x00C3, 0x0084]),
        s(&[0x00C3, 0x0096]),
        s(&[0x00C3, 0x009C]),
        s(&[0x00C3, 0x0178]), // sharp-s family
        s(&[0x00C2, 0x00B7]), // middle dot
        s(&[0x00E2, 0x2020]), // dagger family (arrows)
        s(&[0x00E2, 0x0153]), // low-9 quote
        s(&[0x00E2, 0x2019]), // right single quote
        s(&[0x00E2, 0x2026]), // ellipsis
    ]
}

fn repo_root() -> PathBuf {
    // crates/nova-core -> ../.. is the workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "target" || name.starts_with('.') {
                continue;
            }
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn all_rust_sources_are_clean_utf8_without_bom_or_mojibake() {
    let root = repo_root();
    let crates = root.join("crates");
    let mut files = Vec::new();
    rust_sources(&crates, &mut files);
    assert!(
        !files.is_empty(),
        "no .rs files found under {}",
        crates.display()
    );

    let mut problems = Vec::new();
    let markers = mojibake_markers();
    for path in &files {
        let bytes = std::fs::read(path).expect("read source file");
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();

        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            problems.push(format!("{rel}: UTF-8 BOM present"));
            continue;
        }
        let text = match std::str::from_utf8(&bytes) {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("{rel}: invalid UTF-8 ({e})"));
                continue;
            }
        };
        for marker in &markers {
            if text.contains(marker.as_str()) {
                problems.push(format!(
                    "{rel}: mojibake marker U+{:04X?}",
                    marker.chars().next().map(|c| c as u32)
                ));
                break;
            }
        }
    }

    assert!(
        problems.is_empty(),
        "encoding problems in {} file(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn m21_files_contain_correct_german_strings() {
    // Spot-check that the repaired M21 files actually contain the intended
    // German text (guards against over- or under-correction).
    let root = repo_root();
    let checks: &[(&str, &[&str])] = &[
        (
            "crates/nova-app/src/enterprise_view.rs",
            &[
                "keine Anmeldung",
                "Workspace öffnen",
                "Integritätsprobleme",
                "Änderungen",
                "geändert",
            ],
        ),
        (
            "crates/nova-ent/src/review.rs",
            &[
                "In Prüfung",
                "Änderungen angefordert",
                "Ungültiger Review-Übergang",
            ],
        ),
        (
            "crates/nova-ent/src/versions.rs",
            &[
                "Keine strukturellen Änderungen",
                "gültiges UTF-8",
                "beschädigt",
            ],
        ),
        (
            "crates/nova-ent/src/builds.rs",
            &["Mindestens ein Build-Ziel nötig"],
        ),
    ];
    for (rel, needles) in checks {
        let text = std::fs::read_to_string(root.join(rel)).expect("read file");
        for needle in *needles {
            assert!(
                text.contains(needle),
                "{rel}: missing expected string {needle:?}"
            );
        }
    }
}
