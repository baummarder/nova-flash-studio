//! Core data model for NØVA Flash Studio.
//!
//! This crate is deliberately UI- and renderer-agnostic. Both editor modes
//! (code and animation) operate on the same [`Project`] value, which is why an
//! object created on the stage is immediately visible to the code mode and
//! vice versa.

pub mod error;
pub mod model;
pub mod time;

pub use error::{ProjectError, ProjectResult};
pub use model::*;
