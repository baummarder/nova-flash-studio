use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("E/A-Fehler: {0}")]
    Io(#[from] std::io::Error),
    #[error("NØVA-Projektfehler: {0}")]
    Nova(#[from] nova_core::ProjectError),
    #[error("SWF konnte nicht gelesen werden: {0}")]
    Swf(String),
    #[error("FLA/XFL konnte nicht gelesen werden: {0}")]
    Fla(String),
    #[error("ZIP-Fehler: {0}")]
    Zip(String),
    #[error("XML-Fehler: {0}")]
    Xml(String),
    #[error("SWF-Export fehlgeschlagen: {0}")]
    Build(#[from] nova_build::BuildError),
    #[error("Nicht unterstützt: {0}")]
    Unsupported(String),
    #[error("Unbekanntes Dateiformat: {0}")]
    UnknownFormat(PathBuf),
}

pub type FormatResult<T> = Result<T, FormatError>;
