//! Optional local AI/integration endpoints (M21.8).
//!
//! Everything is **disabled by default** and purely declarative: the
//! config stores endpoint URLs and model names; API keys are referenced
//! by environment-variable *name* only — never stored in plaintext.
//! No code path here performs network I/O; actual calls go through the
//! existing `nova-ai` provider layer when the user explicitly enables an
//! integration and configures credentials there.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{EntError, EntResult};
use crate::store::{ENT_SCHEMA_VERSION, now_unix, read_or, write_json};
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationKind {
    /// Local Ollama-compatible HTTP endpoint (e.g. http://localhost:11434).
    Ollama,
    /// Generic OpenAI-compatible local endpoint.
    OpenAiCompatible,
    /// Future team endpoint — reserved, must stay disabled.
    TeamEndpoint,
}

impl IntegrationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama (lokal)",
            Self::OpenAiCompatible => "OpenAI-kompatibel (lokal)",
            Self::TeamEndpoint => "Team-Endpunkt (reserviert)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Integration {
    pub schema: u32,
    pub id: Uuid,
    pub name: String,
    pub kind: IntegrationKind,
    /// Base URL, e.g. http://localhost:11434. http(s) only; no credentials
    /// in the URL.
    pub endpoint: String,
    pub model: String,
    /// Name of an environment variable holding the API key — the value
    /// itself is never persisted.
    pub api_key_env: Option<String>,
    /// Master switch. Default false — integrations are opt-in.
    pub enabled: bool,
    /// Project-scoped AI policy overrides (project_id → allow flag).
    pub project_policy: std::collections::BTreeMap<Uuid, bool>,
    pub created_unix: u64,
    pub updated_unix: u64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for Integration {
    fn default() -> Self {
        let now = now_unix();
        Self {
            schema: ENT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: String::new(),
            kind: IntegrationKind::Ollama,
            endpoint: String::new(),
            model: String::new(),
            api_key_env: None,
            enabled: false,
            project_policy: std::collections::BTreeMap::new(),
            created_unix: now,
            updated_unix: now,
            extra: serde_json::Map::new(),
        }
    }
}

impl Integration {
    pub fn validate(&self) -> EntResult<()> {
        if self.name.trim().is_empty() {
            return Err(EntError::Invalid(
                "Integrationsname darf nicht leer sein".into(),
            ));
        }
        if self.enabled {
            // Only validate endpoint shape when actually enabled.
            if !self.endpoint.starts_with("http://") && !self.endpoint.starts_with("https://") {
                return Err(EntError::Invalid(
                    "Endpoint muss mit http:// oder https:// beginnen".into(),
                ));
            }
            if self.endpoint.contains('@') {
                return Err(EntError::Invalid(
                    "Zugangsdaten dürfen nicht Teil der Endpoint-URL sein".into(),
                ));
            }
        }
        if let Some(env) = &self.api_key_env
            && !env.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(EntError::Invalid(
                "API-Key-Umgebungsvariable darf nur [A-Za-z0-9_] enthalten".into(),
            ));
        }
        Ok(())
    }

    /// Whether this integration may be used for a given project.
    /// Disabled globally → false; project policy can only further restrict.
    pub fn allowed_for(&self, project_id: Option<Uuid>) -> bool {
        if !self.enabled {
            return false;
        }
        if let Some(pid) = project_id
            && let Some(&allowed) = self.project_policy.get(&pid)
        {
            return allowed;
        }
        true
    }
}

/// Whole integrations config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IntegrationConfig {
    pub schema: u32,
    pub integrations: Vec<Integration>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            schema: ENT_SCHEMA_VERSION,
            integrations: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

fn load_config(workspace: &Workspace) -> EntResult<IntegrationConfig> {
    let mut cfg: IntegrationConfig =
        read_or(&workspace.integrations_path(), IntegrationConfig::default())?;
    if cfg.schema == 0 {
        cfg.schema = ENT_SCHEMA_VERSION;
    }
    Ok(cfg)
}

fn save_config(workspace: &Workspace, cfg: &IntegrationConfig) -> EntResult<()> {
    write_json(&workspace.integrations_path(), cfg)
}

pub fn list_integrations(workspace: &Workspace) -> EntResult<Vec<Integration>> {
    Ok(load_config(workspace)?.integrations)
}

pub fn get_integration(workspace: &Workspace, id: Uuid) -> EntResult<Integration> {
    load_config(workspace)?
        .integrations
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| EntError::NotFound("Integration nicht gefunden".into()))
}

pub fn save_integration(
    workspace: &Workspace,
    integration: &Integration,
) -> EntResult<Integration> {
    let mut cfg = load_config(workspace)?;
    integration.validate()?;
    let mut i = integration.clone();
    i.schema = ENT_SCHEMA_VERSION;
    i.updated_unix = now_unix();
    if let Some(existing) = cfg.integrations.iter_mut().find(|x| x.id == i.id) {
        *existing = i.clone();
    } else {
        cfg.integrations.push(i.clone());
    }
    save_config(workspace, &cfg)?;
    Ok(i)
}

pub fn delete_integration(workspace: &Workspace, id: Uuid) -> EntResult<()> {
    let mut cfg = load_config(workspace)?;
    cfg.integrations.retain(|i| i.id != id);
    save_config(workspace, &cfg)
}

/// Resolves the API key from the referenced environment variable.
/// Returns None when unset — callers must treat that as "not configured".
pub fn resolve_api_key(integration: &Integration) -> Option<String> {
    let env = integration.api_key_env.as_ref()?;
    std::env::var(env).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Organization;

    fn ws(tag: &str) -> (Workspace, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova-ent-int-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = Workspace::create(
            &dir,
            &Organization {
                name: "Org".into(),
                ..Default::default()
            },
        )
        .unwrap();
        (ws, dir)
    }

    #[test]
    fn disabled_by_default_and_validation() {
        let (ws, dir) = ws("int");
        let i = save_integration(
            &ws,
            &Integration {
                name: "Ollama".into(),
                endpoint: "http://localhost:11434".into(),
                model: "qwen2.5".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!i.enabled);
        assert!(!i.allowed_for(None));

        // Enable without valid endpoint fails.
        let bad = Integration {
            id: i.id,
            enabled: true,
            endpoint: "ftp://x".into(),
            ..i.clone()
        };
        assert!(save_integration(&ws, &bad).is_err());

        // Credentials in URL rejected.
        let bad2 = Integration {
            enabled: true,
            endpoint: "http://user:pass@localhost:11434".into(),
            ..i.clone()
        };
        assert!(bad2.validate().is_err());

        // Valid enabled config persists; project policy can restrict.
        let good = Integration {
            enabled: true,
            project_policy: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(Uuid::new_v4(), false);
                m
            },
            ..i.clone()
        };
        let saved = save_integration(&ws, &good).unwrap();
        assert!(saved.allowed_for(None));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
