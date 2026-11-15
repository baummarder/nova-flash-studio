//! AI provider, privacy, project-context and safe-edit layer for NØVA Flash Studio.
//!
//! The crate is UI-independent. The desktop app renders the AI Agent Editor and
//! the provider settings, while this crate owns the real HTTP/provider logic,
//! secure credential storage, project context construction, build diagnostics
//! and root-confined file operations.

pub mod access;
pub mod agent;
pub mod agent_loop;
pub mod build;
pub mod credentials;
pub mod diff;
pub mod error;
pub mod files;
pub mod http;
pub mod operations;
pub mod provider;
pub mod review;
pub mod runner;
pub mod sandbox;
pub mod tasks;

pub use access::{
    ACCESS_FILE, AccessLevel, AccessPolicy, AccessStore, Grant, GuardedWorkspace, SENSITIVE_RULES,
    is_sensitive,
};
pub use agent::{
    AgentOutcome, AgentReply, AgentSession, ContextSelection, PreparedRequest, SYSTEM_PROMPT,
    build_context, execute_prepared, parse_agent_reply,
};
pub use agent_loop::{AgentLoop, LoopOutcome, PauseReason, parse_implementation, parse_plan};
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
    apply_operations_with, preview_operations, preview_operations_with,
    project_compatibility_report,
};
pub use provider::{
    ChatMessage, ChatResponse, ConnectionTest, ProviderConfig, ProviderKind, ProviderManager,
    ProviderSettings, SETTINGS_FILE, chat, default_config_dir, discover_models, load_settings,
    save_settings, test_connection,
};
pub use review::{ApplyReport, ReviewItem, ReviewSession, stage_changes};
pub use runner::{CommandRunner, CommandSpec, allowlist as command_allowlist, find_spec};
pub use sandbox::{DEFAULT_MAX_SANDBOX_BYTES, Sandbox};
pub use tasks::{
    AgentLimits, CommandRecord, PlanStep, TASKS_FILE, Task, TaskEvent, TaskPlan, TaskStatus,
    TaskStore,
};
