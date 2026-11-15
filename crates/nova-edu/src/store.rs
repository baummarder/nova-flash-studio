//! Atomic JSON persistence helpers shared by all edu stores.
//!
//! Pattern mirrors `nova-app::autosave`: serialize to a temp file in the
//! same directory, then rename over the target. Reads tolerate a UTF-8
//! BOM. Unknown fields are preserved on round-trip where `#[serde(flatten)]`
//! extras are declared; otherwise serde defaults apply.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{EduError, EduResult};

pub const EDU_SCHEMA_VERSION: u32 = 1;

/// Serializes `value` and writes it atomically to `path`.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> EduResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json)?;
    // Best-effort replace; on Windows rename fails if target exists on old
    // APIs — remove first (small window, same pattern as autosave.rs).
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Reads JSON, tolerating a UTF-8 BOM. Returns `Ok(None)` when the file is
/// absent so callers can apply defaults.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> EduResult<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    let text = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let value = serde_json::from_str(&text)?;
    Ok(Some(value))
}

/// Reads JSON or returns the provided default.
pub fn read_or<T: DeserializeOwned>(path: &Path, default: T) -> EduResult<T> {
    Ok(read_json(path)?.unwrap_or(default))
}

/// Deletes a file if present; missing files are not an error.
pub fn remove_if_exists(path: &Path) -> EduResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(EduError::Io(e)),
    }
}

/// Recursive directory delete; missing directories are fine.
pub fn remove_dir_if_exists(path: &Path) -> EduResult<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(EduError::Io(e)),
    }
}

/// Canonical-ish absolute path for diagnostics (does not require existence).
pub fn display_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}
