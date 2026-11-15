//! Agent execution engine for development tasks (M26.2/26.3/26.5).
//!
//! Two phases per task:
//!
//! 1. **Plan** — the provider analyses the granted project files and returns
//!    a structured [`TaskPlan`]. Plans that intend file changes require
//!    explicit user approval before implementation starts.
//! 2. **Implement** — bounded iterations inside the isolated
//!    [`ReviewSession`]: the agent proposes file changes (staged, never
//!    written to originals) and allowlisted build/test commands whose
//!    results feed the next iteration (self-correction). Iteration,
//!    runtime, file-count, workspace-size and command-count limits pause
//!    the loop with a visible reason.
//!
//! Everything file-related goes through [`GuardedWorkspace`]; commands go
//! through [`CommandRunner`]'s allowlist. The agent can never apply its own
//! changes — the [`ReviewSession`] is handed to the user for per-file
//! decisions exactly like M25.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::access::GuardedWorkspace;
use crate::agent::parse_agent_reply;
use crate::error::{AiError, AiResult};
use crate::http::HttpTransport;
use crate::provider::{ChatMessage, ProviderManager, chat};
use crate::review::ReviewSession;
use crate::runner::{CommandRunner, summarize_output};
use crate::tasks::{AgentLimits, Task, TaskPlan, TaskStatus};

/// System prompt for the planning phase.
const PLAN_PROMPT: &str = r##"Du bist der Planungs-Agent von NØVA Flash Studio.
Analysiere die Aufgabe und den freigegebenen Projektkontext. Antworte AUSSCHLIESSLICH mit einem JSON-Objekt:
{
  "goal": "kurzes Ziel",
  "analysis": "was gefunden wurde (Dateien, Abhängigkeiten, Architektur)",
  "affected_files": ["projekt-relative/pfade"],
  "new_files": ["projekt-relative/pfade"],
  "steps": [{"description": "Schritt", "files": ["pfad"]}],
  "expected_effects": "Auswirkungen",
  "planned_tests": "welche Builds/Tests",
  "risks": "Risiken/Regressionen"
}
Regeln: Nur Dateien nennen, die im Kontext sichtbar/freigegeben sind. Keine Pfade erfinden. Keine Secrets. Bei reinen Analyseaufgaben affected_files/new_files/steps leer lassen."##;

/// System prompt for the implementation phase.
const IMPLEMENT_PROMPT: &str = r##"Du bist der Implementierungs-Agent von NØVA Flash Studio.
Du arbeitest NUR im isolierten Arbeitsbereich: Dateiänderungen werden als Vorschläge gestaged und niemals direkt ins Projekt geschrieben. Der Nutzer übernimmt sie nach Review.

Antworte AUSSCHLIESSLICH mit einem JSON-Objekt:
{
  "message": "kurze Statusmeldung",
  "changes": [{"path": "projekt-relativ", "content": "vollständiger neuer Inhalt", "reason": "...", "delete": false}],
  "commands": ["erlaubter-befehlsname"],
  "operations": [],
  "done": false
}
Befehle nur aus dieser Allowlist: cargo-check, cargo-test, cargo-fmt-check, cargo-clippy, test-installer, test-edition-packaging, test-archive, nova-build. "done":true nur wenn keine weiteren Änderungen/Tests nötig sind. Bei Build-Fehlern analysiere die Ausgabe und korrigiere die betroffenen Dateien im nächsten Schritt. Entferne niemals Tests, um einen Build zu erzwingen."##;

/// Outcome of one agent-loop run handed back to the UI.
#[derive(Debug, Default)]
pub struct LoopOutcome {
    pub plan: TaskPlan,
    /// `Some` when implementation produced staged changes awaiting review.
    pub review: Option<ReviewSession>,
    pub commands: Vec<crate::tasks::CommandRecord>,
    /// Agent narration lines for the activity log.
    pub messages: Vec<String>,
    /// Project-model operations proposed during implementation.
    pub operations: Vec<crate::operations::ProjectOperation>,
    pub operation_errors: Vec<String>,
    pub status: TaskStatus,
    pub error: Option<String>,
}

/// Why the loop paused (limits).
#[derive(Debug, Clone)]
pub enum PauseReason {
    Iterations,
    Time,
    Files,
    WorkspaceSize,
    Commands,
}

impl PauseReason {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Iterations => "Iterationslimit erreicht",
            Self::Time => "Zeitlimit erreicht",
            Self::Files => "Maximale Anzahl geänderter Dateien erreicht",
            Self::WorkspaceSize => "Maximale Arbeitsbereichsgröße erreicht",
            Self::Commands => "Maximale Anzahl Build-/Testläufe erreicht",
        }
    }
}

/// Drives plan + implementation phases for one task. Runs synchronously —
/// the UI executes it on a worker thread like provider requests.
pub struct AgentLoop {
    pub limits: AgentLimits,
    pub cancel: Arc<AtomicBool>,
}

impl Default for AgentLoop {
    fn default() -> Self {
        Self {
            limits: AgentLimits::default(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl AgentLoop {
    pub fn new(limits: AgentLimits) -> Self {
        Self {
            limits,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    fn check_cancel(&self) -> AiResult<()> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(AiError::Unsupported(
                "Agent-Aufgabe wurde durch den Benutzer abgebrochen.".into(),
            ))
        } else {
            Ok(())
        }
    }

    /// Resolve the task's provider and credential; cloud requires consent.
    fn provider(
        &self,
        manager: &ProviderManager,
        task: &Task,
        cloud_consent: bool,
    ) -> AiResult<(crate::provider::ProviderConfig, Option<String>)> {
        let provider = manager
            .selected(task.provider_id.as_deref())
            .ok_or(AiError::NoProvider)?
            .clone();
        if !provider.enabled {
            return Err(AiError::ProviderUnavailable(provider.name.clone()));
        }
        if provider.is_cloud() && !cloud_consent {
            return Err(AiError::CloudConsentRequired);
        }
        let key = manager.credential(&provider.id)?;
        if provider.needs_credential() && key.as_deref().is_none_or(str::is_empty) {
            return Err(AiError::MissingCredential(provider.name.clone()));
        }
        Ok((provider, key))
    }

    fn chat(
        &self,
        provider: &crate::provider::ProviderConfig,
        key: Option<&str>,
        system: &str,
        user: &str,
        transport: &dyn HttpTransport,
    ) -> AiResult<String> {
        let messages = vec![
            ChatMessage::system(system.to_string()),
            ChatMessage::user(user.to_string()),
        ];
        let response = chat(provider, key, &messages, transport)?;
        Ok(response.content)
    }

    /// Phase 1+2 entry: analyse → plan → (approved) implement.
    /// `plan_only` returns after planning (status AwaitingApproval or
    /// Completed for read-only plans).
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        manager: &ProviderManager,
        task: &mut Task,
        guarded: &GuardedWorkspace,
        transport: &dyn HttpTransport,
        staging_root: &Path,
        workdir_env: &[(String, String)],
        cloud_consent: bool,
        plan_only: bool,
    ) -> AiResult<LoopOutcome> {
        let (provider, key) = self.provider(manager, task, cloud_consent)?;
        self.run_inner(
            task,
            &provider,
            key.as_deref(),
            guarded,
            transport,
            staging_root,
            workdir_env,
            plan_only,
        )
    }

    /// Same as [`Self::run`] but with an already-resolved provider — used by
    /// the UI worker thread which cannot hold the `ProviderManager`.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_provider(
        &self,
        task: &mut Task,
        provider: &crate::provider::ProviderConfig,
        key: Option<&str>,
        guarded: &GuardedWorkspace,
        transport: &dyn HttpTransport,
        staging_root: &Path,
        workdir_env: &[(String, String)],
        plan_only: bool,
    ) -> AiResult<LoopOutcome> {
        self.run_inner(
            task,
            provider,
            key,
            guarded,
            transport,
            staging_root,
            workdir_env,
            plan_only,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_inner(
        &self,
        task: &mut Task,
        provider: &crate::provider::ProviderConfig,
        key: Option<&str>,
        guarded: &GuardedWorkspace,
        transport: &dyn HttpTransport,
        staging_root: &Path,
        workdir_env: &[(String, String)],
        plan_only: bool,
    ) -> AiResult<LoopOutcome> {
        self.check_cancel()?;
        let started = Instant::now();
        let mut outcome = LoopOutcome::default();

        // ------------------------------------------------ analyse+plan
        task.event(TaskStatus::Analyzing, "Projektkontext wird gesammelt.");
        let readable = guarded.list_readable_files();
        let mut context_files: Vec<String> = Vec::new();
        let mut context_chars = 0usize;
        for file in readable.iter().take(40) {
            if context_chars > 24_000 {
                break;
            }
            if let Ok(content) = guarded.read(file) {
                let clipped: String = content.chars().take(4_000).collect();
                context_chars += clipped.len();
                context_files.push(format!("--- {file} ---\n{clipped}"));
            }
        }
        let plan_user = format!(
            "Aufgabe: {}\n\nBeschreibung:\n{}\n\nFreigegebene Dateien ({}):\n{}\n\nDateiinhalte (gekürzt):\n{}",
            task.title,
            task.description,
            readable.len(),
            readable.join("\n"),
            context_files.join("\n\n")
        );
        task.event(TaskStatus::Planning, "Planungs-Anfrage an den Anbieter.");
        let plan_text = self.chat(provider, key, PLAN_PROMPT, &plan_user, transport)?;
        let plan = parse_plan(&plan_text).unwrap_or_else(|| TaskPlan {
            goal: task.title.clone(),
            analysis: plan_text.trim().chars().take(2_000).collect(),
            ..Default::default()
        });
        task.plan = plan.clone();
        outcome.plan = plan.clone();
        outcome.messages.push(format!(
            "Plan: {} ({} betroffene Datei(en))",
            plan.goal,
            plan.affected_files.len() + plan.new_files.len()
        ));

        if plan_only {
            task.status = if plan.requires_approval() {
                TaskStatus::AwaitingApproval
            } else {
                TaskStatus::Completed
            };
            task.event(
                task.status,
                if plan.requires_approval() {
                    "Plan erstellt — wartet auf Freigabe."
                } else {
                    "Analyse abgeschlossen (keine Dateiänderungen nötig)."
                },
            );
            outcome.status = task.status;
            return Ok(outcome);
        }
        if plan.requires_approval() && !task.plan_approved {
            task.status = TaskStatus::AwaitingApproval;
            task.event(
                TaskStatus::AwaitingApproval,
                "Plan wartet auf ausdrückliche Freigabe.",
            );
            outcome.status = task.status;
            return Ok(outcome);
        }

        // ------------------------------------------------ implement loop
        let mut review = ReviewSession::new(guarded.root(), staging_root)?;
        task.staging_dir = Some(review.staging_dir().to_string_lossy().to_string());
        let mut commands_run = 0u32;
        let mut iterations = 0u32;
        let mut last_command_summary = String::new();
        let runner = CommandRunner::with_cancel(Arc::clone(&self.cancel));

        loop {
            self.check_cancel()?;
            if iterations >= self.limits.max_iterations {
                return self.pause(task, &mut outcome, review, PauseReason::Iterations, started);
            }
            if started.elapsed() > Duration::from_secs(self.limits.max_seconds) {
                return self.pause(task, &mut outcome, review, PauseReason::Time, started);
            }
            if review.items().len() > self.limits.max_changed_files {
                return self.pause(task, &mut outcome, review, PauseReason::Files, started);
            }
            let staged_bytes: u64 = review
                .items()
                .iter()
                .filter_map(|item| item.staged.as_ref())
                .filter_map(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .sum();
            if staged_bytes > self.limits.max_workspace_bytes {
                return self.pause(
                    task,
                    &mut outcome,
                    review,
                    PauseReason::WorkspaceSize,
                    started,
                );
            }
            if commands_run >= self.limits.max_commands {
                return self.pause(task, &mut outcome, review, PauseReason::Commands, started);
            }
            iterations += 1;
            task.event(
                TaskStatus::Executing,
                format!("Iteration {iterations}: Agent arbeitet im isolierten Bereich."),
            );

            // Build iteration prompt with current staged state.
            let staged_summary = review
                .items()
                .iter()
                .map(|item| {
                    format!(
                        "- {} ({}): +{}/-{}",
                        item.path,
                        item.kind.label(),
                        item.diff.added,
                        item.diff.removed
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let impl_user = format!(
                "Aufgabe: {}\n\nPlan-Ziel: {}\n\nBisherige Staged-Änderungen:\n{}\n\nLetzte Build-Ausgabe:\n{}\n\nBitte nächsten Schritt liefern (changes/commands/done).",
                task.description,
                plan.goal,
                if staged_summary.is_empty() {
                    "(noch keine)".to_string()
                } else {
                    staged_summary
                },
                if last_command_summary.is_empty() {
                    "(noch keine)".to_string()
                } else {
                    last_command_summary.chars().take(6_000).collect()
                }
            );
            let reply_text = self.chat(provider, key, IMPLEMENT_PROMPT, &impl_user, transport)?;
            let reply = parse_implementation(&reply_text);
            if !reply.message.is_empty() {
                outcome.messages.push(reply.message.clone());
                task.event(TaskStatus::Executing, reply.message.clone());
            }

            // Stage changes (never touches originals).
            let mut staged_any = false;
            for change in &reply.changes {
                match review.stage_or_update(guarded, change) {
                    Ok(()) => staged_any = true,
                    Err(error) => outcome
                        .operation_errors
                        .push(format!("{}: {error}", change.path)),
                }
            }
            outcome.operations.extend(reply.operations.clone());
            outcome
                .operation_errors
                .extend(reply.operation_errors.clone());

            // Run allowlisted commands.
            let mut ran_command = false;
            for command_id in &reply.commands {
                if commands_run >= self.limits.max_commands {
                    break;
                }
                if !crate::runner::find_spec(command_id).is_some() {
                    outcome
                        .operation_errors
                        .push(format!("Befehl '{command_id}': nicht in der Allowlist"));
                    continue;
                }
                task.event(TaskStatus::Testing, format!("Führe '{command_id}' aus."));
                match runner.run_allowlisted(
                    command_id,
                    guarded.root(),
                    workdir_env,
                    Duration::from_secs(600),
                ) {
                    Ok(result) => {
                        commands_run += 1;
                        outcome.commands.push(result.record.clone());
                        last_command_summary = format!(
                            "$ {}\n(exit {})\n{}",
                            result.record.command,
                            result.record.exit_code,
                            summarize_output(&result.output)
                        );
                        ran_command = true;
                    }
                    Err(error) => {
                        outcome
                            .operation_errors
                            .push(format!("Befehl '{command_id}': {error}"));
                    }
                }
            }

            if reply.done || (!staged_any && !ran_command && reply.commands.is_empty()) {
                break;
            }
        }

        outcome.review = Some(review);
        outcome.status = TaskStatus::Reviewing;
        task.status = TaskStatus::Reviewing;
        task.progress = 0.9;
        task.review_summary = format!(
            "{} Dateiänderung(en) im Review-Workspace",
            outcome
                .review
                .as_ref()
                .map(|r| r.items().len())
                .unwrap_or(0)
        );
        task.event(
            TaskStatus::Reviewing,
            "Implementierung beendet — Änderungen warten auf Nutzer-Review.",
        );
        Ok(outcome)
    }

    /// Pause path: keep the review session + partial results visible.
    #[allow(clippy::too_many_arguments)]
    fn pause(
        &self,
        task: &mut Task,
        outcome: &mut LoopOutcome,
        review: ReviewSession,
        reason: PauseReason,
        started: Instant,
    ) -> AiResult<LoopOutcome> {
        let _ = started;
        task.event(
            TaskStatus::Failed,
            format!("Pausiert: {}", reason.message()),
        );
        task.error = Some(reason.message().to_string());
        outcome.review = Some(review);
        outcome.status = TaskStatus::Failed;
        outcome.error = Some(reason.message().to_string());
        Ok(std::mem::take(outcome))
    }
}

/// Parsed implementation reply (extends agent reply with commands/done).
#[derive(Debug, Default)]
pub struct ImplementationReply {
    pub message: String,
    pub changes: Vec<crate::files::ProposedChange>,
    pub commands: Vec<String>,
    pub operations: Vec<crate::operations::ProjectOperation>,
    pub operation_errors: Vec<String>,
    pub done: bool,
}

/// Tolerant JSON extraction for the implementation protocol.
pub fn parse_implementation(text: &str) -> ImplementationReply {
    let base = parse_agent_reply(text);
    let mut reply = ImplementationReply {
        message: base.message,
        changes: base.changes,
        operations: base.operations,
        operation_errors: base.operation_errors,
        ..Default::default()
    };
    // Re-extract raw JSON for commands/done (parse_agent_reply keeps raw).
    if let Some(json) = extract_json(&base.raw)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&json)
    {
        reply.commands = value
            .get("commands")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        reply.done = value.get("done").and_then(|d| d.as_bool()).unwrap_or(false);
    }
    reply
}

fn extract_json(text: &str) -> Option<String> {
    if let Some(start) = text.find("```") {
        let rest = &text[start + 3..];
        let rest = rest.strip_prefix("json").unwrap_or(rest).trim_start();
        if let Some(end) = rest.find("```") {
            let candidate = rest[..end].trim();
            if candidate.starts_with('{') {
                return Some(candidate.to_string());
            }
        }
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| text[start..=end].to_string())
}

/// Parse the planning JSON tolerantly; falls back to fields as text.
pub fn parse_plan(text: &str) -> Option<TaskPlan> {
    let json = extract_json(text)?;
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    let get_str = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let get_list = |key: &str| -> Vec<String> {
        value
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let steps = value
        .get("steps")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    Some(crate::tasks::PlanStep {
                        description: s.get("description")?.as_str()?.to_string(),
                        files: s
                            .get("files")
                            .and_then(|f| f.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|f| f.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(TaskPlan {
        goal: get_str("goal"),
        analysis: get_str("analysis"),
        affected_files: get_list("affected_files"),
        new_files: get_list("new_files"),
        steps,
        expected_effects: get_str("expected_effects"),
        planned_tests: get_str("planned_tests"),
        risks: get_str("risks"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implementation_reply_extracts_commands_and_done() {
        let reply = parse_implementation(
            r#"{"message":"ok","changes":[],"commands":["cargo-check","cargo-test"],"done":false}"#,
        );
        assert_eq!(reply.commands, vec!["cargo-check", "cargo-test"]);
        assert!(!reply.done);
        let done = parse_implementation(r#"{"message":"fertig","done":true}"#);
        assert!(done.done);
    }

    #[test]
    fn plan_json_parses_all_fields() {
        let plan = parse_plan(
            r#"{"goal":"Fix","analysis":"a","affected_files":["x.rs"],"new_files":["y.rs"],"steps":[{"description":"s","files":["x.rs"]}],"expected_effects":"e","planned_tests":"t","risks":"r"}"#,
        )
        .unwrap();
        assert_eq!(plan.goal, "Fix");
        assert_eq!(plan.affected_files, vec!["x.rs"]);
        assert_eq!(plan.steps.len(), 1);
        assert!(plan.requires_approval());
    }

    #[test]
    fn plan_without_changes_needs_no_approval() {
        let plan = parse_plan(r#"{"goal":"Analysiere","analysis":"nichts zu tun"}"#).unwrap();
        assert!(!plan.requires_approval());
    }

    // ------------------------------------------------ end-to-end loop

    use crate::access::{AccessLevel, AccessStore, GuardedWorkspace};
    use crate::files::Workspace;
    use crate::http::{HttpResponse, MockTransport};
    use crate::provider::{ProviderConfig, ProviderKind};

    fn ollama_body(content: &str) -> Vec<u8> {
        serde_json::json!({"message": {"content": content}, "done": true})
            .to_string()
            .into_bytes()
    }

    fn fixture(name: &str) -> (std::path::PathBuf, GuardedWorkspace, AccessStore) {
        let root = std::env::temp_dir().join(format!("nova-ai-loop-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.as"), "class Main {}\n").unwrap();
        let mut store = AccessStore::fallback();
        store.grant(&root, &root, AccessLevel::ReadWrite).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let guarded = GuardedWorkspace::new(workspace, store.policy_for(&root));
        (root, guarded, store)
    }

    fn provider() -> ProviderConfig {
        let mut provider = ProviderConfig::new(ProviderKind::OllamaLocal);
        provider.base_url = "http://localhost:11434".into();
        provider.model = "test-model".into();
        provider
    }

    #[test]
    fn full_task_flow_plan_approve_implement_apply() {
        let (root, guarded, _store) = fixture("e2e");
        let staging =
            std::env::temp_dir().join(format!("nova-ai-loop-staging-{}", std::process::id()));
        let plan_json = r#"{"goal":"Neue Klasse","analysis":"main.as vorhanden","affected_files":["src/new.as"],"new_files":["src/new.as"],"steps":[{"description":"Datei erstellen","files":["src/new.as"]}],"expected_effects":"neue Datei","planned_tests":"cargo-test","risks":"keine"}"#;
        let impl_json = r#"{"message":"erstellt","changes":[{"path":"src/new.as","content":"class New {}\n"}],"commands":[],"done":true}"#;
        let transport = MockTransport::new(vec![
            HttpResponse::json(200, ollama_body(plan_json)),
            HttpResponse::json(200, ollama_body(plan_json)),
            HttpResponse::json(200, ollama_body(impl_json)),
        ]);
        let engine = AgentLoop::default();
        let mut task = Task::new(&root, "Neue Klasse", "Füge src/new.as hinzu").unwrap();

        // Phase 1: plan only.
        let outcome = engine
            .run_with_provider(
                &mut task,
                &provider(),
                None,
                &guarded,
                &transport,
                &staging,
                &[],
                true,
            )
            .unwrap();
        assert_eq!(task.status, TaskStatus::AwaitingApproval);
        assert!(task.plan.requires_approval());
        assert!(outcome.review.is_none());

        // Phase 2: approved implementation.
        task.plan_approved = true;
        let outcome = engine
            .run_with_provider(
                &mut task,
                &provider(),
                None,
                &guarded,
                &transport,
                &staging,
                &[],
                false,
            )
            .unwrap();
        assert_eq!(task.status, TaskStatus::Reviewing);
        let mut review = outcome.review.expect("review session");
        assert_eq!(review.items().len(), 1);
        // Original project untouched before user apply.
        assert!(!root.join("src/new.as").exists());
        // Apply accepted change → file appears.
        let report = review.apply(&guarded).unwrap();
        assert_eq!(report.applied.len(), 1);
        assert_eq!(
            std::fs::read_to_string(root.join("src/new.as")).unwrap(),
            "class New {}\n"
        );
    }

    #[test]
    fn iterations_limit_pauses_with_reason() {
        let (root, guarded, _store) = fixture("limit");
        let staging =
            std::env::temp_dir().join(format!("nova-ai-loop-limit-{}", std::process::id()));
        let plan_json = r#"{"goal":"x","affected_files":["a"],"steps":[{"description":"s"}]}"#;
        // Implementation reply never says done → loop must hit iteration cap.
        let impl_json = r#"{"message":"noch nicht","changes":[{"path":"src/x.as","content":"x"}],"commands":[],"done":false}"#;
        let mut responses: Vec<HttpResponse> =
            vec![HttpResponse::json(200, ollama_body(plan_json))];
        for _ in 0..10 {
            responses.push(HttpResponse::json(200, ollama_body(impl_json)));
        }
        let transport = MockTransport::new(responses);
        let limits = AgentLimits {
            max_iterations: 2,
            ..Default::default()
        };
        let engine = AgentLoop {
            limits,
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let mut task = Task::new(&root, "loop", "desc").unwrap();
        task.plan_approved = true;
        let outcome = engine
            .run_with_provider(
                &mut task,
                &provider(),
                None,
                &guarded,
                &transport,
                &staging,
                &[],
                false,
            )
            .unwrap();
        assert_eq!(task.status, TaskStatus::Failed);
        assert!(task.error.unwrap().contains("Iteration"));
        // Partial staged work is preserved for review, originals intact.
        assert!(outcome.review.is_some());
        assert!(!root.join("src/x.as").exists());
    }

    #[test]
    fn cancelled_task_stops_promptly() {
        let (root, guarded, _store) = fixture("cancel");
        let staging =
            std::env::temp_dir().join(format!("nova-ai-loop-cancel-{}", std::process::id()));
        let transport = MockTransport::new(vec![]);
        let engine = AgentLoop::default();
        engine.cancel();
        let mut task = Task::new(&root, "cancel", "desc").unwrap();
        assert!(
            engine
                .run_with_provider(
                    &mut task,
                    &provider(),
                    None,
                    &guarded,
                    &transport,
                    &staging,
                    &[],
                    true,
                )
                .is_err()
        );
    }
}
