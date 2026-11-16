# MEGA MILESTONE 28 — Final Report

**Native AI Tool Calling & Editor Integration** — branch `main`,
basis `138c19c` (post-M27). Version `0.18.0-rc1 → 0.19.0-rc1`.

## 1. Architecture changes

- **`crates/nova-ai/src/tools.rs`** (new, ~1.700 lines incl. tests) —
  provider-independent tool layer:
  - `ToolSpec` (name, description, JSON-schema `parameters`,
    `RiskClass`, write flag), `TOOL_SCHEMA_VERSION = 1`.
  - `ToolRegistry` — explicit registration only; `with_project_tools()`
    registers all 22 M28 tools.
  - `ToolCall` (unique id, name, arguments) / `ToolOutcome` (bounded
    JSON output, structured `ToolErrorCode`, duration) / `ToolRecord`
    persisted per task.
  - `ToolContext` — `GuardedWorkspace` (M25) + optional `ReviewSession`
    + lazily loaded `.nova` `Project` (via `nova-formats`).
- **`provider.rs`** — `chat_tools()` + `ToolChatResponse`: OpenAI-style
  `tools`/`tool_calls` for Ollama (`/api/chat`) and OpenAI-compatible
  (`/chat/completions`); Ollama `options` vs OpenAI `temperature`
  handled per kind; tool results serialized in provider-native shapes
  (`tool_name` for Ollama, `tool_call_id` for OpenAI).
- **`agent_loop.rs`** — each implementation iteration now runs a
  bounded multi-turn tool loop (≤8 rounds/iteration, ≤40 calls/task):
  native `tool_calls` executed via registry; text answers still use the
  M26 JSON protocol; providers rejecting `tools` fall back once; a
  validated JSON-envelope parser (`parse_tool_calls_from_text`) covers
  models that emit calls as text — free text is never executed.
  Sandbox re-syncs whenever staged state changes.
- **`tasks.rs`** — `Task.tool_records` history +
  `AgentLimits.max_tool_calls` (default 40).
- **`access.rs`** — `is_readable()` helper.

## 2. Tools (22)

Read-only: `project.get_info`, `get_tree`, `get_metadata`,
`search_files`, `read_file`, `get_file_diff`, `list_scenes`,
`get_scene`, `list_assets`, `get_asset_info`, `file.read`.
Staging: `file.create`, `file.update`, `file.apply_patch`,
`editor.script_write`, `editor.scene_create`, `editor.layer_create`,
`editor.object_create`, `editor.object_update`, `editor.keyframe_create`,
`editor.frame_label`, `editor.tween_create`, `editor.symbol_create`,
`editor.place_symbol`.
Destructive: `file.delete`, `file.rename` (stage `Delete` items that
default to unaccepted → explicit per-file confirmation in review).

## 3. Security model (M28.6)

- M25 grants + sensitive denylist apply to every read; writes require a
  `ReviewSession` (read-only grants fail with `permission_denied`).
- File tools only stage — originals are never written; `.nova` editor
  ops go through `apply_operations_with` (validated, transactional)
  and are staged as a whole-file change.
- `apply_patch` requires each `search` to match exactly once.
- Path resolution/traversal/symlink rules unchanged (GuardedWorkspace +
  ReviewSession + Sandbox).
- Limits: 40 tool calls/task, 8 rounds/iteration, 32 KB args, 12 KB
  output, existing iteration/time/file/command caps.
- No secrets beyond existing denylist coverage; tool outputs bounded.

## 4. Provider compatibility

| Provider | Native tools | Fallback |
|---|---|---|
| Ollama local/cloud | yes (`tools` in `/api/chat`) | JSON protocol / text-envelope parse |
| Groq / OpenAI-compatible | yes (`/chat/completions`) | same |

No faked support: if the endpoint rejects `tools`, the loop falls back
to the M26 JSON protocol with a logged notice.

## 5. Editor coverage — honest gaps

Implemented via existing `ProjectOperation`: scene create, layer
create, object create/update (transform), keyframe create, frame label,
tween create, symbol create, place symbol, `.as` script write.
**Open (no domain API — documented, not stubbed):** scene rename/
reorder/delete, object/frame removal, tween parameter editing, bone/IK
read+edit beyond existing ops, live-UI state changes (tools run
headless on files only).

## 6. Tests

- 10 new `tools::tests`: registry/schemas, unknown tool, denylist,
  staging without touching originals, destructive defaults, patch
  conflicts, no-review denial, provider-shape parsing, arg limits,
  `.nova` editor staging.
- `cargo test --workspace`: all PASS · `cargo fmt --check`: clean ·
  `cargo clippy --workspace --all-targets -D warnings`: clean.

## 7. Files changed

`nova-ai`: tools.rs (new), provider.rs, agent_loop.rs, tasks.rs,
access.rs, lib.rs. `nova-app`: ai_editor.rs (TOOLS history).
Docs: README, AGENTS, CHANGELOG, docs/AI_AGENT_EDITOR.md.
Version: Cargo.toml/Cargo.lock 0.19.0-rc1.

## 8. Commits

- `3cb7f25` — M28.1–28.6 backend: tools.rs, provider `chat_tools`,
  agent-loop integration, task records/limits
- `6850c74` — M28.7 UI: TOOLS history in task detail
- `9460371` — docs + version 0.19.0-rc1

## 9. Builds (0.19.0-rc1, SHA-256)

| Artifact | SHA-256 |
|---|---|
| user portable | `560A056AFB609116D9A30E1159B2E4423224E6D11E192A629A042A0A6E24EE57` |
| user setup | `B55D7C94F9DADB075B2BCBD806908064D6BD6730FD7574C53AC14FA64A581FCE` |
| user exe | `D15B147367E05E63F6CB569600390043C7075229EBC894658CC9C69B197486BD` |
| edu portable | `EC9836DCF8F7C7ECBAD237138E7A7E0FE185E45FB6E58D467960483628A4F762` |
| edu setup | `5EFE62888C075E8AB49851A268415A72C6E273BC66D6C4ED17D3699DB11BAFA4` |
| edu exe | `40ABA9CED7B3813F7E59CA966503978E2C71B130D24D832D1CB2B58FA839D6EF` |
| enterprise portable | `7BEC5C5FEB11E3C79C7B6C72D6C3F5F497E6DD28860B2B9F696081F2E077254F` |
| enterprise setup | `8C2E0787AEDF6105B159F948496CBCDBC881288DDA1256B7416CCB84C4570BE0` |
| enterprise exe | `E20C79B9C589E5739483E9F17E7F344F81054365AAE037A4674173D4EB6EBA8C` |

`test-edition-packaging.ps1` + `archive-releases.ps1`: PASS.
`test-installer.ps1` (all editions, headless install/verify/uninstall):
PASS.

## 10. Limitations / manual tests

- Tool calling requires a model that supports `tools`; fallback keeps
  M26 behavior. Interactive multi-provider verification is manual.
- `.nova` discovery scans the granted tree (skips target/dist/etc.).
- GUI: run a task that triggers tool calls → check TOOLS history,
  review staged `.nova`/file changes, sandbox retest.
