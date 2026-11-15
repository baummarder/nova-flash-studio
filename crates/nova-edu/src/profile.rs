//! Local role/profile — workflow convenience, NOT authentication.
//!
//! There are no accounts, passwords or verification. The profile only
//! records who the local user claims to be so submissions and feedback
//! carry a readable author name.

use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::EduResult;
use crate::store::EDU_SCHEMA_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EduRole {
    Teacher,
    Student,
    Assistant,
}

impl EduRole {
    pub fn label(self) -> &'static str {
        match self {
            EduRole::Teacher => "Lehrkraft",
            EduRole::Student => "Lernende/r",
            EduRole::Assistant => "Assistenz",
        }
    }

    pub fn label_en(self) -> &'static str {
        match self {
            EduRole::Teacher => "Teacher",
            EduRole::Student => "Student",
            EduRole::Assistant => "Assistant",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalProfile {
    pub schema: u32,
    pub id: Uuid,
    /// Display name used in submissions/feedback authorship.
    pub display_name: String,
    /// Optional pseudonym shown instead of the real name.
    pub pseudonym: Option<String>,
    pub role: EduRole,
    /// Class/group the student belongs to (student role only).
    pub member_id: Option<Uuid>,
}

impl Default for LocalProfile {
    fn default() -> Self {
        Self {
            schema: EDU_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            display_name: String::new(),
            pseudonym: None,
            role: EduRole::Teacher,
            member_id: None,
        }
    }
}

impl LocalProfile {
    pub fn author_name(&self) -> &str {
        self.pseudonym
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .unwrap_or(&self.display_name)
    }

    pub fn profile_path(config_dir: &Path) -> std::path::PathBuf {
        crate::workspace::local_config_dir(config_dir).join("profile.json")
    }

    pub fn load(config_dir: &Path) -> EduResult<Self> {
        let mut profile: Self =
            crate::store::read_or(&Self::profile_path(config_dir), Self::default())?;
        if profile.schema == 0 {
            profile.schema = EDU_SCHEMA_VERSION;
        }
        Ok(profile)
    }

    pub fn save(&self, config_dir: &Path) -> EduResult<()> {
        crate::store::write_json(&Self::profile_path(config_dir), self)
    }

    pub fn is_configured(&self) -> bool {
        !self.display_name.trim().is_empty()
    }

    pub fn validate(&self) -> EduResult<()> {
        if self.display_name.trim().is_empty() && self.pseudonym.is_none() {
            return Err(crate::error::EduError::Invalid(
                "Anzeigename oder Pseudonym erforderlich".into(),
            ));
        }
        if self.display_name.chars().count() > 80 {
            return Err(crate::error::EduError::Invalid(
                "Anzeigename zu lang (max. 80 Zeichen)".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn author_prefers_pseudonym() {
        let p = LocalProfile {
            display_name: "Anna Beispiel".into(),
            pseudonym: Some("A.B.".into()),
            ..Default::default()
        };
        assert_eq!(p.author_name(), "A.B.");
    }

    #[test]
    fn role_labels() {
        assert_eq!(EduRole::Teacher.label(), "Lehrkraft");
        assert_eq!(EduRole::Student.label_en(), "Student");
    }
}
