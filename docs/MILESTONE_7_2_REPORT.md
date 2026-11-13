# Milestone 7.2 report — AI Agent Editor and SymbolClass reconstruction

**Version:** `0.3.0-rc1`  
**Status:** implemented release candidate; not stable  
**Date:** 2026-11-13

This report records what Milestone 7.2 actually implements and what was actually
verified. It is deliberately narrower than a feature announcement: mocked AI
tests are not a live-provider test, a process launch is not playback, and a
successful SWF build is not Adobe compatibility.

## 1. Scope delivered

Milestone 7.2 adds an integrated AI development workspace to the existing
editor, a provider/credential layer, root-confined file editing and
`SymbolClass` reconstruction for imported SWFs. It preserves the Milestone
1–7.1 project model, `.nova` format, CLI behavior and the
Adobe-default/Ruffle-explicit runtime policy.

### Integrated AI Agent Editor

- Opened from the toolbar (**AI Agent**), the **KI** menu and `Ctrl+Shift+A`.
- Replaces the central authoring area while open and restores the normal
  project tree, properties, timeline and central editor when closed.
- Resizable panels: project/context explorer, source editor, chat, and a bottom
  output area with Build, Diagnose, Diff and Aktivität tabs.
- Uses the open `Project` and `AppState`; it is not a second editor, project
  model or undo stack.
- Context selection is explicit: project structure, timeline/object metadata,
  script metadata, assets, compiler diagnostics, selected editor code and
  selected scripts. Only checked scripts contribute source text.
- Chat requests are prepared on the UI thread and executed on a worker thread;
  the UI remains responsive and exposes elapsed time and a cancel action.
- Proposed changes are shown as a diff and require **Alle bestätigen**;
  **Verwerfen** writes nothing.
- File undo restores the previous content of the last applied file. Project
  changes participate in the normal `Ctrl+Z` snapshot history.

### Providers and credentials

Implemented provider kinds:

| Kind | Endpoint default | Discovery | Credential |
| --- | --- | --- | --- |
| Ollama Local | `http://localhost:11434` | `GET /api/tags` | not required |
| Ollama Cloud | `https://ollama.com` | `GET /api/tags` | required |
| Groq | `https://api.groq.com/openai/v1` | `GET /models` | required |
| OpenAI-compatible | configurable | `GET /models` | endpoint-dependent |

The provider manager supports add, edit, enable/disable, default selection,
per-session override, model discovery, connection testing and removal. Removing
a provider deletes its credential after confirmation.

API keys are stored through a `CredentialStore`:

- Windows: DPAPI (`CryptProtectData`) scoped to the current user, with
  NØVA-specific entropy; the encrypted blob lives in the per-user configuration
  directory.
- Other platforms: a restricted-file fallback that is explicitly labelled as
  **not encrypted**. It is not presented as equivalent to DPAPI.
- Tests: in-memory store only; no test writes a real key.

Provider JSON (`ai-providers.json`) contains metadata only. Keys are not written
to provider JSON, project files, logs, activity entries, error messages, URLs or
`nova ai-status` output. Error text from providers is sanitized and truncated.

### Privacy and cloud consent

- Endpoint classification is conservative: `localhost`, loopback and
  single-label host names are local; everything else is cloud.
- A cloud request is rejected until the session has explicit
  **Cloud-Zustimmung**. Consent is per session and is not a persisted blanket
  permission.
- The whole project is not uploaded by default. Source text is included only for
  explicitly checked scripts; the context is bounded to 48,000 characters total
  and 8,000 characters per selected source.
- Preserved ABC is described as bytecode, not source. Its bytes are not sent as
  source text.
- The agent has no shell, arbitrary process or outside-workspace file access.
  The only requested action is a local build.

### Safe file operations

`nova-ai::Workspace` is the only path used by agent apply/save operations:

- Rejects absolute paths, drive prefixes, `..`, empty paths and symlink escapes.
- Rejects creation over an existing file and modification/deletion of a missing
  file.
- Produces a line-oriented diff before anything is written.
- Writes through a same-directory temporary file and replace step.
- Rolls back already-written files when a later file in the same accepted batch
  fails.
- Keeps previous content for file undo.
- Applies the project-model update only after disk writes succeed. On a model
  error it restores the project snapshot and active-script selection, then rolls
  back written files.
- Blocks preserved-ABC scripts from **modify, delete and source-file overwrite**;
  the exporter keeps the original ABC bytes instead of inventing source.

### `SymbolClass` reconstruction

- The SWF importer collects `SymbolClass` links into
  `SourceInfo::symbol_classes`.
- Reconstructed objects retain their original character id in
  `ObjectInstance::source_character`.
- On export, preserved links are mapped through `source_character`; the first
  matching reconstructed object is wrapped in a one-frame sprite and re-emitted
  in a `SymbolClass` tag.
- Links to characters that are not visible, not exportable or not mapped are
  reported as warnings and are **not invented**.
- If preserved `SymbolClass` links exist, the exporter does not add a separate
  inferred document class.
- The importer/exporter round-trip is covered by a test that parses the
  re-exported SWF independently and checks the `SymbolClass` table.

### CLI

- `nova build <file> [out.swf]` runs the compiler/exporter with build
  diagnostics and writes the SWF. The default output uses the existing safe
  export path, so a `.swf` input is not overwritten.
- `nova ai-status` prints provider metadata, local/cloud classification,
  credential backend and whether a credential is present. It never prints the
  credential value.

## 2. Tests and verification

### Automated tests

`cargo test --workspace` passes with **254 tests**:

| Suite | Tests |
| --- | ---: |
| `nova-ai` | 33 |
| `nova-app` | 74 |
| `nova-as` unit | 34 |
| `nova-as` compiler integration | 29 |
| `nova-build` unit | 4 |
| `nova-build` SWF script integration | 5 |
| `nova-core` | 17 |
| `nova-formats` | 20 |
| `nova-script` | 13 |
| `nova-swf` unit | 21 |
| `nova-swf` integration | 3 |
| doctest | 1 |
| **Total** | **254** |

`cargo fmt --all -- --check` passes.

The 33 `nova-ai` tests use `MockTransport` and a memory credential store. They
cover provider JSON privacy, model discovery, authenticated headers, rejected
credentials without key leakage, chat parsing, cloud consent, per-session
overrides, context inclusion/exclusion, preserved-ABC handling, root
confinement/traversal, create-overwrite and modify-missing protection, diff
preview, atomic apply/undo and build diagnostics.

The four new app tests verify that AI previews create, modify and delete project
scripts correctly, clear object links on delete, preserve script identity on
modify, and reject preserved ABC for both modify and delete.

### Runtime and manual evidence

| Check | Result |
| --- | --- |
| `nova build samples/as3-example.nova <temp>.swf` | **PASS** — 538 bytes, 48 frames, 1 object, 1 shape, 1 sprite, 1 script |
| `nova ai-status` | **PASS** — prints DPAPI backend and “Keine KI-Anbieter konfiguriert.” without a key |
| Adobe projector path/status (`--startup-check`) | **PASS** — `Adobe Flash Player Projector · Windows-EXE erkannt`, `first-launch-setup: false`, `bereit: true` |
| Adobe process launch through the final packaged app `--open samples/as3-example.swf` | **PASS for process launch** — `flashplayer_32_sa_debug` started with main window `Adobe Flash Player 32`; NØVA stayed running |
| AI workspace opened with `Ctrl+Shift+A` in the packaged app | **PASS for process stability** — window stayed alive after the shortcut; no panel-by-panel visual inspection |
| Adobe visual playback | **NOT VERIFIED** |
| Ruffle 0.7.1 playback of regenerated `as3-example` | **PASS in the earlier Milestone 7 evidence** (`avm_trace: Box gestartet`, no AVM2 errors) |
| Live Ollama/Groq/OpenAI-compatible request | **NOT RUN** — no live provider configured for the test run |
| AI-generated code reviewed/executed from a live model | **NOT RUN** |
| Adobe authoring compatibility for FLA/XFL | **NOT VERIFIED** |

The Adobe projector is configured only in per-user settings
(`%APPDATA%\NOVA Flash Studio\flash-player.json`). It is not hardcoded in source,
not bundled and not required for the application to start.

## 3. Packaging and artifacts

The Windows release is rebuilt after the Milestone 7.2 changes:

- portable ZIP: `dist/windows/NOVA-Flash-Studio-0.3.0-rc1-windows-x64.zip`
- installer: `dist/windows/NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe`
- checksums: `dist/windows/SHA256SUMS.txt`

The artifacts remain **unsigned**. The Setup executable is a self-contained
single-file .NET 8 wizard; it embeds the portable payload ZIP. The package
scripts run the workspace tests, release build, version/icon metadata checks,
`--version`/`--startup-check` smoke tests, payload smoke tests and checksum
generation.

AI provider settings, DPAPI credential blobs and runtime settings are per-user
data under `%APPDATA%\NOVA Flash Studio`. They are not included in the portable
ZIP, the installer payload or the uninstall manifest. Uninstall never deletes
them.

### Artifact hashes

Filled from the final packaging run:

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `NØVA Flash Studio.exe` | 23,522,816 | `5AE00C26DA352E51EB81159DF6D05436BF893E06E803626D34321CE13B30AC0B` |
| `NOVA-Flash-Studio-0.3.0-rc1-windows-x64.zip` | 9,882,733 | `DD3A5BDEAA0C9EA0305FB7A602C0ABDE5BA2D49CC0D7FF20C4A1351C68184763` |
| `NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe` | 81,795,347 | `87BAB81BE244F1404A42C862F777D1DB73E6D1B1385A1AC1649A5538C67ECE98` |

The authoritative list is `dist/windows/SHA256SUMS.txt`. The checksum file and
all generated Markdown files are written as UTF-8 without BOM. The ZIP contains
44 entries, including this report; `SHA256SUMS.txt` is generated after the ZIP
and therefore lives beside it in `dist/windows`, not inside it. If a packaged
copy of this report predates the final checksum-only packaging pass, the values
in `SHA256SUMS.txt` are authoritative.

## 4. Explicit non-claims

The following are **not** claimed:

- Full Adobe Flash / Animate compatibility, FLA authoring compatibility or
  byte-identical SWF reconstruction.
- Adobe visual playback, timeline rendering or script execution. Only the
  process launch was observed.
- A working live AI provider connection, live model quality or live generated
  code. All AI tests use mocked HTTP.
- That reconstructed or decompiled code is original source.
- That every imported `SymbolClass` link can be reconstructed; unmappable links
  are reported instead.
- That the installer, app or ZIP is signed.
- That macOS or Linux packages were built on their native hosts in this run.

## 5. Main implementation files

| Area | Files |
| --- | --- |
| Provider/credential/HTTP layer | `crates/nova-ai/src/provider.rs`, `credentials.rs`, `http.rs`, `error.rs` |
| Agent context and consent | `crates/nova-ai/src/agent.rs` |
| Safe edits and diff | `crates/nova-ai/src/files.rs`, `diff.rs` |
| Build diagnostics | `crates/nova-ai/src/build.rs` |
| AI workspace UI | `crates/nova-app/src/ai_editor.rs` |
| Provider settings UI | `crates/nova-app/src/ai_providers.rs` |
| Shell integration | `crates/nova-app/src/app.rs` |
| SymbolClass reconstruction | `crates/nova-formats/src/formats/swf.rs`, `crates/nova-build/src/lib.rs` |
| CLI | `crates/nova-cli/src/main.rs` |
| Tests | `crates/nova-ai/src/*`, `crates/nova-app/src/ai_editor.rs`, `crates/nova-formats/src/formats/swf.rs` |
| Documentation | `README.md`, `docs/AI_AGENT_EDITOR.md`, `docs/AI_PROVIDER_GUIDE.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`, `docs/PACKAGING.md` |
