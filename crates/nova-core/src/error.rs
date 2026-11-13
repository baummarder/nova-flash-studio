use std::path::PathBuf;

use thiserror::Error;

pub type ProjectResult<T> = Result<T, ProjectError>;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid .nova JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error(
        "project format version {found} is newer than the supported version {supported}; \
         please update NØVA Flash Studio"
    )]
    UnsupportedVersion { found: u32, supported: u32 },

    #[error("invalid project: {0}")]
    Validation(String),

    #[error("not a NØVA project (expected format tag '{expected}', found '{found}')")]
    NotAProject { expected: String, found: String },

    #[error("scene not found: {0}")]
    SceneNotFound(uuid::Uuid),

    #[error("object not found: {0}")]
    ObjectNotFound(uuid::Uuid),

    #[error("cannot write project to {0}")]
    Path(PathBuf),
}
