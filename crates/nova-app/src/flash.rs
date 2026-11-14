//! External Flash runtime integration.
//!
//! NØVA never changes file associations on its own and never downloads a Flash
//! runtime. It launches the runtime the user selected explicitly:
//!
//! - Adobe Flash Player Projector (the default when configured and valid);
//! - Ruffle (only when the user explicitly selects it).
//!
//! There is no silent fallback. If the selected runtime is missing or invalid,
//! the launch fails with a configuration message instead of trying another
//! player. The SWF path is always passed as a separate process argument; no
//! shell command string is built from a file name.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const SETTINGS_FILE: &str = "flash-player.json";

#[derive(Debug, thiserror::Error)]
pub enum FlashError {
    #[error("Die SWF-Datei existiert nicht: {0}")]
    MissingSwf(PathBuf),
    #[error("Der Pfad ist keine SWF-Datei: {0}")]
    NotSwf(PathBuf),
    #[error("Die ausgewählte Runtime existiert nicht: {0}")]
    MissingProjector(PathBuf),
    #[error("Die ausgewählte Runtime ist keine gültige ausführbare Datei: {0}")]
    InvalidProjector(PathBuf),
    #[error(
        "Kein Adobe Flash Player Projector konfiguriert. Öffne Flash Player → Runtime-Einstellungen … und wähle den Projector aus – oder wechsle dort ausdrücklich zu Ruffle."
    )]
    NoAdobeProjector,
    #[error(
        "Ruffle ist als Runtime ausgewählt, aber kein Ruffle-Pfad konfiguriert. Öffne Flash Player → Runtime-Einstellungen …"
    )]
    NoRuffleConfigured,
    #[error("Die Runtime konnte nicht gestartet werden: {0}")]
    Spawn(String),
    #[error("Einstellungen konnten nicht gelesen oder geschrieben werden: {0}")]
    Config(String),
    #[error("Dateisystemfehler: {0}")]
    Io(String),
}

impl FlashError {
    /// True for errors that concern the SWF target rather than the runtime.
    /// The UI uses this to avoid pointing at Runtime settings when the movie
    /// itself is missing or invalid.
    pub fn is_swf_problem(&self) -> bool {
        matches!(self, Self::MissingSwf(_) | Self::NotSwf(_))
    }
}

/// The runtime used for "Open in Flash Player".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    /// Adobe Flash Player Projector. This is the default runtime.
    #[default]
    AdobeProjector,
    /// Ruffle Desktop. Only used after an explicit user selection.
    Ruffle,
}

impl RuntimeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::AdobeProjector => "Adobe Flash Player Projector",
            Self::Ruffle => "Ruffle",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::AdobeProjector => "Adobe Projector",
            Self::Ruffle => "Ruffle",
        }
    }
}

/// The persisted result of a structural runtime validation. This is not a
/// playback result: a successful record only means the file looks like a
/// native executable. Actual playback requires a separate runtime test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationRecord {
    pub runtime: RuntimeKind,
    pub path: PathBuf,
    pub ok: bool,
    pub message: String,
    pub checked_at_unix: u64,
}

impl ValidationRecord {
    pub fn success(runtime: RuntimeKind, path: &Path, info: &ProjectorInfo) -> Self {
        Self {
            runtime,
            path: path.to_path_buf(),
            ok: true,
            message: format!(
                "{} erkannt: {} ({}). Strukturprüfung bestanden; Prozessstart und Wiedergabe nicht geprüft.",
                runtime.label(),
                info.kind.label(),
                human_size(info.size)
            ),
            checked_at_unix: now_unix(),
        }
    }

    pub fn failure(runtime: RuntimeKind, path: &Path, message: impl Into<String>) -> Self {
        Self {
            runtime,
            path: path.to_path_buf(),
            ok: false,
            message: message.into(),
            checked_at_unix: now_unix(),
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{} · {}",
            if self.ok { "OK" } else { "Fehler" },
            self.message
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlashSettings {
    /// Adobe is the default runtime. It is never silently replaced by Ruffle.
    pub default_runtime: RuntimeKind,
    /// Adobe Flash Player Projector executable (or macOS `.app` bundle).
    pub adobe_projector_path: Option<PathBuf>,
    /// Optional Ruffle Desktop executable (or macOS `.app` bundle).
    pub ruffle_path: Option<PathBuf>,
    /// True once the user configured or deliberately postponed first-launch setup.
    pub first_launch_setup_done: bool,
    /// Last structural validation, persisted for the Runtime settings section.
    pub last_validation: Option<ValidationRecord>,
}

impl Default for FlashSettings {
    fn default() -> Self {
        Self {
            default_runtime: RuntimeKind::AdobeProjector,
            adobe_projector_path: None,
            ruffle_path: None,
            first_launch_setup_done: false,
            last_validation: None,
        }
    }
}

impl FlashSettings {
    pub fn selected_path(&self) -> Option<&Path> {
        match self.default_runtime {
            RuntimeKind::AdobeProjector => self.adobe_projector_path.as_deref(),
            RuntimeKind::Ruffle => self.ruffle_path.as_deref(),
        }
    }

    pub fn has_valid_adobe(&self) -> bool {
        self.adobe_projector_path
            .as_deref()
            .is_some_and(|path| validate_projector(path).is_ok())
    }

    /// Saving any configured runtime is a completed setup action. This keeps
    /// the first-launch dialog from reappearing after a user explicitly saves a
    /// Ruffle-only configuration. An empty configuration is left untouched so
    /// that a deliberate reset still re-enables first-launch setup.
    pub fn mark_setup_done_if_configured(&mut self) {
        if self.adobe_projector_path.is_some() || self.ruffle_path.is_some() {
            self.first_launch_setup_done = true;
        }
    }
}

/// Validate every non-empty runtime path before a settings save. Returns the
/// structural validation record for the selected runtime when it is valid.
/// Any invalid non-empty path is returned as an error, so a broken path never
/// replaces a previously valid configuration.
pub fn validate_settings_paths(
    settings: &FlashSettings,
) -> Result<Option<ValidationRecord>, ValidationRecord> {
    let mut selected = None;
    for (runtime, path) in [
        (
            RuntimeKind::AdobeProjector,
            settings.adobe_projector_path.as_deref(),
        ),
        (RuntimeKind::Ruffle, settings.ruffle_path.as_deref()),
    ] {
        let Some(path) = path else {
            continue;
        };
        match validate_projector(path) {
            Ok(info) => {
                if runtime == settings.default_runtime {
                    selected = Some(ValidationRecord::success(runtime, path, &info));
                }
            }
            Err(error) => {
                return Err(ValidationRecord::failure(runtime, path, error.to_string()));
            }
        }
    }
    Ok(selected)
}

/// A first-launch setup dialog is required until the user either configures a
/// valid Adobe Projector or deliberately postpones setup. A valid configured
/// Adobe path suppresses the dialog even if the flag was never written.
pub fn needs_first_launch_setup(settings: &FlashSettings) -> bool {
    !settings.first_launch_setup_done && !settings.has_valid_adobe()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Platform-specific variants are intentionally kept across targets.
pub enum ProjectorKind {
    WindowsExe,
    MacBundle,
    MacExecutable,
    LinuxExecutable,
}

impl ProjectorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::WindowsExe => "Windows-EXE",
            Self::MacBundle => "macOS-App-Bundle",
            Self::MacExecutable => "macOS-Executable",
            Self::LinuxExecutable => "Linux-Executable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectorInfo {
    pub path: PathBuf,
    pub kind: ProjectorKind,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStatus {
    pub ready: bool,
    pub runtime: RuntimeKind,
    pub mode: String,
    pub detail: String,
    pub executable: Option<PathBuf>,
}

impl PlayerStatus {
    pub fn summary(&self) -> String {
        format!("{} · {}", self.mode, self.detail)
    }
}

/// Result of starting a runtime process. This is deliberately not called a
/// playback result: NØVA can only confirm that the OS created the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchReport {
    pub runtime: RuntimeKind,
    pub executable: PathBuf,
    pub swf: PathBuf,
    pub process_id: u32,
}

impl LaunchReport {
    pub fn message(&self) -> String {
        format!(
            "{} gestartet (PID {}). Die tatsächliche SWF-Wiedergabe wurde nicht verifiziert.",
            self.runtime.label(),
            self.process_id
        )
    }
}

pub fn load_settings() -> Result<FlashSettings, FlashError> {
    load_settings_from(&settings_path()?)
}

pub fn save_settings(settings: &FlashSettings) -> Result<(), FlashError> {
    save_settings_to(&settings_path()?, settings)
}

pub fn settings_path() -> Result<PathBuf, FlashError> {
    Ok(config_dir()?.join(SETTINGS_FILE))
}

/// The on-disk shape accepts both the current runtime fields and the legacy
/// `use_system_default` / `projector_path` fields from v0.3.0-rc1.
#[derive(Debug, Default, Deserialize)]
struct StoredSettings {
    #[serde(default)]
    default_runtime: Option<RuntimeKind>,
    #[serde(default)]
    adobe_projector_path: Option<PathBuf>,
    #[serde(default)]
    ruffle_path: Option<PathBuf>,
    #[serde(default)]
    first_launch_setup_done: Option<bool>,
    #[serde(default)]
    last_validation: Option<ValidationRecord>,
    #[serde(default)]
    use_system_default: Option<bool>,
    #[serde(default)]
    projector_path: Option<PathBuf>,
}

impl StoredSettings {
    fn has_current_shape(&self) -> bool {
        self.default_runtime.is_some()
            || self.adobe_projector_path.is_some()
            || self.ruffle_path.is_some()
            || self.first_launch_setup_done.is_some()
            || self.last_validation.is_some()
    }

    fn into_settings(self) -> FlashSettings {
        if self.has_current_shape() {
            return FlashSettings {
                default_runtime: self.default_runtime.unwrap_or_default(),
                adobe_projector_path: self.adobe_projector_path,
                ruffle_path: self.ruffle_path,
                first_launch_setup_done: self.first_launch_setup_done.unwrap_or(false),
                last_validation: self.last_validation,
            };
        }

        let mut settings = FlashSettings::default();
        let Some(path) = self.projector_path else {
            return settings;
        };

        let explicitly_manual = self.use_system_default == Some(false);
        let ruffle_like = path_is_ruffle(&path);
        if ruffle_like {
            settings.ruffle_path = Some(path.clone());
            if explicitly_manual {
                settings.default_runtime = RuntimeKind::Ruffle;
            }
        } else {
            settings.adobe_projector_path = Some(path);
        }
        settings.first_launch_setup_done = explicitly_manual;
        settings
    }
}

pub fn load_settings_from(path: &Path) -> Result<FlashSettings, FlashError> {
    if !path.exists() {
        return Ok(FlashSettings::default());
    }
    let bytes = fs::read(path).map_err(|error| FlashError::Config(error.to_string()))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let stored: StoredSettings = serde_json::from_slice(bytes)
        .map_err(|error| FlashError::Config(format!("{}: {error}", path.display())))?;
    Ok(stored.into_settings())
}

pub fn save_settings_to(path: &Path, settings: &FlashSettings) -> Result<(), FlashError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| FlashError::Io(error.to_string()))?;
    }
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| FlashError::Config(error.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(|error| FlashError::Io(error.to_string()))?;
    // std::fs::rename cannot replace an existing file on Windows. Remove the
    // old file first so saving twice works; the temporary file still prevents
    // a truncated settings file if serialization or writing fails.
    if path.exists() {
        fs::remove_file(path).map_err(|error| FlashError::Io(error.to_string()))?;
    }
    fs::rename(&temporary, path).map_err(|error| FlashError::Io(error.to_string()))?;
    Ok(())
}

pub fn validate_swf(path: &Path) -> Result<(), FlashError> {
    let metadata = fs::metadata(path).map_err(|_| FlashError::MissingSwf(path.to_path_buf()))?;
    if !metadata.is_file() {
        return Err(FlashError::MissingSwf(path.to_path_buf()));
    }
    let is_swf = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("swf"));
    if !is_swf {
        return Err(FlashError::NotSwf(path.to_path_buf()));
    }
    // Read access is part of "reachable"; the three-byte SWF signature also
    // catches renamed or truncated files before handing them to a player.
    let mut header = [0u8; 3];
    let read =
        fs::File::open(path).and_then(|mut file| std::io::Read::read_exact(&mut file, &mut header));
    if read.is_err() || !(header == *b"FWS" || header == *b"CWS" || header == *b"ZWS") {
        return Err(FlashError::NotSwf(path.to_path_buf()));
    }
    Ok(())
}

/// Validate a runtime executable without launching its GUI. This checks the
/// file type and, on Unix, the executable bit. It is intentionally a structural
/// test; process launch is exercised by "Open in Flash Player".
pub fn validate_projector(path: &Path) -> Result<ProjectorInfo, FlashError> {
    let metadata =
        fs::metadata(path).map_err(|_| FlashError::MissingProjector(path.to_path_buf()))?;

    #[cfg(target_os = "macos")]
    {
        if metadata.is_dir() && path.extension().is_some_and(|ext| ext == "app") {
            let info = path.join("Contents").join("Info.plist");
            if info.is_file() {
                return Ok(ProjectorInfo {
                    path: path.to_path_buf(),
                    kind: ProjectorKind::MacBundle,
                    size: 0,
                });
            }
            return Err(FlashError::InvalidProjector(path.to_path_buf()));
        }
    }

    if !metadata.is_file() {
        return Err(FlashError::InvalidProjector(path.to_path_buf()));
    }

    let mut header = [0u8; 4];
    let read =
        fs::File::open(path).and_then(|mut file| std::io::Read::read_exact(&mut file, &mut header));
    if read.is_err() {
        return Err(FlashError::InvalidProjector(path.to_path_buf()));
    }

    let kind = detect_kind(path, &header)
        .ok_or_else(|| FlashError::InvalidProjector(path.to_path_buf()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(FlashError::InvalidProjector(path.to_path_buf()));
        }
    }

    Ok(ProjectorInfo {
        path: path.to_path_buf(),
        kind,
        size: metadata.len(),
    })
}

fn detect_kind(path: &Path, header: &[u8; 4]) -> Option<ProjectorKind> {
    #[cfg(target_os = "windows")]
    {
        if header.starts_with(b"MZ")
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Some(ProjectorKind::WindowsExe);
        }
    }

    #[cfg(target_os = "macos")]
    {
        const MACHO: [[u8; 4]; 6] = [
            [0xfe, 0xed, 0xfa, 0xce],
            [0xfe, 0xed, 0xfa, 0xcf],
            [0xce, 0xfa, 0xed, 0xfe],
            [0xcf, 0xfa, 0xed, 0xfe],
            [0xca, 0xfe, 0xba, 0xbe],
            [0xbe, 0xba, 0xfe, 0xca],
        ];
        if MACHO.contains(header) {
            return Some(ProjectorKind::MacExecutable);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if header == b"\x7fELF" {
            return Some(ProjectorKind::LinuxExecutable);
        }
    }

    // Keep the function total for targets whose native check is above.
    let _ = path;
    None
}

pub fn status(settings: &FlashSettings) -> PlayerStatus {
    let runtime = settings.default_runtime;
    let path = settings.selected_path();
    match path {
        None => PlayerStatus {
            ready: false,
            runtime,
            mode: runtime.label().to_string(),
            detail: if runtime == RuntimeKind::AdobeProjector {
                "Nicht konfiguriert. Adobe ist die Standard-Runtime; Ruffle wird nur nach ausdrücklicher Auswahl verwendet.".to_string()
            } else {
                "Ruffle ist ausgewählt, aber kein Ruffle-Pfad konfiguriert.".to_string()
            },
            executable: None,
        },
        Some(path) => match validate_projector(path) {
            Ok(info) => PlayerStatus {
                ready: true,
                runtime,
                mode: runtime.label().to_string(),
                detail: format!("{} erkannt.", info.kind.label()),
                executable: Some(path.to_path_buf()),
            },
            Err(error) => PlayerStatus {
                ready: false,
                runtime,
                mode: runtime.label().to_string(),
                detail: error.to_string(),
                executable: Some(path.to_path_buf()),
            },
        },
    }
}

/// Structural validation used by the "Prüfen" button. It never launches the
/// player and never claims playback success.
pub fn test_runtime(path: &Path, runtime: RuntimeKind) -> Result<ValidationRecord, FlashError> {
    let info = validate_projector(path)?;
    Ok(ValidationRecord::success(runtime, path, &info))
}

fn human_size(bytes: u64) -> String {
    if bytes == 0 {
        "App-Bundle".to_string()
    } else if bytes < 1024 * 1024 {
        format!("{:.0} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Open an exported SWF with the selected runtime. The SWF path is always
/// passed as one argument. A successful return only confirms process creation;
/// it does not confirm that the SWF rendered or executed.
pub fn open_swf(swf: &Path, settings: &FlashSettings) -> Result<LaunchReport, FlashError> {
    let (mut command, runtime, executable) = prepare_launch(swf, settings)?;
    let child = command
        .spawn()
        .map_err(|error| FlashError::Spawn(error.to_string()))?;
    Ok(LaunchReport {
        runtime,
        executable,
        swf: swf.to_path_buf(),
        process_id: child.id(),
    })
}

/// Build the process command for the selected runtime. This is separated from
/// `open_swf` so tests can verify the exact executable and argument without
/// starting a GUI process.
fn prepare_launch(
    swf: &Path,
    settings: &FlashSettings,
) -> Result<(Command, RuntimeKind, PathBuf), FlashError> {
    validate_swf(swf)?;
    let runtime = settings.default_runtime;
    let executable = match runtime {
        RuntimeKind::AdobeProjector => settings
            .adobe_projector_path
            .as_deref()
            .ok_or(FlashError::NoAdobeProjector)?,
        RuntimeKind::Ruffle => settings
            .ruffle_path
            .as_deref()
            .ok_or(FlashError::NoRuffleConfigured)?,
    };
    validate_projector(executable)?;
    let mut command = runtime_command(executable, runtime);
    command.arg(swf);
    Ok((command, runtime, executable.to_path_buf()))
}

fn runtime_command(executable: &Path, runtime: RuntimeKind) -> Command {
    #[cfg(target_os = "macos")]
    {
        if executable.extension().is_some_and(|ext| ext == "app") {
            let mut command = Command::new("open");
            command.arg("-a").arg(executable);
            return command;
        }
    }
    let _ = runtime;
    Command::new(executable)
}

fn config_dir() -> Result<PathBuf, FlashError> {
    // Edition-aware (M19): User keeps the legacy directory, Edu and
    // Enterprise get isolated sibling directories.
    nova_core::Edition::current()
        .config_dir()
        .ok_or_else(|| FlashError::Config("Kein Konfigurationsverzeichnis gefunden.".to_string()))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn path_is_ruffle(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().contains("ruffle"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("nova-flash-{tag}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    fn fake_projector(path: &Path) {
        #[cfg(target_os = "windows")]
        {
            let mut bytes = vec![0u8; 128];
            bytes[0] = b'M';
            bytes[1] = b'Z';
            fs::write(path, bytes).expect("write fake exe");
        }
        #[cfg(target_os = "linux")]
        {
            fs::write(path, b"\x7fELF\x02\x01\x01\0").expect("write fake elf");
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("chmod");
        }
        #[cfg(target_os = "macos")]
        {
            fs::write(path, [0xfe, 0xed, 0xfa, 0xce, 0, 0, 0, 0]).expect("write fake mach-o");
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("chmod");
        }
    }

    fn fake_swf(path: &Path) {
        fs::write(path, b"FWS\x0a\x00\x00\x00").expect("write swf");
    }

    #[test]
    fn default_settings_select_adobe_and_need_setup() {
        let settings = FlashSettings::default();
        assert_eq!(settings.default_runtime, RuntimeKind::AdobeProjector);
        assert_eq!(settings.adobe_projector_path, None);
        assert_eq!(settings.ruffle_path, None);
        assert!(needs_first_launch_setup(&settings));
        assert!(!status(&settings).ready);
    }

    #[test]
    fn settings_roundtrip_with_unicode_and_spaces() {
        let dir = temp_dir("settings");
        let path = dir.join("settings with space.json");
        let settings = FlashSettings {
            default_runtime: RuntimeKind::AdobeProjector,
            adobe_projector_path: Some(dir.join("Flash Player Projector Ø.exe")),
            ruffle_path: Some(dir.join("Ruffle Ø.exe")),
            first_launch_setup_done: true,
            last_validation: Some(ValidationRecord::failure(
                RuntimeKind::AdobeProjector,
                &dir.join("Flash Player Projector Ø.exe"),
                "nicht gefunden",
            )),
        };
        save_settings_to(&path, &settings).expect("save");
        let loaded = load_settings_from(&path).expect("load");
        assert_eq!(loaded, settings);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn saving_twice_replaces_the_previous_settings_file() {
        let dir = temp_dir("settings-overwrite");
        let path = dir.join("flash-player.json");
        let first = FlashSettings::default();
        save_settings_to(&path, &first).expect("first save");
        let second = FlashSettings {
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        save_settings_to(&path, &second).expect("second save");
        assert_eq!(load_settings_from(&path).expect("load"), second);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn first_launch_setup_is_skipped_when_adobe_is_valid() {
        let dir = temp_dir("first-launch-valid");
        let projector = dir.join("Flash Player Projector.exe");
        fake_projector(&projector);
        let settings = FlashSettings {
            adobe_projector_path: Some(projector),
            ..FlashSettings::default()
        };
        assert!(!needs_first_launch_setup(&settings));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn first_launch_setup_is_not_repeated_after_skipping() {
        let settings = FlashSettings {
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        assert!(!needs_first_launch_setup(&settings));
    }

    #[test]
    fn missing_swf_is_rejected() {
        let dir = temp_dir("missing-swf");
        let error =
            open_swf(&dir.join("missing.swf"), &FlashSettings::default()).expect_err("must reject");
        assert!(matches!(error, FlashError::MissingSwf(_)));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn wrong_extension_is_rejected() {
        let dir = temp_dir("wrong-ext");
        let path = dir.join("movie.txt");
        fs::write(&path, b"not a swf").expect("write");
        let error = validate_swf(&path).expect_err("must reject");
        assert!(matches!(error, FlashError::NotSwf(_)));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn projector_validation_accepts_native_binary() {
        let dir = temp_dir("projector");
        #[cfg(target_os = "windows")]
        let path = dir.join("Flash Player Projector.exe");
        #[cfg(not(target_os = "windows"))]
        let path = dir.join("flash-player");
        fake_projector(&path);
        let info = validate_projector(&path).expect("valid projector");
        assert!(info.size > 0);
        let record = test_runtime(&path, RuntimeKind::AdobeProjector).expect("record");
        assert!(record.ok);
        assert!(record.message.contains("nicht geprüft"));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn spaces_umlauts_and_unicode_in_swf_and_runtime_paths_are_safe_arguments() {
        let dir = temp_dir("spaces-unicode");
        let swf = dir.join("Mein Film äöü Ø 日本語 🎬 mit Leerzeichen.swf");
        fake_swf(&swf);
        validate_swf(&swf).expect("swf path");
        let projector = dir.join("Flash Player Projektor äöü Ø 日本語 🎬.exe");
        fake_projector(&projector);
        let settings = FlashSettings {
            adobe_projector_path: Some(projector.clone()),
            ..FlashSettings::default()
        };
        let (command, runtime, executable) =
            prepare_launch(&swf, &settings).expect("prepare launch with Unicode paths");
        assert_eq!(runtime, RuntimeKind::AdobeProjector);
        assert_eq!(executable, projector);
        assert_eq!(command.get_program(), projector.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![swf.as_os_str()]
        );
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn invalid_swf_magic_is_rejected() {
        let dir = temp_dir("invalid-swf");
        let path = dir.join("renamed.swf");
        fs::write(&path, b"not a flash movie").expect("write");
        let error = validate_swf(&path).expect_err("must reject invalid magic");
        assert!(matches!(error, FlashError::NotSwf(_)));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn exported_swf_is_ready_for_the_player() {
        let dir = temp_dir("export-open");
        let path = dir.join("exported film.swf");
        let output = nova_build::build_swf(&nova_core::Project::demo()).expect("build demo SWF");
        fs::write(&path, &output.bytes).expect("write SWF");
        validate_swf(&path).expect("exported SWF must be a valid player target");
        assert!(fs::metadata(&path).expect("metadata").len() > 0);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn opening_an_exported_swf_prepares_selected_runtime_and_one_argument() {
        let dir = temp_dir("prepare-launch");
        let swf = dir.join("Mein Film Ø.swf");
        fake_swf(&swf);
        let adobe = dir.join("Flash Player Projector.exe");
        let ruffle = dir.join("Ruffle.exe");
        fake_projector(&adobe);
        fake_projector(&ruffle);

        let mut settings = FlashSettings {
            default_runtime: RuntimeKind::AdobeProjector,
            adobe_projector_path: Some(adobe.clone()),
            ruffle_path: Some(ruffle.clone()),
            first_launch_setup_done: true,
            last_validation: None,
        };
        let (command, runtime, executable) =
            prepare_launch(&swf, &settings).expect("prepare Adobe launch");
        assert_eq!(runtime, RuntimeKind::AdobeProjector);
        assert_eq!(executable, adobe);
        assert_eq!(command.get_program(), adobe.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![swf.as_os_str()]
        );

        settings.default_runtime = RuntimeKind::Ruffle;
        let (command, runtime, executable) =
            prepare_launch(&swf, &settings).expect("prepare Ruffle launch");
        assert_eq!(runtime, RuntimeKind::Ruffle);
        assert_eq!(executable, ruffle);
        assert_eq!(command.get_program(), ruffle.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![swf.as_os_str()]
        );
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_runtime_is_a_clear_error() {
        let dir = temp_dir("missing-projector");
        let settings = FlashSettings {
            default_runtime: RuntimeKind::AdobeProjector,
            adobe_projector_path: Some(dir.join("missing.exe")),
            ..FlashSettings::default()
        };
        let status = status(&settings);
        assert!(!status.ready);
        assert!(status.detail.contains("existiert nicht"));
        assert!(matches!(
            validate_projector(&dir.join("missing.exe")),
            Err(FlashError::MissingProjector(_))
        ));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn adobe_is_default_and_ruffle_is_not_used_implicitly() {
        let dir = temp_dir("no-silent-ruffle");
        let ruffle = dir.join("ruffle.exe");
        fake_projector(&ruffle);
        let settings = FlashSettings {
            default_runtime: RuntimeKind::AdobeProjector,
            ruffle_path: Some(ruffle),
            ..FlashSettings::default()
        };
        let status = status(&settings);
        assert!(!status.ready);
        assert_eq!(status.runtime, RuntimeKind::AdobeProjector);

        let swf = dir.join("movie.swf");
        fake_swf(&swf);
        let error = open_swf(&swf, &settings).expect_err("must not fall back to Ruffle");
        assert!(matches!(error, FlashError::NoAdobeProjector));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn explicit_ruffle_selection_uses_only_the_ruffle_path() {
        let dir = temp_dir("explicit-ruffle");
        let ruffle = dir.join("Ruffle.exe");
        fake_projector(&ruffle);
        let settings = FlashSettings {
            default_runtime: RuntimeKind::Ruffle,
            ruffle_path: Some(ruffle.clone()),
            ..FlashSettings::default()
        };
        let status = status(&settings);
        assert!(status.ready);
        assert_eq!(status.runtime, RuntimeKind::Ruffle);
        assert_eq!(status.executable, Some(ruffle.clone()));
        let command = runtime_command(&ruffle, RuntimeKind::Ruffle);
        assert_eq!(command.get_program(), ruffle.as_os_str());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn switching_back_to_adobe_preserves_both_paths() {
        let dir = temp_dir("switch-back");
        let adobe = dir.join("flashplayer.exe");
        let ruffle = dir.join("ruffle.exe");
        fake_projector(&adobe);
        fake_projector(&ruffle);
        let mut settings = FlashSettings {
            default_runtime: RuntimeKind::Ruffle,
            adobe_projector_path: Some(adobe.clone()),
            ruffle_path: Some(ruffle),
            first_launch_setup_done: true,
            last_validation: None,
        };
        assert_eq!(status(&settings).runtime, RuntimeKind::Ruffle);
        settings.default_runtime = RuntimeKind::AdobeProjector;
        let status = status(&settings);
        assert!(status.ready);
        assert_eq!(status.runtime, RuntimeKind::AdobeProjector);
        assert_eq!(status.executable, Some(adobe));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn legacy_system_default_migrates_to_adobe_setup() {
        let dir = temp_dir("legacy-system");
        let path = dir.join("flash-player.json");
        fs::write(
            &path,
            br#"{"use_system_default": true, "projector_path": null}"#,
        )
        .expect("write legacy settings");
        let settings = load_settings_from(&path).expect("load");
        assert_eq!(settings.default_runtime, RuntimeKind::AdobeProjector);
        assert!(!settings.first_launch_setup_done);
        assert!(needs_first_launch_setup(&settings));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn legacy_adobe_projector_migrates_without_losing_the_path() {
        let dir = temp_dir("legacy-adobe");
        let path = dir.join("flash-player.json");
        let projector = dir.join("Flash Player Projector.exe");
        fs::write(
            &path,
            format!(
                "{{\"use_system_default\": false, \"projector_path\": {}}}",
                serde_json::to_string(&projector).expect("path json")
            ),
        )
        .expect("write legacy settings");
        let settings = load_settings_from(&path).expect("load");
        assert_eq!(settings.default_runtime, RuntimeKind::AdobeProjector);
        assert_eq!(settings.adobe_projector_path, Some(projector));
        assert!(settings.first_launch_setup_done);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn legacy_ruffle_projector_migrates_as_explicit_ruffle() {
        let dir = temp_dir("legacy-ruffle");
        let path = dir.join("flash-player.json");
        let ruffle = dir.join("ruffle.exe");
        fs::write(
            &path,
            format!(
                "{{\"use_system_default\": false, \"projector_path\": {}}}",
                serde_json::to_string(&ruffle).expect("path json")
            ),
        )
        .expect("write legacy settings");
        let settings = load_settings_from(&path).expect("load");
        assert_eq!(settings.default_runtime, RuntimeKind::Ruffle);
        assert_eq!(settings.ruffle_path, Some(ruffle));
        assert!(settings.first_launch_setup_done);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn settings_file_with_utf8_bom_is_accepted() {
        let dir = temp_dir("settings-bom");
        let path = dir.join("flash-player.json");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            br#"{"default_runtime":"adobe_projector","adobe_projector_path":null,"ruffle_path":null,"first_launch_setup_done":true,"last_validation":null}"#,
        );
        fs::write(&path, bytes).expect("write bom settings");
        let settings = load_settings_from(&path).expect("load bom settings");
        assert!(settings.first_launch_setup_done);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn invalid_settings_json_is_reported() {
        let dir = temp_dir("invalid-settings");
        let path = dir.join("flash-player.json");
        fs::write(&path, b"{not json").expect("write");
        let error = load_settings_from(&path).expect_err("must reject");
        assert!(matches!(error, FlashError::Config(_)));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn launch_report_does_not_claim_playback() {
        let report = LaunchReport {
            runtime: RuntimeKind::AdobeProjector,
            executable: PathBuf::from("flashplayer.exe"),
            swf: PathBuf::from("movie.swf"),
            process_id: 42,
        };
        assert!(report.message().contains("nicht verifiziert"));
        assert!(!report.message().contains("erfolgreich"));
    }

    #[test]
    fn configured_runtime_marks_first_launch_setup_done() {
        let mut settings = FlashSettings {
            ruffle_path: Some(PathBuf::from("Ruffle.exe")),
            ..FlashSettings::default()
        };
        settings.mark_setup_done_if_configured();
        assert!(settings.first_launch_setup_done);
        assert!(!needs_first_launch_setup(&settings));
    }

    #[test]
    fn empty_or_postponed_settings_keep_first_launch_state() {
        let mut empty = FlashSettings::default();
        empty.mark_setup_done_if_configured();
        assert!(!empty.first_launch_setup_done);
        assert!(needs_first_launch_setup(&empty));

        let mut postponed = FlashSettings {
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        postponed.mark_setup_done_if_configured();
        assert!(postponed.first_launch_setup_done);
        assert!(!needs_first_launch_setup(&postponed));
    }

    #[test]
    fn invalid_adobe_path_requires_first_launch_setup() {
        let dir = temp_dir("first-launch-invalid");
        let projector = dir.join("not a projector.exe");
        fs::write(&projector, b"not an executable").expect("write invalid runtime");
        let settings = FlashSettings {
            adobe_projector_path: Some(projector),
            ..FlashSettings::default()
        };
        assert!(needs_first_launch_setup(&settings));
        assert!(!status(&settings).ready);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn postponed_setup_survives_settings_roundtrip() {
        let dir = temp_dir("postponed-roundtrip");
        let path = dir.join("flash-player.json");
        let settings = FlashSettings {
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        save_settings_to(&path, &settings).expect("save postponed settings");
        let loaded = load_settings_from(&path).expect("load postponed settings");
        assert!(loaded.first_launch_setup_done);
        assert!(!needs_first_launch_setup(&loaded));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn validate_settings_paths_rejects_invalid_non_selected_path() {
        let dir = temp_dir("validate-non-selected");
        let invalid_adobe = dir.join("Adobe.exe");
        fs::write(&invalid_adobe, b"not an executable").expect("write invalid Adobe path");
        let ruffle = dir.join("Ruffle.exe");
        fake_projector(&ruffle);
        let settings = FlashSettings {
            default_runtime: RuntimeKind::Ruffle,
            adobe_projector_path: Some(invalid_adobe.clone()),
            ruffle_path: Some(ruffle),
            ..FlashSettings::default()
        };
        let record = validate_settings_paths(&settings).expect_err("invalid path must block save");
        assert_eq!(record.runtime, RuntimeKind::AdobeProjector);
        assert_eq!(record.path, invalid_adobe);
        assert!(!record.ok);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn validate_settings_paths_returns_the_selected_runtime_record() {
        let dir = temp_dir("validate-selected");
        let adobe = dir.join("Adobe.exe");
        let ruffle = dir.join("Ruffle.exe");
        fake_projector(&adobe);
        fake_projector(&ruffle);
        let mut settings = FlashSettings {
            default_runtime: RuntimeKind::AdobeProjector,
            adobe_projector_path: Some(adobe),
            ruffle_path: Some(ruffle),
            ..FlashSettings::default()
        };
        let record = validate_settings_paths(&settings)
            .expect("valid paths")
            .expect("selected record");
        assert_eq!(record.runtime, RuntimeKind::AdobeProjector);
        assert!(record.ok);

        settings.default_runtime = RuntimeKind::Ruffle;
        let record = validate_settings_paths(&settings)
            .expect("valid paths")
            .expect("selected record");
        assert_eq!(record.runtime, RuntimeKind::Ruffle);
        assert!(record.ok);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_selected_runtime_is_reported_by_open() {
        let dir = temp_dir("open-missing-runtime");
        let swf = dir.join("movie.swf");
        fake_swf(&swf);
        let settings = FlashSettings {
            adobe_projector_path: Some(dir.join("moved projector.exe")),
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        let error = open_swf(&swf, &settings).expect_err("missing runtime must fail");
        assert!(matches!(error, FlashError::MissingProjector(_)));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn invalid_selected_runtime_is_reported_by_open() {
        let dir = temp_dir("open-invalid-runtime");
        let swf = dir.join("movie.swf");
        fake_swf(&swf);
        let projector = dir.join("invalid projector.exe");
        fs::write(&projector, b"not an executable").expect("write invalid runtime");
        let settings = FlashSettings {
            adobe_projector_path: Some(projector),
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        let error = open_swf(&swf, &settings).expect_err("invalid runtime must fail");
        assert!(matches!(error, FlashError::InvalidProjector(_)));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_ruffle_configuration_is_a_clear_error() {
        let dir = temp_dir("open-missing-ruffle");
        let swf = dir.join("movie.swf");
        fake_swf(&swf);
        let settings = FlashSettings {
            default_runtime: RuntimeKind::Ruffle,
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        let status = status(&settings);
        assert!(!status.ready);
        assert!(status.detail.contains("Ruffle"));
        let error = open_swf(&swf, &settings).expect_err("missing Ruffle path must fail");
        assert!(matches!(error, FlashError::NoRuffleConfigured));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn explicit_ruffle_does_not_require_an_adobe_path() {
        let dir = temp_dir("ruffle-without-adobe");
        let swf = dir.join("movie.swf");
        fake_swf(&swf);
        let ruffle = dir.join("Ruffle.exe");
        fake_projector(&ruffle);
        let settings = FlashSettings {
            default_runtime: RuntimeKind::Ruffle,
            ruffle_path: Some(ruffle.clone()),
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        let (command, runtime, executable) =
            prepare_launch(&swf, &settings).expect("Ruffle must launch without Adobe");
        assert_eq!(runtime, RuntimeKind::Ruffle);
        assert_eq!(executable, ruffle);
        assert_eq!(command.get_program(), ruffle.as_os_str());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn explicit_adobe_does_not_require_a_ruffle_path() {
        let dir = temp_dir("adobe-without-ruffle");
        let swf = dir.join("movie.swf");
        fake_swf(&swf);
        let adobe = dir.join("Adobe.exe");
        fake_projector(&adobe);
        let settings = FlashSettings {
            adobe_projector_path: Some(adobe.clone()),
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        let (command, runtime, executable) =
            prepare_launch(&swf, &settings).expect("Adobe must launch without Ruffle");
        assert_eq!(runtime, RuntimeKind::AdobeProjector);
        assert_eq!(executable, adobe);
        assert_eq!(command.get_program(), adobe.as_os_str());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn swf_problems_are_classified_separately_from_runtime_problems() {
        assert!(FlashError::MissingSwf(PathBuf::from("missing.swf")).is_swf_problem());
        assert!(FlashError::NotSwf(PathBuf::from("movie.txt")).is_swf_problem());
        assert!(!FlashError::MissingProjector(PathBuf::from("missing.exe")).is_swf_problem());
        assert!(!FlashError::InvalidProjector(PathBuf::from("invalid.exe")).is_swf_problem());
        assert!(!FlashError::NoAdobeProjector.is_swf_problem());
        assert!(!FlashError::NoRuffleConfigured.is_swf_problem());
        assert!(!FlashError::Spawn("spawn failed".to_string()).is_swf_problem());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn invalid_pe_fails_at_process_launch() {
        let dir = temp_dir("spawn-failure");
        let swf = dir.join("movie.swf");
        fake_swf(&swf);
        let projector = dir.join("Ruffle.exe");
        fake_projector(&projector);
        let settings = FlashSettings {
            default_runtime: RuntimeKind::Ruffle,
            ruffle_path: Some(projector),
            first_launch_setup_done: true,
            ..FlashSettings::default()
        };
        let error = open_swf(&swf, &settings).expect_err("a fake MZ file must not launch");
        assert!(
            matches!(error, FlashError::Spawn(_)),
            "unexpected error: {error:?}"
        );
        fs::remove_dir_all(dir).ok();
    }
}
