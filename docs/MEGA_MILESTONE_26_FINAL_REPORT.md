# MEGA MILESTONE 26 — Final Report

**AI Agent Developer Workflow** — the AI Agent mode is now a bounded
development agent: natural-language tasks → structured plan → isolated
implementation with build/test self-correction → existing M25 review →
explicit user apply. Branch `main` · basis `c017a28` (post-M24/25).

## 1. Architektur-Audit (M26.0)

Reusable components identified and extended — **no parallel systems**:

| Component | Reuse |
|---|---|
| `GuardedWorkspace`/`AccessStore` (M25) | All agent file reads/writes — unchanged enforcement point |
| `ReviewSession` (M25) | Per-task isolated staging + apply/rollback; extended with `stage_or_update` for iterations |
| `parse_agent_reply` | Implementation replies reuse the proven JSON protocol (+ `commands`/`done`) |
| `ProviderManager`/`provider::chat` | Plan+implement calls; provider+key resolved on UI thread, worker uses `run_with_provider` |
| `Workspace::resolve`/atomic writes | Apply path untouched |
| Diff tab / accept_pending | Task review renders through the existing per-file accept engine |

New: `tasks.rs`, `runner.rs`, `agent_loop.rs`. Risks documented: staged
changes are not compiled into the project before apply (commands run
against the current project state — honest limitation); provider JSON
protocol is prompt-based, not tool-calling.

## 2. Implemented functions

- **Task Manager (M26.1):** `Task` with id/title/description/provider/
  status/progress/plan/commands/events; 10 statuses; create, plan,
  approve, implement, cancel, rerun, history remove; persisted per
  project in `ai-tasks.json` (edition config dir, never in `.nova`).
- **Planning Engine (M26.2):** structured `TaskPlan` (goal, analysis,
  affected/new files, steps, effects, tests, risks); approval gate —
  plans touching files stop at *Planfreigabe nötig*; read-only analysis
  completes without approval.
- **Execution Engine (M26.3):** per-task staging dir; iterations with
  `stage_or_update`; self-correction via command output feedback;
  limits (iterations, seconds, changed files, workspace bytes, command
  count) pause with visible reason; cooperative cancel.
- **Build & Test (M26.4):** allowlist-only `CommandRunner` — fixed argv
  vectors (cargo check/test/fmt-check/clippy, three ps1 suites,
  nova build), no shell, timeout, kill-on-cancel, bounded output,
  full `CommandRecord` (command, workdir, start, duration, exit,
  output tail). Unknown commands rejected.
- **Self-correction (M26.5):** failing build/test output is summarized
  and fed into the next iteration; agent cannot remove tests (prompt
  rule + no test-deletion command exists); limits prevent fake loops.
- **Review & Apply (M26.6):** staged `ReviewSession` flows into the
  existing Diff tab — per-file accept/reject, conflicts, grant
  re-check, rollback; task marked Completed on successful apply.
- **History (M26.7):** `ai-tasks.json` per project — plan, events,
  command records, timestamps; `sanitize_for_history` redacts
  credential-shaped strings; rerun/remove supported.
- **Provider integration (M26.8):** per-task provider via session
  override; cloud consent enforced before any context leaves;
  provider never touches the filesystem (all FS via GuardedWorkspace).
- **UI (M26.9):** new left **AUFGABEN** panel — task list, NL input,
  Plan erstellen / freigeben & implementieren / Erneut / Stopp,
  detail view (plan, steps, risks, command records, activity),
  Agent-Limits collapsible. Classic NØVA styling kept.
- **Settings (M26.10):** `ai-agent.json` per edition — max iterations,
  runtime, changed files, commands, history toggle.

## 3. Changed/new files

- `crates/nova-ai/src/tasks.rs` (new) — Task/TaskPlan/AgentLimits/TaskStore
- `crates/nova-ai/src/runner.rs` (new) — allowlist CommandRunner
- `crates/nova-ai/src/agent_loop.rs` (new) — plan+implement engine
- `crates/nova-ai/src/review.rs` — `stage_or_update`
- `crates/nova-ai/src/lib.rs` — module exports
- `crates/nova-app/src/ai_editor.rs` — tasks panel, worker, limits UI,
  poll/merge, completion hook
- `docs/AI_AGENT_EDITOR.md` — M26 sections

## 4. Security architecture

Unchanged M25 boundaries + new layers: plan approval gate (no file
staging before user approval), allowlist argv (no shell), iteration/
runtime/file/command caps, cooperative cancel, secret scrubbing in
history, cloud consent before context egress. Agent can never click
apply — review decisions are UI-only.

## 5. Test results

- `cargo test --workspace`: **all PASS, 0 failed** (incl. 8 new:
  task persistence/lifecycle, plan approval gate, runner
  allowlist/argv-reject/cancel/record, plan+impl parsers, **full e2e**
  plan→approve→implement→staged→apply via MockTransport, iteration-limit
  pause preserving staged work, prompt cancel)
- `cargo fmt --all -- --check`: clean · `cargo clippy --workspace
  --all-targets -- -D warnings`: clean
- `test-installer.ps1` all editions: PASS · `test-edition-packaging` +
  `test-archive-releases`: PASS (archive refreshed with `-Force`)

Incident note: a PowerShell text roundtrip briefly mojibake'd
`ai_editor.rs`; detected via umlaut-count diff vs HEAD and fully
repaired (143 segments) — verified by compile + tests + diff review.

## 6. Builds (0.17.0-rc1, SHA-256)

| Artifact | SHA-256 |
|---|---|
| user portable | `9FE4F97781AAA8FE40E43E48499667B737B2DBF5E45F67EF91DD9582F9CD0012` |
| user setup | `D10228F7CAA4A3CD90CB3E901F4F5B6859529D7A228B6D53714F19F10E8C2F57` |
| user exe | `E5FBC9904BF37BB1112DEDDAC9C906FBDFD3AB864F2C1A4F66907B123B0BF997` |
| edu portable | `0DDC37C08DA462FE491BCACB0A6BE5468ACE8948FA5E2481C989BD95637F8CB1` |
| edu setup | `272813D138490DA2EEE82EF65D2D3FEB0B966E80262B34866184DCD6BDC5EA56` |
| edu exe | `7C70317CE739F4CBBEDC5435DA6FCCB2BC5F7F3533471C54A89DE84E432FC038` |
| enterprise portable | `168FEF64008B2349F10C716F14ED66E074725ED2D972CB727AA196DEC4020A8E` |
| enterprise setup | `280405E30264E119272431B5A056779CADAFF8994913F4C6948644D4DD8002B5` |
| enterprise exe | `0EF993B649CEB6A864B0CEDEA709433725D5BDA2254934A26660F692A17CC154` |

## 7. Known limitations

- Commands execute in the **project working dir**, i.e. against the
  current (unmodified) sources — staged changes are verified by review,
  not by a pre-apply compile of the staged tree. A sandbox-copy build
  is a future option.
- Provider protocol is prompt-based JSON (not native tool-calling);
  small models may need iteration to emit valid JSON — parser is
  tolerant and falls back to text.
- Plan re-runs on approve (fresh analysis) — costs one extra request.
- GUI flows (task panel, dialogs) verified by compile + backend tests;
  interactive click-through remains a manual check.

## 8. Manual GUI tests still required

1. Open AI Agent → AUFGABEN panel → enter task → Plan erstellen →
   review plan → freigeben → watch iterations/commands → Diff review →
   accept.
2. Cancel mid-run; rerun a failed task; verify history persists after
   restart; limits dialog values persist.

## 9. Commits

- `acaedcd` — M26 implementation
- `af9b754` — M26 docs (this report)
