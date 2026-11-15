//! Error type for the Enterprise domain layer.

use std::path::PathBuf;

use thiserror::Error;

pub type EntResult<T> = Result<T, EntError>;

#[derive(Debug, Error)]
pub enum EntError {
    #[error("I/O-Fehler: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON-Fehler: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP-Fehler: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Projektfehler: {0}")]
    Project(#[from] nova_core::ProjectError),
    #[error("{0}")]
    Invalid(String),
    #[error("Nicht gefunden: {0}")]
    NotFound(String),
    #[error("Konflikt: {0}")]
    Conflict(String),
    #[error("Integrität: {0}")]
    Integrity(String),
    #[error("Paket: {0}")]
    Package(String),
    #[error("Schema-Version {found} wird nicht unterstützt (erwartet {expected})")]
    Schema { found: u32, expected: u32 },
    #[error("Pfad '{path}' nicht erlaubt: {reason}")]
    Path { path: String, reason: String },
}

/// Joins a workspace-relative path under `root`, rejecting absolute paths,
/// `..` traversal and drive prefixes. Mirrors `nova-edu::safe_join`.
pub fn safe_join(root: &std::path::Path, rel: &str) -> EntResult<PathBuf> {
    let rel_path = std::path::Path::new(rel);
    if rel_path.is_absolute() {
        return Err(EntError::Path {
            path: rel.into(),
            reason: "absoluter Pfad nicht erlaubt".into(),
        });
    }
    for component in rel_path.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err(EntError::Path {
                    path: rel.into(),
                    reason: ".. nicht erlaubt".into(),
                });
            }
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                return Err(EntError::Path {
                    path: rel.into(),
                    reason: "Laufwerks-/Root-Präfix nicht erlaubt".into(),
                });
            }
            _ => {}
        }
    }
    Ok(root.join(rel_path))
}

/// Filesystem-safe file name (no separators, no `..`, no drive letters).
pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('.').to_string();
    if trimmed.is_empty() {
        "asset".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}
