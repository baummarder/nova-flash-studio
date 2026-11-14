//! AI provider, privacy, project-context and safe-edit layer for NØVA Flash Studio.
//!
//! The crate is UI-independent. The desktop app renders the AI Agent Editor and
//! the provider settings, while this crate owns the real HTTP/provider logic,
//! secure credential storage, project context construction, build diagnostics
//! and root-confined file operations.

pub mod agent;
pub mod build;
pub mod credentials;
pub mod diff;
pub mod error;
pub mod files;
pub mod http;
pub mod operations;
pub mod provider;

pub use agent::{
    AgentOutcome, AgentReply, AgentSession, ContextSelection, PreparedRequest, SYSTEM_PROMPT,
    build_context, execute_prepared, parse_agent_reply,
};
pub use build::{BuildDiagnostic, BuildReport, analyze_project_scripts, build_project};
pub use credentials::{
    CredentialStore, DpapiCredentialStore, MemoryCredentialStore, RestrictedFileCredentialStore,
    default_store,
};
pub use diff::{DiffKind, DiffLine, DiffPreview, diff_text};
pub use error::{AiError, AiResult};
pub use files::{AppliedChange, ChangeKind, ChangePreview, ProposedChange, Workspace};
pub use http::{HttpRequest, HttpResponse, HttpTransport, MockTransport, ReqwestTransport};
pub use operations::{
    MAX_OPERATIONS, ObjectShape, OperationError, OperationPlan, ProjectOperation, apply_operations,
    preview_operations,
};
pub use provider::{
    ChatMessage, ChatResponse, ConnectionTest, ProviderConfig, ProviderKind, ProviderManager,
    ProviderSettings, SETTINGS_FILE, chat, default_config_dir, discover_models, load_settings,
    save_settings, test_connection,
};
