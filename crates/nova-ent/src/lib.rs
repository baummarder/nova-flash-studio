//! `nova-ent` — Enterprise Edition domain layer for NØVA Flash Studio.
//!
//! Headless, offline-first, file-based. Mirrors the `nova-edu` architecture:
//! a workspace directory holds org metadata, team/role records, project
//! registrations (paths only — `.nova` files are never moved), a shared
//! asset library, project snapshots, review workflows, build profiles,
//! audit log and optional local integration configs.
//!
//! Security posture:
//! - Roles/permissions are **workflow policy**, not authentication. Real
//!   enforcement is the OS filesystem ACL; no pseudo-login exists.
//! - Packages are hardened ZIPs verified before any write.
//! - Integrations are disabled by default; secrets are env-var references.

pub mod admin;
pub mod assets;
pub mod builds;
pub mod error;
pub mod integrations;
pub mod package;
pub mod profile;
pub mod review;
pub mod store;
pub mod team;
pub mod versions;
pub mod workspace;

pub use error::{EntError, EntResult};
pub use workspace::{Organization, Workspace, WorkspacePrefs};
