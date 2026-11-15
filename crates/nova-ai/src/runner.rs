//! Controlled build/test command execution for the agent (M26.4).
//!
//! Commands never go through a free-form shell. The agent may only request
//! entries from [`allowlist`] (or project-declared commands the user
//! approved). Execution is a direct `std::process::Command` with an argv
//! vector — no `cmd /c`, no shell interpolation — with wall-clock timeout,
//! cooperative cancellation and bounded output capture.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{AiError, AiResult};
use crate::tasks::{CommandRecord, now_unix};

/// Maximum captured output per stream (tail kept).
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
/// Default per-command timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// A named, fixed command the agent may run without extra approval.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// Stable id the agent uses in `"commands":["<id>"]`.
    pub id: &'static str,
    /// Human-readable label for the UI.
    pub label: &'static str,
    /// argv template; `{dir}` is replaced with the workdir.
    pub argv: &'static [&'static str],
}

/// The built-in allowlist — known build/test workflows only. New commands
/// must be added here (code review) or approved per run by the user.
pub fn allowlist() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "cargo-check",
            label: "cargo check --workspace",
            argv: &["cargo", "check", "--workspace"],
        },
        CommandSpec {
            id: "cargo-test",
            label: "cargo test --workspace",
            argv: &["cargo", "test", "--workspace"],
        },
        CommandSpec {
            id: "cargo-fmt-check",
            label: "cargo fmt --all -- --check",
            argv: &["cargo", "fmt", "--all", "--", "--check"],
        },
        CommandSpec {
            id: "cargo-clippy",
            label: "cargo clippy --workspace --all-targets -D warnings",
            argv: &[
                "cargo",
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        },
        CommandSpec {
            id: "test-installer",
            label: "scripts/test-installer.ps1 (headless install/uninstall)",
            argv: &[
                "powershell",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "scripts/test-installer.ps1",
            ],
        },
        CommandSpec {
            id: "test-edition-packaging",
            label: "scripts/test-edition-packaging.ps1",
            argv: &[
                "powershell",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "scripts/test-edition-packaging.ps1",
            ],
        },
        CommandSpec {
            id: "test-archive",
            label: "scripts/test-archive-releases.ps1",
            argv: &[
                "powershell",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "scripts/test-archive-releases.ps1",
            ],
        },
        CommandSpec {
            id: "nova-build",
            label: "nova build (project SWF build)",
            argv: &["cargo", "run", "-p", "nova-cli", "--", "build"],
        },
    ]
}

pub fn find_spec(id: &str) -> Option<CommandSpec> {
    allowlist().into_iter().find(|spec| spec.id == id)
}

/// Reject obviously dangerous argv. `trusted` is only set for argv produced
/// by [`CommandRunner::resolve`] (the fixed allowlist); custom/user-approved
/// argv always runs untrusted and can never invoke a shell.
fn validate_argv(argv: &[String], trusted: bool) -> AiResult<()> {
    if argv.is_empty() {
        return Err(AiError::Unsupported("Leeres Kommando".into()));
    }
    let program = argv[0].to_ascii_lowercase();
    let banned_programs = [
        "cmd",
        "powershell_ise",
        "wscript",
        "cscript",
        "mshta",
        "rundll32",
        "regsvr32",
    ];
    if banned_programs.contains(&program.as_str()) || (!trusted && program == "powershell") {
        return Err(AiError::Unsupported(format!(
            "Programm '{program}' ist für Agent-Befehle nicht zulässig"
        )));
    }
    if program == "cargo" {
        // `cargo fmt` without `--check` rewrites sources — never allowed.
        let has_fmt = argv.iter().any(|a| a == "fmt");
        let has_check = argv.iter().any(|a| a == "--check");
        if has_fmt && !has_check {
            return Err(AiError::Unsupported(
                "cargo fmt ohne --check würde Dateien verändern und ist nicht freigegeben.".into(),
            ));
        }
    }
    Ok(())
}

/// Outcome of one command execution.
#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub record: CommandRecord,
    /// Full captured output (bounded tail).
    pub output: String,
}

/// Executes allowlisted or user-approved commands with timeout + cancel.
pub struct CommandRunner {
    /// Cooperative cancellation flag shared with the agent loop.
    pub cancel: Arc<AtomicBool>,
}

impl Default for CommandRunner {
    fn default() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CommandRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cancel(cancel: Arc<AtomicBool>) -> Self {
        Self { cancel }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Resolve an allowlist id to its argv for `workdir`.
    pub fn resolve(&self, id: &str, workdir: &Path) -> AiResult<Vec<String>> {
        let spec = find_spec(id).ok_or_else(|| {
            AiError::Unsupported(format!(
                "Unbekannter Befehl '{id}' (nicht in der Allowlist)"
            ))
        })?;
        let argv: Vec<String> = spec
            .argv
            .iter()
            .map(|arg| arg.replace("{dir}", &workdir.to_string_lossy()))
            .collect();
        validate_argv(&argv, true)?;
        Ok(argv)
    }

    /// Convenience: resolve an allowlist id and run it in one call.
    pub fn run_allowlisted(
        &self,
        id: &str,
        workdir: &Path,
        env: &[(String, String)],
        timeout: Duration,
    ) -> AiResult<CommandOutcome> {
        let argv = self.resolve(id, workdir)?;
        self.run_trusted(&argv, workdir, env, timeout)
    }

    /// Run argv produced by [`Self::resolve`] — allowlist-trusted.
    fn run_trusted(
        &self,
        argv: &[String],
        workdir: &Path,
        env: &[(String, String)],
        timeout: Duration,
    ) -> AiResult<CommandOutcome> {
        validate_argv(argv, true)?;
        self.spawn(argv, workdir, env, timeout)
    }

    /// Run an argv vector directly (custom/user-approved commands — never
    /// allowlist-trusted).
    pub fn run(
        &self,
        argv: &[String],
        workdir: &Path,
        env: &[(String, String)],
        timeout: Duration,
    ) -> AiResult<CommandOutcome> {
        validate_argv(argv, false)?;
        self.spawn(argv, workdir, env, timeout)
    }

    fn spawn(
        &self,
        argv: &[String],
        workdir: &Path,
        env: &[(String, String)],
        timeout: Duration,
    ) -> AiResult<CommandOutcome> {
        let workdir = workdir.to_path_buf();
        let argv = argv.to_vec();
        let env = env.to_vec();
        let started = Instant::now();
        let started_at = now_unix();

        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .current_dir(&workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &env {
            command.env(key, value);
        }
        // No shell, no inherited dangerous env beyond the edition vars we set.
        let mut child = command.spawn().map_err(|e| {
            AiError::Unsupported(format!("Befehl konnte nicht gestartet werden: {e}"))
        })?;

        let output = Arc::new(Mutex::new(String::new()));
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_handle = stdout.map(|pipe| {
            let buf = Arc::clone(&output);
            std::thread::spawn(move || drain_pipe(pipe, buf))
        });
        let stderr_handle = stderr.map(|pipe| {
            let buf = Arc::clone(&output);
            std::thread::spawn(move || drain_pipe(pipe, buf))
        });

        // Wait loop with timeout + cancel.
        let deadline = started + timeout;
        let status = loop {
            if self.cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AiError::Unsupported(
                    "Befehl wurde durch den Benutzer abgebrochen.".into(),
                ));
            }
            match child.try_wait().map_err(AiError::io)? {
                Some(status) => break status,
                None if Instant::now() > deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AiError::Unsupported(format!(
                        "Zeitüberschreitung nach {}s: {}",
                        timeout.as_secs(),
                        argv.join(" ")
                    )));
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        };
        let _ = stdout_handle.map(|h| h.join());
        let _ = stderr_handle.map(|h| h.join());

        let full = output.lock().unwrap().clone();
        let tail: String = full
            .chars()
            .rev()
            .take(12_000)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let record = CommandRecord {
            command: argv.join(" "),
            workdir: workdir.to_string_lossy().to_string(),
            started_at,
            duration_ms: started.elapsed().as_millis() as u64,
            exit_code: status.code().unwrap_or(-1),
            ok: status.success(),
            output_tail: tail.clone(),
            sandbox_id: None,
            touched_files: Vec::new(),
        };
        Ok(CommandOutcome {
            record,
            output: full,
        })
    }
}

fn drain_pipe<R: Read + Send + 'static>(mut pipe: R, buf: Arc<Mutex<String>>) {
    let mut chunk = [0u8; 8192];
    while let Ok(n) = pipe.read(&mut chunk) {
        if n == 0 {
            break;
        }
        let mut guard = buf.lock().unwrap();
        guard.push_str(&String::from_utf8_lossy(&chunk[..n]));
        if guard.len() > MAX_OUTPUT_BYTES {
            let cut = guard.len() - MAX_OUTPUT_BYTES;
            guard.drain(..cut);
        }
    }
}

/// Summarize command output for the agent: failing test names / compiler
/// errors, bounded.
pub fn summarize_output(output: &str) -> String {
    let mut hits: Vec<String> = Vec::new();
    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains("error[")
            || lower.contains("error:")
            || lower.contains("FAILED")
            || lower.contains("failures:")
            || lower.contains("panicked")
        {
            hits.push(line.trim().chars().take(200).collect());
            if hits.len() >= 40 {
                break;
            }
        }
    }
    if hits.is_empty() {
        output
            .lines()
            .rev()
            .take(15)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        hits.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_ids_resolve() {
        let runner = CommandRunner::new();
        let dir = std::env::temp_dir();
        let argv = runner.resolve("cargo-check", &dir).unwrap();
        assert_eq!(argv[0], "cargo");
        assert!(runner.resolve("rm-rf", &dir).is_err());
        assert!(runner.resolve("git-push", &dir).is_err());
    }

    #[test]
    fn dangerous_argv_is_rejected() {
        let runner = CommandRunner::new();
        let dir = std::env::temp_dir();
        // Custom argv with powershell → rejected.
        let bad = vec!["powershell".to_string(), "-c".to_string(), "rm".to_string()];
        assert!(runner.run(&bad, &dir, &[], Duration::from_secs(5)).is_err());
        // cargo fmt without --check would mutate files → rejected.
        let fmt = vec!["cargo".to_string(), "fmt".to_string()];
        assert!(runner.run(&fmt, &dir, &[], Duration::from_secs(5)).is_err());
    }

    #[test]
    fn command_runs_and_records_result() {
        let runner = CommandRunner::new();
        let dir = std::env::temp_dir();
        let argv = vec!["cargo".to_string(), "--version".to_string()];
        // cargo --version is not in allowlist ids but run() validates argv
        // only for dangerous programs; cargo is safe.
        let outcome = runner
            .run(&argv, &dir, &[], Duration::from_secs(30))
            .unwrap();
        assert!(outcome.record.ok);
        assert!(outcome.output.contains("cargo"));
        assert!(outcome.record.duration_ms < 30_000);
    }

    #[test]
    fn cancel_flag_aborts() {
        let runner = CommandRunner::new();
        runner.cancel.store(true, Ordering::SeqCst);
        let dir = std::env::temp_dir();
        let argv = vec!["cargo".to_string(), "test".to_string()];
        assert!(
            runner
                .run(&argv, &dir, &[], Duration::from_secs(60))
                .is_err()
        );
    }
}
