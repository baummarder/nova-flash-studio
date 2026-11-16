//! Native typed tool calling for the NØVA AI agent (M28).
//!
//! Tools are provider-independent: each [`ToolSpec`] declares name,
//! description, JSON-schema-ish argument contract and a [`RiskClass`].
//! Calls carry unique ids; arguments are validated before execution and
//! every outcome has a structured error code. There is **no arbitrary
//! code execution** — file tools stage into the M25 [`ReviewSession`],
//! editor tools go through the validated [`ProjectOperation`] pipeline
//! and are written back as a staged `.nova` file change.
//!
//! Risk classes (M28.6):
//! - `ReadOnly` — project/file inspection, never writes.
//! - `Staging` — prepares changes in the review session only.
//! - `Destructive` — deletes/renames; the staged `Delete` item defaults
//!   to *not accepted* in review, so the user must explicitly confirm
//!   each concrete action before apply.

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use nova_core::Project;
use nova_formats::import_bytes;

use crate::access::GuardedWorkspace;
use crate::files::ProposedChange;
use crate::operations::{MAX_OPERATIONS, ProjectOperation, apply_operations_with};
use crate::review::ReviewSession;

/// Tool schema version — bump on breaking argument changes.
pub const TOOL_SCHEMA_VERSION: u32 = 1;

/// Bound for a single tool result fed back to the model.
pub const MAX_TOOL_OUTPUT: usize = 12_000;
/// Bound for tool argument JSON.
pub const MAX_TOOL_ARGS: usize = 32_000;
/// Default max tool calls per task (also in [`crate::tasks::AgentLimits`]).
pub const DEFAULT_MAX_TOOL_CALLS: u32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    ReadOnly,
    Staging,
    Destructive,
}

impl RiskClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::Staging => "staging",
            Self::Destructive => "destructive",
        }
    }
}

/// Static tool description (registry entry).
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the arguments (`parameters`).
    pub parameters: Value,
    pub risk: RiskClass,
    /// True when the tool may stage file/project changes.
    pub writes: bool,
}

impl ToolSpec {
    /// OpenAI/Ollama-compatible function description.
    pub fn provider_json(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// One structured tool invocation from the model.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Structured tool error codes (M28.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolErrorCode {
    UnknownTool,
    InvalidArguments,
    PermissionDenied,
    NotFound,
    Conflict,
    LimitExceeded,
    Unsupported,
    Internal,
}

impl ToolErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnknownTool => "unknown_tool",
            Self::InvalidArguments => "invalid_arguments",
            Self::PermissionDenied => "permission_denied",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::LimitExceeded => "limit_exceeded",
            Self::Unsupported => "unsupported",
            Self::Internal => "internal",
        }
    }
}

/// Result of one tool execution (fed back to the model + task history).
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub call_id: String,
    pub name: String,
    pub risk: RiskClass,
    pub ok: bool,
    /// Bounded JSON/text payload returned to the model.
    pub output: String,
    pub error_code: Option<ToolErrorCode>,
    pub error: Option<String>,
    pub duration_ms: u128,
}

impl ToolOutcome {
    fn ok(call: &ToolCall, spec: &ToolSpec, output: Value, duration: Instant) -> Self {
        Self {
            call_id: call.id.clone(),
            name: call.name.clone(),
            risk: spec.risk,
            ok: true,
            output: truncate_json(&output),
            error_code: None,
            error: None,
            duration_ms: duration.elapsed().as_millis(),
        }
    }

    fn err(
        call: &ToolCall,
        spec: Option<&ToolSpec>,
        code: ToolErrorCode,
        message: impl Into<String>,
        duration: Instant,
    ) -> Self {
        Self {
            call_id: call.id.clone(),
            name: call.name.clone(),
            risk: spec.map(|s| s.risk).unwrap_or(RiskClass::ReadOnly),
            ok: false,
            output: String::new(),
            error_code: Some(code),
            error: Some(message.into()),
            duration_ms: duration.elapsed().as_millis(),
        }
    }

    /// Compact record persisted on the task (M28.7 tool history).
    pub fn record(&self) -> ToolRecord {
        ToolRecord {
            call_id: self.call_id.clone(),
            name: self.name.clone(),
            risk: self.risk.label().to_string(),
            ok: self.ok,
            duration_ms: self.duration_ms as u64,
            error: self.error.clone(),
        }
    }
}

/// Persisted per-task tool-call record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRecord {
    pub call_id: String,
    pub name: String,
    pub risk: String,
    pub ok: bool,
    pub duration_ms: u64,
    #[serde(default)]
    pub error: Option<String>,
}

/// Explicit tool registry — no implicit registrations.
pub struct ToolRegistry {
    tools: HashMap<String, ToolSpec>,
    pub max_calls_per_task: u32,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_project_tools()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            max_calls_per_task: DEFAULT_MAX_TOOL_CALLS,
        }
    }

    pub fn register(&mut self, spec: ToolSpec) {
        self.tools.insert(spec.name.clone(), spec);
    }

    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.get(name)
    }

    pub fn specs(&self) -> Vec<&ToolSpec> {
        let mut specs: Vec<&ToolSpec> = self.tools.values().collect();
        specs.sort_by(|a, b| a.name.cmp(&b.name));
        specs
    }

    /// Registry with all M28 tools (project, file, editor subset).
    pub fn with_project_tools() -> Self {
        let mut registry = Self::new();
        for spec in all_tool_specs() {
            registry.register(spec);
        }
        registry
    }
}

// ------------------------------------------------------------------ context

/// Everything a tool needs. File access goes through `guarded` (M25);
/// changes go through `review` (staging); the `.nova` model is loaded
/// lazily from the project file and written back as a staged file change.
pub struct ToolContext<'a> {
    pub guarded: &'a GuardedWorkspace,
    /// Mutable review session for staging tools (`None` = read-only mode;
    /// write tools then fail with `permission_denied`).
    pub review: Option<&'a mut ReviewSession>,
    /// Workspace-relative `.nova` project file used by editor tools.
    pub project_path: Option<String>,
    /// Lazily loaded project model for editor tools.
    project: Option<Project>,
    /// Staged `.nova` bytes accumulator is unnecessary — we serialize the
    /// whole project on every editor write.
    project_dirty: bool,
}

impl<'a> ToolContext<'a> {
    pub fn new(guarded: &'a GuardedWorkspace, review: Option<&'a mut ReviewSession>) -> Self {
        Self {
            guarded,
            review,
            project_path: None,
            project: None,
            project_dirty: false,
        }
    }

    /// Resolve the `.nova` project file: explicit path, first readable
    /// `.nova` under the workspace root (the source-file lister only
    /// covers text extensions, so `.nova` is found by a shallow scan),
    /// or `not_found`.
    fn project_rel(&mut self) -> Result<String, ToolOutcomeErr> {
        if let Some(path) = &self.project_path {
            return Ok(path.clone());
        }
        let mut found: Option<String> = None;
        let mut stack = vec![self.guarded.root().to_path_buf()];
        let mut depth_guard = 0usize;
        'outer: while let Some(dir) = stack.pop() {
            depth_guard += 1;
            if depth_guard > 4_000 {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.')
                    || matches!(name.as_str(), "target" | "node_modules" | "dist" | "build")
                {
                    continue;
                }
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !name.to_ascii_lowercase().ends_with(".nova") {
                    continue;
                }
                let Ok(rel) = path.strip_prefix(self.guarded.root()) else {
                    continue;
                };
                let rel = rel.to_string_lossy().replace('\\', "/");
                if self.guarded.is_readable(&rel) {
                    found = Some(rel);
                    break 'outer;
                }
            }
        }
        let nova = found.ok_or_else(|| {
            ToolOutcomeErr::new(
                ToolErrorCode::NotFound,
                "Kein lesbares .nova-Projekt im freigegebenen Bereich gefunden.",
            )
        })?;
        self.project_path = Some(nova.clone());
        Ok(nova)
    }

    /// Load (once) the project model from the workspace `.nova` file.
    fn project(&mut self) -> Result<&mut Project, ToolOutcomeErr> {
        if self.project.is_none() {
            let rel = self.project_rel()?;
            let bytes = self
                .guarded
                .read_bytes(&rel)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::PermissionDenied, e.to_string()))?;
            let imported = import_bytes(nova_formats::detect_format(&bytes), None, bytes)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::Internal, e.to_string()))?;
            self.project = Some(imported.project);
        }
        Ok(self.project.as_mut().unwrap())
    }

    /// Serialize the (possibly edited) project and stage it as a file
    /// change. Called after every successful editor tool.
    fn stage_project(&mut self) -> Result<(), ToolOutcomeErr> {
        let project = self.project.as_ref().ok_or_else(|| {
            ToolOutcomeErr::new(ToolErrorCode::Internal, "Projekt nicht geladen.")
        })?;
        let rel = self
            .project_path
            .clone()
            .ok_or_else(|| ToolOutcomeErr::new(ToolErrorCode::Internal, "Kein Projektpfad."))?;
        // Serialize via a temp file (nova::save is path-based).
        let tmp = std::env::temp_dir().join(format!(
            "nova-ai-tool-{}-{}.nova",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        nova_formats::save_nova(project, &tmp)
            .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::Internal, e.to_string()))?;
        let bytes = std::fs::read(&tmp)
            .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::Internal, e.to_string()))?;
        let _ = std::fs::remove_file(&tmp);
        let content = String::from_utf8(bytes)
            .map_err(|_| ToolOutcomeErr::new(ToolErrorCode::Internal, ".nova ist kein UTF-8."))?;
        let change = ProposedChange::modify(&rel, &content);
        let review = self.review.as_deref_mut().ok_or_else(|| {
            ToolOutcomeErr::new(ToolErrorCode::PermissionDenied, "Kein Review-Workspace.")
        })?;
        review
            .stage_or_update(self.guarded, &change)
            .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::Conflict, e.to_string()))?;
        self.project_dirty = true;
        Ok(())
    }
}

/// Internal error carrier so tool fns can `?` into `ToolOutcome::err`.
struct ToolOutcomeErr {
    code: ToolErrorCode,
    message: String,
}

impl ToolOutcomeErr {
    fn new(code: ToolErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

// ------------------------------------------------------------------ exec

/// Execute one validated call. Unknown tools and oversized args are
/// rejected before any work happens.
pub fn execute_tool(
    registry: &ToolRegistry,
    ctx: &mut ToolContext,
    call: &ToolCall,
) -> ToolOutcome {
    let started = Instant::now();
    let spec = match registry.get(&call.name) {
        Some(spec) => spec,
        None => {
            return ToolOutcome::err(
                call,
                None,
                ToolErrorCode::UnknownTool,
                format!(
                    "Unbekanntes Tool '{}' — in der Registry nicht registriert.",
                    call.name
                ),
                started,
            );
        }
    };
    let args_len = serde_json::to_string(&call.arguments)
        .map(|s| s.len())
        .unwrap_or(usize::MAX);
    if args_len > MAX_TOOL_ARGS {
        return ToolOutcome::err(
            call,
            Some(spec),
            ToolErrorCode::LimitExceeded,
            "Tool-Argumente überschreiten die Größenbegrenzung.",
            started,
        );
    }
    match dispatch(ctx, call, spec) {
        Ok(value) => ToolOutcome::ok(call, spec, value, started),
        Err(err) => ToolOutcome::err(call, Some(spec), err.code, err.message, started),
    }
}

fn need_write(ctx: &mut ToolContext, spec: &ToolSpec) -> Result<(), ToolOutcomeErr> {
    if spec.risk == RiskClass::ReadOnly {
        return Err(ToolOutcomeErr::new(
            ToolErrorCode::Unsupported,
            "Interner Fehler: Read-Tool versucht zu schreiben.",
        ));
    }
    if ctx.review.is_none() {
        return Err(ToolOutcomeErr::new(
            ToolErrorCode::PermissionDenied,
            "Keine Schreib-Freigabe (Review-Session fehlt).",
        ));
    }
    Ok(())
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolOutcomeErr> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ToolOutcomeErr::new(
                ToolErrorCode::InvalidArguments,
                format!("Pflichtargument '{key}' fehlt oder ist leer."),
            )
        })
}

fn arg_opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str()).map(str::trim)
}

fn arg_usize(args: &Value, key: &str) -> Result<usize, ToolOutcomeErr> {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| {
            ToolOutcomeErr::new(
                ToolErrorCode::InvalidArguments,
                format!("Pflichtargument '{key}' muss eine Zahl sein."),
            )
        })
}

fn arg_f32(args: &Value, key: &str) -> Result<f32, ToolOutcomeErr> {
    args.get(key)
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .ok_or_else(|| {
            ToolOutcomeErr::new(
                ToolErrorCode::InvalidArguments,
                format!("Pflichtargument '{key}' muss eine Zahl sein."),
            )
        })
}

fn dispatch(
    ctx: &mut ToolContext,
    call: &ToolCall,
    spec: &ToolSpec,
) -> Result<Value, ToolOutcomeErr> {
    let args = &call.arguments;
    match call.name.as_str() {
        // ------------------------------------------------ project reads
        "project.get_info" => {
            let files = ctx.guarded.list_readable_files();
            Ok(json!({
                "workspace_root": ctx.guarded.root().to_string_lossy(),
                "readable_files": files.len(),
                "project_file": ctx.project_path.clone(),
            }))
        }
        "project.get_tree" => {
            let depth: usize = args
                .get("depth")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(4)
                .min(10);
            let files = ctx.guarded.list_readable_files();
            let mut tree: HashMap<String, Vec<String>> = HashMap::new();
            for file in files.iter().take(500) {
                let parts: Vec<&str> = file.split('/').collect();
                if parts.len() > depth {
                    continue;
                }
                tree.entry(parts[..parts.len().saturating_sub(1)].join("/"))
                    .or_default()
                    .push(parts.last().unwrap_or(&"").to_string());
            }
            Ok(json!({"directories": tree, "truncated": files.len() > 500}))
        }
        "project.get_metadata" => {
            let project = ctx.project()?;
            let first = project.scenes.first();
            Ok(json!({
                "name": project.metadata.name,
                "description": project.metadata.description,
                "author": project.metadata.author,
                "stage_width": first.map(|s| s.width),
                "stage_height": first.map(|s| s.height),
                "fps": first.map(|s| s.fps),
                "scenes": project.scenes.len(),
                "symbols": project.symbols.len(),
                "assets": project.assets.len(),
                "scripts": project.scripts.len(),
            }))
        }
        "project.search_files" => {
            let query = arg_str(args, "query")?;
            let max = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(50)
                .min(200);
            let needle = query.to_lowercase();
            let mut hits = Vec::new();
            for file in ctx.guarded.list_readable_files() {
                if hits.len() >= max {
                    break;
                }
                let Ok(content) = ctx.guarded.read(&file) else {
                    continue;
                };
                for (idx, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&needle) {
                        hits.push(json!({
                            "file": file, "line": idx + 1,
                            "text": line.chars().take(200).collect::<String>(),
                        }));
                        if hits.len() >= max {
                            break;
                        }
                    }
                }
            }
            Ok(json!({"query": query, "results": hits, "count": hits.len()}))
        }
        "project.read_file" | "file.read" => {
            let path = arg_str(args, "path")?;
            let content = ctx
                .guarded
                .read(path)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::PermissionDenied, e.to_string()))?;
            let truncated = content.chars().count() > 20_000;
            let text: String = content.chars().take(20_000).collect();
            Ok(json!({"path": path, "content": text, "truncated": truncated}))
        }
        "project.get_file_diff" => {
            let path = arg_str(args, "path")?;
            let review = ctx.review.as_deref().ok_or_else(|| {
                ToolOutcomeErr::new(ToolErrorCode::PermissionDenied, "Kein Review-Workspace.")
            })?;
            if let Some(item) = review.items().iter().find(|i| i.path == path) {
                return Ok(json!({
                    "path": path,
                    "kind": format!("{:?}", item.kind),
                    "added": item.diff.added,
                    "removed": item.diff.removed,
                    "accepted": item.accepted,
                }));
            }
            Ok(json!({"path": path, "staged": false}))
        }
        "project.list_scenes" => {
            let project = ctx.project()?;
            Ok(json!({"scenes": project
                .scenes
                .iter()
                .map(|s| json!({
                    "name": s.name,
                    "layers": s.layers.len(),
                    "width": s.width,
                    "height": s.height,
                }))
                .collect::<Vec<_>>()}))
        }
        "project.get_scene" => {
            let name = arg_opt_str(args, "name");
            let project = ctx.project()?;
            let scene = name
                .and_then(|n| project.scenes.iter().find(|s| s.name == n))
                .or_else(|| project.scenes.first())
                .ok_or_else(|| {
                    ToolOutcomeErr::new(ToolErrorCode::NotFound, "Keine Szene gefunden.")
                })?;
            let objects: Vec<Value> = scene
                .layers
                .iter()
                .flat_map(|l| l.frames.iter())
                .flat_map(|f| f.objects.iter())
                .take(200)
                .map(|o| {
                    json!({
                        "name": o.name,
                        "kind": format!("{:?}", o.kind),
                        "x": o.transform.x,
                        "y": o.transform.y,
                    })
                })
                .collect();
            Ok(json!({
                "name": scene.name,
                "width": scene.width,
                "height": scene.height,
                "fps": scene.fps,
                "layers": scene.layers.iter().map(|l| json!({
                    "name": l.name, "kind": format!("{:?}", l.kind),
                    "frames": l.frames.len(),
                })).collect::<Vec<_>>(),
                "objects": objects,
            }))
        }
        "project.list_assets" => {
            let project = ctx.project()?;
            Ok(json!({"assets": project.assets.iter().map(|a| json!({
                "name": a.name, "kind": format!("{:?}", a.kind),
            })).collect::<Vec<_>>()}))
        }
        "project.get_asset_info" => {
            let name = arg_str(args, "name")?;
            let project = ctx.project()?;
            let asset = project
                .assets
                .iter()
                .find(|a| a.name == name)
                .ok_or_else(|| {
                    ToolOutcomeErr::new(
                        ToolErrorCode::NotFound,
                        format!("Asset '{name}' nicht gefunden."),
                    )
                })?;
            Ok(json!({
                "name": asset.name,
                "kind": format!("{:?}", asset.kind),
            }))
        }

        // ------------------------------------------------ file staging
        "file.create" => {
            need_write(ctx, spec)?;
            let path = arg_str(args, "path")?;
            let content = arg_str(args, "content")?;
            stage_change(ctx, ProposedChange::create(path, content))?;
            Ok(json!({"staged": "create", "path": path}))
        }
        "file.update" => {
            need_write(ctx, spec)?;
            let path = arg_str(args, "path")?;
            let content = arg_str(args, "content")?;
            stage_change(ctx, ProposedChange::modify(path, content))?;
            Ok(json!({"staged": "update", "path": path}))
        }
        "file.delete" => {
            // Destructive: staged delete defaults to unaccepted in review —
            // the user must confirm the concrete file in the diff view.
            need_write(ctx, spec)?;
            let path = arg_str(args, "path")?;
            stage_change(ctx, ProposedChange::delete(path))?;
            Ok(json!({
                "staged": "delete",
                "path": path,
                "note": "Löschen erfordert ausdrückliche Bestätigung im Review.",
            }))
        }
        "file.rename" => {
            need_write(ctx, spec)?;
            let from = arg_str(args, "from")?;
            let to = arg_str(args, "to")?;
            // Rename = create target + delete source, both staged.
            let content = ctx
                .guarded
                .read(from)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::PermissionDenied, e.to_string()))?;
            stage_change(ctx, ProposedChange::create(to, &content))?;
            stage_change(ctx, ProposedChange::delete(from))?;
            Ok(json!({"staged": "rename", "from": from, "to": to}))
        }
        "file.apply_patch" => {
            need_write(ctx, spec)?;
            let path = arg_str(args, "path")?;
            let hunks = args
                .get("hunks")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    ToolOutcomeErr::new(
                        ToolErrorCode::InvalidArguments,
                        "'hunks' muss ein Array von {search, replace} sein.",
                    )
                })?;
            let mut content = ctx
                .guarded
                .read(path)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::PermissionDenied, e.to_string()))?;
            let mut applied = 0usize;
            for hunk in hunks.iter().take(50) {
                let search = hunk.get("search").and_then(|v| v.as_str()).unwrap_or("");
                let replace = hunk.get("replace").and_then(|v| v.as_str()).unwrap_or("");
                if search.is_empty() {
                    return Err(ToolOutcomeErr::new(
                        ToolErrorCode::InvalidArguments,
                        "Patch-Hunk ohne 'search'-Text.",
                    ));
                }
                let occurrences = content.matches(search).count();
                if occurrences != 1 {
                    return Err(ToolOutcomeErr::new(
                        ToolErrorCode::Conflict,
                        format!(
                            "Patch-Suche in '{path}' ergab {occurrences} Treffer (genau 1 erwartet)."
                        ),
                    ));
                }
                content = content.replacen(search, replace, 1);
                applied += 1;
            }
            stage_change(ctx, ProposedChange::modify(path, &content))?;
            Ok(json!({"staged": "patch", "path": path, "hunks_applied": applied}))
        }

        // ------------------------------------------------ editor tools
        // Each maps to a validated ProjectOperation; the whole .nova file
        // is rewritten into staging afterwards.
        "editor.scene_create" => {
            let op = op_json("add_scene", json!({"name": arg_str(args, "name")?}));
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "add_scene"}))
        }
        "editor.layer_create" => {
            let mut body = json!({"op": "add_layer", "name": arg_str(args, "name")?});
            if let Some(v) = arg_opt_str(args, "scene") {
                body["scene"] = json!(v);
            }
            if let Some(v) = arg_opt_str(args, "symbol") {
                body["symbol"] = json!(v);
            }
            let op: ProjectOperation = serde_json::from_value(body)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::InvalidArguments, e.to_string()))?;
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "add_layer"}))
        }
        "editor.object_create" => {
            let mut body = json!({
                "op": "add_object",
                "name": arg_str(args, "name")?,
                "x": arg_f32(args, "x")?,
                "y": arg_f32(args, "y")?,
                "width": arg_f32(args, "width")?,
                "height": arg_f32(args, "height")?,
                "shape": arg_opt_str(args, "shape").unwrap_or("rect"),
                "text": arg_opt_str(args, "text").unwrap_or(""),
            });
            for key in ["scene", "symbol", "layer", "fill"] {
                if let Some(v) = arg_opt_str(args, key) {
                    body[key] = json!(v);
                }
            }
            let op: ProjectOperation = serde_json::from_value(body)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::InvalidArguments, e.to_string()))?;
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "add_object"}))
        }
        "editor.object_update" => {
            let name = arg_str(args, "name")?;
            let mut body = json!({"op": "update_object", "name": name});
            for key in [
                "scene", "layer", "fill", "text", "rotation", "scale_x", "scale_y", "x", "y",
            ] {
                if let Some(v) = args.get(key)
                    && !v.is_null()
                {
                    body[key] = v.clone();
                }
            }
            if body.as_object().map(|o| o.len()).unwrap_or(0) <= 2 {
                return Err(ToolOutcomeErr::new(
                    ToolErrorCode::InvalidArguments,
                    "Keine zu ändernden Felder angegeben.",
                ));
            }
            let op: ProjectOperation = serde_json::from_value(body)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::InvalidArguments, e.to_string()))?;
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "update_object"}))
        }
        "editor.keyframe_create" => {
            let mut body = json!({
                "op": "add_keyframe",
                "layer": arg_str(args, "layer")?,
                "frame": arg_usize(args, "frame")?,
            });
            for key in ["scene", "symbol", "label"] {
                if let Some(v) = arg_opt_str(args, key) {
                    body[key] = json!(v);
                }
            }
            if args.get("duplicate_previous").and_then(|v| v.as_bool()) == Some(true) {
                body["duplicate_previous"] = json!(true);
            }
            let op: ProjectOperation = serde_json::from_value(body)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::InvalidArguments, e.to_string()))?;
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "add_keyframe"}))
        }
        "editor.frame_label" => {
            let op = op_json(
                "set_frame_label",
                json!({
                    "layer": arg_str(args, "layer")?,
                    "frame": arg_usize(args, "frame")?,
                    "label": arg_str(args, "label")?,
                }),
            );
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "set_frame_label"}))
        }
        "editor.tween_create" => {
            let mut body = json!({
                "op": "add_tween",
                "layer": arg_str(args, "layer")?,
                "frame": arg_usize(args, "frame")?,
            });
            for key in ["scene", "symbol", "easing", "kind"] {
                if let Some(v) = arg_opt_str(args, key) {
                    body[key] = json!(v);
                }
            }
            let op: ProjectOperation = serde_json::from_value(body)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::InvalidArguments, e.to_string()))?;
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "add_tween"}))
        }
        "editor.symbol_create" => {
            let op = op_json(
                "add_symbol",
                json!({
                    "name": arg_str(args, "name")?,
                    "width": arg_f32(args, "width")?,
                    "height": arg_f32(args, "height")?,
                }),
            );
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "add_symbol"}))
        }
        "editor.place_symbol" => {
            let mut body = json!({
                "op": "place_symbol",
                "symbol": arg_str(args, "symbol")?,
                "name": arg_str(args, "name")?,
                "x": arg_f32(args, "x")?,
                "y": arg_f32(args, "y")?,
            });
            for key in ["scene", "symbol_target", "layer", "fill"] {
                if let Some(v) = arg_opt_str(args, key) {
                    body[key] = json!(v);
                }
            }
            let op: ProjectOperation = serde_json::from_value(body)
                .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::InvalidArguments, e.to_string()))?;
            editor_apply(ctx, &[op])?;
            Ok(json!({"applied": "place_symbol"}))
        }
        "editor.script_write" => {
            // .as files are workspace files — route through file staging.
            let path = arg_str(args, "path")?;
            if !path.to_ascii_lowercase().ends_with(".as") {
                return Err(ToolOutcomeErr::new(
                    ToolErrorCode::InvalidArguments,
                    "script_write akzeptiert nur .as-Pfade.",
                ));
            }
            need_write(ctx, spec)?;
            let content = arg_str(args, "content")?;
            stage_change(ctx, ProposedChange::modify(path, content))?;
            Ok(json!({"staged": "script", "path": path}))
        }

        _ => Err(ToolOutcomeErr::new(
            ToolErrorCode::UnknownTool,
            format!("Tool '{}' ist nicht implementiert.", call.name),
        )),
    }
}

fn op_json(op: &str, fields: Value) -> ProjectOperation {
    let mut body = fields;
    body["op"] = json!(op);
    serde_json::from_value(body).expect("static op json must deserialize")
}

fn stage_change(ctx: &mut ToolContext, change: ProposedChange) -> Result<(), ToolOutcomeErr> {
    let review = ctx.review.as_deref_mut().ok_or_else(|| {
        ToolOutcomeErr::new(ToolErrorCode::PermissionDenied, "Kein Review-Workspace.")
    })?;
    review
        .stage_or_update(ctx.guarded, &change)
        .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::Conflict, e.to_string()))
}

/// Apply validated project operations to the loaded model and stage the
/// resulting `.nova` file. Transactional via `apply_operations_with`.
fn editor_apply(ctx: &mut ToolContext, ops: &[ProjectOperation]) -> Result<(), ToolOutcomeErr> {
    if ops.len() > MAX_OPERATIONS {
        return Err(ToolOutcomeErr::new(
            ToolErrorCode::LimitExceeded,
            format!("Maximal {MAX_OPERATIONS} Operationen pro Tool-Aufruf."),
        ));
    }
    let guarded = ctx.guarded;
    {
        let project = ctx.project()?;
        apply_operations_with(project, ops, &|path| {
            guarded.read_bytes(path).map_err(|e| e.to_string())
        })
        .map_err(|e| ToolOutcomeErr::new(ToolErrorCode::InvalidArguments, e.to_string()))?;
    }
    ctx.stage_project()
}

fn truncate_json(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    if text.chars().count() <= MAX_TOOL_OUTPUT {
        return text;
    }
    let mut out: String = text.chars().take(MAX_TOOL_OUTPUT).collect();
    out.push_str("…[truncated]");
    out
}

// ------------------------------------------------------------------ specs

fn schema(props: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

fn s(name: &str, description: &str, parameters: Value, risk: RiskClass) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
        risk,
        writes: risk != RiskClass::ReadOnly,
    }
}

fn all_tool_specs() -> Vec<ToolSpec> {
    let str_prop = |desc: &str| json!({"type": "string", "description": desc});
    let num_prop = |desc: &str| json!({"type": "number", "description": desc});
    vec![
        s(
            "project.get_info",
            "Workspace-Informationen: root, Anzahl lesbarer Dateien, Projektdatei.",
            schema(json!({}), &[]),
            RiskClass::ReadOnly,
        ),
        s(
            "project.get_tree",
            "Dateibaum des freigegebenen Bereichs (Verzeichnisse → Dateinamen).",
            schema(
                json!({"depth": {"type": "integer", "description": "Max. Pfadtiefe (default 4, max 10)"}}),
                &[],
            ),
            RiskClass::ReadOnly,
        ),
        s(
            "project.get_metadata",
            "Projekt-Metadaten: Name, Beschreibung, Stage, Szenen-/Symbol-/Asset-Zahlen.",
            schema(json!({}), &[]),
            RiskClass::ReadOnly,
        ),
        s(
            "project.search_files",
            "Volltextsuche in allen lesbaren Projektdateien.",
            schema(
                json!({
                    "query": str_prop("Suchbegriff (case-insensitive)"),
                    "max_results": {"type": "integer", "description": "Trefferlimit (default 50, max 200)"},
                }),
                &["query"],
            ),
            RiskClass::ReadOnly,
        ),
        s(
            "project.read_file",
            "Liest eine freigegebene Projektdatei (max. 20000 Zeichen).",
            schema(
                json!({"path": str_prop("Projekt-relativer Pfad")}),
                &["path"],
            ),
            RiskClass::ReadOnly,
        ),
        s(
            "file.read",
            "Alias für project.read_file.",
            schema(
                json!({"path": str_prop("Projekt-relativer Pfad")}),
                &["path"],
            ),
            RiskClass::ReadOnly,
        ),
        s(
            "project.get_file_diff",
            "Diff-Status einer Datei im Review-Workspace (falls gestaged).",
            schema(
                json!({"path": str_prop("Projekt-relativer Pfad")}),
                &["path"],
            ),
            RiskClass::ReadOnly,
        ),
        s(
            "project.list_scenes",
            "Listet alle Szenen des Projekts.",
            schema(json!({}), &[]),
            RiskClass::ReadOnly,
        ),
        s(
            "project.get_scene",
            "Liest Szenen-Details: Layer, Frames, Objekte (optional per name).",
            schema(
                json!({"name": str_prop("Szenenname (default: erste Szene)")}),
                &[],
            ),
            RiskClass::ReadOnly,
        ),
        s(
            "project.list_assets",
            "Listet alle registrierten Assets.",
            schema(json!({}), &[]),
            RiskClass::ReadOnly,
        ),
        s(
            "project.get_asset_info",
            "Asset-Informationen per Name.",
            schema(json!({"name": str_prop("Asset-Name")}), &["name"]),
            RiskClass::ReadOnly,
        ),
        s(
            "file.create",
            "Erstellt eine neue Datei im Review-Workspace (Original bleibt unverändert).",
            schema(
                json!({
                    "path": str_prop("Projekt-relativer Zielpfad"),
                    "content": str_prop("Vollständiger Dateiinhalt"),
                }),
                &["path", "content"],
            ),
            RiskClass::Staging,
        ),
        s(
            "file.update",
            "Ersetzt den vollständigen Inhalt einer Datei im Review-Workspace.",
            schema(
                json!({
                    "path": str_prop("Projekt-relativer Pfad"),
                    "content": str_prop("Neuer vollständiger Inhalt"),
                }),
                &["path", "content"],
            ),
            RiskClass::Staging,
        ),
        s(
            "file.delete",
            "Löscht eine Datei (staged; erfordert Bestätigung im Review).",
            schema(
                json!({"path": str_prop("Projekt-relativer Pfad")}),
                &["path"],
            ),
            RiskClass::Destructive,
        ),
        s(
            "file.rename",
            "Benennt eine Datei um (staged als create+delete; Bestätigung im Review).",
            schema(
                json!({
                    "from": str_prop("Alter Pfad"),
                    "to": str_prop("Neuer Pfad"),
                }),
                &["from", "to"],
            ),
            RiskClass::Destructive,
        ),
        s(
            "file.apply_patch",
            "Wendet {search, replace}-Hunks auf eine Datei an (jeder search muss genau 1 Treffer haben).",
            schema(
                json!({
                    "path": str_prop("Projekt-relativer Pfad"),
                    "hunks": {"type": "array", "items": {"type": "object", "properties": {
                        "search": {"type": "string"}, "replace": {"type": "string"}}}},
                }),
                &["path", "hunks"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.scene_create",
            "Erstellt eine neue Szene im Projekt (.nova wird gestaged).",
            schema(json!({"name": str_prop("Szenenname")}), &["name"]),
            RiskClass::Staging,
        ),
        s(
            "editor.layer_create",
            "Erstellt eine Ebene in einer Szene oder einem Symbol.",
            schema(
                json!({
                    "name": str_prop("Ebenenname"),
                    "scene": str_prop("Szenenname (optional)"),
                    "symbol": str_prop("Symbolname (optional)"),
                }),
                &["name"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.object_create",
            "Erstellt ein Objekt (rect/ellipse/text) auf einer Timeline.",
            schema(
                json!({
                    "name": str_prop("Objektname"),
                    "x": num_prop("X-Position"), "y": num_prop("Y-Position"),
                    "width": num_prop("Breite"), "height": num_prop("Höhe"),
                    "shape": str_prop("rect|ellipse|text"),
                    "fill": str_prop("Füllfarbe #rrggbb (optional)"),
                    "text": str_prop("Textinhalt (optional)"),
                    "scene": str_prop("Szenenname (optional)"),
                    "symbol": str_prop("Symbolname (optional)"),
                    "layer": str_prop("Ebenenname (optional)"),
                }),
                &["name", "x", "y", "width", "height"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.object_update",
            "Aktualisiert Transformation/Eigenschaften eines Objekts.",
            schema(
                json!({
                    "name": str_prop("Objektname"),
                    "x": num_prop("X"), "y": num_prop("Y"),
                    "rotation": num_prop("Rotation"),
                    "scale_x": num_prop("Skalierung X"), "scale_y": num_prop("Skalierung Y"),
                    "fill": str_prop("Füllfarbe"), "text": str_prop("Text"),
                    "scene": str_prop("Szenenname"), "layer": str_prop("Ebenenname"),
                }),
                &["name"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.keyframe_create",
            "Erstellt einen Keyframe auf einer Ebene.",
            schema(
                json!({
                    "layer": str_prop("Ebenenname"),
                    "frame": {"type": "integer", "description": "1-basierte Framenummer"},
                    "scene": str_prop("Szenenname (optional)"),
                    "symbol": str_prop("Symbolname (optional)"),
                    "label": str_prop("Frame-Label (optional)"),
                    "duplicate_previous": {"type": "boolean", "description": "Vorherige Objekte kopieren"},
                }),
                &["layer", "frame"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.frame_label",
            "Setzt ein Frame-Label auf einer Ebene.",
            schema(
                json!({
                    "layer": str_prop("Ebenenname"),
                    "frame": {"type": "integer"},
                    "label": str_prop("Label-Text"),
                }),
                &["layer", "frame", "label"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.tween_create",
            "Erstellt einen Tween ab einem Keyframe.",
            schema(
                json!({
                    "layer": str_prop("Ebenenname"),
                    "frame": {"type": "integer"},
                    "scene": str_prop("Szenenname"), "symbol": str_prop("Symbolname"),
                    "easing": str_prop("Easing-Name (optional)"),
                    "kind": str_prop("Tween-Art (optional)"),
                }),
                &["layer", "frame"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.symbol_create",
            "Erstellt ein Bibliotheks-Symbol.",
            schema(
                json!({
                    "name": str_prop("Symbolname"),
                    "width": num_prop("Breite"), "height": num_prop("Höhe"),
                }),
                &["name", "width", "height"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.place_symbol",
            "Platziert eine Symbol-Instanz auf einer Timeline.",
            schema(
                json!({
                    "symbol": str_prop("Bibliotheks-Symbolname"),
                    "name": str_prop("Instanzname"),
                    "x": num_prop("X"), "y": num_prop("Y"),
                    "scene": str_prop("Szenenname"), "layer": str_prop("Ebenenname"),
                    "symbol_target": str_prop("Ziel-Symbol (verschachtelt)"),
                }),
                &["symbol", "name", "x", "y"],
            ),
            RiskClass::Staging,
        ),
        s(
            "editor.script_write",
            "Schreibt eine .as-Datei (staged wie file.update).",
            schema(
                json!({
                    "path": str_prop(".as-Pfad"),
                    "content": str_prop("AS3-Quelltext"),
                }),
                &["path", "content"],
            ),
            RiskClass::Staging,
        ),
    ]
}

// ------------------------------------------------------------------ parsing

/// Parse a provider tool-call message into validated [`ToolCall`]s.
/// Malformed entries are skipped (never executed).
pub fn parse_tool_calls(value: &Value) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let Some(list) = value
        .get("tool_calls")
        .or_else(|| value.pointer("/message/tool_calls"))
        .and_then(|v| v.as_array())
    else {
        return calls;
    };
    for (index, entry) in list.iter().enumerate() {
        let function = entry.get("function").unwrap_or(entry);
        let name = function
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let arguments = match function.get("arguments") {
            Some(Value::String(text)) => serde_json::from_str(text).unwrap_or(Value::Null),
            Some(v @ Value::Object(_)) => v.clone(),
            _ => Value::Null,
        };
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("call-{index}"));
        calls.push(ToolCall {
            id,
            name,
            arguments,
        });
    }
    calls
}

/// Fallback for providers/models without native tool calling: extract a
/// JSON tool-call envelope from plain text. Only *validated* JSON is
/// accepted — free text is never treated as executable.
pub fn parse_tool_calls_from_text(text: &str) -> Vec<ToolCall> {
    let trimmed = text.trim();
    let candidate = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|t| t.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    let Ok(value) = serde_json::from_str::<Value>(candidate) else {
        return Vec::new();
    };
    if let Some(list) = value.get("tool_calls").and_then(|v| v.as_array()) {
        let wrapped = json!({"tool_calls": list});
        return parse_tool_calls(&wrapped);
    }
    // Single-call shorthand: {"name": "...", "arguments": {...}}
    if let (Some(name), true) = (
        value.get("name").and_then(|v| v.as_str()),
        value.get("arguments").is_some(),
    ) {
        return vec![ToolCall {
            id: "call-0".to_string(),
            name: name.to_string(),
            arguments: value.get("arguments").cloned().unwrap_or(Value::Null),
        }];
    }
    Vec::new()
}

/// Serialize tool results for the next model turn (provider-specific
/// message shape handled by the caller).
pub fn tool_result_message(call: &ToolCall, outcome: &ToolOutcome, ollama: bool) -> Value {
    let content = if outcome.ok {
        outcome.output.clone()
    } else {
        format!(
            "{{\"error\":{{\"code\":\"{}\",\"message\":{}}}}}",
            outcome
                .error_code
                .as_ref()
                .map(|c| c.as_str())
                .unwrap_or("internal"),
            serde_json::to_string(outcome.error.as_deref().unwrap_or("Tool fehlgeschlagen."))
                .unwrap_or_default()
        )
    };
    if ollama {
        json!({
            "role": "tool",
            "content": content,
            "tool_name": call.name,
        })
    } else {
        json!({
            "role": "tool",
            "tool_call_id": call.id,
            "name": call.name,
            "content": content,
        })
    }
}

/// Assistant message echo for multi-turn tool loops.
pub fn assistant_tool_calls_message(content: &str, calls: &[ToolCall]) -> Value {
    json!({
        "role": "assistant",
        "content": content,
        "tool_calls": calls
            .iter()
            .map(|c| json!({
                "id": c.id,
                "type": "function",
                "function": {"name": c.name, "arguments": c.arguments},
            }))
            .collect::<Vec<_>>(),
    })
}

/// System prompt describing available tools (used by the compatibility
/// layer and as documentation for the model).
pub fn tools_prompt(registry: &ToolRegistry) -> String {
    let mut out = String::from(
        "Du kannst projektbezogene Werkzeugaufrufe (tool_calls) durchführen. \
         Verfügbare Werkzeuge:\n",
    );
    for spec in registry.specs() {
        out.push_str(&format!(
            "- {} ({}): {}\n",
            spec.name,
            spec.risk.label(),
            spec.description
        ));
    }
    out.push_str(
        "Antworte entweder mit normalem Text oder mit tool_calls. \
         Bei Destruktiven Aktionen (Löschen) staged du nur — der Nutzer \
         bestätigt jede Aktion im Review. Ändere niemals Originaldateien \
         direkt; alle Änderungen laufen über ReviewSession und Sandbox.",
    );
    out
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{AccessLevel, AccessPolicy, Grant};
    use crate::files::Workspace;
    use std::fs;

    fn fixture(name: &str) -> (std::path::PathBuf, std::path::PathBuf, GuardedWorkspace) {
        let base = std::env::temp_dir().join(format!("nova-tools-{name}-{}", uuid::Uuid::new_v4()));
        let root = base.join("project");
        let staging = base.join("staging");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(root.join("src/main.as"), "class Main {\n  int x;\n}\n").unwrap();
        fs::write(root.join(".env"), "SECRET=1\n").unwrap();
        let canonical = root.canonicalize().unwrap();
        let workspace = Workspace::new(&canonical).unwrap();
        let guarded = GuardedWorkspace::new(
            workspace,
            AccessPolicy {
                read_only: false,
                grants: vec![Grant {
                    path: canonical.clone(),
                    level: AccessLevel::ReadWrite,
                }],
            },
        );
        (canonical, staging, guarded)
    }

    fn call(name: &str, arguments: Value) -> ToolCall {
        ToolCall {
            id: format!("call-{name}"),
            name: name.to_string(),
            arguments,
        }
    }

    #[test]
    fn registry_lists_all_specs_with_schemas() {
        let registry = ToolRegistry::with_project_tools();
        let specs = registry.specs();
        assert!(specs.len() >= 20);
        for spec in &specs {
            assert!(spec.parameters.get("type").is_some(), "{}", spec.name);
        }
        assert!(registry.get("project.read_file").is_some());
        assert!(registry.get("file.delete").unwrap().risk == RiskClass::Destructive);
        assert!(registry.get("project.read_file").unwrap().risk == RiskClass::ReadOnly);
    }

    #[test]
    fn unknown_tool_returns_structured_error() {
        let (_r, staging, guarded) = fixture("unknown");
        let mut review = ReviewSession::new(&_r, &staging).unwrap();
        let registry = ToolRegistry::with_project_tools();
        let mut ctx = ToolContext::new(&guarded, Some(&mut review));
        let outcome = execute_tool(&registry, &mut ctx, &call("evil.exec", json!({})));
        assert!(!outcome.ok);
        assert_eq!(outcome.error_code, Some(ToolErrorCode::UnknownTool));
    }

    #[test]
    fn read_tools_respect_sensitive_denylist() {
        let (_r, staging, guarded) = fixture("denylist");
        let mut review = ReviewSession::new(&_r, &staging).unwrap();
        let registry = ToolRegistry::with_project_tools();
        let mut ctx = ToolContext::new(&guarded, Some(&mut review));
        let ok = execute_tool(
            &registry,
            &mut ctx,
            &call("project.read_file", json!({"path": "src/main.as"})),
        );
        assert!(ok.ok, "{:?}", ok.error);
        let denied = execute_tool(
            &registry,
            &mut ctx,
            &call("project.read_file", json!({"path": ".env"})),
        );
        assert!(!denied.ok);
        assert_eq!(denied.error_code, Some(ToolErrorCode::PermissionDenied));
    }

    #[test]
    fn file_tools_stage_without_touching_originals() {
        let (root, staging, guarded) = fixture("stage");
        let mut review = ReviewSession::new(&root, &staging).unwrap();
        let registry = ToolRegistry::with_project_tools();
        let mut ctx = ToolContext::new(&guarded, Some(&mut review));
        let created = execute_tool(
            &registry,
            &mut ctx,
            &call(
                "file.create",
                json!({"path": "src/new.as", "content": "class New {}\n"}),
            ),
        );
        assert!(created.ok, "{:?}", created.error);
        let updated = execute_tool(
            &registry,
            &mut ctx,
            &call(
                "file.update",
                json!({"path": "src/main.as", "content": "class Main { int y; }\n"}),
            ),
        );
        assert!(updated.ok, "{:?}", updated.error);
        drop(ctx);
        // Original untouched; review holds both staged changes.
        assert_eq!(
            fs::read_to_string(root.join("src/main.as")).unwrap(),
            "class Main {\n  int x;\n}\n"
        );
        assert!(!root.join("src/new.as").exists());
        assert_eq!(review.items().len(), 2);
    }

    #[test]
    fn destructive_delete_defaults_unaccepted_in_review() {
        let (root, staging, guarded) = fixture("delete");
        let mut review = ReviewSession::new(&root, &staging).unwrap();
        let registry = ToolRegistry::with_project_tools();
        let mut ctx = ToolContext::new(&guarded, Some(&mut review));
        let outcome = execute_tool(
            &registry,
            &mut ctx,
            &call("file.delete", json!({"path": "src/main.as"})),
        );
        assert!(outcome.ok, "{:?}", outcome.error);
        drop(ctx);
        let item = review
            .items()
            .iter()
            .find(|i| i.path == "src/main.as")
            .expect("delete staged");
        assert!(
            !item.accepted,
            "delete must require explicit review acceptance"
        );
    }

    #[test]
    fn apply_patch_requires_unique_search() {
        let (root, staging, _guarded) = fixture("patch");
        let mut review = ReviewSession::new(&root, &staging).unwrap();
        let registry = ToolRegistry::with_project_tools();
        fs::write(root.join("src/dup.as"), "foo foo\n").unwrap();
        // Workspace scanned at construction — recreate guarded to see it.
        let guarded = GuardedWorkspace::new(
            Workspace::new(&root).unwrap(),
            AccessPolicy {
                read_only: false,
                grants: vec![Grant {
                    path: root.clone(),
                    level: AccessLevel::ReadWrite,
                }],
            },
        );
        let mut ctx = ToolContext::new(&guarded, Some(&mut review));
        // two matches ? conflict
        let bad = execute_tool(
            &registry,
            &mut ctx,
            &call(
                "file.apply_patch",
                json!({"path": "src/dup.as", "hunks": [{"search": "foo", "replace": "bar"}]}),
            ),
        );
        assert!(!bad.ok);
        assert_eq!(bad.error_code, Some(ToolErrorCode::Conflict));
        // unique match ? ok
        let good = execute_tool(
            &registry,
            &mut ctx,
            &call(
                "file.apply_patch",
                json!({"path": "src/main.as", "hunks": [{"search": "int x;", "replace": "int y;"}]}),
            ),
        );
        assert!(good.ok, "{:?}", good.error);
        drop(ctx);
        let item = review
            .items()
            .iter()
            .find(|i| i.path == "src/main.as")
            .unwrap();
        let staged = item.staged_content().unwrap();
        assert!(staged.contains("int y;"));
    }

    #[test]
    fn write_tools_fail_without_review_session() {
        let (_r, _s, guarded) = fixture("noreview");
        let registry = ToolRegistry::with_project_tools();
        let mut ctx = ToolContext::new(&guarded, None);
        let outcome = execute_tool(
            &registry,
            &mut ctx,
            &call("file.create", json!({"path": "a.txt", "content": "x"})),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.error_code, Some(ToolErrorCode::PermissionDenied));
    }

    #[test]
    fn tool_call_parsing_handles_provider_shapes() {
        // OpenAI style
        let openai = json!({"tool_calls": [{
            "id": "c1", "type": "function",
            "function": {"name": "project.read_file", "arguments": "{\"path\":\"a.txt\"}"}
        }]});
        let calls = parse_tool_calls(&openai);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "project.read_file");
        assert_eq!(calls[0].arguments["path"], "a.txt");
        // Nested message.tool_calls (Ollama response shape)
        let ollama = json!({"message": {"tool_calls": [{
            "function": {"name": "file.delete", "arguments": {"path": "x"}}
        }]}});
        let calls = parse_tool_calls(&ollama);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "file.delete");
        // Text fallback: JSON envelope
        let text_calls = parse_tool_calls_from_text(
            "```json\n{\"tool_calls\":[{\"id\":\"t\",\"function\":{\"name\":\"project.get_info\",\"arguments\":{}}}]}\n```",
        );
        assert_eq!(text_calls.len(), 1);
        // Plain prose ? nothing (never executed)
        assert!(parse_tool_calls_from_text("Ich habe die Datei geändert.").is_empty());
    }

    #[test]
    fn oversized_arguments_rejected() {
        let (_r, staging, guarded) = fixture("bigargs");
        let mut review = ReviewSession::new(&_r, &staging).unwrap();
        let registry = ToolRegistry::with_project_tools();
        let mut ctx = ToolContext::new(&guarded, Some(&mut review));
        let huge = "x".repeat(MAX_TOOL_ARGS + 10);
        let outcome = execute_tool(
            &registry,
            &mut ctx,
            &call("file.create", json!({"path": "big.txt", "content": huge})),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.error_code, Some(ToolErrorCode::LimitExceeded));
    }

    #[test]
    fn editor_scene_create_stages_nova_file() {
        let (root, staging, _guarded) = fixture("editor");
        let project = Project::new_empty("ToolTest");
        project.save(&root.join("game.nova")).unwrap();
        // Recreate the guarded workspace so the file list includes game.nova.
        let guarded = GuardedWorkspace::new(
            Workspace::new(&root).unwrap(),
            AccessPolicy {
                read_only: false,
                grants: vec![Grant {
                    path: root.clone(),
                    level: AccessLevel::ReadWrite,
                }],
            },
        );
        let mut review = ReviewSession::new(&root, &staging).unwrap();
        let registry = ToolRegistry::with_project_tools();
        let mut ctx = ToolContext::new(&guarded, Some(&mut review));
        let outcome = execute_tool(
            &registry,
            &mut ctx,
            &call("editor.scene_create", json!({"name": "Level2"})),
        );
        assert!(outcome.ok, "{:?}", outcome.error);
        drop(ctx);
        let item = review
            .items()
            .iter()
            .find(|i| i.path == "game.nova")
            .expect(".nova staged");
        let staged = item.staged_content().unwrap();
        assert!(staged.contains("Level2"));
        // Original .nova unchanged on disk.
        let on_disk = fs::read_to_string(root.join("game.nova")).unwrap();
        assert!(!on_disk.contains("Level2"));
    }
}
