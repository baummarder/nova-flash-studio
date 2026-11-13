//! Extensible provider configuration, model discovery, connection tests and chat.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::credentials::{CredentialStore, default_store};
use crate::error::{AiError, AiResult};
use crate::http::{HttpRequest, HttpResponse, HttpTransport, ReqwestTransport};

pub const SETTINGS_FILE: &str = "ai-providers.json";
pub const CONFIG_OVERRIDE: &str = "NOVA_FLASH_STUDIO_CONFIG_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OllamaLocal,
    OllamaCloud,
    Groq,
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::OllamaLocal => "Ollama (lokal)",
            Self::OllamaCloud => "Ollama Cloud",
            Self::Groq => "Groq",
            Self::OpenAiCompatible => "OpenAI-kompatibel",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::OllamaLocal => "http://localhost:11434",
            Self::OllamaCloud => "https://ollama.com",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::OpenAiCompatible => "http://localhost:8000/v1",
        }
    }

    pub fn default_name(self) -> &'static str {
        self.label()
    }

    pub fn needs_credential(self) -> bool {
        matches!(self, Self::OllamaCloud | Self::Groq)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

fn default_enabled() -> bool {
    true
}

impl ProviderConfig {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            id: new_provider_id(kind),
            name: kind.default_name().to_string(),
            kind,
            base_url: kind.default_base_url().to_string(),
            model: String::new(),
            enabled: true,
            temperature: None,
        }
    }

    pub fn is_cloud(&self) -> bool {
        !endpoint_is_local(&self.base_url)
    }

    pub fn needs_credential(&self) -> bool {
        self.kind.needs_credential()
            || (self.is_cloud() && self.kind == ProviderKind::OpenAiCompatible)
    }

    pub fn validate(&self) -> AiResult<()> {
        if self.name.trim().is_empty() {
            return Err(AiError::Config("Anbietername ist leer.".into()));
        }
        if self.base_url.trim().is_empty() {
            return Err(AiError::Config("Basis-URL ist leer.".into()));
        }
        if !(self.base_url.starts_with("http://") || self.base_url.starts_with("https://")) {
            return Err(AiError::Config(
                "Basis-URL muss mit http:// oder https:// beginnen.".into(),
            ));
        }
        if self.kind == ProviderKind::OllamaLocal && self.is_cloud() {
            return Err(AiError::Config(
                "Ollama (lokal) muss auf localhost oder eine lokale Adresse zeigen. \
                 Für entfernte Endpunkte wähle Ollama Cloud oder OpenAI-kompatibel."
                    .into(),
            ));
        }
        Ok(())
    }
}

fn new_provider_id(kind: ProviderKind) -> String {
    let prefix = match kind {
        ProviderKind::OllamaLocal => "ollama-local",
        ProviderKind::OllamaCloud => "ollama-cloud",
        ProviderKind::Groq => "groq",
        ProviderKind::OpenAiCompatible => "openai-compatible",
    };
    format!("{prefix}-{}", uuid::Uuid::new_v4().simple())
}

fn endpoint_is_local(url: &str) -> bool {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host_port = rest.split('/').next().unwrap_or(rest);
    let host = if host_port.starts_with('[') {
        host_port
            .split(']')
            .next()
            .unwrap_or(host_port)
            .trim_start_matches('[')
            .to_string()
    } else {
        host_port.split(':').next().unwrap_or(host_port).to_string()
    };
    let host = host.to_ascii_lowercase();
    host == "localhost"
        || host == "::1"
        || host == "0.0.0.0"
        || host.starts_with("127.")
        || (!host.contains('.') && !host.is_empty())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            version: 1,
            default_provider: None,
            providers: Vec::new(),
        }
    }
}

impl ProviderSettings {
    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|provider| provider.id == id)
    }

    pub fn provider_mut(&mut self, id: &str) -> Option<&mut ProviderConfig> {
        self.providers.iter_mut().find(|provider| provider.id == id)
    }

    pub fn selected(&self, override_id: Option<&str>) -> Option<&ProviderConfig> {
        let id = override_id
            .filter(|id| !id.is_empty())
            .or(self.default_provider.as_deref())?;
        self.provider(id)
    }

    pub fn ensure_default(&mut self) {
        let valid = self
            .default_provider
            .as_deref()
            .and_then(|id| self.provider(id))
            .is_some_and(|provider| provider.enabled);
        if valid {
            return;
        }
        self.default_provider = self
            .providers
            .iter()
            .find(|provider| provider.enabled)
            .map(|provider| provider.id.clone());
    }
}

pub struct ProviderManager {
    pub settings: ProviderSettings,
    credentials: Box<dyn CredentialStore>,
    transport: Arc<dyn HttpTransport>,
    config_dir: PathBuf,
}

impl ProviderManager {
    pub fn load(config_dir: impl Into<PathBuf>) -> AiResult<Self> {
        let config_dir = config_dir.into();
        let settings = load_settings(&config_dir)?;
        let transport = Arc::new(ReqwestTransport::new()?);
        Ok(Self {
            settings,
            credentials: default_store(&config_dir),
            transport,
            config_dir,
        })
    }

    pub fn with_parts(
        settings: ProviderSettings,
        credentials: Box<dyn CredentialStore>,
        transport: Arc<dyn HttpTransport>,
        config_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            settings,
            credentials,
            transport,
            config_dir: config_dir.into(),
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn transport(&self) -> &dyn HttpTransport {
        self.transport.as_ref()
    }

    pub fn credential_backend(&self) -> &'static str {
        self.credentials.backend_name()
    }

    pub fn save(&self) -> AiResult<()> {
        save_settings(&self.config_dir, &self.settings)
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.settings.provider(id)
    }

    pub fn provider_mut(&mut self, id: &str) -> Option<&mut ProviderConfig> {
        self.settings.provider_mut(id)
    }

    pub fn selected(&self, override_id: Option<&str>) -> Option<&ProviderConfig> {
        self.settings.selected(override_id)
    }

    pub fn enabled(&self) -> Vec<&ProviderConfig> {
        self.settings
            .providers
            .iter()
            .filter(|provider| provider.enabled)
            .collect()
    }

    pub fn add(&mut self, provider: ProviderConfig) -> AiResult<String> {
        provider.validate()?;
        if self.settings.provider(&provider.id).is_some() {
            return Err(AiError::Config(format!(
                "Anbieter-ID '{}' existiert bereits.",
                provider.id
            )));
        }
        let id = provider.id.clone();
        self.settings.providers.push(provider);
        self.settings.ensure_default();
        Ok(id)
    }

    pub fn update(&mut self, provider: ProviderConfig) -> AiResult<()> {
        provider.validate()?;
        let Some(existing) = self.settings.provider_mut(&provider.id) else {
            return Err(AiError::ProviderUnavailable(provider.id));
        };
        *existing = provider;
        self.settings.ensure_default();
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> AiResult<()> {
        self.settings.providers.retain(|provider| provider.id != id);
        if self.settings.default_provider.as_deref() == Some(id) {
            self.settings.default_provider = None;
        }
        self.settings.ensure_default();
        self.credentials.delete(id)
    }

    pub fn set_default(&mut self, id: &str) -> AiResult<()> {
        if self.settings.provider(id).is_none() {
            return Err(AiError::ProviderUnavailable(id.to_string()));
        }
        self.settings.default_provider = Some(id.to_string());
        Ok(())
    }

    pub fn has_credential(&self, id: &str) -> AiResult<bool> {
        self.credentials.has(id)
    }

    pub fn set_credential(&self, id: &str, secret: &str) -> AiResult<()> {
        if secret.trim().is_empty() {
            self.credentials.delete(id)
        } else {
            self.credentials.set(id, secret)
        }
    }

    pub fn delete_credential(&self, id: &str) -> AiResult<()> {
        self.credentials.delete(id)
    }

    pub fn credential(&self, id: &str) -> AiResult<Option<String>> {
        self.credentials.get(id)
    }

    pub fn discover_models(&self, id: &str) -> AiResult<Vec<String>> {
        let provider = self
            .settings
            .provider(id)
            .ok_or_else(|| AiError::ProviderUnavailable(id.to_string()))?;
        if !provider.enabled {
            return Err(AiError::ProviderUnavailable(id.to_string()));
        }
        let key = self.credentials.get(id)?;
        discover_models(provider, key.as_deref(), self.transport.as_ref())
    }

    pub fn test_connection(&self, id: &str) -> AiResult<ConnectionTest> {
        let provider = self
            .settings
            .provider(id)
            .ok_or_else(|| AiError::ProviderUnavailable(id.to_string()))?;
        if !provider.enabled {
            return Err(AiError::ProviderUnavailable(id.to_string()));
        }
        let key = self.credentials.get(id)?;
        test_connection(provider, key.as_deref(), self.transport.as_ref())
    }

    pub fn chat(&self, id: &str, messages: &[ChatMessage]) -> AiResult<ChatResponse> {
        let provider = self
            .settings
            .provider(id)
            .ok_or_else(|| AiError::ProviderUnavailable(id.to_string()))?;
        if !provider.enabled {
            return Err(AiError::ProviderUnavailable(id.to_string()));
        }
        let key = self.credentials.get(id)?;
        chat(provider, key.as_deref(), messages, self.transport.as_ref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTest {
    pub models: Vec<String>,
    pub message: String,
}

pub fn discover_models(
    provider: &ProviderConfig,
    key: Option<&str>,
    transport: &dyn HttpTransport,
) -> AiResult<Vec<String>> {
    provider.validate()?;
    let key = require_key(provider, key)?;
    let (url, headers) = match provider.kind {
        ProviderKind::OllamaLocal | ProviderKind::OllamaCloud => (
            join_endpoint(&provider.base_url, "/api/tags"),
            ollama_headers(key),
        ),
        ProviderKind::Groq | ProviderKind::OpenAiCompatible => (
            join_endpoint(&provider.base_url, "/models"),
            bearer_headers(key),
        ),
    };
    let mut request = HttpRequest::get(url).timeout(30);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let response = transport.execute(request)?;
    check_status(&response)?;
    let value = parse_json(&response.body)?;
    let models = match provider.kind {
        ProviderKind::OllamaLocal | ProviderKind::OllamaCloud => {
            let entries = value
                .get("models")
                .and_then(|models| models.as_array())
                .ok_or_else(|| {
                    AiError::Protocol("Ollama-Antwort enthält kein 'models'-Array.".into())
                })?;
            let mut models = Vec::new();
            for entry in entries {
                if let Some(name) = entry.get("name").and_then(|name| name.as_str()) {
                    models.push(name.to_string());
                } else if let Some(name) = entry.get("model").and_then(|name| name.as_str()) {
                    models.push(name.to_string());
                }
            }
            models
        }
        ProviderKind::Groq | ProviderKind::OpenAiCompatible => {
            let entries = value
                .get("data")
                .and_then(|data| data.as_array())
                .ok_or_else(|| {
                    AiError::Protocol("OpenAI-Antwort enthält kein 'data'-Array.".into())
                })?;
            entries
                .iter()
                .filter_map(|entry| entry.get("id").and_then(|id| id.as_str()))
                .map(ToString::to_string)
                .collect()
        }
    };
    Ok(models)
}

pub fn test_connection(
    provider: &ProviderConfig,
    key: Option<&str>,
    transport: &dyn HttpTransport,
) -> AiResult<ConnectionTest> {
    let models = discover_models(provider, key, transport)?;
    let message = if models.is_empty() {
        "Verbindung erfolgreich; der Anbieter meldet keine Modelle.".to_string()
    } else {
        format!(
            "Verbindung erfolgreich; {} Modell(e) gefunden.",
            models.len()
        )
    };
    Ok(ConnectionTest { models, message })
}

pub fn chat(
    provider: &ProviderConfig,
    key: Option<&str>,
    messages: &[ChatMessage],
    transport: &dyn HttpTransport,
) -> AiResult<ChatResponse> {
    provider.validate()?;
    if provider.model.trim().is_empty() {
        return Err(AiError::Config(format!(
            "Für '{}' ist kein Modell ausgewählt.",
            provider.name
        )));
    }
    let key = require_key(provider, key)?;
    let (url, body, headers) = match provider.kind {
        ProviderKind::OllamaLocal | ProviderKind::OllamaCloud => {
            let body = serde_json::json!({
                "model": provider.model,
                "messages": messages,
                "stream": false,
                "options": {
                    "temperature": provider.temperature.unwrap_or(0.2)
                }
            });
            (
                join_endpoint(&provider.base_url, "/api/chat"),
                body,
                ollama_headers(key),
            )
        }
        ProviderKind::Groq | ProviderKind::OpenAiCompatible => {
            let body = serde_json::json!({
                "model": provider.model,
                "messages": messages,
                "temperature": provider.temperature.unwrap_or(0.2),
                "stream": false
            });
            (
                join_endpoint(&provider.base_url, "/chat/completions"),
                body,
                bearer_headers(key),
            )
        }
    };
    let bytes = serde_json::to_vec(&body).map_err(|error| {
        AiError::Config(format!("Anfrage konnte nicht serialisiert werden: {error}"))
    })?;
    let mut request = HttpRequest::post_json(url, bytes).timeout(120);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let response = transport.execute(request)?;
    check_status(&response)?;
    let value = parse_json(&response.body)?;
    let content = match provider.kind {
        ProviderKind::OllamaLocal | ProviderKind::OllamaCloud => value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .or_else(|| value.get("response").and_then(|content| content.as_str()))
            .ok_or_else(|| AiError::Protocol("Ollama-Antwort enthält keinen Text.".into()))?,
        ProviderKind::Groq | ProviderKind::OpenAiCompatible => value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .ok_or_else(|| AiError::Protocol("OpenAI-Antwort enthält keinen Text.".into()))?,
    };
    Ok(ChatResponse {
        content: content.to_string(),
        model: provider.model.clone(),
    })
}

fn require_key<'a>(provider: &ProviderConfig, key: Option<&'a str>) -> AiResult<Option<&'a str>> {
    let key = key.filter(|key| !key.trim().is_empty());
    if provider.needs_credential() && key.is_none() {
        return Err(AiError::MissingCredential(provider.name.clone()));
    }
    Ok(key)
}

fn bearer_headers(key: Option<&str>) -> Vec<(String, String)> {
    key.map(|key| vec![("Authorization".to_string(), format!("Bearer {key}"))])
        .unwrap_or_default()
}

fn ollama_headers(key: Option<&str>) -> Vec<(String, String)> {
    bearer_headers(key)
}

fn join_endpoint(base: &str, suffix: &str) -> String {
    let base = base.trim_end_matches('/');
    if suffix == "/api/tags" && base.ends_with("/api") {
        return format!("{base}/tags");
    }
    if suffix == "/api/chat" && base.ends_with("/api") {
        return format!("{base}/chat");
    }
    format!("{base}{suffix}")
}

fn check_status(response: &HttpResponse) -> AiResult<()> {
    if (200..300).contains(&response.status) {
        return Ok(());
    }
    let message = error_message(&response.body);
    match response.status {
        401 | 403 => Err(AiError::Authentication {
            status: response.status,
        }),
        408 | 504 => Err(AiError::Timeout("Anbieter".into())),
        status => Err(AiError::Http { status, message }),
    }
}

fn error_message(body: &[u8]) -> String {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return "Der Anbieter hat die Anfrage abgelehnt.".into();
    };
    let message = value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(|message| message.as_str())
                .or_else(|| error.as_str())
        })
        .or_else(|| value.get("message").and_then(|message| message.as_str()))
        .unwrap_or("Der Anbieter hat die Anfrage abgelehnt.");
    sanitize_error(message)
}

fn sanitize_error(message: &str) -> String {
    let mut message: String = message.chars().take(320).collect();
    for marker in ["Bearer ", "sk-", "gsk_", "ollama_"] {
        while let Some(index) = message.find(marker) {
            let end = message[index..]
                .find(|character: char| {
                    character.is_whitespace() || character == '"' || character == '\''
                })
                .map(|offset| index + offset)
                .unwrap_or(message.len());
            message.replace_range(index..end, "[redacted]");
        }
    }
    message
}

fn parse_json(body: &[u8]) -> AiResult<serde_json::Value> {
    serde_json::from_slice(body)
        .map_err(|error| AiError::Protocol(format!("JSON konnte nicht gelesen werden: {error}")))
}

pub fn load_settings(config_dir: &Path) -> AiResult<ProviderSettings> {
    let path = config_dir.join(SETTINGS_FILE);
    if !path.exists() {
        return Ok(ProviderSettings::default());
    }
    let bytes = fs::read(&path).map_err(AiError::io)?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let settings: ProviderSettings = serde_json::from_slice(bytes)
        .map_err(|error| AiError::Config(format!("{}: {error}", path.display())))?;
    if settings.version != 1 {
        return Err(AiError::Config(format!(
            "Unbekannte AI-Konfigurationsversion {}.",
            settings.version
        )));
    }
    Ok(settings)
}

pub fn save_settings(config_dir: &Path, settings: &ProviderSettings) -> AiResult<()> {
    fs::create_dir_all(config_dir).map_err(AiError::io)?;
    let path = config_dir.join(SETTINGS_FILE);
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| {
        AiError::Config(format!(
            "Konfiguration konnte nicht serialisiert werden: {error}"
        ))
    })?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(AiError::io)?;
    if path.exists() {
        fs::remove_file(&path).map_err(AiError::io)?;
    }
    fs::rename(&temporary, &path).map_err(AiError::io)
}

pub fn default_config_dir() -> AiResult<PathBuf> {
    if let Some(path) = std::env::var_os(CONFIG_OVERRIDE) {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("APPDATA")
            .ok_or_else(|| AiError::Config("APPDATA ist nicht gesetzt.".into()))?;
        Ok(PathBuf::from(base).join("NOVA Flash Studio"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| AiError::Config("HOME ist nicht gesetzt.".into()))?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("NOVA Flash Studio"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok_or_else(|| AiError::Config("Kein Konfigurationsverzeichnis gefunden.".into()))?;
        Ok(base.join("nova-flash-studio"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::{CredentialStore, MemoryCredentialStore};
    use crate::http::MockTransport;

    fn config(kind: ProviderKind, base: &str) -> ProviderConfig {
        let mut provider = ProviderConfig::new(kind);
        provider.base_url = base.into();
        provider.model = "test-model".into();
        provider
    }

    fn manager(
        settings: ProviderSettings,
        responses: Vec<HttpResponse>,
        key: Option<&str>,
    ) -> (ProviderManager, Arc<MockTransport>) {
        let transport = Arc::new(MockTransport::new(responses));
        let credentials = Box::new(MemoryCredentialStore::new());
        if let Some(key) = key {
            if let Some(provider) = settings.providers.first() {
                credentials.set(&provider.id, key).unwrap();
            }
        }
        let manager = ProviderManager::with_parts(
            settings,
            credentials,
            transport.clone(),
            std::env::temp_dir().join("nova-ai-provider-test"),
        );
        (manager, transport)
    }

    #[test]
    fn provider_json_never_contains_the_api_key() {
        let mut settings = ProviderSettings::default();
        let mut provider = config(ProviderKind::Groq, "https://api.groq.com/openai/v1");
        provider.name = "Groq".into();
        settings.providers.push(provider.clone());
        settings.default_provider = Some(provider.id.clone());
        let text = serde_json::to_string(&settings).unwrap();
        assert!(!text.contains("secret"));
        assert!(!text.contains("api_key"));
        assert!(!text.contains("apiKey"));
    }

    #[test]
    fn discovers_ollama_models_and_sends_bearer_token_for_cloud() {
        let provider = config(ProviderKind::OllamaCloud, "https://ollama.com");
        let body = br#"{"models":[{"name":"llama3.2:latest"},{"model":"qwen2.5"}]}"#;
        let transport = MockTransport::new(vec![HttpResponse::json(200, body.to_vec())]);
        let models = discover_models(&provider, Some("secret-key"), &transport).unwrap();
        assert_eq!(models, vec!["llama3.2:latest", "qwen2.5"]);
        let request = &transport.requests()[0];
        assert_eq!(request.url, "https://ollama.com/api/tags");
        assert_eq!(
            request.header_value("authorization"),
            Some("Bearer secret-key")
        );
    }

    #[test]
    fn discovers_groq_models() {
        let provider = config(ProviderKind::Groq, "https://api.groq.com/openai/v1");
        let body = br#"{"data":[{"id":"llama-3.3-70b"},{"id":"mixtral"}]}"#;
        let transport = MockTransport::new(vec![HttpResponse::json(200, body.to_vec())]);
        let models = discover_models(&provider, Some("key"), &transport).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(
            transport.requests()[0].url,
            "https://api.groq.com/openai/v1/models"
        );
    }

    #[test]
    fn invalid_credentials_are_reported_without_echoing_the_key() {
        let provider = config(ProviderKind::Groq, "https://api.groq.com/openai/v1");
        let transport = MockTransport::new(vec![HttpResponse::json(
            401,
            br#"{"error":{"message":"bad key sk-secret"}}"#.to_vec(),
        )]);
        let error = discover_models(&provider, Some("sk-secret"), &transport).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("401"));
        assert!(!text.contains("sk-secret"));
    }

    #[test]
    fn network_failure_is_clear_for_unavailable_local_ollama() {
        let provider = config(ProviderKind::OllamaLocal, "http://localhost:11434");
        let transport = MockTransport::new(vec![]);
        let error = discover_models(&provider, None, &transport).unwrap_err();
        assert!(matches!(error, AiError::Network(_)));
    }

    #[test]
    fn ollama_chat_is_parsed() {
        let provider = config(ProviderKind::OllamaLocal, "http://localhost:11434");
        let body = br#"{"message":{"role":"assistant","content":"Hallo"}}"#;
        let transport = MockTransport::new(vec![HttpResponse::json(200, body.to_vec())]);
        let response = chat(&provider, None, &[ChatMessage::user("Hi")], &transport).unwrap();
        assert_eq!(response.content, "Hallo");
        assert_eq!(
            transport.requests()[0].url,
            "http://localhost:11434/api/chat"
        );
    }

    #[test]
    fn openai_chat_is_parsed_and_requires_key() {
        let provider = config(ProviderKind::OpenAiCompatible, "https://example.invalid/v1");
        let transport = MockTransport::new(vec![HttpResponse::json(
            200,
            br#"{"choices":[{"message":{"content":"OK"}}]}"#.to_vec(),
        )]);
        assert!(matches!(
            chat(&provider, None, &[ChatMessage::user("Hi")], &transport),
            Err(AiError::MissingCredential(_))
        ));
        let response = chat(
            &provider,
            Some("key"),
            &[ChatMessage::user("Hi")],
            &transport,
        )
        .unwrap();
        assert_eq!(response.content, "OK");
    }

    #[test]
    fn default_provider_and_override_are_explicit() {
        let mut settings = ProviderSettings::default();
        let local = config(ProviderKind::OllamaLocal, "http://localhost:11434");
        let cloud = config(ProviderKind::OllamaCloud, "https://ollama.com");
        let local_id = local.id.clone();
        let cloud_id = cloud.id.clone();
        settings.providers = vec![local, cloud];
        settings.default_provider = Some(local_id.clone());
        settings.ensure_default();
        assert_eq!(settings.selected(None).unwrap().id, local_id);
        assert_eq!(settings.selected(Some(&cloud_id)).unwrap().id, cloud_id);
        assert!(settings.selected(Some(&cloud_id)).unwrap().is_cloud());
        assert!(!settings.selected(Some(&local_id)).unwrap().is_cloud());
    }

    #[test]
    fn settings_roundtrip_is_atomic_and_does_not_include_secrets() {
        let dir =
            std::env::temp_dir().join(format!("nova-ai-settings-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut settings = ProviderSettings::default();
        let mut provider = config(ProviderKind::Groq, "https://api.groq.com/openai/v1");
        provider.name = "Mein Groq".into();
        settings.providers.push(provider);
        save_settings(&dir, &settings).unwrap();
        let loaded = load_settings(&dir).unwrap();
        assert_eq!(loaded.providers[0].name, "Mein Groq");
        let text = fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap();
        assert!(!text.contains("apiKey"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_endpoint_detection_is_conservative() {
        assert!(endpoint_is_local("http://localhost:11434"));
        assert!(endpoint_is_local("http://127.0.0.1:8000/v1"));
        assert!(endpoint_is_local("http://ollama:11434"));
        assert!(!endpoint_is_local("https://api.groq.com/openai/v1"));
        assert!(!endpoint_is_local("https://ollama.com"));
    }

    #[test]
    fn manager_switches_models_and_tracks_connection_status() {
        let mut settings = ProviderSettings::default();
        let mut provider = config(ProviderKind::OllamaLocal, "http://localhost:11434");
        provider.name = "Lokal".into();
        let id = provider.id.clone();
        settings.providers.push(provider);
        settings.default_provider = Some(id.clone());
        let (mut manager, _transport) = manager(settings, vec![], None);
        manager.provider_mut(&id).unwrap().model = "llama3".into();
        assert_eq!(manager.selected(None).unwrap().model, "llama3");
    }
}
