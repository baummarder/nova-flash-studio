//! Core data model for NØVA Flash Studio.
//!
//! This crate is deliberately UI- and renderer-agnostic. Both editor modes
//! (code and animation) operate on the same [`Project`] value, which is why an
//! object created on the stage is immediately visible to the code mode and
//! vice versa.

pub mod collision;
pub mod edition;
pub mod error;
pub mod game;
pub mod gamekit;
pub mod games;
pub mod model;
pub mod particles;
pub mod template;
pub mod tilemap;
pub mod time;

pub use edition::{CONFIG_DIR_OVERRIDE, EDITION_ENV, Edition};
pub use error::{ProjectError, ProjectResult};
pub use model::*;
pub use template::{ProjectTemplate, template_by_id};
