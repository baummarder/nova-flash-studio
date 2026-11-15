# AI Agent Editor

The AI Agent Editor is an integrated workspace in NØVA Flash Studio, not a
second editor and not a separate project model. It uses the same open project,
the same compiler/exporter and the same undo history as the normal Code and
Animation modes.

## Opening the workspace

- Toolbar: **AI Agent**
- Menu: **KI → AI Agent Editor**
- Keyboard: `Ctrl+Shift+A`

The toolbar and menu are available in both Code and Animation modes. The AI
workspace temporarily gives the central area to its own resizable panels. The
normal project tree, properties panel, timeline and central editor keep their
state and return unchanged when you close the workspace with **Zurück zum
Editor**.

## Layout

```text
┌──────────────────────────────────────────────────────────────────────┐
│ AI AGENT EDITOR  [Zurück] [Anbieter] [Build] [Diagnose]  provider ▾ │
├───────────────┬──────────────────────────────┬───────────────────────┤
│ Projekt &     │ Source editor                │ Chat                  │
│ Kontext       │ (same project scripts,       │ (provider, messages,  │
│               │  provenance and ABC guard)   │  quick prompts)       │
│ scripts       │                              │                       │
│ files         │                              │                       │
├───────────────┴──────────────────────────────┴───────────────────────┤
│ Build | Diagnose | Diff | Aktivität                                  │
└──────────────────────────────────────────────────────────────────────┘
```

The left explorer, right chat and bottom output panels are resizable; the
central source editor fills the remaining space. The bottom output area can be
collapsed by dragging its splitter.

## Project explorer and context selection

The left panel lists:

- project scripts with provenance and an `[ABC]` marker for preserved bytecode;
- scenes, layers, frames and object counts through the project structure
  context option;
- editable source-like files under the configured working folder.

Only checked scripts contribute **source text** to the context. Script metadata
is separate and can be included without source. The panel shows the approximate
source-character count before sending.

Other context switches:

- project structure;
- timeline/object placements;
- script metadata;
- assets;
- compiler diagnostics;
- selected editor code;
- selected stage objects (their IDs are included when the timeline context is
  enabled).

The working folder is project-relative by default. **Projektordner folgen**
restores automatic tracking; **OK** sets an explicit folder. File operations are
confined to this folder.

## Source editor

The central editor edits the selected project script in place. It shows the
script path, provenance label and an `PRESERVED ABC – Quelltext gesperrt` guard.

- **Projekt übernehmen** updates the project model. A new `.as` file can be
  added to the project this way.
- **Datei speichern** writes the editor buffer through the same safe workspace
  used by agent changes.
- If a script has preserved AVM2 ABC, source editing is disabled. The exporter
  must keep embedding the preserved bytecode; NØVA will not silently replace it
  with reconstructed text.
- A selection in the editor can be attached as context with **Editor-Auswahl
  als Kontext**.

The normal Code mode editor remains the primary syntax-highlighted editor. The
AI workspace editor is intentionally a focused source surface with the same
project state.

## Chat and agent requests

The right panel shows the conversation, the active provider, its model and a
`LOKAL`/`CLOUD` badge.

- **Senden** prepares the context and request on the UI thread, then performs
  the network call on a worker thread. The UI stays responsive and shows a
  spinner and elapsed time.
- **Abbrechen** detaches the current result. The worker may finish in the
  background, but its result is ignored.
- **Chat leeren** clears conversation and pending changes.
- Quick prompts cover project explanation, build errors and selected code.

The system prompt requires the model to answer with a JSON envelope:

```json
{
  "message": "Explanation for the user",
  "changes": [
    {
      "path": "scripts/Main.as",
      "content": "complete new file content",
      "reason": "short reason",
      "delete": false
    }
  ],
  "actions": ["build"]
}
```

If the model returns ordinary prose, NØVA displays it as a message and proposes
no changes. Unknown actions such as shell commands are ignored.

## Diff review and safe application

Every proposed change is previewed before anything is written.

- **Diff** shows added/removed/context lines and the reason.
- **Alle bestätigen** applies the accepted subset. Per-file checkboxes in the
  Diff tab decide which changes are taken; deletes start unchecked and need an
  explicit opt-in. Blocked items (conflicts, revoked grants) stay listed.
- **Verwerfen** discards all pending proposals and the staging area.
- **Datei-Rückgängig** restores the previous content of the last applied file.
  Project-model undo remains available through the normal `Ctrl+Z` history.

## Project access grants (M25)

The agent has **no implicit filesystem access**. On the first visit per
project the permission prompt offers:

- **A** — grant individual files/folders via the native picker
  (read + propose changes);
- **B** — grant the whole project tree (read + propose changes);
- **C** — read-only access to the project tree.
- **Später entscheiden** — no file access; chat still works.

Grants are stored per project root in `ai-access.json` inside the edition's
AI config directory (`%APPDATA%\<edition>`). They never migrate to another
project. **Berechtigungen…** (explorer section) lists every grant, allows
per-grant revoke, a project-wide read-only cap, and full revocation.

Even inside a read-write grant, sensitive paths are hard-denied (shown in
the dialog): `.env*`, `*.pem/.key/.p12/.pfx/.keystore/.kdbx`, `id_rsa*`,
`id_ed25519*`, `credentials`, `.netrc`, `.git-credentials`, `.ssh/`,
`.aws/`, `.gnupg/`, `.git/`.

## Review workspace (M25)

Agent file proposals are staged in an isolated directory (never written to
the project directly). Each staged change snapshots the original bytes;
the Diff tab shows per-file diffs with accept checkboxes. On apply:

1. the current original is re-read and compared to the snapshot — a
   mismatch blocks that file as a **conflict** (never overwritten);
2. write permission is re-checked immediately before writing;
3. writes are atomic (temp file + replace);
4. a mid-apply failure rolls back everything already written.

Safety rules:

1. Paths are project-root confined. Absolute paths, drive prefixes, `..`
   traversal and symlink escapes are rejected.
2. Every read/write additionally requires an explicit grant covering the
   path; grants are per project and revocable at any time.
3. A creation never silently becomes an overwrite. A modification of a missing
   file is rejected.
4. Writes use a same-directory temporary file and replace step. On failure,
   previously written files in the same accepted batch are rolled back.
5. Deletions are explicit, previewed, unchecked by default and kept in the
   undo record.
6. Preserved ABC scripts cannot be modified or deleted through the agent.
7. Project model updates are applied only after file writes succeed; on a model
   error the project snapshot is restored and written files are rolled back.
8. The agent cannot run shell commands, arbitrary processes, or access files
   outside granted paths. Grants cannot be extended by agent output or by
   project files.

## Build, diagnostics and activity

- **Build** runs the existing `nova-build` pipeline and shows byte/frame/object
  statistics, warnings and failures.
- **Diagnose** runs the existing AS3 analyzer for all non-preserved scripts.
- **Diff** is the confirmation surface.
- **Aktivität** records requests, responses, blocked changes, confirmed writes
  and build results.

A model may request a build through `"actions":["build"]`; this is a local build,
not a remote tool. Builds never apply file changes.

## Provider selection and privacy

The header selector supports a per-session provider override without changing
the saved default. Cloud providers require the **Cloud-Zustimmung** checkbox
before any request. See [AI_PROVIDER_GUIDE.md](AI_PROVIDER_GUIDE.md) for setup,
credential storage, endpoint behavior and privacy details.

## Provenance and compatibility rules

- Preserved ABC is never presented as original source.
- Reconstructed/decompiled source keeps its provenance label.
- The agent is told not to claim full Adobe FLA compatibility.
- Native `.fla` export is not offered. The experimental XFL ZIP remains
  explicitly unverified.
- A successful build is not the same as verified playback. Runtime status is
  documented separately.

## Structured project operations (M8)

M8 adds a second, typed change channel beside file diffs. The agent can return a
JSON `operations` array. Each entry is tagged with `op` and changes the in-memory
`.nova` document, not a workspace file.

Supported operations (maximum 32 per batch):

| Operation | Effect |
| --- | --- |
| `rename_project` | Project name |
| `set_stage` | Stage width/height, optional fps/background, optional scene |
| `add_scene` | New scene |
| `add_layer` | New layer with optional frame count |
| `add_object` | Rect, ellipse or static text on a scene/layer |
| `update_object` | x/y, rotation, scale, fill, text, font size; requires an unambiguous name (+ optional layer/scene) |
| `add_keyframe` | 1-based frame; optional label and `duplicate_previous` |
| `set_frame_label` | 1-based frame and non-empty label |
| `add_tween` | Motion tween with easing; requires a later keyframe containing the same object IDs |
| `add_symbol` | Rect/rounded-rect symbol |
| `place_symbol` | Symbol instance in a scene/layer |
| `add_script` | Original AS3 source file under the project workspace |

The UI shows a human-readable preview of every operation and every validation
error. Nothing is applied until the user presses **Alle bestätigen**. The whole
batch is transactional: a failure leaves the project untouched. Successful
application records one undo snapshot. Operations never delete project content
and cannot replace preserved ABC.

One important interaction: `add_tween` does not invent a destination keyframe.
It fails unless a later keyframe already contains objects with the same IDs.
Use `add_keyframe` with `duplicate_previous: true`, then move the duplicated
objects with `update_object` at the later frame (or author those edits in the
timeline).

File diffs and structured operations can be confirmed together. If either side
fails, the app rolls back the model and reports the error; file changes already
written are restored where the workspace undo path supports it.

### Preview text and errors

Operation previews are shown in the Diff tab under “Projektoperationen
(Dokumentmodell)”. Validation errors block the apply button. `operation_errors`
from a malformed model reply are shown separately, so a bad operation is not
silently dropped.

## Known limits

- One request runs at a time.
- Responses are not streamed.
- The context is bounded (48,000 characters total, 8,000 characters per selected
  source) to avoid unbounded uploads.
- The editor is a focused source surface, not a replacement for the full Code
  mode editor's syntax highlighting.
- The agent's proposals can be wrong. Review the diff; NØVA never applies
  changes without confirmation.
- The build is the existing subset compiler/exporter. Unsupported ActionScript
  is reported as diagnostics, not translated.
- The context builder does not send binary assets, ABC bytes or files outside
  the selected context.

## Milestone 7.3 live validation

The backend workflow was exercised against a real local Ollama service
(`0.40.16`, 32 installed models) with `qwen2.5-coder:7b`:

| Step | Result |
| --- | --- |
| Provider discovery and chat | **PASS** |
| Agent request/JSON parsing | **PASS** — 2,009 context characters, `cloud=false` |
| Explanation of selected `scripts/Main.as` | **PASS** |
| Proposal for a new `scripts/Added.as` | **PASS** |
| Reject proposal | **PASS** — no file written, project unchanged |
| Accept proposal | **PASS** — file and project script list updated; `scripts/Main.as` intact |
| Build after accept | **PASS** — 1,478 bytes, 48 frames, 145 objects, 4 shapes, 2 sprites, 2 scripts |
| File undo | **PASS** — new script removed |
| Unavailable endpoint | **PASS** — returned a network error, not a fabricated reply |
| Cloud/Groq providers | **NOT AVAILABLE** — no credential or endpoint configured |
| Full interactive GUI click-through | **PARTIAL** — backend workflow verified; every panel and credential-entry dialog was not repeated |

The live tests are opt-in and ignored by default:

```powershell
cargo test -p nova-ai --test live_ollama -- --ignored --nocapture --test-threads=1
cargo test -p nova-app live_agent_workflow_reject_accept_build_and_undo -- --ignored --nocapture --test-threads=1
```

## Tests

`crates/nova-ai` contains mocked tests for:

- provider JSON privacy and settings round-trip;
- Ollama and OpenAI-compatible model discovery;
- authenticated cloud headers;
- credential rejection without key leakage;
- chat response parsing and missing credentials;
- cloud consent enforcement and per-session overrides;
- context inclusion/exclusion and preserved-ABC handling;
- path traversal, create-overwrite protection, diff preview, apply/undo;
- build success, compiler diagnostics and preserved-ABC skipping.

`crates/nova-ai/tests/live_ollama.rs` adds three ignored live tests. The app
suite adds one ignored live end-to-end test in `ai_editor.rs`. The default
workspace suite remains offline and uses `MockTransport`/`MemoryCredentialStore`.

The full workspace suite is run with `cargo test --workspace`.
