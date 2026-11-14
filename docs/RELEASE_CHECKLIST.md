# Release Candidate Checklist — v0.4.0-rc1

This checklist produced `v0.4.0-rc1` (Mega Milestone 8). It is a **release
candidate**, not a stable release. Every item records what was actually done;
nothing below is claimed without evidence. Status categories: **PASS**,
**FAIL**, **PARTIAL**, **NOT VERIFIED**, **NOT AVAILABLE**. The historical
`v0.3.0-rc2` checklist follows the M8 section unchanged.

## M8 verification (v0.4.0-rc1)

### Version and preservation

- [x] `[workspace.package] version = "0.4.0-rc1"` in `Cargo.toml`
- [x] `Cargo.lock` updated by the rc1 build
- [x] Installer `AssemblyVersion`/`FileVersion` = `0.4.0.0`
- [x] Package filenames carry `0.4.0-rc1`
- [x] `NØVA Flash Studio.exe --version` prints `NØVA Flash Studio 0.4.0-rc1`
- [x] `Setup.exe --version` prints `NØVA Flash Studio Setup 0.4.0-rc1`
- [x] Validated rc2 artifacts preserved in `dist/archive/0.3.0-rc2/`
- [x] `.nova` format version remains `1`; old projects load with serde defaults

### Build and tests

- [x] `cargo fmt --all -- --check` — clean
- [x] `cargo check --workspace` — clean, no warnings
- [x] `cargo test --workspace` — **PASS**, 281 passed, 0 failed, 4 ignored
- [x] M8 focused tests for text, scenes, folders, templates, AS3 collections and
      AI operations — **PASS**
- [x] Release build through `scripts/build-release.ps1 -Target windows` — **PASS**

### Artifacts and installer

- [x] Portable ZIP and embedded `NØVA Flash Studio.exe` — **PASS**
- [x] `Setup.exe --smoke-test` — **PASS**, 46 payload entries, ZIP hash match
- [x] Headless per-user install — **PASS**, installed exe version verified
- [x] HKCU uninstall entry created — **PASS**, `DisplayVersion 0.4.0-rc1`
- [x] Headless quiet uninstall — **PASS**, directory and registry entry removed
- [x] `python scripts/verify-packaging.py` — **PASS**
- [x] `dist/windows/SHA256SUMS.txt` generated — **PASS**
- [x] Binaries unsigned; no signing claim
- [ ] macOS/Linux native builds — **NOT VERIFIED** (scripts only)

### Feature gates

- [x] Static text objects, vector font and shapes export — **PASS** (structural)
- [x] Layer folders, visibility/lock, validation and repair — **PASS**
- [x] Multi-scene export and boundary labels — **PASS** (structural, opt-in)
- [x] Four real templates — **PASS**
- [x] Object clipboard/duplicate/align/distribute/grid snap — **PASS**
- [x] Code find/replace/project search — **PASS**
- [x] AS3 arrays/objects/indexing ABC — **PASS** (structural)
- [x] Structured AI project operations, preview and transaction — **PASS**
- [ ] Bitmaps, audio, fonts, runtime text — **NOT AVAILABLE**
- [ ] Nested symbol timelines, masks, shape tweens — **NOT AVAILABLE**
- [ ] Full AS3 control flow/API and Flash API layer — **NOT AVAILABLE**
- [ ] Plugin/extension permission system — **NOT AVAILABLE**

### Runtime and format honesty

- [x] Ruffle M7.3 visual playback retained as historical **PASS**
- [ ] M8 text/scene visual playback — **NOT VERIFIED**
- [ ] Adobe visual playback — **NOT VERIFIED**
- [ ] AS3 collection/index runtime semantics — **NOT VERIFIED**
- [ ] Multi-frame `SymbolClass` execution after rebuild — **PARTIAL**
- [ ] XFL tween/keyframe round-trip — **PARTIAL**
- [ ] Adobe Animate/XFL authoring compatibility — **NOT AVAILABLE**
- [ ] Native `.fla` export — **NOT AVAILABLE**
- [ ] Live cloud AI providers in M8 — **NOT VERIFIED**
- [ ] Full interactive GUI/AI workflow in M8 — **NOT VERIFIED**

### Repository

- [x] M8 source changes committed on local `main`
- [ ] Remote repository — **NOT AVAILABLE**; no remote configured

## Historical v0.3.0-rc2 checklist

This checklist produced `v0.3.0-rc2`. It is a **release candidate**, not a stable
release. Every item records what was actually done; nothing below is claimed
without evidence.

## Version consistency

- [x] `[workspace.package] version = "0.3.0-rc2"` in `Cargo.toml`
- [x] `Cargo.lock` updated by the rc2 `cargo check`/release build
- [x] Installer `AssemblyVersion`/`FileVersion` = `0.3.0.0`
- [x] `InformationalVersion` flows from `NovaAppVersion` build property
- [x] Package filenames carry `0.3.0-rc2` (generated from `Cargo.toml`)
- [x] `NØVA Flash Studio.exe --version` prints `NØVA Flash Studio 0.3.0-rc2`
- [x] `Setup.exe --version` prints `NØVA Flash Studio Setup 0.3.0-rc2`

## Repository and Git

- [x] Git 2.55.0.windows.5 available; `main` initialized because `.git` was absent
- [x] Baseline commit `165416f3a0ca3f7aebe9672ec38c3224e41cb6af`
- [x] `.gitignore` excludes `target/`, `dist/`, packaging binaries/objects, logs,
      secrets, provider/runtime settings, credentials and local projects
- [x] No real API key or credential is tracked or present in the working tree
- [ ] Remote repository — **NOT AVAILABLE**; no remote configured

## Build and test

- [x] `cargo fmt --all -- --check` — clean
- [x] `cargo check --workspace` — clean
- [x] `cargo test --workspace` — **PASS**, 254 passed, 0 failed, 4 ignored
- [x] `cargo test -p nova-ai --test live_ollama -- --ignored` — **PASS**,
      3 live Ollama tests
- [x] `cargo test -p nova-app live_agent_workflow_reject_accept_build_and_undo
      -- --ignored` — **PASS**, live AI Agent backend E2E
- [x] `dotnet build packaging/windows/installer/NovaFlashStudio.Setup.csproj -c
      Release` — 0 warnings, 0 errors
- [x] `powershell scripts/build-release.ps1 -Target windows` — workspace tests,
      release build, ZIP, Setup.exe, payload/installer smoke tests and checksums
- [x] `python scripts/verify-packaging.py` — all checks pass
- [x] `SHA256SUMS.txt` and packaged Markdown checked as UTF-8 without BOM

## Installer and portable package

- [x] `Setup.exe --version` — **PASS**, `NØVA Flash Studio Setup 0.3.0-rc2`
- [x] `Setup.exe --smoke-test` — **PASS**, payload hash, 45 entries, one EXE,
      EULA and `smoke-test: OK`
- [x] Portable ZIP extraction — **PASS**, 45 entries with binary, docs and legal
      files
- [x] Portable `--version` / `--startup-check` — **PASS**
- [x] Fresh rc2 per-user install to a temporary directory with
      `--no-desktop --no-start-menu --no-assoc` — **PASS**
- [x] Installed rc2 app `--version` / `--startup-check` — **PASS**
- [x] Headless rc2 uninstall — **PASS**, install directory removed and
      `%APPDATA%\NOVA Flash Studio\flash-player.json` hash unchanged
- [x] Earlier Milestone 5/6 scenarios: all-users install to `C:\Program Files`,
      HKLM, common shortcuts and UAC; non-elevated all-users refusal; same-scope
      upgrade; scope-mismatch refusal; explicit `--migrate` — **PASS in those
      milestones**
- [ ] Rc2 all-users/UAC install — **NOT VERIFIED** in M7.3; requires a
      disposable profile/VM
- [ ] Actual UAC "No" click-through — **NOT VERIFIED** interactively
- [x] Portable ZIP is independent of installed copies
- [ ] Code signing — **NOT AVAILABLE**; no legitimate certificate, artifacts are
      unsigned

## Application QA

- [x] `nova check` / `nova export` / `nova probe` / `nova demo` on samples
- [x] Corrupt and malformed `.nova` inputs → clear errors, exit 1
- [x] Missing input files → clear "not found" errors
- [x] SWF export writes via temp file + rename (no truncation on failure)
- [x] `--startup-check` reports version, icon, runtime status, first-launch flag,
      paths and config path
- [x] Temp-config runtime checks: empty config → Adobe default/not ready;
      missing Adobe path → clear error/no fallback; explicit Ruffle → ready;
      invalid JSON → visible config error/no fallback
- [x] `.swf` startup path recognition is case-insensitive (automated test)
- [ ] Full interactive GUI walkthrough (Code Mode / Animation Mode editing) —
      **PARTIAL**; manual, not automated

## Performance (Windows release build, single host)

- [x] `--startup-check`, 5 runs — avg 84.7 ms; min 72.4 ms; max 119.6 ms
- [x] GUI launch, 3 s after start — 179.3 MB working set; 200.5 MB peak;
      window created; clean exit
- [x] Large project load (`nova check`, 2.33 MB JSON, 100 layers, 10,000 frames,
      2,000 authored objects) — 28 ms
- [x] Large project build — 247 ms; 2,034,019-byte SWF
- [x] Large SWF import — 2,124 ms; small SWF import — 15 ms
- [x] Peak working set during large build — ~95 MB (polled)
- [x] Measurements are single-host evidence, not a benchmark guarantee

## Security

- [x] Windows credential backend uses DPAPI `CryptProtectData`
- [x] `dpapi_roundtrip_does_not_store_plaintext` — **PASS**
- [x] Provider JSON never contains an API key — **PASS**
- [x] Provider error redaction — **PASS**
- [x] Credential filename sanitization — **PASS**
- [x] Workspace root confinement, path traversal/symlink rejection — **PASS**
- [x] Atomic preview/apply/undo — **PASS**
- [x] No provider API key found in environment or `%APPDATA%` provider config
- [ ] Full interactive GUI credential-entry session — **PARTIAL**; write-only UI
      and automated tests exist, but the full click-through was not repeated

## Milestone 7 — Runtime integration

- [x] Adobe Flash Player Projector is the default external runtime
- [x] Ruffle is explicit-only; no silent Ruffle or system-default fallback
- [x] First-launch setup dialog with browse, structural validation, persistence
      and "Später einrichten"
- [x] Runtime settings: default runtime, Adobe path, optional Ruffle path,
      browse, Prüfen, live status, last validation, reset, diagnostics
- [x] Settings stored per user outside projects; legacy settings migration;
      UTF-8 BOM tolerance; safe replacement on Windows
- [x] Safe separate-argument launch; spaces/Unicode; missing file/runtime and
      spawn errors; honest "playback not verified" reporting
- [x] `.swf` startup argument handled as a play request, not a project
- [x] Ruffle 0.7.1 app-launch playback **PASS** for a regenerated
      `as3-example` SWF
- [x] Ruffle 0.7.1 visual playback **PASS** for the M7.3 `validation.swf`:
      document-class marker, linked-sprite rotation, tween motion and labels
- [x] Adobe debug projector 32.0.0.465 path, process and window launch —
      **PARTIAL**
- [ ] Adobe visual playback — **NOT VERIFIED**; the window opened but the SWF
      area rendered blank and no trace log appeared
- [x] Current `.swf` association state inspected only; NØVA is not silently
      registered and no association was changed

## AS3 / SWF

- [x] AS3 compatibility matrix documented (`docs/AS3_COMPATIBILITY.md`)
- [x] ABC emitter runtime bugs fixed (4 defects, see
      `docs/SWF_RUNTIME_COMPATIBILITY.md`)
- [x] Generated SWFs execute in Ruffle 0.7.1 (`trace` output and visual capture)
- [x] `nova probe` structural validation on all generated artifacts
- [x] One-frame `SymbolClass` fixture executes its linked `Added` class before
      and after import/re-export — **PASS**
- [ ] Multi-frame rebuilt SWF visibly executes the linked class across frames —
      **PARTIAL**; structural links survive, visible linked-class execution does
      not
- [ ] Ruffle `Error #1056` for named timeline children — non-fatal, unfixed
- [ ] Adobe Flash Player runtime — **NOT VERIFIED**; installed debug projector
      starts a blank window

## Universal formats (.nova / .fla / .swf)

- [x] `.nova` remains native, format version 1 and backward compatible
- [x] ZIP/XFL `.fla` import for the documented subset
- [x] SWF import for FWS/CWS/ZWS, SWF 9/10/13, shapes, sprites, layers,
      frames, labels and preserved ABC
- [x] SWF export from native, imported-FLA and imported-SWF projects
- [x] Compatibility reports for imports and exports
- [x] Open/Save/Export filters and CLI commands unchanged
- [x] Native `.fla` export refused; experimental XFL ZIP documented
- [x] XFL export `validation-xfl.zip` — 1,953 bytes with `DOMDocument.xml`,
      `scripts/Main.as`, `scripts/Added.as` and `README.txt`
- [x] XFL import — 2 layers, 24 frames, 2 scripts, 1 symbol, 2 shapes;
      `README.txt` reported as unsupported
- [ ] XFL export/re-import round-trip — **PARTIAL**; frames 1–22 empty and tween
      data lost; geometry/library symbols approximated
- [ ] Generated FLA/XFL opened in Adobe Animate/Flash — **NOT AVAILABLE**; Adobe
      Animate is not installed and no Adobe-authored `.fla` fixture exists
- [x] `SymbolClass` character-link reconstruction for preserved ABC where the
      character maps to a reconstructed object; unmappable links reported
- [ ] Reconstructed scripted SWF playback in Ruffle — **PARTIAL**; verified for a
      one-frame fixture, not for every multi-frame reconstructed class

## Milestone 7.2 / 7.3 — AI Agent Editor

- [x] Integrated AI workspace via toolbar, **KI** menu and `Ctrl+Shift+A`
- [x] Resizable explorer, source editor, chat, build, diagnostics, diff and
      activity panels
- [x] Ollama Local/Cloud, Groq and OpenAI-compatible provider kinds
- [x] Enable/disable, model discovery, default provider, connection tests,
      edit/remove and per-session override
- [x] API keys in Windows DPAPI; explicit restricted-file fallback elsewhere;
      no keys in JSON, logs or error messages
- [x] Cloud classification and explicit per-session consent
- [x] User-selected bounded context; no whole-project upload by default
- [x] Root-confined create/modify/delete, diff preview, accept/reject, atomic
      writes, rollback and file undo
- [x] Preserved ABC blocked from modify/delete through the agent
- [x] `nova build` and `nova ai-status` CLI diagnostics
- [x] Mocked AI/provider tests, file-operation tests, app apply tests
- [x] Live local Ollama: 32 models discovered; `qwen2.5-coder:7b` chat and
      agent requests — **PASS**
- [x] Live AI Agent backend E2E: explanation, proposal, reject without write,
      accept with file/project update, build and file undo — **PASS**
- [x] Unavailable endpoint returns a network error instead of a fabricated reply
- [ ] Live Ollama Cloud/Groq/OpenAI-compatible cloud provider — **NOT AVAILABLE**;
      no credential or endpoint configured; no request attempted
- [ ] Full interactive AI GUI click-through — **PARTIAL**
- [ ] Adobe playback of AI-modified output — **NOT VERIFIED**

## Artifacts and checksums

- [x] `NØVA Flash Studio.exe`
- [x] `NOVA-Flash-Studio-0.3.0-rc2-windows-x64.zip`
- [x] `NOVA-Flash-Studio-0.3.0-rc2-windows-x64-Setup.exe`
- [x] `dist/windows/SHA256SUMS.txt` is authoritative for final sizes and hashes
- [x] Previous rc1 artifacts backed up outside `dist/windows` before the rc2
      build and not presented as the current release
- [ ] Signing — **NOT AVAILABLE**; artifacts remain unsigned

## Honesty requirements

- [x] EULA remains marked DRAFT pending legal review
- [x] No signing claims — binaries are unsigned
- [x] No Adobe compatibility or visual-playback claim beyond process/window
      launch; Adobe playback remains **NOT VERIFIED**
- [x] No cloud-AI claim; only local Ollama was exercised live
- [x] No XFL authoring-compatibility claim; Adobe Animate was not available
- [x] No invented company/legal/contact details
- [x] Third-party notices regenerated and bundled
- [x] Portable distribution preserved alongside installer

## Known limitations carried into the RC

- AS3 subset only (~half the language surface; unsupported constructs error out)
- External Flash API members are unchecked until runtime
- GUI testing is manual; automated coverage is CLI/library level
- UAC denial path verified only non-interactively
- Adobe Flash Player integration is configuration/launch only — the player is
  never bundled, and visual playback is **NOT VERIFIED**
- Adobe Animate authoring compatibility and XFL authoring compatibility are
  **NOT AVAILABLE** as verified claims
- XFL round-trip loses the authored tween and leaves frames 1–22 empty
- Multi-frame SWF import/re-export preserves structural SymbolClass links but not
  visible linked-class execution across rebuilt frames
- Ruffle logs a non-fatal `Error #1056` for named timeline children
- Live cloud AI providers and the full interactive AI GUI workflow are not
  verified
