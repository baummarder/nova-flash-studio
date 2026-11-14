//! Edition profiles: branding, startup copy and the feature/module registry
//! for the three NØVA products (M19).
//!
//! Everything here is presentation and packaging metadata resolved from
//! [`nova_core::Edition::current`]. Feature *code* stays shared; the
//! registry exists so future edition-specific modules (classroom tools in
//! M20, administration/collaboration in M21) have a single, testable
//! registration point instead of scattered `if edition == …` checks.
//!
//! This is NOT a licensing system: there are no keys, activation, online
//! checks, seat limits or DRM. All editions work fully offline.

use nova_core::Edition;

/// A registered capability area. Modules listed here are available in the
/// running edition; entries marked `(geplant: Mxx)` are documented future
/// work and are NOT presented as implemented functionality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditionModule {
    pub id: &'static str,
    pub label: &'static str,
    /// `true` while the module is a reserved slot for a future milestone.
    pub reserved: bool,
}

/// Static presentation/registry data for one edition.
#[derive(Debug, Clone, Copy)]
pub struct EditionProfile {
    pub edition: Edition,
    /// Full product name shown in the title bar, splash and About dialog.
    pub app_name: &'static str,
    /// Short tagline shown on the splash screen.
    pub tagline: &'static str,
    /// One-line description of the edition's audience.
    pub audience: &'static str,
    /// egui viewport / window-manager identifier (unique per edition so
    /// installed editions can coexist without id collisions).
    pub bundle_id: &'static str,
    /// Registered modules. `reserved` entries describe planned work.
    pub modules: &'static [EditionModule],
}

const CORE_MODULES: &[EditionModule] = &[
    EditionModule {
        id: "vector",
        label: "Vektor-Editor",
        reserved: false,
    },
    EditionModule {
        id: "timeline",
        label: "Timeline & Keyframes",
        reserved: false,
    },
    EditionModule {
        id: "skeletal",
        label: "Skelett-Animation & SKINNING",
        reserved: false,
    },
    EditionModule {
        id: "game",
        label: "Game-Entwicklung",
        reserved: false,
    },
    EditionModule {
        id: "as3",
        label: "ActionScript-3-Subset",
        reserved: false,
    },
    EditionModule {
        id: "formats",
        label: "SWF/XFL/FLA-Kompatibilität",
        reserved: false,
    },
    EditionModule {
        id: "assets",
        label: "Asset-Verwaltung",
        reserved: false,
    },
    EditionModule {
        id: "ai",
        label: "KI-gestützte Bearbeitung",
        reserved: false,
    },
    EditionModule {
        id: "compat",
        label: "Projekt-Kompatibilität",
        reserved: false,
    },
];

const EDU_MODULES: &[EditionModule] = &[
    EditionModule {
        id: "vector",
        label: "Vektor-Editor",
        reserved: false,
    },
    EditionModule {
        id: "timeline",
        label: "Timeline & Keyframes",
        reserved: false,
    },
    EditionModule {
        id: "skeletal",
        label: "Skelett-Animation & SKINNING",
        reserved: false,
    },
    EditionModule {
        id: "game",
        label: "Game-Entwicklung",
        reserved: false,
    },
    EditionModule {
        id: "as3",
        label: "ActionScript-3-Subset",
        reserved: false,
    },
    EditionModule {
        id: "formats",
        label: "SWF/XFL/FLA-Kompatibilität",
        reserved: false,
    },
    EditionModule {
        id: "assets",
        label: "Asset-Verwaltung",
        reserved: false,
    },
    EditionModule {
        id: "ai",
        label: "KI-gestützte Bearbeitung",
        reserved: false,
    },
    EditionModule {
        id: "compat",
        label: "Projekt-Kompatibilität",
        reserved: false,
    },
    EditionModule {
        id: "classroom",
        label: "Klassenzimmer-Werkzeuge",
        reserved: true,
    },
];

const ENTERPRISE_MODULES: &[EditionModule] = &[
    EditionModule {
        id: "vector",
        label: "Vektor-Editor",
        reserved: false,
    },
    EditionModule {
        id: "timeline",
        label: "Timeline & Keyframes",
        reserved: false,
    },
    EditionModule {
        id: "skeletal",
        label: "Skelett-Animation & SKINNING",
        reserved: false,
    },
    EditionModule {
        id: "game",
        label: "Game-Entwicklung",
        reserved: false,
    },
    EditionModule {
        id: "as3",
        label: "ActionScript-3-Subset",
        reserved: false,
    },
    EditionModule {
        id: "formats",
        label: "SWF/XFL/FLA-Kompatibilität",
        reserved: false,
    },
    EditionModule {
        id: "assets",
        label: "Asset-Verwaltung",
        reserved: false,
    },
    EditionModule {
        id: "ai",
        label: "KI-gestützte Bearbeitung",
        reserved: false,
    },
    EditionModule {
        id: "compat",
        label: "Projekt-Kompatibilität",
        reserved: false,
    },
    EditionModule {
        id: "administration",
        label: "Administration & Zusammenarbeit",
        reserved: true,
    },
];

/// Profile for the resolved edition.
pub fn profile() -> EditionProfile {
    profile_for(Edition::current())
}

fn profile_for(edition: Edition) -> EditionProfile {
    match edition {
        Edition::User => EditionProfile {
            edition,
            app_name: "NØVA Flash Studio",
            tagline: "VECTOR · TIMELINE · SWF",
            audience: "Für Creator, Hobby-Entwickler und Animatoren",
            bundle_id: "studio.nova.flash.user",
            modules: CORE_MODULES,
        },
        Edition::Edu => EditionProfile {
            edition,
            app_name: "NØVA Flash Studio Edu",
            tagline: "LERNEN · ERSTELLEN · VERSTEHEN",
            audience: "Für Schulen, Lehrkräfte und Lernende",
            bundle_id: "studio.nova.flash.edu",
            modules: EDU_MODULES,
        },
        Edition::Enterprise => EditionProfile {
            edition,
            app_name: "NØVA Flash Studio Enterprise",
            tagline: "PRODUKTION · KOLLABORATION · KONTROLLE",
            audience: "Für Teams und professionelle Organisationen",
            bundle_id: "studio.nova.flash.enterprise",
            modules: ENTERPRISE_MODULES,
        },
    }
}

/// Active modules only (excludes reserved future slots).
pub fn active_modules() -> Vec<&'static EditionModule> {
    profile().modules.iter().filter(|m| !m.reserved).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_edition_profile_is_complete() {
        for edition in Edition::ALL {
            let profile = profile_for(edition);
            assert_eq!(profile.edition, edition);
            assert!(profile.app_name.contains("NØVA"));
            assert!(!profile.tagline.is_empty());
            assert!(!profile.audience.is_empty());
            assert!(profile.bundle_id.starts_with("studio.nova.flash"));
            assert!(profile.modules.len() >= CORE_MODULES.len());
            // Every edition shares the full core module set.
            for module in CORE_MODULES {
                assert!(
                    profile
                        .modules
                        .iter()
                        .any(|m| m.id == module.id && !m.reserved),
                    "edition {edition:?} missing core module {}",
                    module.id
                );
            }
        }
    }

    #[test]
    fn reserved_modules_are_marked_not_implemented() {
        // Edu and Enterprise expose exactly one reserved slot each; nothing
        // else may be reserved (reserved = documented future work only).
        for modules in [EDU_MODULES, ENTERPRISE_MODULES] {
            let reserved: Vec<_> = modules.iter().filter(|m| m.reserved).collect();
            assert_eq!(reserved.len(), 1);
        }
        assert!(CORE_MODULES.iter().all(|m| !m.reserved));
    }
}
