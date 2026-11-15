//! Integrated AI Agent Editor workspace.
//!
//! The workspace is a first-class application surface: project explorer,
//! source editor, chat, activity/build output and diff preview all live here.
//! Network work is prepared on the UI thread and executed on a worker thread.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eframe::egui::{self, Context, RichText, TextEdit, Ui};
use nova_ai::{
    AgentOutcome, AgentSession, AppliedChange, BuildReport, ChangeKind, ChangePreview, DiffKind,
    ProviderManager, ReqwestTransport, Workspace, apply_operations_with, build_project,
    default_config_dir, default_store, execute_prepared, load_settings, preview_operations_with,
};
use nova_as::{Diagnostic, Severity, analyze};
use nova_core::{CodeProvenance, Project, Script, ScriptId};

use crate::ai_providers::ProviderDialog;
use crate::state::{AppState, LogLevel, UiAction};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTab {
    Build,
    Diagnostics,
    Diff,
    Activity,
}

#[derive(Debug, Clone)]
pub struct ActivityLine {
    pub text: String,
    pub error: bool,
}

pub struct AiEditor {
    pub open: bool,
    pub providers: ProviderManager,
    pub provider_dialog: ProviderDialog,
    pub session: AgentSession,
    pub workspace: Option<Workspace>,
    pub workspace_root: String,
    pub workspace_files: Vec<String>,
    pub follow_project: bool,
    pub last_project_path: Option<PathBuf>,
    pub selected_script: Option<ScriptId>,
    pub editor_script: Option<ScriptId>,
    pub editor_path: String,
    pub editor_buffer: String,
    pub last_synced_source: String,
    pub editor_selection: String,
    pub chat_input: String,
    pub activity: Vec<ActivityLine>,
    pub build_report: Option<BuildReport>,
    pub diagnostics: Vec<Diagnostic>,
    pub pending_previews: Vec<ChangePreview>,
    pub preview_errors: Vec<String>,
    pub operation_previews: Vec<String>,
    pub operation_errors: Vec<String>,
    pub applied: Vec<AppliedChange>,
    pub error: Option<String>,
    pub status: String,
    pub busy: bool,
    job: Option<Receiver<Result<AgentOutcome, String>>>,
    job_started: Instant,
    pub output_tab: OutputTab,
    pub startup_error: Option<String>,
    last_diagnostics_refresh: Instant,
    pub auto_build: bool,
}

impl AiEditor {
    pub fn new() -> Self {
        let config_dir = default_config_dir().unwrap_or_else(|_| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".nova-ai")
        });
        let (providers, startup_error) = match ProviderManager::load(&config_dir) {
            Ok(manager) => (manager, None),
            Err(error) => {
                let settings = load_settings(&config_dir).unwrap_or_default();
                let transport = Arc::new(ReqwestTransport::new().unwrap_or_default());
                let manager = ProviderManager::with_parts(
                    settings,
                    default_store(&config_dir),
                    transport,
                    config_dir,
                );
                (manager, Some(error.to_string()))
            }
        };
        let workspace_root = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_string_lossy()
            .to_string();
        let mut editor = Self {
            open: false,
            providers,
            provider_dialog: ProviderDialog::default(),
            session: AgentSession::default(),
            workspace: None,
            workspace_root,
            workspace_files: Vec::new(),
            follow_project: true,
            last_project_path: None,
            selected_script: None,
            editor_script: None,
            editor_path: String::new(),
            editor_buffer: String::new(),
            last_synced_source: String::new(),
            editor_selection: String::new(),
            chat_input: String::new(),
            activity: Vec::new(),
            build_report: None,
            diagnostics: Vec::new(),
            pending_previews: Vec::new(),
            preview_errors: Vec::new(),
            operation_previews: Vec::new(),
            operation_errors: Vec::new(),
            applied: Vec::new(),
            error: None,
            status: "Bereit".into(),
            busy: false,
            job: None,
            job_started: Instant::now(),
            output_tab: OutputTab::Activity,
            startup_error,
            last_diagnostics_refresh: Instant::now() - Duration::from_secs(10),
            auto_build: false,
        };
        editor.refresh_workspace();
        if let Some(error) = &editor.startup_error {
            editor.activity.push(ActivityLine {
                text: format!("KI-Konfiguration: {error}"),
                error: true,
            });
        }
        editor
    }

    pub fn show_provider_dialog(&mut self, ctx: &Context, state: &mut AppState) {
        self.provider_dialog.show(ctx, &mut self.providers, state);
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        project: &mut Project,
        state: &mut AppState,
    ) -> Vec<UiAction> {
        self.sync_project(project, state);
        self.poll_job(project, state);
        if self.busy {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let mut actions = Vec::new();

        egui::Panel::top("ai_editor_header")
            .exact_size(38.0)
            .show(ui, |ui| {
                self.header(ui, project, state, &mut actions);
            });
        egui::Panel::bottom("ai_editor_output")
            .resizable(true)
            .default_size(210.0)
            .min_size(130.0)
            .max_size(520.0)
            .show(ui, |ui| {
                self.output_panel(ui, project, state);
            });
        egui::Panel::left("ai_editor_explorer")
            .resizable(true)
            .default_size(238.0)
            .min_size(190.0)
            .max_size(390.0)
            .show(ui, |ui| {
                self.explorer(ui, project, state);
            });
        egui::Panel::right("ai_editor_chat")
            .resizable(true)
            .default_size(420.0)
            .min_size(300.0)
            .max_size(650.0)
            .show(ui, |ui| {
                self.chat_panel(ui, project, state);
            });
        egui::CentralPanel::default().show(ui, |ui| {
            self.editor_panel(ui, project, state);
        });

        actions
    }

    fn header(
        &mut self,
        ui: &mut Ui,
        project: &Project,
        state: &mut AppState,
        actions: &mut Vec<UiAction>,
    ) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("AI AGENT EDITOR")
                    .strong()
                    .size(15.0)
                    .color(theme::ACCENT),
            );
            ui.separator();
            if ui.button("Zurück zum Editor").clicked() {
                self.open = false;
            }
            if ui.button("KI-Anbieter").clicked() {
                actions.push(UiAction::OpenAiProviders);
            }
            if ui.button("Build").clicked() {
                self.run_build(project, state);
            }
            if ui
                .add_enabled(!self.busy, egui::Button::new("Diagnose"))
                .clicked()
            {
                self.refresh_diagnostics(project);
                self.output_tab = OutputTab::Diagnostics;
            }
            ui.separator();
            let selected = self.providers.selected(self.session.provider_override.as_deref());
            let label = selected
                .map(|provider| {
                    format!(
                        "{} · {}{}",
                        provider.name,
                        if provider.model.is_empty() {
                            "kein Modell"
                        } else {
                            provider.model.as_str()
                        },
                        if provider.is_cloud() { " · CLOUD" } else { " · LOKAL" }
                    )
                })
                .unwrap_or_else(|| "Kein Anbieter".into());
            egui::ComboBox::from_id_salt("ai_provider_selector")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    let default_name = self
                        .providers
                        .settings
                        .default_provider
                        .as_deref()
                        .and_then(|id| self.providers.provider(id))
                        .map(|provider| provider.name.clone())
                        .unwrap_or_else(|| "kein Standard".into());
                    if ui
                        .selectable_label(
                            self.session.provider_override.is_none(),
                            format!("Standard ({default_name})"),
                        )
                        .clicked()
                    {
                        self.session.provider_override = None;
                    }
                    let enabled: Vec<(String, String)> = self
                        .providers
                        .enabled()
                        .iter()
                        .map(|provider| (provider.id.clone(), provider.name.clone()))
                        .collect();
                    for (id, name) in enabled {
                        if ui
                            .selectable_label(
                                self.session.provider_override.as_deref() == Some(id.as_str()),
                                name,
                            )
                            .clicked()
                        {
                            self.session.provider_override = Some(id);
                        }
                    }
                });
            if let Some(provider) = self.providers.selected(self.session.provider_override.as_deref())
                && provider.is_cloud() {
                    ui.checkbox(
                        &mut self.session.cloud_consent,
                        RichText::new("Cloud-Zustimmung").color(theme::AMBER),
                    )
                    .on_hover_text(
                        "Ohne diese Zustimmung wird kein Projektkontext an einen Cloud-Anbieter gesendet.",
                    );
                }
            if self.busy {
                ui.spinner();
                ui.label(
                    RichText::new(format!(
                        "Anfrage läuft … {:.0}s",
                        self.job_started.elapsed().as_secs_f32()
                    ))
                    .small()
                    .color(theme::TEXT_DIM),
                );
            }
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if !self.status.is_empty() {
                        ui.label(
                            RichText::new(&self.status)
                                .small()
                                .color(if self.error.is_some() {
                                    theme::RED
                                } else {
                                    theme::TEXT_DIM
                                }),
                        );
                    }
                },
            );
        });
    }

    fn sync_project(&mut self, project: &Project, state: &mut AppState) {
        let source_path = state.path.clone().or_else(|| {
            project
                .source
                .as_ref()
                .and_then(|source| source.original_path.clone())
                .map(PathBuf::from)
        });
        if self.follow_project && source_path != self.last_project_path {
            if let Some(parent) = source_path.as_deref().and_then(|path| path.parent()) {
                self.workspace_root = parent.to_string_lossy().to_string();
                self.refresh_workspace();
            }
            self.last_project_path = source_path;
            self.selected_script = state
                .active_script
                .or_else(|| project.scripts.first().map(|script| script.id));
            self.load_editor(project);
            self.refresh_diagnostics(project);
        }
        if self.selected_script.is_none() {
            self.selected_script = state
                .active_script
                .or_else(|| project.scripts.first().map(|script| script.id));
        }
        if self.editor_script != self.selected_script {
            self.load_editor(project);
        }
        if let Some(id) = self.editor_script {
            if let Some(script) = project.scripts.iter().find(|script| script.id == id) {
                if script.source != self.last_synced_source
                    && self.editor_buffer == self.last_synced_source
                {
                    self.editor_buffer = script.source.clone();
                    self.last_synced_source = script.source.clone();
                }
            } else {
                self.selected_script = project.scripts.first().map(|script| script.id);
                self.load_editor(project);
            }
        }
        if self.last_diagnostics_refresh.elapsed() > Duration::from_secs(2) {
            self.refresh_diagnostics(project);
        }
    }

    fn refresh_workspace(&mut self) {
        match Workspace::new(&self.workspace_root) {
            Ok(workspace) => {
                self.workspace_files = workspace.list_source_files();
                self.workspace = Some(workspace);
                self.error = None;
            }
            Err(error) => {
                self.workspace = None;
                self.workspace_files.clear();
                self.error = Some(error.to_string());
            }
        }
    }

    fn load_editor(&mut self, project: &Project) {
        self.editor_script = self.selected_script;
        self.editor_selection.clear();
        if let Some(id) = self.selected_script
            && let Some(script) = project.scripts.iter().find(|script| script.id == id)
        {
            self.editor_path = script.path.clone();
            self.editor_buffer = script.source.clone();
            self.last_synced_source = script.source.clone();
            return;
        }
        if !self.editor_path.is_empty()
            && let Some(workspace) = &self.workspace
            && let Ok(source) = workspace.read(&self.editor_path)
        {
            self.editor_buffer = source.clone();
            self.last_synced_source = source;
            return;
        }
        self.editor_path.clear();
        self.editor_buffer.clear();
        self.last_synced_source.clear();
    }

    fn refresh_diagnostics(&mut self, project: &Project) {
        let mut diagnostics = Vec::new();
        for script in &project.scripts {
            if script.abc.is_some() {
                continue;
            }
            let (_, script_diagnostics) = analyze(&script.source);
            diagnostics.extend(script_diagnostics);
        }
        self.diagnostics = diagnostics;
        self.last_diagnostics_refresh = Instant::now();
    }

    fn start_request(&mut self, project: &Project, state: &mut AppState) {
        if self.busy {
            return;
        }
        let message = self.chat_input.trim().to_string();
        if message.is_empty() {
            self.error = Some("Bitte eine Aufgabe eingeben.".into());
            return;
        }
        let prepared = match self.session.prepare(
            &self.providers,
            project,
            state.frame,
            state.mode.label(),
            &self.diagnostics,
            &message,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.error = Some(error.to_string());
                self.activity.push(ActivityLine {
                    text: format!("Anfrage abgebrochen: {error}"),
                    error: true,
                });
                return;
            }
        };
        self.session
            .messages
            .push(nova_ai::ChatMessage::user(message));
        self.chat_input.clear();
        self.error = None;
        self.status = "Anfrage läuft …".into();
        self.busy = true;
        self.job_started = Instant::now();
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        std::thread::spawn(move || {
            let result = match ReqwestTransport::new() {
                Ok(transport) => {
                    execute_prepared(prepared, &transport).map_err(|error| error.to_string())
                }
                Err(error) => Err(error.to_string()),
            };
            let _ = sender.send(result);
        });
    }

    fn poll_job(&mut self, project: &mut Project, state: &mut AppState) {
        let Some(receiver) = &self.job else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => {
                Some(Err("Der KI-Worker wurde unerwartet beendet.".into()))
            }
            Err(TryRecvError::Empty) => None,
        };
        let Some(result) = result else {
            return;
        };
        self.job = None;
        self.busy = false;
        match result {
            Ok(outcome) => {
                self.session.messages.push(nova_ai::ChatMessage::assistant(
                    outcome.reply.message.clone(),
                ));
                self.session.pending_changes = outcome.reply.changes.clone();
                self.session.pending_operations = outcome.reply.operations.clone();
                self.session.last_context = outcome.context.clone();
                self.activity.push(ActivityLine {
                    text: format!(
                        "Antwort von {} ({}, {} Kontextzeichen, {} Dateiänderung(en), {} Projektoperation(en))",
                        outcome.provider_name,
                        outcome.model,
                        outcome.context_chars,
                        outcome.reply.changes.len(),
                        outcome.reply.operations.len()
                    ),
                    error: false,
                });
                for warning in &outcome.reply.operation_errors {
                    self.activity.push(ActivityLine {
                        text: warning.clone(),
                        error: true,
                    });
                }
                self.prepare_previews(project);
                if !self.pending_previews.is_empty() || !self.session.pending_operations.is_empty()
                {
                    self.output_tab = OutputTab::Diff;
                }
                let total = outcome.reply.changes.len() + outcome.reply.operations.len();
                self.status = if total == 0 {
                    "Antwort erhalten.".into()
                } else {
                    format!(
                        "{} Vorschlag/Vorschläge zur Prüfung. Nichts wurde automatisch angewendet.",
                        total
                    )
                };
                state.log(
                    LogLevel::Info,
                    format!(
                        "KI-Antwort von {} / {} verarbeitet.",
                        outcome.provider_name, outcome.model
                    ),
                );
                if outcome.reply.wants_build() || self.auto_build {
                    self.run_build(project, state);
                }
            }
            Err(error) => {
                self.error = Some(error.clone());
                self.status = "KI-Anfrage fehlgeschlagen.".into();
                self.activity.push(ActivityLine {
                    text: format!("Fehler: {error}"),
                    error: true,
                });
                state.log(LogLevel::Error, format!("KI-Anfrage: {error}"));
            }
        }
    }

    fn prepare_previews(&mut self, project: &Project) {
        self.pending_previews.clear();
        self.preview_errors.clear();
        self.operation_previews.clear();
        self.operation_errors.clear();
        let read_asset = |path: &str| -> Result<Vec<u8>, String> {
            self.workspace
                .as_ref()
                .ok_or_else(|| "Kein Projektordner ausgewählt.".to_string())?
                .read_bytes(path)
                .map_err(|error| error.to_string())
        };
        let plan = preview_operations_with(project, &self.session.pending_operations, &read_asset);
        self.operation_previews = plan.summaries;
        self.operation_errors = plan.errors;
        if self.session.pending_changes.is_empty() {
            return;
        }
        let Some(workspace) = &self.workspace else {
            self.preview_errors
                .push("Kein gültiger Projektordner ausgewählt.".into());
            return;
        };
        for change in &self.session.pending_changes {
            if let Some(script) = project
                .scripts
                .iter()
                .find(|script| script.path == change.path)
                && script.abc.is_some()
            {
                self.preview_errors.push(format!(
                        "{}: Preserved ABC wird geschützt. Der erhaltene Bytecode darf weder durch Quelltext ersetzt noch gelöscht werden.",
                        change.path
                    ));
                continue;
            }
            match workspace.preview(change) {
                Ok(preview) => self.pending_previews.push(preview),
                Err(error) => self
                    .preview_errors
                    .push(format!("{}: {error}", change.path)),
            }
        }
    }

    fn accept_pending(&mut self, project: &mut Project, state: &mut AppState) {
        if self.pending_previews.is_empty() && self.session.pending_operations.is_empty() {
            self.error = Some("Keine Änderungen zur Bestätigung.".into());
            return;
        }
        if !self.preview_errors.is_empty() || !self.operation_errors.is_empty() {
            self.error = Some(
                "Mindestens ein Vorschlag ist blockiert. Entferne ihn zuerst durch Verwerfen."
                    .into(),
            );
            return;
        }
        if !self.pending_previews.is_empty() && self.workspace.is_none() {
            self.error = Some("Kein gültiger Projektordner ausgewählt.".into());
            return;
        }
        let previews = self.pending_previews.clone();
        let operations = self.session.pending_operations.clone();
        let project_before = project.clone();
        let active_script_before = state.active_script;
        let mut applied = Vec::new();
        if let Some(workspace) = &self.workspace {
            for preview in &previews {
                let change = nova_ai::ProposedChange {
                    path: preview.path.clone(),
                    content: preview.new_content.clone().unwrap_or_default(),
                    reason: preview.reason.clone(),
                    delete: preview.kind == ChangeKind::Delete,
                    kind: Some(preview.kind),
                };
                match workspace.apply(&change) {
                    Ok(change) => applied.push(change),
                    Err(error) => {
                        for applied_change in applied.iter().rev() {
                            let _ = workspace.undo(applied_change);
                        }
                        self.error = Some(format!(
                            "Änderungen wurden nicht übernommen: {error}. Bereits geschriebene Dateien wurden zurückgesetzt."
                        ));
                        return;
                    }
                }
            }
        }

        let mut model_error = None;
        for preview in &previews {
            if let Err(error) = apply_preview_to_project(project, preview, state) {
                model_error = Some(error);
                break;
            }
        }
        if model_error.is_none() && !operations.is_empty() {
            let read_asset = |path: &str| -> Result<Vec<u8>, String> {
                self.workspace
                    .as_ref()
                    .ok_or_else(|| "Kein Projektordner ausgewählt.".to_string())?
                    .read_bytes(path)
                    .map_err(|error| error.to_string())
            };
            if let Err(error) = apply_operations_with(project, &operations, &read_asset) {
                model_error = Some(format!("Projektoperationen nicht angewendet: {error}"));
            }
        }
        if let Some(error) = model_error {
            *project = project_before;
            state.active_script = active_script_before;
            if let Some(workspace) = &self.workspace {
                for applied_change in applied.iter().rev() {
                    let _ = workspace.undo(applied_change);
                }
            }
            self.error = Some(error);
            return;
        }

        self.applied.extend(applied);
        state.dirty = true;
        self.session.pending_changes.clear();
        self.session.pending_operations.clear();
        self.pending_previews.clear();
        self.preview_errors.clear();
        self.operation_previews.clear();
        self.operation_errors.clear();
        self.output_tab = OutputTab::Activity;
        self.status = "Änderungen bestätigt und übernommen.".into();
        self.activity.push(ActivityLine {
            text: format!(
                "{} Dateiänderung(en) und {} Projektoperation(en) bestätigt.",
                previews.len(),
                operations.len()
            ),
            error: false,
        });
        state.log(
            LogLevel::Info,
            format!(
                "KI-Änderungen bestätigt: {} Datei(en), {} Projektoperation(en).",
                previews.len(),
                operations.len()
            ),
        );
        self.refresh_diagnostics(project);
        self.refresh_workspace();
    }

    fn reject_pending(&mut self, state: &mut AppState) {
        let count = self.session.pending_changes.len() + self.session.pending_operations.len();
        self.session.pending_changes.clear();
        self.session.pending_operations.clear();
        self.pending_previews.clear();
        self.preview_errors.clear();
        self.operation_previews.clear();
        self.operation_errors.clear();
        self.status = "Änderungen verworfen.".into();
        self.activity.push(ActivityLine {
            text: format!("{count} vorgeschlagene Änderung(en) verworfen."),
            error: false,
        });
        state.set_status("KI-Änderungen verworfen.");
    }

    fn undo_last_file(&mut self, state: &mut AppState) {
        let Some(applied) = self.applied.last().cloned() else {
            self.error = Some("Keine Dateiänderung zum Rückgängigmachen.".into());
            return;
        };
        let Some(workspace) = &self.workspace else {
            self.error = Some("Kein gültiger Projektordner ausgewählt.".into());
            return;
        };
        match workspace.undo(&applied) {
            Ok(()) => {
                self.applied.pop();
                self.status = format!("Datei '{}' wiederhergestellt.", applied.path);
                self.activity.push(ActivityLine {
                    text: format!(
                        "Datei-Rückgängig: {} (Projektmodell ggf. mit Strg+Z zurücknehmen)",
                        applied.path
                    ),
                    error: false,
                });
                state.dirty = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn run_build(&mut self, project: &Project, state: &mut AppState) {
        let report = build_project(project);
        self.build_report = Some(report.clone());
        self.refresh_diagnostics(project);
        self.output_tab = OutputTab::Build;
        if report.ok {
            self.status = report.summary.clone();
            self.error = None;
            self.activity.push(ActivityLine {
                text: report.summary.clone(),
                error: false,
            });
            state.log(LogLevel::Info, report.summary.clone());
        } else {
            self.status = report.summary.clone();
            self.error = Some(report.summary.clone());
            self.activity.push(ActivityLine {
                text: report.summary.clone(),
                error: true,
            });
            state.log(LogLevel::Error, report.summary.clone());
        }
        for warning in &report.warnings {
            self.activity.push(ActivityLine {
                text: format!("Build-Hinweis: {warning}"),
                error: false,
            });
        }
    }

    fn explorer(&mut self, ui: &mut Ui, project: &mut Project, state: &mut AppState) {
        ui.add_space(4.0);
        ui.label(
            RichText::new("PROJEKT & KONTEXT")
                .strong()
                .small()
                .color(theme::ACCENT),
        );
        ui.label(
            RichText::new("Arbeitsordner")
                .small()
                .color(theme::TEXT_DIM),
        );
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.workspace_root)
                    .desired_width(ui.available_width() - 58.0),
            );
            if ui.button("OK").clicked() {
                self.follow_project = false;
                self.refresh_workspace();
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("Projektordner folgen").clicked() {
                self.follow_project = true;
                self.last_project_path = None;
            }
            if ui.small_button("Aktualisieren").clicked() {
                self.refresh_workspace();
            }
        });
        ui.separator();
        ui.checkbox(
            &mut self.session.context.include_project_structure,
            "Projektstruktur",
        );
        ui.checkbox(
            &mut self.session.context.include_timeline,
            "Timeline/Objekte",
        );
        ui.checkbox(
            &mut self.session.context.include_script_metadata,
            "Skript-Metadaten",
        );
        ui.checkbox(
            &mut self.session.context.include_script_sources,
            "Quelltexte ausgewählter Skripte",
        );
        ui.checkbox(&mut self.session.context.include_assets, "Assets");
        ui.checkbox(
            &mut self.session.context.include_diagnostics,
            "Compiler-Diagnostik",
        );
        ui.label(
            RichText::new(format!(
                "Kontext: {} Skript(e), ca. {} Zeichen Quelltext",
                self.session.context.selected_count(),
                self.session.context.selected_source_chars(project)
            ))
            .small()
            .color(theme::TEXT_DIM),
        );
        if !self.editor_selection.is_empty() {
            ui.horizontal(|ui| {
                if ui.small_button("Editor-Auswahl als Kontext").clicked() {
                    self.session.context.selected_code = self.editor_selection.clone();
                }
                if ui.small_button("Auswahl löschen").clicked() {
                    self.session.context.selected_code.clear();
                }
            });
            ui.label(
                RichText::new(format!(
                    "Auswahl: {} Zeichen",
                    self.session.context.selected_code.chars().count()
                ))
                .small()
                .color(theme::TEXT_FAINT),
            );
        }
        ui.separator();
        ui.label(
            RichText::new("SKRIPTE")
                .strong()
                .small()
                .color(theme::ACCENT),
        );
        let scripts: Vec<(ScriptId, String, CodeProvenance, bool)> = project
            .scripts
            .iter()
            .map(|script| {
                (
                    script.id,
                    script.path.clone(),
                    script.provenance,
                    script.abc.is_some(),
                )
            })
            .collect();
        if scripts.is_empty() {
            ui.label(
                RichText::new("Keine Skripte im Projekt.")
                    .small()
                    .color(theme::TEXT_DIM),
            );
        }
        for (id, path, provenance, has_abc) in scripts {
            ui.horizontal(|ui| {
                let mut selected = self.session.context.selected_scripts.contains(&id);
                if ui.checkbox(&mut selected, "").changed() {
                    if selected {
                        self.session.context.selected_scripts.push(id);
                    } else {
                        self.session
                            .context
                            .selected_scripts
                            .retain(|value| *value != id);
                    }
                }
                let label = if has_abc {
                    format!("{} [ABC]", path)
                } else {
                    path.clone()
                };
                if ui
                    .selectable_label(self.selected_script == Some(id), label)
                    .clicked()
                {
                    self.selected_script = Some(id);
                    self.load_editor(project);
                }
                if provenance != CodeProvenance::OriginalSource {
                    ui.label(
                        RichText::new(provenance.label())
                            .small()
                            .color(theme::AMBER),
                    );
                }
            });
        }
        ui.separator();
        ui.label(
            RichText::new("DATEIEN IM ARBEITSORDNER")
                .strong()
                .small()
                .color(theme::ACCENT),
        );
        let project_paths: HashSet<String> = project
            .scripts
            .iter()
            .map(|script| script.path.clone())
            .collect();
        let files = self.workspace_files.clone();
        egui::ScrollArea::vertical()
            .id_salt("ai_workspace_files")
            .max_height(240.0)
            .show(ui, |ui| {
                if files.is_empty() {
                    ui.label(
                        RichText::new("Keine Quelltexte gefunden.")
                            .small()
                            .color(theme::TEXT_DIM),
                    );
                }
                for file in files {
                    let label = if project_paths.contains(&file) {
                        format!("{} [Projekt]", file)
                    } else {
                        file.clone()
                    };
                    if ui
                        .selectable_label(
                            self.editor_script.is_none() && self.editor_path == file,
                            label,
                        )
                        .clicked()
                    {
                        if let Some(script) =
                            project.scripts.iter().find(|script| script.path == file)
                        {
                            self.selected_script = Some(script.id);
                            self.load_editor(project);
                        } else {
                            self.selected_script = None;
                            self.editor_script = None;
                            self.editor_path = file.clone();
                            self.editor_buffer = self
                                .workspace
                                .as_ref()
                                .and_then(|workspace| workspace.read(&file).ok())
                                .unwrap_or_default();
                            self.last_synced_source = self.editor_buffer.clone();
                            self.editor_selection.clear();
                        }
                    }
                }
            });
        ui.separator();
        ui.label(
            RichText::new(format!(
                "Objektauswahl: {} · Frame {}",
                state.selection.object_ids().len(),
                state.frame + 1
            ))
            .small()
            .color(theme::TEXT_DIM),
        );
    }

    fn editor_panel(&mut self, ui: &mut Ui, project: &mut Project, state: &mut AppState) {
        if self.editor_path.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(
                        "Wähle links ein Skript oder eine Datei. Der Editor bleibt Teil des Projekts.",
                    )
                    .color(theme::TEXT_DIM),
                );
            });
            return;
        }
        let script = self
            .editor_script
            .and_then(|id| project.scripts.iter().find(|script| script.id == id))
            .map(|script| {
                (
                    script.id,
                    script.path.clone(),
                    script.source.clone(),
                    script.abc.is_some(),
                    script.provenance,
                )
            });
        let (read_only, provenance, has_abc) = match &script {
            Some((_, _, _, has_abc, provenance)) => (*has_abc, *provenance, *has_abc),
            None => (false, CodeProvenance::OriginalSource, false),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.editor_path).strong().color(theme::TEXT));
            ui.label(RichText::new(provenance.label()).small().color(
                if provenance == CodeProvenance::OriginalSource {
                    theme::GREEN
                } else {
                    theme::AMBER
                },
            ));
            if has_abc {
                ui.label(
                    RichText::new("PRESERVED ABC – Quelltext gesperrt")
                        .small()
                        .color(theme::AMBER),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!read_only, egui::Button::new("Datei speichern"))
                    .clicked()
                {
                    self.save_editor_file(project, state);
                }
                if ui.button("Projekt übernehmen").clicked() {
                    self.apply_editor_to_project(project, state);
                }
            });
        });
        if has_abc {
            ui.label(
                RichText::new(
                    "Der erhaltene AVM2-Bytecode wird nicht verändert. Eine Quelltextänderung ist hier bewusst gesperrt.",
                )
                .small()
                .color(theme::AMBER),
            );
        }
        ui.separator();
        let available = ui.available_size();
        let output = egui::ScrollArea::both()
            .id_salt("ai_code_editor")
            .show(ui, |ui| {
                let mut edit = TextEdit::multiline(&mut self.editor_buffer)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(available.x.max(400.0))
                    .desired_rows(24)
                    .lock_focus(true)
                    .interactive(!read_only);
                if read_only {
                    edit = edit.interactive(false);
                }
                edit.show(ui)
            });
        self.editor_selection = output
            .inner
            .cursor_range
            .map(|range| {
                let start: usize = range.primary.index.min(range.secondary.index).into();
                let end: usize = range.primary.index.max(range.secondary.index).into();
                self.editor_buffer
                    .chars()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .collect::<String>()
            })
            .unwrap_or_default();
        if !read_only
            && let Some((id, _, source, _, _)) = script
            && self.editor_buffer != source
        {
            if let Some(script) = project.scripts.iter_mut().find(|script| script.id == id) {
                script.source = self.editor_buffer.clone();
            }
            self.last_synced_source = self.editor_buffer.clone();
            state.dirty = true;
        }
        if !self.editor_selection.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Auswahl: {} Zeichen · als Kontext übernehmen",
                    self.editor_selection.chars().count()
                ))
                .small()
                .color(theme::TEXT_FAINT),
            );
        }
    }

    fn save_editor_file(&mut self, project: &mut Project, state: &mut AppState) {
        if let Some(script) = project
            .scripts
            .iter()
            .find(|script| self.editor_script == Some(script.id) || script.path == self.editor_path)
            && script.abc.is_some()
        {
            self.error = Some(
                    "Preserved ABC wird geschützt; die Datei darf nicht mit Quelltext überschrieben werden."
                        .into(),
                );
            return;
        }
        let Some(workspace) = &self.workspace else {
            self.error = Some("Kein gültiger Projektordner ausgewählt.".into());
            return;
        };
        let change =
            nova_ai::ProposedChange::inferred(self.editor_path.clone(), self.editor_buffer.clone());
        match workspace.apply(&change) {
            Ok(applied) => {
                self.applied.push(applied);
                self.status = format!("Datei '{}' gespeichert.", self.editor_path);
                self.activity.push(ActivityLine {
                    text: format!("Datei gespeichert: {}", self.editor_path),
                    error: false,
                });
                self.refresh_workspace();
                state.dirty = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn apply_editor_to_project(&mut self, project: &mut Project, state: &mut AppState) {
        if let Some(id) = self.editor_script
            && let Some(script) = project.scripts.iter_mut().find(|script| script.id == id)
        {
            if script.abc.is_some() {
                self.error =
                    Some("Preserved ABC kann nicht durch Quelltext ersetzt werden.".into());
                return;
            }
            script.source = self.editor_buffer.clone();
            self.last_synced_source = self.editor_buffer.clone();
            state.dirty = true;
            self.status = format!("Projektmodell für '{}' aktualisiert.", script.path);
            self.refresh_diagnostics(project);
            return;
        }
        if let Some(script) = project
            .scripts
            .iter_mut()
            .find(|script| script.path == self.editor_path)
        {
            if script.abc.is_some() {
                self.error =
                    Some("Preserved ABC kann nicht durch Quelltext ersetzt werden.".into());
                return;
            }
            let script_id = script.id;
            let script_path = script.path.clone();
            script.source = self.editor_buffer.clone();
            self.selected_script = Some(script_id);
            self.editor_script = Some(script_id);
            self.last_synced_source = self.editor_buffer.clone();
            state.dirty = true;
            self.status = format!("Projektmodell für '{script_path}' aktualisiert.");
            self.refresh_diagnostics(project);
            return;
        }
        let mut script = Script::new(self.editor_path.clone(), self.editor_buffer.clone());
        script.provenance = CodeProvenance::OriginalSource;
        self.selected_script = Some(script.id);
        self.editor_script = Some(script.id);
        self.last_synced_source = self.editor_buffer.clone();
        project.scripts.push(script);
        state.dirty = true;
        self.refresh_diagnostics(project);
        self.status = "Neues Skript in das Projekt übernommen.".into();
    }

    fn chat_panel(&mut self, ui: &mut Ui, project: &mut Project, state: &mut AppState) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("CHAT").strong().small().color(theme::ACCENT));
            if let Some(provider) = self
                .providers
                .selected(self.session.provider_override.as_deref())
            {
                let (label, color) = if provider.is_cloud() {
                    ("CLOUD", theme::AMBER)
                } else {
                    ("LOKAL", theme::GREEN)
                };
                ui.label(RichText::new(label).small().color(color));
                if provider.is_cloud() {
                    ui.label(
                        RichText::new("Kontext nur mit Zustimmung")
                            .small()
                            .color(theme::TEXT_FAINT),
                    );
                }
            } else {
                ui.label(
                    RichText::new("Kein Anbieter konfiguriert")
                        .small()
                        .color(theme::RED),
                );
            }
        });
        let messages = self.session.messages.clone();
        let available = (ui.available_height() - 150.0).max(120.0);
        egui::ScrollArea::vertical()
            .id_salt("ai_chat_scroll")
            .stick_to_bottom(true)
            .max_height(available)
            .show(ui, |ui| {
                if messages.is_empty() {
                    ui.label(
                        RichText::new(
                            "Der Agent sieht nur den Kontext, den du links auswählst. Änderungen erscheinen zuerst als Diff.",
                        )
                        .color(theme::TEXT_DIM),
                    );
                }
                for message in &messages {
                    let (role, color) = match message.role.as_str() {
                        "user" => ("Du", theme::BLUE),
                        "assistant" => ("Agent", theme::ACCENT),
                        _ => (message.role.as_str(), theme::TEXT_DIM),
                    };
                    ui.label(RichText::new(role).small().strong().color(color));
                    ui.label(RichText::new(&message.content).color(theme::TEXT));
                    ui.add_space(4.0);
                }
            });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Projekt erklären").clicked() {
                self.chat_input =
                    "Erkläre die Struktur dieses Projekts und nenne die wichtigsten Risiken.".into();
            }
            if ui.small_button("Build-Fehler erklären").clicked() {
                self.chat_input =
                    "Erkläre die aktuellen Compiler-/Build-Fehler und schlage konkrete Änderungen vor.".into();
            }
            if ui.small_button("Auswahl erklären").clicked() {
                self.chat_input =
                    "Erkläre den ausgewählten Code und ob er zum vorhandenen Projekt passt.".into();
            }
            if !self.session.context.selected_code.is_empty()
                && ui.small_button("Auswahl entfernen").clicked()
            {
                self.session.context.selected_code.clear();
            }
        });
        let input_height = 62.0;
        ui.add_sized(
            [ui.available_width(), input_height],
            TextEdit::multiline(&mut self.chat_input)
                .desired_rows(3)
                .hint_text("Aufgabe an den Agenten …"),
        );
        ui.horizontal(|ui| {
            let can_send = !self.busy && !self.chat_input.trim().is_empty();
            if ui
                .add_enabled(can_send, egui::Button::new("Senden"))
                .clicked()
            {
                self.start_request(project, state);
            }
            if ui
                .add_enabled(self.busy, egui::Button::new("Abbrechen"))
                .clicked()
            {
                self.job = None;
                self.busy = false;
                self.status = "Anfrage abgebrochen (Worker läuft ggf. noch aus).".into();
                self.activity.push(ActivityLine {
                    text: "Anfrage vom Benutzer abgebrochen.".into(),
                    error: false,
                });
            }
            if ui.button("Chat leeren").clicked() {
                self.session.clear();
                self.pending_previews.clear();
                self.preview_errors.clear();
                self.operation_previews.clear();
                self.operation_errors.clear();
                self.status = "Chat geleert.".into();
            }
        });
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).small().color(theme::RED));
        }
        let blocked = self.preview_errors.len() + self.operation_errors.len();
        if blocked > 0 {
            ui.label(
                RichText::new(format!("{blocked} blockierte Änderung(en) – siehe Diff."))
                    .small()
                    .color(theme::AMBER),
            );
        }
        if self.session.context.selected_code.chars().count() > 0 {
            ui.label(
                RichText::new(format!(
                    "Ausgewählter Code im Kontext: {} Zeichen",
                    self.session.context.selected_code.chars().count()
                ))
                .small()
                .color(theme::TEXT_FAINT),
            );
        }
    }

    fn output_panel(&mut self, ui: &mut Ui, project: &mut Project, state: &mut AppState) {
        ui.horizontal(|ui| {
            for (tab, label) in [
                (OutputTab::Build, "Build"),
                (OutputTab::Diagnostics, "Diagnose"),
                (OutputTab::Diff, "Diff"),
                (OutputTab::Activity, "Aktivität"),
            ] {
                if ui.selectable_label(self.output_tab == tab, label).clicked() {
                    self.output_tab = tab;
                }
            }
            ui.separator();
            if ui.button("Build ausführen").clicked() {
                self.run_build(project, state);
            }
            if ui
                .add_enabled(
                    !self.applied.is_empty(),
                    egui::Button::new("Datei-Rückgängig"),
                )
                .clicked()
            {
                self.undo_last_file(state);
            }
            if self.output_tab == OutputTab::Diff {
                ui.separator();
                if ui
                    .add_enabled(
                        (!self.pending_previews.is_empty()
                            || !self.session.pending_operations.is_empty())
                            && self.preview_errors.is_empty()
                            && self.operation_errors.is_empty(),
                        egui::Button::new("Alle bestätigen"),
                    )
                    .clicked()
                {
                    self.accept_pending(project, state);
                }
                if ui
                    .add_enabled(
                        !self.session.pending_changes.is_empty()
                            || !self.session.pending_operations.is_empty(),
                        egui::Button::new("Verwerfen"),
                    )
                    .clicked()
                {
                    self.reject_pending(state);
                }
            }
        });
        ui.separator();
        egui::ScrollArea::both()
            .id_salt("ai_output_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| match self.output_tab {
                OutputTab::Build => {
                    if let Some(report) = &self.build_report {
                        ui.label(
                            RichText::new(&report.summary).strong().color(if report.ok {
                                theme::GREEN
                            } else {
                                theme::RED
                            }),
                        );
                        ui.label(
                            RichText::new(report.text())
                                .monospace()
                                .small()
                                .color(theme::TEXT_DIM),
                        );
                    } else {
                        ui.label(
                            RichText::new("Noch kein Build in dieser Sitzung.")
                                .color(theme::TEXT_DIM),
                        );
                    }
                }
                OutputTab::Diagnostics => {
                    if self.diagnostics.is_empty() {
                        ui.label(
                            RichText::new("Keine Compiler-Diagnostik.")
                                .color(theme::TEXT_DIM),
                        );
                    }
                    for diagnostic in &self.diagnostics {
                        ui.label(
                            RichText::new(format!(
                                "{}:{}:{}  {}",
                                diagnostic.span.line,
                                diagnostic.span.column,
                                diagnostic.span.len(),
                                diagnostic.message
                            ))
                            .monospace()
                            .small()
                            .color(match diagnostic.severity {
                                Severity::Error => theme::RED,
                                Severity::Warning => theme::AMBER,
                            }),
                        );
                    }
                }
                OutputTab::Diff => {
                    if !self.preview_errors.is_empty() {
                        for error in &self.preview_errors {
                            ui.label(RichText::new(error).color(theme::RED));
                        }
                    }
                    if !self.operation_errors.is_empty() {
                        for error in &self.operation_errors {
                            ui.label(RichText::new(error).color(theme::RED));
                        }
                    }
                    if !self.operation_previews.is_empty() {
                        ui.label(
                            RichText::new("Projektoperationen (Dokumentmodell)")
                                .strong()
                                .color(theme::TEXT),
                        );
                        for summary in &self.operation_previews {
                            ui.label(
                                RichText::new(format!("• {summary}"))
                                    .small()
                                    .color(theme::TEXT_DIM),
                            );
                        }
                        ui.label(
                            RichText::new("Die Operationen werden transaktional angewendet und können mit Undo zurückgenommen werden.")
                                .small()
                                .color(theme::TEXT_FAINT),
                        );
                        ui.separator();
                    }
                    if self.pending_previews.is_empty() && self.operation_previews.is_empty() {
                        ui.label(
                            RichText::new("Keine Änderung zur Prüfung. Der Agent wendet nie automatisch etwas an.")
                                .color(theme::TEXT_DIM),
                        );
                    }
                    for preview in &self.pending_previews {
                        ui.label(
                            RichText::new(format!(
                                "{}  ·  {}",
                                preview.summary(),
                                if preview.reason.is_empty() {
                                    "ohne Begründung"
                                } else {
                                    preview.reason.as_str()
                                }
                            ))
                            .strong()
                            .color(theme::TEXT),
                        );
                        if preview.kind == ChangeKind::Delete {
                            ui.label(
                                RichText::new("Die Datei wird gelöscht; der vorherige Inhalt bleibt für Undo erhalten.")
                                    .small()
                                    .color(theme::AMBER),
                            );
                        }
                        for line in &preview.diff.lines {
                            let (prefix, color) = match line.kind {
                                DiffKind::Added => ("+", theme::GREEN),
                                DiffKind::Removed => ("-", theme::RED),
                                DiffKind::Context => (" ", theme::TEXT_FAINT),
                            };
                            ui.label(
                                RichText::new(format!("{prefix} {}", line.text))
                                    .monospace()
                                    .small()
                                    .color(color),
                            );
                        }
                        ui.separator();
                    }
                }
                OutputTab::Activity => {
                    if self.activity.is_empty() {
                        ui.label(
                            RichText::new("Noch keine Aktivität.")
                                .color(theme::TEXT_DIM),
                        );
                    }
                    for line in self.activity.iter().rev().take(200) {
                        ui.label(
                            RichText::new(&line.text)
                                .small()
                                .color(if line.error {
                                    theme::RED
                                } else {
                                    theme::TEXT_DIM
                                }),
                        );
                    }
                }
            });
    }
}

fn apply_preview_to_project(
    project: &mut Project,
    preview: &ChangePreview,
    state: &mut AppState,
) -> Result<(), String> {
    match preview.kind {
        ChangeKind::Delete => {
            let Some(index) = project
                .scripts
                .iter()
                .position(|script| script.path == preview.path)
            else {
                return Ok(());
            };
            if project.scripts[index].abc.is_some() {
                return Err(format!(
                    "{}: Preserved ABC darf nicht durch den Agenten gelöscht werden.",
                    preview.path
                ));
            }
            let id = project.scripts[index].id;
            project.scripts.remove(index);
            for scene in &mut project.scenes {
                for layer in &mut scene.layers {
                    for frame in &mut layer.frames {
                        for object in &mut frame.objects {
                            if object.script == Some(id) {
                                object.script = None;
                            }
                        }
                    }
                }
            }
            if state.active_script == Some(id) {
                state.active_script = project.scripts.first().map(|script| script.id);
            }
        }
        ChangeKind::Create | ChangeKind::Modify => {
            let content = preview.new_content.clone().unwrap_or_default();
            if let Some(script) = project
                .scripts
                .iter_mut()
                .find(|script| script.path == preview.path)
            {
                if script.abc.is_some() {
                    return Err(format!("{}: Preserved ABC wird geschützt.", preview.path));
                }
                script.source = content;
            } else if preview.path.ends_with(".as") {
                project
                    .scripts
                    .push(Script::new(preview.path.clone(), content));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_ai::{
        ContextSelection, DiffPreview, MemoryCredentialStore, ObjectShape, ProjectOperation,
        ProviderConfig, ProviderKind, ProviderSettings,
    };

    fn preview(kind: ChangeKind, path: &str, content: &str) -> ChangePreview {
        ChangePreview {
            path: path.to_string(),
            kind,
            exists: kind != ChangeKind::Create,
            old_content: (kind != ChangeKind::Create).then(|| "old\n".to_string()),
            new_content: (kind != ChangeKind::Delete).then(|| content.to_string()),
            diff: DiffPreview {
                added: 0,
                removed: 0,
                lines: Vec::new(),
            },
            reason: "test".into(),
        }
    }

    #[test]
    fn structured_operations_apply_without_a_workspace_and_clear_pending() {
        let mut project = Project::new_empty("Operationen");
        let mut state = AppState::new(&project);
        let mut editor = AiEditor::new();
        editor.session.pending_operations = vec![
            ProjectOperation::AddLayer {
                name: "Game".into(),
                scene: None,
                symbol: None,
                frames: Some(3),
            },
            ProjectOperation::AddObject {
                name: "Boden".into(),
                scene: None,
                symbol: None,
                layer: Some("Game".into()),
                shape: ObjectShape::Rect,
                x: 0.0,
                y: 300.0,
                width: 550.0,
                height: 80.0,
                fill: Some("#334455".into()),
                text: String::new(),
                font_size: None,
            },
        ];
        editor.prepare_previews(&project);
        assert!(
            editor.operation_errors.is_empty(),
            "{:?}",
            editor.operation_errors
        );
        assert_eq!(editor.operation_previews.len(), 2);
        editor.accept_pending(&mut project, &mut state);
        assert!(editor.error.is_none(), "{:?}", editor.error);
        assert!(editor.session.pending_operations.is_empty());
        let scene = project.edit_scene(state.edit_symbol).unwrap();
        let layer = scene
            .layers
            .iter()
            .find(|layer| layer.name == "Game")
            .expect("operation-created layer");
        assert_eq!(layer.frames.len(), 3);
        assert_eq!(layer.frames[0].objects[0].name, "Boden");
    }

    #[test]
    fn create_preview_adds_a_script_to_the_project() {
        let mut project = Project::demo();
        let mut state = AppState::new(&project);
        apply_preview_to_project(
            &mut project,
            &preview(ChangeKind::Create, "scripts/Added.as", "class Added {}\n"),
            &mut state,
        )
        .unwrap();
        let script = project
            .scripts
            .iter()
            .find(|script| script.path == "scripts/Added.as")
            .expect("created script");
        assert_eq!(script.source, "class Added {}\n");
    }

    #[test]
    fn modify_preview_updates_source_without_changing_identity() {
        let mut project = Project::demo();
        let mut state = AppState::new(&project);
        let id = project.scripts[0].id;
        let path = project.scripts[0].path.clone();
        apply_preview_to_project(
            &mut project,
            &preview(ChangeKind::Modify, &path, "class Changed {}\n"),
            &mut state,
        )
        .unwrap();
        assert_eq!(project.scripts[0].id, id);
        assert_eq!(project.scripts[0].source, "class Changed {}\n");
    }

    #[test]
    fn delete_preview_removes_script_and_clears_object_links() {
        let mut project = Project::demo();
        let mut state = AppState::new(&project);
        let id = project.scripts[0].id;
        let path = project.scripts[0].path.clone();
        let mut linked = false;
        for scene in &mut project.scenes {
            for layer in &mut scene.layers {
                for frame in &mut layer.frames {
                    for object in &mut frame.objects {
                        if !linked {
                            object.script = Some(id);
                            linked = true;
                        }
                    }
                }
            }
        }
        assert!(linked, "demo project has at least one object");
        apply_preview_to_project(
            &mut project,
            &preview(ChangeKind::Delete, &path, ""),
            &mut state,
        )
        .unwrap();
        assert!(!project.scripts.iter().any(|script| script.id == id));
        assert!(
            !project
                .scenes
                .iter()
                .flat_map(|scene| &scene.layers)
                .flat_map(|layer| &layer.frames)
                .flat_map(|frame| &frame.objects)
                .any(|object| object.script == Some(id))
        );
    }

    #[test]
    fn preserved_abc_is_rejected_for_modify_and_delete() {
        let mut project = Project::demo();
        let mut state = AppState::new(&project);
        let path = project.scripts[0].path.clone();
        project.scripts[0].abc = Some(vec![1, 2, 3]);
        let modify = apply_preview_to_project(
            &mut project,
            &preview(ChangeKind::Modify, &path, "class Replaced {}\n"),
            &mut state,
        );
        assert!(modify.is_err());
        let delete = apply_preview_to_project(
            &mut project,
            &preview(ChangeKind::Delete, &path, ""),
            &mut state,
        );
        assert!(delete.is_err());
    }

    #[test]
    #[ignore = "requires a running local Ollama instance and an installed model"]
    fn live_agent_workflow_reject_accept_build_and_undo() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let root =
            std::env::temp_dir().join(format!("nova-live-agent-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(root.join("scripts")).expect("create test workspace");
        let config_dir = root.join(".config");
        let mut project = Project::demo();
        for script in &project.scripts {
            let path = root.join(&script.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create script directory");
            }
            std::fs::write(&path, &script.source).expect("write script");
        }
        let original_main = project
            .scripts
            .iter()
            .find(|script| script.path.ends_with("Main.as"))
            .map(|script| script.source.clone())
            .expect("demo project has Main.as");

        let transport = Arc::new(ReqwestTransport::new().expect("HTTP client"));
        let credentials = Box::new(MemoryCredentialStore::new());
        let mut manager = ProviderManager::with_parts(
            ProviderSettings::default(),
            credentials,
            transport,
            &config_dir,
        );
        let model =
            std::env::var("NOVA_LIVE_OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5-coder:7b".into());
        let mut provider = ProviderConfig::new(ProviderKind::OllamaLocal);
        provider.name = "Ollama Live E2E".into();
        provider.model = model.clone();
        provider.temperature = Some(0.0);
        let provider_id = manager.add(provider).expect("add live provider");
        manager
            .set_default(&provider_id)
            .expect("select live provider");

        let mut editor = AiEditor::new();
        editor.providers = manager;
        editor.startup_error = None;
        editor.follow_project = false;
        editor.workspace_root = root.to_string_lossy().to_string();
        editor.refresh_workspace();
        editor.session.context = ContextSelection {
            include_project_structure: true,
            include_timeline: true,
            include_script_metadata: true,
            include_script_sources: true,
            include_assets: true,
            include_diagnostics: true,
            selected_scripts: project.scripts.iter().map(|script| script.id).collect(),
            selected_objects: Vec::new(),
            selected_code: String::new(),
            notes: String::new(),
        };
        let mut state = AppState::new(&project);

        let explanation = editor
            .session
            .request(
                &editor.providers,
                &project,
                0,
                "Code",
                &editor.diagnostics,
                "Erkläre in zwei Sätzen die Aufgabe von scripts/Main.as. \
                 Ändere keine Dateien und antworte auf Deutsch.",
            )
            .expect("live explanation");
        println!("LIVE UI agent explanation: {}", explanation.reply.message);
        assert!(!explanation.reply.message.trim().is_empty());
        assert!(!explanation.cloud);
        assert!(explanation.reply.changes.is_empty());

        let proposal = editor
            .session
            .request(
                &editor.providers,
                &project,
                0,
                "Code",
                &editor.diagnostics,
                "Erstelle eine neue ActionScript-3-Datei scripts/Added.as mit einer \
                 minimalen öffentlichen Klasse Added. Antworte ausschließlich mit dem \
                 vorgegebenen JSON-Objekt. changes muss genau einen Eintrag mit path \
                 scripts/Added.as, vollständigem content, reason und delete=false enthalten.",
            )
            .expect("live proposal");
        println!(
            "LIVE UI agent proposal: {} change(s): {:?}",
            proposal.reply.changes.len(),
            proposal
                .reply
                .changes
                .iter()
                .map(|change| change.path.as_str())
                .collect::<Vec<_>>()
        );
        let change = proposal
            .reply
            .changes
            .first()
            .cloned()
            .expect("model proposed a file change");
        assert!(!change.path.is_empty());
        let changed_path = root.join(&change.path);

        editor.session.pending_changes = vec![change.clone()];
        editor.prepare_previews(&project);
        assert!(
            editor.preview_errors.is_empty(),
            "preview errors: {:?}",
            editor.preview_errors
        );
        assert_eq!(editor.pending_previews.len(), 1);
        editor.reject_pending(&mut state);
        assert!(!changed_path.exists(), "rejected change was written");
        assert_eq!(project.scripts.len(), 1, "rejected change touched project");

        editor.session.pending_changes = vec![change];
        editor.prepare_previews(&project);
        editor.accept_pending(&mut project, &mut state);
        assert!(editor.error.is_none(), "accept error: {:?}", editor.error);
        assert!(changed_path.exists(), "accepted change was not written");
        let written = std::fs::read_to_string(&changed_path).expect("read accepted file");
        assert!(written.contains("class Added"));
        assert!(
            project
                .scripts
                .iter()
                .any(|script| script.path.ends_with("Added.as")),
            "accepted change did not update the project model"
        );
        assert_eq!(
            project
                .scripts
                .iter()
                .find(|script| script.path.ends_with("Main.as"))
                .map(|script| script.source.clone()),
            Some(original_main),
            "the original script was modified"
        );

        editor.run_build(&project, &mut state);
        let report = editor.build_report.as_ref().expect("build report");
        println!("LIVE UI agent build: {}", report.summary);
        assert!(report.ok, "build failed: {}", report.text());
        assert!(!editor.activity.is_empty());

        editor.undo_last_file(&mut state);
        assert!(!changed_path.exists(), "file undo did not remove the file");

        let _ = std::fs::remove_dir_all(&root);
    }
}
