use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("Projektfehler: {0}")]
    Project(#[from] nova_core::ProjectError),

    #[error("SWF-Fehler: {0}")]
    Swf(#[from] nova_swf::SwfError),

    #[error("Das Projekt enthält keine aktive Szene.")]
    NoScene,

    #[error("ActionScript-Compilerfehler:\n{0}")]
    ActionScript(String),

    #[error("{0}")]
    Limit(String),
}
