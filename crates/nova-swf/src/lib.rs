//! Minimal SWF writer used by NØVA Flash Studio.
//!
//! The crate writes SWF files byte for byte against the published SWF file
//! format specification. It is not a full authoring library: the implemented
//! surface is shapes (`DefineShape3`), sprites, frame-based placement, frame
//! labels and AVM2 tag containers (`DoABC`, `DoABC2`, `SymbolClass`).
//!
//! Compatibility limits are documented in `docs/FEASIBILITY.md` and enforced
//! by tests that cross-check emitted files with the independent `swf` crate.

pub mod bits;
pub mod error;
pub mod inspect;
pub mod movie;
pub mod shape;
pub mod tags;
pub mod types;

pub use error::{SwfError, SwfResult};
pub use inspect::{SwfInfo, TagInfo, inspect, inspect_file};
pub use movie::{Compression, Movie, ensure_file_attributes};
pub use shape::{Command, LineStyle, Point, Shape, ShapePath};
pub use tags::Tag;
pub use types::{Color, ColorTransform, Matrix, PlaceObject, Rect};
