# Milestone 7 Report — Adobe-Default Runtime Integration

Date: 2026-11-12 · Platform: Windows 11 x64 · Status: **RC, not stable**

Milestone 7 turns the external-player integration into a complete runtime
configuration flow. Adobe Flash Player Projector is the default; Ruffle is
optional and explicit-only. The milestone preserves the Milestone 5/6 installer,
licensing, packaging and SWF/AS3 work.

## 1. Policy and behavior

- **Adobe Flash Player Projector is the default runtime.**
- **Ruffle is optional** and is used only after the user selects it in Runtime
  settings.
- There is **no system-default `.swf` handler** and **no silent fallback** to
  Ruffle when Adobe is missing or invalid.
- NØVA does **not download or bundle Adobe binaries**. The user selects an
  already installed projector.
- A successful process start is reported as “gestartet; Wiedergabe nicht
  verifiziert”; it is never treated as playback success.

## 2. Implemented changes

### Runtime model (`crates/nova-app/src/flash.rs`)

- `RuntimeKind` (`AdobeProjector`, `Ruffle`) and a persisted default runtime.
- `FlashSettings` now stores `default_runtime`, `adobe_projector_path`,
  `ruffle_path`, `first_launch_setup_done` and `last_validation`.
- First-launch detection requires a valid Adobe path unless setup was explicitly
  postponed.
- Legacy `use_system_default` / `projector_path` files are migrated: Ruffle-like
  paths become explicit Ruffle, other manual paths become the Adobe path, and a
  legacy system-default-only file requires first-launch setup.
- Settings are written to a temporary sibling and replaced safely on Windows;
  UTF-8 BOM is tolerated when reading.
- Runtime path validation checks the native file type (Windows `MZ` + `.exe`,
  macOS Mach-O or `.app` with `Contents/Info.plist`, Linux ELF + executable bit).
- SWF validation checks existence, `.swf` extension and the `FWS`/`CWS`/`ZWS`
  signature.
- Launch command construction uses `Command::new(executable).arg(swf)`; no shell
  string is assembled. macOS `.app` bundles use `open -a <app> <swf>`.
- `LaunchReport` records runtime, executable, SWF and PID, and its message
  explicitly says playback was not verified.

### UI (`crates/nova-app/src/app.rs`, `state.rs`)

- First-launch dialog **Flash Player Projector einrichten** with explanation,
  path field, **Durchsuchen …**, **Prüfen**, **Speichern und fortfahren**, and
  **Später einrichten**.
- **Später** persists `first_launch_setup_done: true`, closes the dialog, keeps
  the app usable and reports that a runtime is still required.
- Dedicated **Runtime** settings section with default-runtime radio buttons,
  Adobe path, optional Ruffle path, browse, structural validation, live status,
  last validation result, reset, diagnostics, save and cancel.
- Saving rejects invalid non-empty paths, so a valid configuration is not
  accidentally replaced by a broken one.
- Explicit Ruffle selection is preserved across restarts; switching back to
  Adobe preserves both paths.
- Missing Adobe with Ruffle configured shows a configuration error and does not
  start Ruffle.
- `.swf` paths passed by a shell association or `--open` are recognised as play
  requests, not parsed as `.nova` projects. If first-launch setup is pending, the
  SWF is opened after successful setup; choosing “Later” clears the pending SWF
  with a warning.

### CLI / startup

- `--startup-check` reports the selected runtime, readiness, first-launch flag,
  Adobe/Ruffle paths and config location.
- `--open <path>` accepts a project or a SWF.
- The help text and menu labels reflect Adobe-default/Ruffle-explicit behavior.

### Documentation

- Rewrote `docs/FLASH_PLAYER_INTEGRATION.md`.
- Updated `docs/SWF_RUNTIME_COMPATIBILITY.md` with structural, path, process
  launch and playback layers.
- Updated `docs/TESTING.md` with Milestone 7 automated and GUI evidence.
- Updated `README.md`, `docs/ROADMAP.md` and `docs/RELEASE_CHECKLIST.md`.

## 3. Verification results

### Automated and build checks

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS — 184 tests, 0 failed (57 in `nova-app`) |
| `cargo check -p nova-app` | PASS — no warnings |
| `cargo build --release --workspace` | PASS — optimized build in 43.72 s |
| `dotnet build packaging/windows/installer/NovaFlashStudio.Setup.csproj -c Release` | PASS — 0 warnings, 0 errors |
| `powershell scripts/build-release.ps1 -Target windows` | PASS — tests, release build, ZIP, Setup.exe, payload/installer smoke tests and checksums |
| `python scripts/verify-packaging.py` | PASS — icons, `Info.plist`, AppStream, desktop entry, legal files and installer sources |

### `--startup-check` scenarios

| Config | Result |
| --- | --- |
| Missing settings | Adobe default, not ready, `first-launch-setup: true` |
| Valid fake Adobe `MZ` executable | Adobe, ready, `first-launch-setup: false` |
| Explicit Ruffle | Ruffle, ready, Adobe path preserved |
| Missing Adobe path, Ruffle configured | Adobe not ready; no Ruffle fallback |
| Legacy `use_system_default: true` | Adobe default, not ready, first-launch setup required |

### GUI and runtime evidence

- First-launch dialog appeared with no configuration; **Später einrichten**
  closed it, wrote `first_launch_setup_done: true`, and left the app usable.
- Runtime settings showed Adobe selected by default and `Windows-EXE erkannt`
  for a fake projector path.
- Selecting Ruffle and saving wrote `default_runtime: "ruffle"` and a matching
  `last_validation` record.
- With Adobe missing and Ruffle configured, **Open in Flash Player** showed a
  configuration error; the process list contained no Ruffle process.
- A regenerated `as3-example` SWF opened through the toolbar with Ruffle 0.7.1:
  Ruffle logged `Opening file:///…/as3-example.swf`, `Loaded SWF version 10,
  resolution 640x360 @ 24 FPS` and `avm_trace: Box gestartet`; no AVM2 errors.
  The app status bar said `Ruffle gestartet; Wiedergabe nicht verifiziert`.
- The same SWF passed via `--open <path>` (the shell-association path) started
  Ruffle and produced the same log evidence.
- The checked-in `samples/as3-example.swf` is an older artifact; passing it
  through the app path produced the expected pre-fix `TypeError #1009`. Runtime
  tests use a freshly exported SWF.

## 4. Adobe limitation

**Adobe Flash Player Projector was not installed in this environment.** Its path
validation, process launch and actual playback are therefore **NOT VERIFIED**.
The app does not download or bundle Adobe binaries, and no Adobe compatibility
claim is made. A tester with a legally obtained projector should repeat the
manual projector test in `README.md` and record the projector build, OS, path
validation, process launch and playback result separately.

## 5. Packaging state

The canonical Windows installer remains the .NET 8 WinForms project under
`packaging/windows/installer/`; the Inno Setup prototype is not part of the
repository. Licensing remains under `legal/` with SPDX texts in `LICENSES/`.
The installer project builds cleanly and the packaging structure check passes.

`scripts/build-release.ps1 -Target windows` regenerated the Windows payload and
installer after the M7 changes. The new artifacts are:

- `dist/windows/NØVA Flash Studio/NØVA Flash Studio.exe`
- `dist/windows/NOVA-Flash-Studio-0.3.0-rc1-windows-x64.zip`
- `dist/windows/NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe`

`SHA256SUMS.txt` was refreshed. The installer packaging script was fixed to read
the existing checksum list explicitly as UTF-8; Windows PowerShell 5.1 otherwise
read the BOM-less file as ANSI and double-encoded the `Ø` in `NØVA` on rewrite.
The regenerated artifacts remain unsigned.

## 6. Remaining work

- Adobe Flash Player Projector smoke test on a machine where it is legally
  available: path, launch and actual playback.
- Interactive UAC-denial click-through.
- Fresh-install verification of the optional Windows `.swf` association; the
  equivalent `--open <swf>` startup path was tested.
- Broader GUI automation and long-running stability/memory profiling.
- Code-signing decision; binaries remain unsigned.
- Legal review of the EULA draft; it remains a draft.
