//! Shared format layer for NØVA Flash Studio.
//!
//! The crate keeps the native `.nova` model as the editable representation and
//! provides real import paths for SWF and FLA/XFL, plus an explicitly
//! experimental XFL compatibility export. Unsupported source data is preserved
//! as original bytes and described in a [`CompatibilityReport`]; it is never
//! silently discarded.

pub mod analysis;
pub mod capabilities;
pub mod conversion;
pub mod error;
pub mod formats;

pub use analysis::{
    AnalysisResult, ProjectAnalysis, ScriptAnalysis, analyze_path, analyze_project,
};
pub use capabilities::{
    Capability, CompatibilityIssue, CompatibilityReport, FormatCapabilities, FormatKind,
    IssueCategory, IssueSeverity, SupportLevel, capabilities,
};
pub use conversion::{
    ExportResult, ImportedProject, detect_format, export_swf, export_xfl, import_bytes, load_path,
    save_nova,
};
pub use error::{FormatError, FormatResult};
