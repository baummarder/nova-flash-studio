//! `nova-edu` — Education Edition domain logic for NØVA Flash Studio.
//!
//! Headless, offline-first, UI-independent. All persistence is plain files
//! inside an edu *workspace* directory plus a small local config area in
//! the edition config dir. Roles are workflow convenience, not
//! authentication.

pub mod assignment;
pub mod classroom;
pub mod error;
pub mod feedback;
pub mod package;
pub mod profile;
pub mod progress;
pub mod report;
pub mod school;
pub mod store;
pub mod submission;
pub mod template;
pub mod workspace;

pub use error::{EduError, EduResult};
pub use store::EDU_SCHEMA_VERSION;
