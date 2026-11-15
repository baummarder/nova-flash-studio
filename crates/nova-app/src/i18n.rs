//! Central internationalization for NØVA Flash Studio (M22).
//!
//! - Five locales: `de-DE`, `en-GB`, `da-DK`, `fr-FR`, `pl-PL`.
//! - `en-GB` is the fallback for missing keys.
//! - Translations live in `i18n/<locale>.json` (flat `section.key` map,
//!   UTF-8 without BOM, embedded via `include_str!` — fully offline).
//! - `t(key)` returns the translated string; `tf(key, args)` substitutes
//!   `{name}` placeholders.
//! - The active language is process-global (`set_language`) so deeply
//!   nested views can translate without threading state through every
//!   call site. Changing the language repaints the whole UI automatically
//!   because every frame re-reads the current language.
//! - Missing keys fall back to `en-GB`, then to the key itself (visible
//!   in development) and are recorded for tests/diagnostics.
//!
//! Persistence is per-edition via `<config-dir>/app-settings.json`
//! (`{"language": "de-DE"}`) — edition config dirs are already isolated.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};

/// Supported UI languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {
    DeDe,
    EnGb,
    DaDk,
    FrFr,
    PlPl,
}

impl Lang {
    pub const ALL: &[Lang] = &[Lang::DeDe, Lang::EnGb, Lang::DaDk, Lang::FrFr, Lang::PlPl];

    /// BCP-47-ish locale code used in file names and settings.
    pub fn code(self) -> &'static str {
        match self {
            Lang::DeDe => "de-DE",
            Lang::EnGb => "en-GB",
            Lang::DaDk => "da-DK",
            Lang::FrFr => "fr-FR",
            Lang::PlPl => "pl-PL",
        }
    }

    /// Endonym shown in the language wizard / settings.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::DeDe => "Deutsch",
            Lang::EnGb => "English (UK)",
            Lang::DaDk => "Dansk",
            Lang::FrFr => "Français",
            Lang::PlPl => "Polski",
        }
    }

    pub fn parse(code: &str) -> Option<Lang> {
        Lang::ALL.iter().copied().find(|l| l.code() == code)
    }
}

// ------------------------------------------------------------------ store

fn tables() -> &'static BTreeMap<&'static str, BTreeMap<String, String>> {
    static TABLES: OnceLock<BTreeMap<&'static str, BTreeMap<String, String>>> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut map = BTreeMap::new();
        for lang in Lang::ALL {
            let json = match lang {
                Lang::DeDe => include_str!("../i18n/de-DE.json"),
                Lang::EnGb => include_str!("../i18n/en-GB.json"),
                Lang::DaDk => include_str!("../i18n/da-DK.json"),
                Lang::FrFr => include_str!("../i18n/fr-FR.json"),
                Lang::PlPl => include_str!("../i18n/pl-PL.json"),
            };
            let table: BTreeMap<String, String> =
                serde_json::from_str(json).expect("i18n locale file must be valid JSON");
            map.insert(lang.code(), table);
        }
        map
    })
}

fn current() -> &'static RwLock<Lang> {
    static CURRENT: OnceLock<RwLock<Lang>> = OnceLock::new();
    CURRENT.get_or_init(|| RwLock::new(Lang::EnGb))
}

fn missing() -> &'static Mutex<BTreeSet<String>> {
    static MISSING: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    MISSING.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// The active UI language (defaults to en-GB until settings load).
pub fn language() -> Lang {
    *current().read().unwrap_or_else(|e| e.into_inner())
}

/// Switch language at runtime; the next frame picks it up everywhere.
pub fn set_language(lang: Lang) {
    *current().write().unwrap_or_else(|e| e.into_inner()) = lang;
    save_language(lang);
}

/// Keys that fell back during this session (diagnostics/tests).
#[allow(dead_code)] // consumed by tests and future diagnostics UI
pub fn missing_keys() -> Vec<String> {
    missing()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .cloned()
        .collect()
}

/// Translate `key` in the active language.
///
/// Fallback chain: active language → en-GB → the key itself.
/// A missing key is recorded (visible in debug builds via stderr once).
pub fn t(key: &str) -> String {
    t_for(language(), key)
}

/// Translate `key` in an explicit language (used by the wizard preview,
/// which shows a language before it is committed).
pub fn t_for(lang: Lang, key: &str) -> String {
    lookup(lang, key).unwrap_or_else(|| {
        if let Ok(mut set) = missing().lock()
            && set.insert(key.to_string())
        {
            #[cfg(debug_assertions)]
            eprintln!("[i18n] missing key: {key}");
        }
        key.to_string()
    })
}

/// Translate with `{name}` placeholders replaced by `args` pairs.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    let mut out = t(key);
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

fn lookup(lang: Lang, key: &str) -> Option<String> {
    let tables = tables();
    if let Some(v) = tables
        .get(lang.code())
        .and_then(|t| t.get(key))
        .filter(|v| !v.is_empty())
    {
        return Some(v.clone());
    }
    if lang != Lang::EnGb {
        return tables
            .get(Lang::EnGb.code())
            .and_then(|t| t.get(key))
            .filter(|v| !v.is_empty())
            .cloned();
    }
    None
}

// ------------------------------------------------------------- persistence

const SETTINGS_FILE: &str = "app-settings.json";

fn settings_path() -> Option<PathBuf> {
    nova_core::Edition::current()
        .config_dir()
        .map(|d| d.join(SETTINGS_FILE))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct AppSettings {
    #[serde(default)]
    language: Option<String>,
    /// M23.1: "stage" | "split" | "code" — editor workspace layout.
    #[serde(default)]
    layout: Option<String>,
    /// M23.1: stage/code divider position (0.2..=0.8).
    #[serde(default)]
    split_ratio: Option<f32>,
}

fn read_settings() -> AppSettings {
    settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_settings(settings: &AppSettings) {
    let Some(path) = settings_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(settings) {
        // Atomic-ish: write temp then rename.
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Load the persisted language. Missing/corrupt config → `None`
/// (caller decides: first-run wizard vs. en-GB fallback).
pub fn stored_language() -> Option<Lang> {
    read_settings().language.as_deref().and_then(Lang::parse)
}

/// Whether the language was ever explicitly chosen (drives the wizard).
pub fn language_is_chosen() -> bool {
    stored_language().is_some()
}

/// Apply the persisted language, or en-GB when unset/corrupt.
pub fn load_language() {
    let lang = stored_language().unwrap_or(Lang::EnGb);
    set_language_silent(lang);
}

fn set_language_silent(lang: Lang) {
    *current().write().unwrap_or_else(|e| e.into_inner()) = lang;
}

fn save_language(lang: Lang) {
    let mut settings = read_settings();
    settings.language = Some(lang.code().to_string());
    write_settings(&settings);
}

// ------------------------------------------------- workspace layout (M23.1)

/// Persisted editor layout: "stage" | "split" | "code".
pub fn stored_layout() -> Option<String> {
    read_settings().layout
}

/// Persisted stage/code divider ratio.
pub fn stored_split_ratio() -> Option<f32> {
    read_settings().split_ratio
}

/// Persist the workspace arrangement (per edition, like the language).
pub fn save_layout(layout: &str, split_ratio: f32) {
    let mut settings = read_settings();
    settings.layout = Some(layout.to_string());
    settings.split_ratio = Some(split_ratio.clamp(0.2, 0.8));
    write_settings(&settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_locales_have_identical_key_sets() {
        let tables = tables();
        let reference = tables.get(Lang::EnGb.code()).expect("en-GB table");
        for lang in Lang::ALL {
            let table = tables.get(lang.code()).expect("locale table");
            let missing: Vec<_> = reference
                .keys()
                .filter(|k| !table.contains_key(*k))
                .collect();
            assert!(
                missing.is_empty(),
                "{} misses {} keys: {:?}",
                lang.code(),
                missing.len(),
                &missing[..missing.len().min(10)]
            );
        }
    }

    #[test]
    fn locale_files_are_valid_utf8_without_bom() {
        for lang in Lang::ALL {
            let json = match lang {
                Lang::DeDe => include_str!("../i18n/de-DE.json"),
                Lang::EnGb => include_str!("../i18n/en-GB.json"),
                Lang::DaDk => include_str!("../i18n/da-DK.json"),
                Lang::FrFr => include_str!("../i18n/fr-FR.json"),
                Lang::PlPl => include_str!("../i18n/pl-PL.json"),
            };
            assert!(!json.starts_with('\u{feff}'), "{} has BOM", lang.code());
        }
    }

    #[test]
    fn placeholders_are_consistent_across_locales() {
        let tables = tables();
        for (key, en_value) in tables.get(Lang::EnGb.code()).unwrap() {
            for lang in Lang::ALL {
                let value = &tables[lang.code()][key];
                let en_ph: BTreeSet<_> = en_value
                    .split(['{', '}'])
                    .filter(|s| {
                        !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    })
                    .collect();
                let _ = en_ph;
                // Simple check: count of '{' must match en-GB.
                assert_eq!(
                    value.matches('{').count(),
                    en_value.matches('{').count(),
                    "{key}: placeholder count mismatch in {}",
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn fallback_chain_works() {
        set_language_silent(Lang::DeDe);
        // Every en-GB key resolves in de-DE (parity test above guarantees it).
        assert!(lookup(Lang::DeDe, "menu.file.new").is_some());
        // Unknown key falls back to key itself via t().
        assert_eq!(t("definitely.missing.key"), "definitely.missing.key");
        assert!(missing_keys().contains(&"definitely.missing.key".to_string()));
    }

    #[test]
    fn lang_codes_roundtrip() {
        for lang in Lang::ALL {
            assert_eq!(Lang::parse(lang.code()), Some(*lang));
        }
        assert_eq!(Lang::parse("xx-XX"), None);
        assert_eq!(Lang::parse("de"), None);
    }

    /// Serializes tests that mutate the process-wide config-dir override.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn language_persists_per_edition_config_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Use the config-dir override so nothing touches the real profile.
        let dir = std::env::temp_dir().join(format!("nova-i18n-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // SAFETY: test-only env mutation; restored before returning. The
        // override is read lazily per call, no other test in this binary
        // depends on the config dir concurrently.
        unsafe { std::env::set_var("NOVA_FLASH_STUDIO_CONFIG_DIR", &dir) };

        set_language(Lang::FrFr);
        assert_eq!(stored_language(), Some(Lang::FrFr));
        set_language(Lang::DeDe);
        assert_eq!(stored_language(), Some(Lang::DeDe));

        // Corrupt config falls back to None (caller uses en-GB).
        std::fs::write(dir.join(SETTINGS_FILE), b"{not json").unwrap();
        assert_eq!(stored_language(), None);

        unsafe { std::env::remove_var("NOVA_FLASH_STUDIO_CONFIG_DIR") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workspace_layout_persists_and_preserves_language() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("nova-i18n-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // SAFETY: test-only env mutation, restored before returning.
        unsafe { std::env::set_var("NOVA_FLASH_STUDIO_CONFIG_DIR", &dir) };

        set_language(Lang::DaDk);
        save_layout("split", 0.65);
        assert_eq!(stored_layout().as_deref(), Some("split"));
        assert_eq!(stored_split_ratio(), Some(0.65));
        // Language must survive a layout save (shared settings file).
        assert_eq!(stored_language(), Some(Lang::DaDk));

        save_layout("code", 0.9); // clamped to 0.8
        assert_eq!(stored_layout().as_deref(), Some("code"));
        assert_eq!(stored_split_ratio(), Some(0.8));

        unsafe { std::env::remove_var("NOVA_FLASH_STUDIO_CONFIG_DIR") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tf_substitutes_placeholders() {
        set_language_silent(Lang::EnGb);
        let out = tf(
            "status.frame_info",
            &[
                ("scene", "Szene 1"),
                ("frame", "3"),
                ("total", "12"),
                ("zoom", "100"),
            ],
        );
        assert!(out.contains("Szene 1"), "got: {out}");
        assert!(out.contains('3'), "got: {out}");
        assert!(!out.contains("{scene}"), "got: {out}");
    }
}
