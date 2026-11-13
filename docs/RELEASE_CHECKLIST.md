# Release Candidate Checklist — v0.3.0-rc1

This checklist produced `v0.3.0-rc1`. It is a **release candidate**, not a
stable release. Every item records what was actually done; nothing below is
claimed without evidence.

## Version consistency

- [x] `[workspace.package] version = "0.3.0-rc1"` in `Cargo.toml`
- [x] Installer `AssemblyVersion`/`FileVersion` = `0.3.0.0`
- [x] `InformationalVersion` flows from `NovaAppVersion` build property
- [x] Package filenames carry `0.3.0-rc1` (generated from `Cargo.toml`)
- [x] `NOVA Flash Studio.exe --version` prints `NØVA Flash Studio 0.3.0-rc1`
      (verified by `package-windows.ps1` smoke test)
- [x] `Setup.exe --version` prints `NØVA Flash Studio Setup 0.3.0-rc1`
      (verified by `package-installer.ps1` smoke test)

## Build & test

- [x] `cargo fmt --all -- --check` — clean
- [x] `cargo test --workspace` — all tests pass (see TESTING.md)
- [x] `dotnet build` installer — 0 warnings, 0 errors
- [x] `cargo build --release --workspace` — optimized build after Milestone 7.2
- [x] `scripts/package-windows.ps1 -SkipTests` and
      `scripts/package-installer.ps1 -UseExistingPayload` after Milestone 7.2 —
      Windows ZIP and Setup.exe regenerated and smoke-tested; `SHA256SUMS.txt`
      refreshed (UTF-8 product name preserved)
- [x] `python scripts/verify-packaging.py` — all checks pass
- [x] `SHA256SUMS.txt` and packaged Markdown checked as UTF-8 without BOM

## Installer scenarios (verified in Milestone 5/6 work)

- [x] Fresh per-user install, non-elevated → `%LOCALAPPDATA%\Programs\NØVA Flash Studio`, HKCU, no UAC
- [x] Per-user uninstall without UAC; `%APPDATA%\NOVA Flash Studio` preserved
- [x] All-users install → `C:\Program Files`, HKLM, common shortcuts, UAC
- [x] Non-elevated all-users install → exit 2, nothing written, no silent fallback
- [x] Same-scope upgrade in place; user data preserved
- [x] Scope-mismatch install attempt → refused (exit 2), no mixed hives
- [x] Explicit `--migrate` per-user ↔ all-users (elevated), both directions
- [x] Shortcut locations per scope (current-user vs common)
- [ ] **Actual UAC "No" click-through** — NOT verified interactively; only the
      non-elevated code path (exit 2, clean abort) was tested
- [x] Portable ZIP independent of installed copies

## Application QA

- [x] `nova check` / `nova export` / `nova probe` / `nova demo` on samples
- [x] Corrupt and malformed `.nova` inputs → clear errors, exit 1
- [x] Missing input files → clear "not found" errors (fixed misleading I/O errors)
- [x] SWF export writes via temp file + rename (no truncation on failure)
- [x] `--startup-check` reports version, icon, runtime status, first-launch flag, paths, config path
- [x] Milestone 7 first-launch setup, Runtime settings, explicit Ruffle and
      missing-Adobe error paths exercised manually; see TESTING.md
- [ ] Full GUI walkthrough (Code Mode / Animation Mode editing) — manual,
      not automated; see TESTING.md

## Milestone 7 — Runtime integration

- [x] Adobe Flash Player Projector is the default external runtime
- [x] Ruffle is explicit-only; no silent Ruffle or system-default fallback
- [x] First-launch setup dialog with browse, structural validation, persistence
      and “Später einrichten”
- [x] Runtime settings: default runtime, Adobe path, optional Ruffle path,
      browse, Prüfen, live status, last validation, reset, diagnostics
- [x] Settings stored per user outside projects; legacy settings migration;
      UTF-8 BOM tolerance; safe replacement on Windows
- [x] Safe separate-argument launch; spaces/Unicode; missing file/runtime and
      spawn errors; honest “playback not verified” reporting
- [x] `.swf` startup argument handled as a play request, not a project
- [x] Ruffle 0.7.1 app-launch playback PASS for a regenerated `as3-example` SWF
- [x] Adobe Flash Player Projector path and process launch tested with a local
      debug projector
- [ ] Adobe visual playback — **NOT VERIFIED**; process start is not playback
      evidence. The projector is not bundled.

## AS3 / SWF

- [x] AS3 compatibility matrix documented (`docs/AS3_COMPATIBILITY.md`)
- [x] ABC emitter runtime bugs fixed (4 defects, see SWF_RUNTIME_COMPATIBILITY.md)
- [x] Generated SWFs execute in Ruffle 0.7.1 (`trace` output observed)
- [x] `nova probe` structural validation on all generated artifacts
- [ ] Adobe Flash Player runtime — NOT VERIFIED (not installed, not bundled)

## Universal formats (.nova / .fla / .swf)

- [x] `.nova` remains native and backward compatible
- [x] ZIP/XFL `.fla` import for the documented subset
- [x] SWF import for FWS/CWS/ZWS, SWF 9/10/13, shapes, sprites, layers,
      frames, labels and preserved ABC
- [x] SWF export from native, imported-FLA and imported-SWF projects
- [x] Compatibility reports for imports and exports
- [x] Open/Save/Export filters and CLI commands
- [x] Native `.fla` export refused; experimental XFL ZIP documented
- [x] Conversion round-trip tests pass (see FORMAT_COMPATIBILITY.md)
- [ ] Generated FLA/XFL opened in Adobe Animate/Flash — **NOT VERIFIED**
- [x] `SymbolClass` character-link reconstruction for preserved ABC where the
      character maps to a reconstructed object; unmappable links reported
- [ ] Reconstructed scripted SWF playback in Ruffle — PARTIAL (loads without
      AVM2 errors; mapped `SymbolClass` links are re-emitted, but visual
      instantiation of every reconstructed class was not verified)

## Milestone 7.2 — AI Agent Editor

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
- [ ] Live Ollama/Groq/OpenAI-compatible smoke test — **NOT RUN**
- [ ] AI-generated code reviewed/executed with a live model — **NOT RUN**
- [ ] Adobe playback of AI-modified output — **NOT VERIFIED**

## Honesty requirements

- [x] EULA remains marked DRAFT pending legal review
- [x] No signing claims — binaries are unsigned
- [x] No Adobe compatibility claims beyond Ruffle evidence and the separately
      recorded Adobe process-launch check; visual Adobe playback remains
      **NOT VERIFIED**
- [x] No invented company/legal/contact details
- [x] Third-party notices regenerated and bundled
- [x] Portable distribution preserved alongside installer

## Known limitations carried into the RC

- AS3 subset only (~half the language surface; unsupported constructs error out)
- External Flash API members are unchecked until runtime
- GUI testing is manual; automated coverage is CLI/library level
- UAC denial path verified only non-interactively
- Adobe Flash Player integration is configuration/launch only — the player
  is never bundled
