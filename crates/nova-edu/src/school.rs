//! School-wide configuration and policy (`school.json`).
//!
//! Policy is configuration, not DRM: it can hide or disable UI features
//! (e.g. cloud AI) but cannot prevent a determined user from using the
//! binaries. That limitation is documented in EDU_GUIDE.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::EduResult;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SchoolConfig {
    pub schema: u32,
    pub school: String,
    /// Optional catalogue of teaching templates inside the workspace.
    pub template_catalog: Option<String>,
    pub policy: SchoolPolicy,
    /// Optional retention hint in days; only a *hint* — nothing is deleted
    /// automatically.
    pub retention_days: Option<u32>,
    pub grade_scale: Vec<GradeLevel>,
    /// Extra forward-compatible fields preserved on round-trip.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SchoolPolicy {
    /// When false, the AI panel hides cloud providers entirely.
    pub allow_cloud_ai: bool,
    /// When false, SWF export menu entries are hidden.
    pub allow_swf_export: bool,
    /// When false, custom starter projects cannot be attached to
    /// assignments (catalog starters still work).
    pub allow_custom_starter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradeLevel {
    pub id: String,
    pub label: String,
}

impl Default for SchoolConfig {
    fn default() -> Self {
        Self {
            schema: crate::store::EDU_SCHEMA_VERSION,
            school: String::new(),
            template_catalog: None,
            policy: SchoolPolicy::default(),
            retention_days: None,
            grade_scale: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for SchoolPolicy {
    fn default() -> Self {
        Self {
            allow_cloud_ai: false,
            allow_swf_export: true,
            allow_custom_starter: true,
        }
    }
}

impl SchoolConfig {
    pub fn load(workspace: &Workspace) -> EduResult<Self> {
        Ok(crate::store::read_or(
            &workspace.school_path(),
            SchoolConfig::default(),
        )?)
    }

    pub fn save(&self, workspace: &Workspace) -> EduResult<()> {
        crate::store::write_json(&workspace.school_path(), self)
    }

    pub fn validate(&self) -> EduResult<()> {
        if let Some(days) = self.retention_days {
            if days == 0 {
                return Err(crate::error::EduError::Invalid(
                    "retentionDays muss > 0 sein oder fehlen".into(),
                ));
            }
        }
        let mut ids = std::collections::HashSet::new();
        for level in &self.grade_scale {
            if level.id.trim().is_empty() || level.label.trim().is_empty() {
                return Err(crate::error::EduError::Invalid(
                    "Notenstufen brauchen id und label".into(),
                ));
            }
            if !ids.insert(level.id.clone()) {
                return Err(crate::error::EduError::Invalid(format!(
                    "Notenstufe '{}' doppelt",
                    level.id
                )));
            }
        }
        if let Some(cat) = &self.template_catalog {
            if cat.contains("..") || std::path::Path::new(cat).is_absolute() {
                return Err(crate::error::EduError::Invalid(
                    "templateCatalog muss ein relativer Pfad ohne .. sein".into(),
                ));
            }
        }
        Ok(())
    }

    /// Effective policy = school.json values (lowest precedence is built-in
    /// defaults already applied via `Default`; workspace overrides live in
    /// prefs, which the app merges — see docs/EDU_GUIDE).
    pub fn effective(&self) -> &SchoolPolicy {
        &self.policy
    }
}

/// Local (per-machine) edu preferences stored in the config dir.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EduPrefs {
    pub schema: u32,
    /// Whether the classroom welcome/onboarding was already shown.
    pub onboarding_done: bool,
    /// Last selected workspace path (display only).
    pub last_workspace: Option<String>,
    /// UI language preference: "de" or "en".
    pub language: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl EduPrefs {
    pub fn load(config_dir: &Path) -> EduResult<Self> {
        let mut prefs: Self = crate::store::read_or(
            &crate::workspace::local_config_dir(config_dir).join("prefs.json"),
            Self::default(),
        )?;
        if prefs.schema == 0 {
            prefs.schema = crate::store::EDU_SCHEMA_VERSION;
        }
        Ok(prefs)
    }

    pub fn save(&self, config_dir: &Path) -> EduResult<()> {
        crate::store::write_json(
            &crate::workspace::local_config_dir(config_dir).join("prefs.json"),
            self,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let policy = SchoolPolicy::default();
        assert!(!policy.allow_cloud_ai, "cloud AI must be off by default");
        assert!(policy.allow_swf_export);
    }

    #[test]
    fn validation_rejects_bad_catalog() {
        let mut cfg = SchoolConfig::default();
        cfg.template_catalog = Some("../escape".into());
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn unknown_fields_roundtrip() {
        let json = r#"{"schema":1,"school":"X","customField":{"a":1}}"#;
        let cfg: SchoolConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.extra.contains_key("customField"));
        let out = serde_json::to_string(&cfg).unwrap();
        assert!(out.contains("customField"));
    }
}
