//! Credential storage.
//!
//! On Windows the default store uses DPAPI (`CryptProtectData`) scoped to the
//! current user. The encrypted blob is written to the per-user configuration
//! directory. API keys are never stored in the provider JSON file.
//!
//! Other platforms use a restricted file as a documented fallback; the app
//! reports which backend is active so users are not misled about the security
//! level. Automated tests use [`MemoryCredentialStore`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{AiError, AiResult};

pub trait CredentialStore: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn get(&self, provider_id: &str) -> AiResult<Option<String>>;
    fn set(&self, provider_id: &str, secret: &str) -> AiResult<()>;
    fn delete(&self, provider_id: &str) -> AiResult<()>;
    fn has(&self, provider_id: &str) -> AiResult<bool> {
        Ok(self.get(provider_id)?.is_some())
    }
}

#[derive(Default)]
pub struct MemoryCredentialStore {
    values: Mutex<BTreeMap<String, String>>,
}

impl MemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for MemoryCredentialStore {
    fn backend_name(&self) -> &'static str {
        "memory (tests only)"
    }

    fn get(&self, provider_id: &str) -> AiResult<Option<String>> {
        Ok(self.values.lock().unwrap().get(provider_id).cloned())
    }

    fn set(&self, provider_id: &str, secret: &str) -> AiResult<()> {
        self.values
            .lock()
            .unwrap()
            .insert(provider_id.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, provider_id: &str) -> AiResult<()> {
        self.values.lock().unwrap().remove(provider_id);
        Ok(())
    }
}

/// Per-user DPAPI-backed store used on Windows.
pub struct DpapiCredentialStore {
    dir: PathBuf,
}

impl DpapiCredentialStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, provider_id: &str) -> PathBuf {
        self.dir.join(format!("{}.cred", safe_id(provider_id)))
    }
}

impl CredentialStore for DpapiCredentialStore {
    fn backend_name(&self) -> &'static str {
        "Windows DPAPI (per user)"
    }

    fn get(&self, provider_id: &str) -> AiResult<Option<String>> {
        let path = self.path(provider_id);
        if !path.exists() {
            return Ok(None);
        }
        let encrypted = fs::read(&path).map_err(AiError::io)?;
        let plain = dpapi_unprotect(&encrypted)?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|_| AiError::Credential("Gespeicherte Anmeldedaten sind ungültig.".into()))
    }

    fn set(&self, provider_id: &str, secret: &str) -> AiResult<()> {
        fs::create_dir_all(&self.dir).map_err(AiError::io)?;
        let encrypted = dpapi_protect(secret.as_bytes())?;
        atomic_write(&self.path(provider_id), &encrypted)
    }

    fn delete(&self, provider_id: &str) -> AiResult<()> {
        let path = self.path(provider_id);
        if path.exists() {
            fs::remove_file(path).map_err(AiError::io)?;
        }
        Ok(())
    }
}

/// Best-effort fallback for non-Windows builds. The file is restricted to the
/// current user where the platform supports Unix permissions, but it is not
/// encrypted. The UI names this backend explicitly.
pub struct RestrictedFileCredentialStore {
    dir: PathBuf,
}

impl RestrictedFileCredentialStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, provider_id: &str) -> PathBuf {
        self.dir.join(format!("{}.cred", safe_id(provider_id)))
    }
}

impl CredentialStore for RestrictedFileCredentialStore {
    fn backend_name(&self) -> &'static str {
        "eingeschränkte Datei (nicht verschlüsselt)"
    }

    fn get(&self, provider_id: &str) -> AiResult<Option<String>> {
        let path = self.path(provider_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(AiError::io)?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| AiError::Credential("Gespeicherte Anmeldedaten sind ungültig.".into()))
    }

    fn set(&self, provider_id: &str, secret: &str) -> AiResult<()> {
        fs::create_dir_all(&self.dir).map_err(AiError::io)?;
        let path = self.path(provider_id);
        atomic_write(&path, secret.as_bytes())?;
        restrict_permissions(&path)?;
        Ok(())
    }

    fn delete(&self, provider_id: &str) -> AiResult<()> {
        let path = self.path(provider_id);
        if path.exists() {
            fs::remove_file(path).map_err(AiError::io)?;
        }
        Ok(())
    }
}

pub fn default_store(config_dir: &Path) -> Box<dyn CredentialStore> {
    let dir = config_dir.join("credentials");
    #[cfg(target_os = "windows")]
    {
        Box::new(DpapiCredentialStore::new(dir))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Box::new(RestrictedFileCredentialStore::new(dir))
    }
}

fn safe_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len() + 8);
    for character in id.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            out.push(character);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("provider");
    }
    format!("{out}-{:x}", out.len())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> AiResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(AiError::io)?;
    }
    let temporary = path.with_extension("cred.tmp");
    fs::write(&temporary, bytes).map_err(AiError::io)?;
    if path.exists() {
        fs::remove_file(path).map_err(AiError::io)?;
    }
    fs::rename(&temporary, path).map_err(AiError::io)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> AiResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).map_err(AiError::io)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions).map_err(AiError::io)
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> AiResult<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn dpapi_protect(plain: &[u8]) -> AiResult<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };

    let entropy = b"NOVA-Flash-Studio-AI-Credentials-v1";
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            &entropy_blob,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(AiError::Credential(format!(
            "DPAPI-Verschlüsselung fehlgeschlagen (Windows-Fehler {}).",
            std::io::Error::last_os_error()
        )));
    }
    let encrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as _);
    }
    Ok(encrypted)
}

#[cfg(target_os = "windows")]
fn dpapi_unprotect(encrypted: &[u8]) -> AiResult<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };

    let entropy = b"NOVA-Flash-Studio-AI-Credentials-v1";
    let input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len() as u32,
        pbData: encrypted.as_ptr() as *mut u8,
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let mut description: *mut u16 = std::ptr::null_mut();
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            &mut description,
            &entropy_blob,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(AiError::Credential(format!(
            "DPAPI-Entschlüsselung fehlgeschlagen (Windows-Fehler {}).",
            std::io::Error::last_os_error()
        )));
    }
    let plain =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as _);
        if !description.is_null() {
            LocalFree(description as _);
        }
    }
    Ok(plain)
}

#[cfg(not(target_os = "windows"))]
fn dpapi_protect(_plain: &[u8]) -> AiResult<Vec<u8>> {
    Err(AiError::Credential(
        "DPAPI ist nur unter Windows verfügbar.".into(),
    ))
}

#[cfg(not(target_os = "windows"))]
fn dpapi_unprotect(_encrypted: &[u8]) -> AiResult<Vec<u8>> {
    Err(AiError::Credential(
        "DPAPI ist nur unter Windows verfügbar.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_roundtrip_and_delete() {
        let store = MemoryCredentialStore::new();
        assert!(!store.has("p1").unwrap());
        store.set("p1", "secret").unwrap();
        assert!(store.has("p1").unwrap());
        assert_eq!(store.get("p1").unwrap().as_deref(), Some("secret"));
        store.delete("p1").unwrap();
        assert!(!store.has("p1").unwrap());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn dpapi_roundtrip_does_not_store_plaintext() {
        let dir = std::env::temp_dir().join(format!("nova-ai-dpapi-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = DpapiCredentialStore::new(&dir);
        store.set("provider", "super-secret-value").unwrap();
        let raw = fs::read(dir.join(format!("{}.cred", safe_id("provider")))).unwrap();
        assert!(
            !raw.windows(18)
                .any(|window| window == b"super-secret-value")
        );
        assert_eq!(
            store.get("provider").unwrap().as_deref(),
            Some("super-secret-value")
        );
        store.delete("provider").unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn credential_filename_cannot_escape_directory() {
        let name = safe_id("../../evil/name");
        assert!(!name.contains('/'));
        assert!(!name.contains('\\'));
        assert!(!name.contains(".."));
    }
}
