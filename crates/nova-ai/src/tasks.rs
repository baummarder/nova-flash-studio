//! Task manager for the AI development agent (M26.1).
//!
//! A [`Task`] captures one natural-language development request together
//! with its plan, execution state, build/test evidence and review status.
//! Tasks are persisted per edition in `ai-tasks.json` inside the AI config
//! directory — never inside `.nova` project files — and are keyed by the
//! canonical project root so history cannot leak between projects.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AiError, AiResult};

pub const TASKS_FILE: &str = "ai-tasks.json";
pub const TASKS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Created,
    Analyzing,
    Planning,
    AwaitingApproval,
    Executing,
    Testing,
    Reviewing,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Created => "Erstellt",
            Self::Analyzing => "Analyse",
            Self::Planning => "Planung",
            Self::AwaitingApproval => "Planfreigabe nötig",
            Self::Executing => "Implementierung",
            Self::Testing => "Build & Tests",
            Self::Reviewing => "Review",
            Self::Completed => "Abgeschlossen",
            Self::Failed => "Fehlgeschlagen",
            Self::Cancelled => "Abgebrochen",
        }
    }

    /// Statuses from which the task may be (re)started by the user.
    pub fn is_runnable(self) -> bool {
        matches!(
            self,
            Self::Created | Self::Failed | Self::Cancelled | Self::Completed
        )
    }

    /// Terminal states — history entries, no further automatic work.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// One implementation-plan entry produced by the planning phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub description: String,
    #[serde(default)]
    pub files: Vec<String>,
}

/// The agent's plan for a task (M26.2). Presented to the user for approval
/// before any staged implementation starts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPlan {
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub analysis: String,
    #[serde(default)]
    pub affected_files: Vec<String>,
    #[serde(default)]
    pub new_files: Vec<String>,
    #[serde(default)]
    pub steps: Vec<PlanStep>,
    #[serde(default)]
    pub expected_effects: String,
    #[serde(default)]
    pub planned_tests: String,
    #[serde(default)]
    pub risks: String,
}

impl TaskPlan {
    /// Plans that only read/analyse need no explicit approval; anything that
    /// intends file changes does.
    pub fn requires_approval(&self) -> bool {
        !self.affected_files.is_empty() || !self.new_files.is_empty() || !self.steps.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.goal.is_empty()
            && self.analysis.is_empty()
            && self.affected_files.is_empty()
            && self.steps.is_empty()
    }
}

/// One recorded agent activity line (never contains secrets).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub at: u64,
    pub status: TaskStatus,
    pub message: String,
}

/// A recorded build/test command result (M26.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    pub command: String,
    pub workdir: String,
    pub started_at: u64,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub ok: bool,
    /// Tail of combined stdout+stderr (bounded).
    pub output_tail: String,
}

/// Iteration limits (M26.3/M26.10) — configurable, persisted with settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLimits {
    /// Max provider round-trips per execution phase.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    /// Max wall-clock seconds for one task execution.
    #[serde(default = "default_max_seconds")]
    pub max_seconds: u64,
    /// Max number of changed files staged per task.
    #[serde(default = "default_max_files")]
    pub max_changed_files: usize,
    /// Max staging workspace size in bytes.
    #[serde(default = "default_max_workspace_bytes")]
    pub max_workspace_bytes: u64,
    /// Max allowlisted build/test command executions per task.
    #[serde(default = "default_max_commands")]
    pub max_commands: u32,
    /// Ask before running a command outside the built-in allowlist.
    #[serde(default = "default_confirm_unknown")]
    pub confirm_unknown_commands: bool,
    /// Persist task history.
    #[serde(default = "default_history_enabled")]
    pub history_enabled: bool,
}

fn default_max_iterations() -> u32 {
    6
}
fn default_max_seconds() -> u64 {
    600
}
fn default_max_files() -> usize {
    20
}
fn default_max_workspace_bytes() -> u64 {
    8 * 1024 * 1024
}
fn default_max_commands() -> u32 {
    10
}
fn default_confirm_unknown() -> bool {
    true
}
fn default_history_enabled() -> bool {
    true
}

impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            max_seconds: default_max_seconds(),
            max_changed_files: default_max_files(),
            max_workspace_bytes: default_max_workspace_bytes(),
            max_commands: default_max_commands(),
            confirm_unknown_commands: default_confirm_unknown(),
            history_enabled: default_history_enabled(),
        }
    }
}

/// One development task (M26.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    /// Canonical project root this task belongs to.
    pub project_root: String,
    /// Provider id chosen for this task (`None` = session default).
    #[serde(default)]
    pub provider_id: Option<String>,
    pub status: TaskStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub progress: f32,
    #[serde(default)]
    pub plan: TaskPlan,
    /// Whether the user approved the plan (required before implementing
    /// file changes).
    #[serde(default)]
    pub plan_approved: bool,
    /// Staging directory for this task's isolated workspace.
    #[serde(default)]
    pub staging_dir: Option<String>,
    #[serde(default)]
    pub commands: Vec<CommandRecord>,
    #[serde(default)]
    pub events: Vec<TaskEvent>,
    /// Review status summary for the UI.
    #[serde(default)]
    pub review_summary: String,
    #[serde(default)]
    pub error: Option<String>,
}

impl Task {
    pub fn new(project_root: &Path, title: &str, description: &str) -> AiResult<Self> {
        let root = project_root.canonicalize().map_err(AiError::io)?;
        let now = now_unix();
        Ok(Self {
            id: Uuid::new_v4(),
            title: title.to_string(),
            description: description.to_string(),
            project_root: root.to_string_lossy().to_string(),
            provider_id: None,
            status: TaskStatus::Created,
            created_at: now,
            updated_at: now,
            progress: 0.0,
            plan: TaskPlan::default(),
            plan_approved: false,
            staging_dir: None,
            commands: Vec::new(),
            events: Vec::new(),
            review_summary: String::new(),
            error: None,
        })
    }

    pub fn event(&mut self, status: TaskStatus, message: impl Into<String>) {
        self.status = status;
        self.updated_at = now_unix();
        self.events.push(TaskEvent {
            at: now_unix(),
            status,
            message: message.into(),
        });
        if self.events.len() > 500 {
            let excess = self.events.len() - 500;
            self.events.drain(0..excess);
        }
    }

    /// Whether secrets could have leaked into persisted text — cheap guard
    /// for history (keys never contain whitespace-separated `key=` patterns
    /// we write ourselves, but belt & braces).
    pub fn sanitize_for_history(&mut self) {
        for event in &mut self.events {
            event.message = scrub_secrets(&event.message);
        }
        if let Some(error) = &mut self.error {
            *error = scrub_secrets(error);
        }
    }
}

fn scrub_secrets(text: &str) -> String {
    // Redact common credential shapes; never store keys in history.
    let mut out = text.to_string();
    for marker in ["sk-", "Bearer ", "api_key=", "apikey=", "password="] {
        if let Some(pos) = out.find(marker) {
            let end = out[pos..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map(|i| pos + i)
                .unwrap_or(out.len());
            out.replace_range(pos..end, "[redacted]");
        }
    }
    out
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ------------------------------------------------------------------ store

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskData {
    version: u32,
    #[serde(default)]
    projects: BTreeMap<String, Vec<Task>>,
}

/// Persistent per-project task history in the edition config dir.
pub struct TaskStore {
    file: PathBuf,
    data: TaskData,
}

impl TaskStore {
    pub fn load(config_dir: &Path) -> AiResult<Self> {
        fs::create_dir_all(config_dir).map_err(AiError::io)?;
        let file = config_dir.join(TASKS_FILE);
        let data = if file.exists() {
            let text = fs::read_to_string(&file).map_err(AiError::io)?;
            match serde_json::from_str::<TaskData>(&text) {
                Ok(data) if data.version == TASKS_VERSION => data,
                _ => TaskData {
                    version: TASKS_VERSION,
                    projects: BTreeMap::new(),
                },
            }
        } else {
            TaskData {
                version: TASKS_VERSION,
                projects: BTreeMap::new(),
            }
        };
        Ok(Self { file, data })
    }

    pub fn fallback() -> Self {
        Self {
            file: std::env::temp_dir().join("nova-ai-tasks-fallback.json"),
            data: TaskData {
                version: TASKS_VERSION,
                projects: BTreeMap::new(),
            },
        }
    }

    pub fn save(&self) -> AiResult<()> {
        let bytes = serde_json::to_vec_pretty(&self.data).map_err(|e| {
            AiError::Unsupported(format!("Aufgaben konnten nicht serialisiert werden: {e}"))
        })?;
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, bytes).map_err(AiError::io)?;
        fs::rename(&tmp, &self.file).map_err(AiError::io)
    }

    fn key(project_root: &Path) -> AiResult<String> {
        Ok(project_root
            .canonicalize()
            .map_err(AiError::io)?
            .to_string_lossy()
            .to_string())
    }

    pub fn list(&self, project_root: &Path) -> Vec<Task> {
        Self::key(project_root)
            .ok()
            .and_then(|key| self.data.projects.get(&key).cloned())
            .unwrap_or_default()
    }

    pub fn get(&self, project_root: &Path, id: Uuid) -> Option<Task> {
        self.list(project_root).into_iter().find(|t| t.id == id)
    }

    /// Insert or replace a task (matched by id).
    pub fn upsert(&mut self, task: Task) -> AiResult<()> {
        let key = Self::key(Path::new(&task.project_root))?;
        let list = self.data.projects.entry(key).or_default();
        if let Some(existing) = list.iter_mut().find(|t| t.id == task.id) {
            *existing = task;
        } else {
            list.push(task);
            list.sort_by_key(|t| t.created_at);
        }
        Ok(())
    }

    pub fn remove(&mut self, project_root: &Path, id: Uuid) -> bool {
        let Ok(key) = Self::key(project_root) else {
            return false;
        };
        let Some(list) = self.data.projects.get_mut(&key) else {
            return false;
        };
        let before = list.len();
        list.retain(|t| t.id != id);
        list.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nova-ai-tasks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn task_lifecycle_and_persistence() {
        let config = temp_project("cfg");
        let project = temp_project("proj");
        let mut store = TaskStore::load(&config).unwrap();

        let mut task = Task::new(&project, "Fix build", "cargo test fails").unwrap();
        task.event(TaskStatus::Analyzing, "started");
        store.upsert(task.clone()).unwrap();
        store.save().unwrap();

        let reloaded = TaskStore::load(&config).unwrap();
        let tasks = reloaded.list(&project);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Fix build");
        assert_eq!(tasks[0].status, TaskStatus::Analyzing);

        // Other project sees nothing.
        let other = temp_project("other");
        assert!(reloaded.list(&other).is_empty());
    }

    #[test]
    fn plan_requires_approval_only_for_file_changes() {
        let read_only = TaskPlan {
            goal: "Analysiere Build".into(),
            analysis: "x".into(),
            ..Default::default()
        };
        assert!(!read_only.requires_approval());
        let mut changing = read_only.clone();
        changing.affected_files.push("src/a.rs".into());
        assert!(changing.requires_approval());
    }

    #[test]
    fn runnable_and_terminal_states() {
        assert!(TaskStatus::Failed.is_runnable());
        assert!(TaskStatus::Created.is_runnable());
        assert!(!TaskStatus::Executing.is_runnable());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(!TaskStatus::Reviewing.is_terminal());
    }

    #[test]
    fn secrets_are_scrubbed_from_history() {
        let mut task = Task::new(&temp_project("sec"), "t", "d").unwrap();
        task.event(TaskStatus::Executing, "key sk-abc123def used");
        task.sanitize_for_history();
        assert!(!task.events[0].message.contains("sk-abc123def"));
    }

    #[test]
    fn limits_have_safe_defaults() {
        let limits = AgentLimits::default();
        assert!(limits.max_iterations >= 1 && limits.max_iterations <= 50);
        assert!(limits.max_seconds >= 30);
        assert!(limits.confirm_unknown_commands);
        assert!(limits.history_enabled);
    }
}
