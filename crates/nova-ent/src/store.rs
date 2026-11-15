//! Atomic JSON persistence helpers shared by all enterprise modules.

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{EntError, EntResult};

/// Current schema version for enterprise sidecar documents.
pub const ENT_SCHEMA_VERSION: u32 = 1;

/// Reads JSON or returns `default` when the file does not exist.
/// Corrupt files are an error — never silently replaced.
pub fn read_or<T: DeserializeOwned + Default>(path: &Path, default: T) -> EntResult<T> {
    if !path.exists() {
        return Ok(default);
    }
    let text = std::fs::read_to_string(path)?;
    let value: T = serde_json::from_str(&text)
        .map_err(|e| EntError::Invalid(format!("{}: {e}", path.display())))?;
    Ok(value)
}

/// Reads JSON that must exist.
pub fn read_required<T: DeserializeOwned>(path: &Path) -> EntResult<T> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

/// Writes JSON atomically: temp file in the same directory, then rename.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> EntResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json)?;
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Appends one JSON line to a log file (audit log). Creates parent dirs.
pub fn append_jsonl<T: Serialize>(path: &Path, value: &T) -> EntResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(value)?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Reads all JSONL entries; malformed lines are skipped (logged by caller
/// if needed — audit logs tolerate partial corruption).
pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> EntResult<Vec<T>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<T>(line) {
            out.push(v);
        }
    }
    Ok(out)
}

pub fn remove_if_exists(path: &Path) -> EntResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn remove_dir_if_exists(path: &Path) -> EntResult<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
