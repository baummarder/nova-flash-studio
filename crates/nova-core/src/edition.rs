//! Product edition identification shared by all NØVA crates (M19).
//!
//! Three editions ship from one repository: **User**, **Edu** and
//! **Enterprise**. The edition is a *packaging and presentation* concern
//! only — it is deliberately NOT part of the `.nova` project format, so
//! projects stay fully interchangeable between editions.
//!
//! Resolution order for [`Edition::current`]:
//! 1. compile-time `NOVA_EDITION` (set by the packaging scripts),
//! 2. runtime `NOVA_EDITION` environment variable (development/testing),
//! 3. [`Edition::User`] (default).

use std::path::PathBuf;

/// Environment variable honoured at compile time (packaging) and runtime
/// (development overrides).
pub const EDITION_ENV: &str = "NOVA_EDITION";

/// Environment variable that overrides the whole configuration directory.
pub const CONFIG_DIR_OVERRIDE: &str = "NOVA_FLASH_STUDIO_CONFIG_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edition {
    /// Private users, hobby developers, animators and indie game developers.
    User,
    /// Schools, teachers, students and educational institutions.
    Edu,
    /// Companies, development teams and professional organizations.
    Enterprise,
}

impl Edition {
    pub const ALL: [Edition; 3] = [Edition::User, Edition::Edu, Edition::Enterprise];

    /// Stable lowercase identifier used in directory names and CLI output.
    pub fn id(self) -> &'static str {
        match self {
            Edition::User => "user",
            Edition::Edu => "edu",
            Edition::Enterprise => "enterprise",
        }
    }

    /// Product display name ("NØVA …").
    pub fn product_name(self) -> &'static str {
        match self {
            Edition::User => "NØVA Flash Studio",
            Edition::Edu => "NØVA Flash Studio Edu",
            Edition::Enterprise => "NØVA Flash Studio Enterprise",
        }
    }

    /// Artifact slug used by packaging scripts (`NOVA-User-…` etc.).
    pub fn artifact_slug(self) -> &'static str {
        match self {
            Edition::User => "User",
            Edition::Edu => "Edu",
            Edition::Enterprise => "Enterprise",
        }
    }

    /// Configuration directory name below the platform config root.
    ///
    /// The User edition keeps the legacy `NOVA Flash Studio` directory so
    /// existing settings, credentials and provider configuration are
    /// preserved without migration. Edu and Enterprise get their own
    /// sibling directories and can therefore be installed next to User
    /// without overwriting each other's data.
    pub fn config_dir_name(self) -> &'static str {
        match self {
            Edition::User => "NOVA Flash Studio",
            Edition::Edu => "NOVA Flash Studio Edu",
            Edition::Enterprise => "NOVA Flash Studio Enterprise",
        }
    }

    /// Temporary recovery/autosave directory name (untitled projects).
    /// Every edition gets its own directory so autosaves never collide.
    pub fn recovery_dir_name(self) -> String {
        format!("nova-flash-studio-{}", self.id())
    }

    /// Parse `user`, `edu` or `enterprise` (case-insensitive).
    pub fn parse(value: &str) -> Option<Edition> {
        match value.trim().to_ascii_lowercase().as_str() {
            "user" => Some(Edition::User),
            "edu" => Some(Edition::Edu),
            "enterprise" => Some(Edition::Enterprise),
            _ => None,
        }
    }

    /// Resolve the active edition (see module docs for precedence).
    pub fn current() -> Edition {
        if let Some(edition) = option_env!("NOVA_EDITION").and_then(Edition::parse) {
            return edition;
        }
        if let Ok(value) = std::env::var(EDITION_ENV)
            && let Some(edition) = Edition::parse(&value)
        {
            return edition;
        }
        Edition::User
    }

    /// Platform configuration directory for this edition.
    ///
    /// Honours [`CONFIG_DIR_OVERRIDE`] first (used by tests and portable
    /// setups). Returns `None` when the platform has no usable base
    /// directory; callers map that to their own error type.
    pub fn config_dir(&self) -> Option<PathBuf> {
        if let Some(path) = std::env::var_os(CONFIG_DIR_OVERRIDE) {
            return Some(PathBuf::from(path));
        }
        #[cfg(target_os = "windows")]
        {
            let base = std::env::var_os("APPDATA")?;
            return Some(PathBuf::from(base).join(self.config_dir_name()));
        }
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME")?;
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join(self.config_dir_name()),
            );
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                })?;
            return Some(base.join(format!("nova-flash-studio-{}", self.id())));
        }
        #[allow(unreachable_code)]
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_editions_have_unique_identity() {
        let mut names = std::collections::HashSet::new();
        for edition in Edition::ALL {
            assert!(names.insert(edition.id().to_string()));
            assert!(names.insert(edition.product_name().to_string()));
            assert!(names.insert(edition.config_dir_name().to_string()));
            assert!(names.insert(edition.recovery_dir_name()));
        }
    }

    #[test]
    fn parse_accepts_known_ids_only() {
        assert_eq!(Edition::parse("user"), Some(Edition::User));
        assert_eq!(Edition::parse("EDU"), Some(Edition::Edu));
        assert_eq!(Edition::parse(" enterprise "), Some(Edition::Enterprise));
        assert_eq!(Edition::parse("pro"), None);
        assert_eq!(Edition::parse(""), None);
    }

    #[test]
    fn user_keeps_legacy_config_dir() {
        // Backwards compatibility: existing installs must not lose settings.
        assert_eq!(Edition::User.config_dir_name(), "NOVA Flash Studio");
        assert_ne!(
            Edition::Edu.config_dir_name(),
            Edition::User.config_dir_name()
        );
        assert_ne!(
            Edition::Enterprise.config_dir_name(),
            Edition::User.config_dir_name()
        );
    }

    #[test]
    fn config_dir_honours_override() {
        // The override constant is process-wide; this test is the only one in
        // nova-core touching it, and the value is restored immediately.
        let previous = std::env::var_os(CONFIG_DIR_OVERRIDE);
        // SAFETY: single-threaded test mutation of a test-only variable,
        // restored before the test returns.
        unsafe { std::env::set_var(CONFIG_DIR_OVERRIDE, "C:/tmp/nova-edition-test") };
        let dir = Edition::Enterprise.config_dir().unwrap();
        assert_eq!(dir, PathBuf::from("C:/tmp/nova-edition-test"));
        // SAFETY: same as above.
        unsafe {
            match previous {
                Some(value) => std::env::set_var(CONFIG_DIR_OVERRIDE, value),
                None => std::env::remove_var(CONFIG_DIR_OVERRIDE),
            }
        }
    }
}
