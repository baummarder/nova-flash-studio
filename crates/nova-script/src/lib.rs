//! QuickJS-based scripting host for editor tools and extensions.
//!
//! The host evaluates JavaScript in a persistent context, captures
//! `console.log`/`info`/`warn`/`error` output, and exposes a small `nova`
//! object. It is deliberately **not** the ActionScript engine: AS3 sources are
//! parsed by `nova-as` and are never executed as JavaScript.
//!
//! ```no_run
//! use nova_script::ScriptEngine;
//!
//! let mut engine = ScriptEngine::new().unwrap();
//! let output = engine.eval("console.log('Hallo'); 6 * 7").unwrap();
//! assert_eq!(output.value, "42");
//! assert_eq!(output.logs, vec!["Hallo".to_string()]);
//! ```

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rquickjs::{CaughtError, Context, Ctx, Runtime, Type, Value};

/// Default wall-clock budget for a single evaluation.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
/// Default heap limit for the script runtime.
pub const DEFAULT_MEMORY_LIMIT: usize = 64 * 1024 * 1024;

/// Everything a successful evaluation produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptOutput {
    /// The completion value of the script, formatted for humans.
    pub value: String,
    /// Lines written through `console.*` during this evaluation.
    pub logs: Vec<String>,
}

impl ScriptOutput {
    /// Renders logs and the completion value as one console block.
    pub fn render(&self) -> String {
        let mut text = self.logs.join("\n");
        if !self.value.is_empty() && self.value != "undefined" {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&self.value);
        }
        text
    }
}

/// Errors raised while setting up or evaluating a script.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("QuickJS konnte nicht initialisiert werden: {0}")]
    Init(String),

    #[error("{message}")]
    Js {
        message: String,
        stack: Option<String>,
        /// Console output produced before the exception was raised.
        logs: Vec<String>,
    },

    #[error("Skript überschritt das Zeitlimit von {limit:?} (Endlosschleife?)")]
    Timeout { limit: Duration, logs: Vec<String> },

    #[error("Skriptfehler: {0}")]
    Engine(String),
}

impl ScriptError {
    /// Console output captured before the failure, if any.
    pub fn logs(&self) -> &[String] {
        match self {
            Self::Js { logs, .. } | Self::Timeout { logs, .. } => logs,
            Self::Init(_) | Self::Engine(_) => &[],
        }
    }

    /// `true` when the evaluation was aborted by the wall-clock guard.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }
}

/// A QuickJS runtime plus one global context.
///
/// The context persists between calls, so a tool can define helper functions
/// once and reuse them. `eval` resets only the console log buffer.
pub struct ScriptEngine {
    context: Context,
    deadline: Arc<Mutex<Option<Instant>>>,
    timeout: Duration,
}

impl ScriptEngine {
    /// Creates an engine with the default timeout and memory limit.
    pub fn new() -> Result<Self, ScriptError> {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    /// Creates an engine with a custom wall-clock budget per evaluation.
    pub fn with_timeout(timeout: Duration) -> Result<Self, ScriptError> {
        let runtime = Runtime::new().map_err(|error| ScriptError::Init(error.to_string()))?;
        runtime.set_memory_limit(DEFAULT_MEMORY_LIMIT);
        runtime.set_max_stack_size(1024 * 1024);

        let deadline: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let handler_deadline = Arc::clone(&deadline);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            let guard = handler_deadline.lock().expect("deadline poisoned");
            guard.is_some_and(|deadline| Instant::now() >= deadline)
        })));

        let context =
            Context::full(&runtime).map_err(|error| ScriptError::Init(error.to_string()))?;

        let engine = Self {
            context,
            deadline,
            timeout,
        };
        engine.install_prelude()?;
        Ok(engine)
    }

    /// The configured wall-clock budget.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Replaces the wall-clock budget for future evaluations.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Evaluates JavaScript in the persistent global context.
    pub fn eval(&mut self, source: &str) -> Result<ScriptOutput, ScriptError> {
        let timeout = self.timeout;
        self.arm_deadline();
        let result = self.context.with(|ctx| {
            // Keep the console buffer scoped to this evaluation.
            let _ = ctx.eval::<Value, _>("globalThis.__nova_log = [];");
            match ctx.eval::<Value, _>(source.as_bytes()) {
                Ok(value) => {
                    let text = format_value(&ctx, &value);
                    Ok(ScriptOutput {
                        value: text,
                        logs: take_logs(&ctx),
                    })
                }
                Err(error) => {
                    let logs = take_logs(&ctx);
                    Err(classify(&ctx, error, timeout, logs))
                }
            }
        });
        self.disarm_deadline();
        result
    }

    /// Evaluates a `.js` file from disk.
    pub fn eval_file(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<ScriptOutput, ScriptError> {
        let source = std::fs::read_to_string(path.as_ref()).map_err(|error| {
            ScriptError::Engine(format!("{}: {error}", path.as_ref().display()))
        })?;
        self.eval(&source)
    }

    /// Makes the current project visible to scripts as `nova.project`.
    ///
    /// `project_json` must be valid JSON; it is embedded as a string literal
    /// and parsed by the runtime, so arbitrary user text is safe.
    pub fn set_project_context(&mut self, project_json: &str) -> Result<(), ScriptError> {
        let literal = json_string_literal(project_json);
        let source = format!("globalThis.nova.project = JSON.parse({literal});");
        self.eval(&source)?;
        Ok(())
    }

    fn install_prelude(&self) -> Result<(), ScriptError> {
        let prelude = r#"
(function () {
  globalThis.__nova_log = [];

  const fmt = (value) => {
    if (typeof value === 'string') return value;
    if (typeof value === 'undefined') return 'undefined';
    if (value === null) return 'null';
    if (typeof value === 'symbol' || typeof value === 'function') return String(value);
    try {
      const json = JSON.stringify(value);
      return json === undefined ? String(value) : json;
    } catch (_error) {
      return String(value);
    }
  };

  globalThis.__nova_fmt = fmt;
  globalThis.console = {
    log: (...args) => { __nova_log.push(args.map(fmt).join(' ')); },
    info: (...args) => { __nova_log.push(args.map(fmt).join(' ')); },
    warn: (...args) => { __nova_log.push('[warn] ' + args.map(fmt).join(' ')); },
    error: (...args) => { __nova_log.push('[error] ' + args.map(fmt).join(' ')); },
  };

  globalThis.nova = {
    version: '__NOVA_VERSION__',
    log: (...args) => { __nova_log.push(args.map(fmt).join(' ')); },
    project: null,
  };
})();
"#
        .replace("__NOVA_VERSION__", env!("CARGO_PKG_VERSION"));

        self.context
            .with(|ctx| ctx.eval::<(), _>(prelude.as_bytes()))
            .map_err(|error| ScriptError::Init(error.to_string()))?;
        Ok(())
    }

    fn arm_deadline(&self) {
        let mut guard = self.deadline.lock().expect("deadline poisoned");
        *guard = Some(Instant::now() + self.timeout);
    }

    fn disarm_deadline(&self) {
        let mut guard = self.deadline.lock().expect("deadline poisoned");
        *guard = None;
    }
}

impl std::fmt::Debug for ScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEngine")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// Runs `__nova_fmt` on a JavaScript value; falls back to the engine's own
/// string conversion if the helper was overwritten by user code.
fn format_value<'js>(ctx: &Ctx<'js>, value: &Value<'js>) -> String {
    if value.is_undefined() {
        return "undefined".to_string();
    }
    if value.is_null() {
        return "null".to_string();
    }
    if value.type_of() == Type::String {
        return value.get::<String>().unwrap_or_default();
    }

    let formatted = ctx
        .globals()
        .get::<_, rquickjs::Function>("__nova_fmt")
        .and_then(|formatter| formatter.call::<_, String>((value.clone(),)));

    match formatted {
        Ok(text) => text,
        Err(_) => {
            let _ = ctx.catch();
            String::new()
        }
    }
}

fn take_logs(ctx: &Ctx<'_>) -> Vec<String> {
    let joined: String = ctx
        .eval("globalThis.__nova_log ? __nova_log.join('\\n') : ''")
        .unwrap_or_default();
    if joined.is_empty() {
        return Vec::new();
    }
    joined.split('\n').map(str::to_string).collect()
}

fn classify<'js>(
    ctx: &Ctx<'js>,
    error: rquickjs::Error,
    limit: Duration,
    logs: Vec<String>,
) -> ScriptError {
    match CaughtError::from_error(ctx, error) {
        CaughtError::Exception(exception) => {
            let message = exception
                .message()
                .unwrap_or_else(|| "Unbekannter JavaScript-Fehler".to_string());
            if message.contains("interrupted") {
                return ScriptError::Timeout { limit, logs };
            }
            ScriptError::Js {
                message,
                stack: exception.stack(),
                logs,
            }
        }
        CaughtError::Value(value) => {
            let message = format_value(ctx, &value);
            ScriptError::Js {
                message,
                stack: None,
                logs,
            }
        }
        CaughtError::Error(error) => {
            let text = error.to_string();
            if text.contains("interrupted") {
                ScriptError::Timeout { limit, logs }
            } else {
                ScriptError::Engine(text)
            }
        }
    }
}

/// Encodes arbitrary text as a JSON/JavaScript string literal.
fn json_string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_expression() {
        let mut engine = ScriptEngine::new().unwrap();
        let output = engine.eval("6 * 7").unwrap();
        assert_eq!(output.value, "42");
        assert!(output.logs.is_empty());
    }

    #[test]
    fn formats_objects_as_json() {
        let mut engine = ScriptEngine::new().unwrap();
        let output = engine
            .eval("({ name: 'Nova', frames: [1, 2, 3] })")
            .unwrap();
        assert_eq!(output.value, r#"{"name":"Nova","frames":[1,2,3]}"#);
    }

    #[test]
    fn captures_console_output() {
        let mut engine = ScriptEngine::new().unwrap();
        let output = engine
            .eval("console.log('frame', 3); console.warn('check'); 1")
            .unwrap();
        assert_eq!(
            output.logs,
            vec!["frame 3".to_string(), "[warn] check".to_string()]
        );
        assert_eq!(output.render(), "frame 3\n[warn] check\n1");
    }

    #[test]
    fn log_buffer_is_per_evaluation() {
        let mut engine = ScriptEngine::new().unwrap();
        engine.eval("console.log('first')").unwrap();
        let second = engine.eval("console.log('second')").unwrap();
        assert_eq!(second.logs, vec!["second".to_string()]);
    }

    #[test]
    fn context_persists_between_calls() {
        let mut engine = ScriptEngine::new().unwrap();
        engine.eval("globalThis.counter = 41;").unwrap();
        let output = engine.eval("counter + 1").unwrap();
        assert_eq!(output.value, "42");
    }

    #[test]
    fn reports_syntax_errors() {
        let mut engine = ScriptEngine::new().unwrap();
        let error = engine.eval("let = ;").unwrap_err();
        match error {
            ScriptError::Js { message, logs, .. } => {
                assert!(!message.is_empty(), "message should not be empty");
                assert!(logs.is_empty());
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn runtime_errors_keep_console_output() {
        let mut engine = ScriptEngine::new().unwrap();
        let error = engine
            .eval("console.log('before'); throw new Error('kaputt')")
            .unwrap_err();
        match error {
            ScriptError::Js { message, logs, .. } => {
                assert_eq!(message, "kaputt");
                assert_eq!(logs, vec!["before".to_string()]);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn reports_thrown_values() {
        let mut engine = ScriptEngine::new().unwrap();
        let error = engine.eval("throw 'boom'").unwrap_err();
        match error {
            ScriptError::Js { message, .. } => assert_eq!(message, "boom"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn exposes_nova_api() {
        let mut engine = ScriptEngine::new().unwrap();
        let output = engine.eval("nova.version").unwrap();
        assert_eq!(output.value, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn project_context_round_trips_json() {
        let mut engine = ScriptEngine::new().unwrap();
        engine
            .set_project_context(r#"{"name":"Demo „Nova“","frames":48}"#)
            .unwrap();
        let output = engine
            .eval("nova.project.name + ':' + nova.project.frames")
            .unwrap();
        assert_eq!(output.value, "Demo „Nova“:48");
    }

    #[test]
    fn interrupt_guard_stops_infinite_loops() {
        let mut engine = ScriptEngine::with_timeout(Duration::from_millis(100)).unwrap();
        let started = Instant::now();
        let error = engine.eval("while (true) {}").unwrap_err();
        assert!(error.is_timeout(), "expected timeout, got {error:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn json_literal_escapes_special_characters() {
        assert_eq!(json_string_literal("a\"b\\c\nd"), r#""a\"b\\c\nd""#);
        let mut engine = ScriptEngine::new().unwrap();
        engine.set_project_context(r#""a\"b\\c\nd""#).unwrap();
        assert_eq!(engine.eval("nova.project").unwrap().value, "a\"b\\c\nd");
    }

    #[test]
    fn timeout_recovers_for_later_calls() {
        let mut engine = ScriptEngine::with_timeout(Duration::from_millis(100)).unwrap();
        assert!(engine.eval("while (true) {}").is_err());
        engine.set_timeout(Duration::from_secs(2));
        assert_eq!(engine.eval("1 + 1").unwrap().value, "2");
    }
}
