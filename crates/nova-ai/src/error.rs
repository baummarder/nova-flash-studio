//! Errors for the AI layer.
//!
//! Error text is deliberately free of credentials. API keys are only ever
//! placed in request headers and are never formatted into these variants.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("AI-Konfiguration: {0}")]
    Config(String),
    #[error("Kein KI-Anbieter konfiguriert. Öffne Einstellungen → KI-Anbieter.")]
    NoProvider,
    #[error("Der Anbieter '{0}' ist deaktiviert oder unbekannt.")]
    ProviderUnavailable(String),
    #[error("Der Anbieter '{0}' benötigt einen API-Schlüssel.")]
    MissingCredential(String),
    #[error(
        "Der Anbieter hat die Anmeldedaten abgelehnt (HTTP {status}). Prüfe den API-Schlüssel."
    )]
    Authentication { status: u16 },
    #[error("Netzwerkfehler: {0}")]
    Network(String),
    #[error("Zeitüberschreitung bei der Anfrage an {0}.")]
    Timeout(String),
    #[error("Der Anbieter antwortete mit HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("Ungültige Antwort des Anbieters: {0}")]
    Protocol(String),
    #[error("Cloud-Anfragen benötigen eine ausdrückliche Zustimmung im AI Agent Editor.")]
    CloudConsentRequired,
    #[error("Sicherer Anmeldedatenspeicher: {0}")]
    Credential(String),
    #[error("Dateisystemfehler: {0}")]
    Io(String),
    #[error("Unsicherer Projektpfad: {0}")]
    UnsafePath(String),
    #[error(
        "Die Datei '{0}' existiert bereits. Vorhandene Dateien werden niemals still überschrieben."
    )]
    WouldOverwrite(PathBuf),
    #[error("Die Datei '{0}' existiert nicht.")]
    MissingFile(PathBuf),
    #[error("Nicht unterstützt: {0}")]
    Unsupported(String),
    #[error("Build: {0}")]
    Build(String),
}

impl AiError {
    pub fn io(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

pub type AiResult<T> = Result<T, AiError>;
