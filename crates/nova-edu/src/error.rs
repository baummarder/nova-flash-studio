//! Error type for all `nova-edu` operations.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum EduError {
    #[error("I/O-Fehler: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON-Fehler: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Ungültiger Schema-Stand {found}, erwartet {expected}")]
    Schema { found: u32, expected: u32 },
    #[error("Paketfehler: {0}")]
    Package(String),
    #[error("Ungültiger Pfad '{0}': {1}")]
    Path(String, String),
    #[error("Integritätsprüfung fehlgeschlagen: {0}")]
    Integrity(String),
    #[error("Konflikt: {0}")]
    Conflict(String),
    #[error("Nicht gefunden: {0}")]
    NotFound(String),
    #[error("Ungültige Eingabe: {0}")]
    Invalid(String),
    #[error("ZIP-Fehler: {0}")]
    Zip(#[from] zip::result::ZipError),
}

pub type EduResult<T> = Result<T, EduError>;

/// Resolve a workspace-relative path safely (no `..`, no absolute, no drive).
pub fn safe_join(root: &std::path::Path, rel: &str) -> EduResult<PathBuf> {
    let rel_path = std::path::Path::new(rel);
    if rel_path.is_absolute() {
        return Err(EduError::Path(
            rel.into(),
            "absoluter Pfad nicht erlaubt".into(),
        ));
    }
    for component in rel_path.components() {
        match component {
            std::path::Component::Normal(_) => {}
            std::path::Component::CurDir => {}
            _ => {
                return Err(EduError::Path(
                    rel.into(),
                    "nur relative Pfade ohne .. oder Laufwerk erlaubt".into(),
                ));
            }
        }
    }
    Ok(root.join(rel_path))
}

/// Filesystem-safe file name from arbitrary display text.
pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "unbenannt".to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}
