# Flash Player / Runtime Integration

NØVA can hand an exported SWF to an external runtime. It is an **integration**,
not an embedded player: NØVA does not execute ActionScript or render the SWF
inside the editor.

Milestone 7 changes the runtime policy:

- **Adobe Flash Player Projector is the default runtime.**
- **Ruffle is optional and is used only after an explicit user selection.**
- There is **no silent fallback**. If Adobe is missing or invalid, NØVA reports
  the configuration problem instead of starting Ruffle or a system-default
  handler.
- NØVA does **not download or bundle Adobe binaries**. The user selects an
  already installed projector.

The visible action is still **Open in Flash Player**. It is available:

- in the top toolbar next to **SWF exportieren**;
- in the **Flash Player** menu;
- in the **SWF exportiert** dialog immediately after an export.

The runtime configuration is reached from **Flash Player → Runtime-Einstellungen …**.

## First launch

On startup NØVA loads the runtime configuration and checks whether a valid Adobe
Flash Player Projector is configured.

- If a valid Adobe path exists, the first-launch dialog is suppressed.
- If no valid Adobe path exists and first-launch setup has not been completed or
  deliberately postponed, NØVA opens **Flash Player Projector einrichten**.
- The dialog explains why a runtime is needed, states that NØVA does not
  download or bundle Adobe binaries, and offers:
  - a path field and **Durchsuchen …**;
  - **Prüfen** for a structural validation;
  - **Speichern und fortfahren** once a valid path is selected;
  - **Später einrichten**.

Choosing **Später einrichten** (or pressing Escape) persists
`first_launch_setup_done: true` and leaves the application usable. Opening a SWF
then shows a configuration error until a runtime is selected in the Runtime
settings. The dialog is not shown again on every launch.

If the settings file contains a valid Adobe path but the flag is still `false`,
the dialog is suppressed because the valid path is enough. This avoids nagging
after an upgrade or after a manual configuration repair.

## Runtime settings

The dedicated **Runtime** section contains:

| Control | Behavior |
| --- | --- |
| Standard-Runtime | Radio selection: **Adobe Flash Player Projector (empfohlen)** or **Ruffle (nur nach ausdrücklicher Auswahl)** |
| Adobe Flash Player Projector | Path editor for the Adobe executable or macOS `.app` bundle |
| Durchsuchen … | Native file picker; spaces and Unicode are preserved |
| App-Bundle … (macOS only) | Native folder picker for an Adobe `.app` bundle |
| Prüfen | Structural validation of the selected file; does not open the player |
| Status | Live status for the selected runtime, including missing/invalid paths |
| Letzte Prüfung | Last persisted structural validation result for the matching path |
| Ruffle (optional) | Optional path editor for Ruffle Desktop |
| Durchsuchen … / Prüfen (Ruffle) | File picker and structural validation for the optional Ruffle path |
| Zurücksetzen | Clears the draft paths, selects Adobe, and re-enables first-launch setup after saving |
| Speichern | Persists the configuration; invalid non-empty paths are rejected |
| Abbrechen | Discards the draft |
| Diagnose kopieren | Copies version, OS, selected runtime, status, paths, last validation, and config location |

Adobe remains selected by default. An explicit Ruffle selection is preserved
across restarts; it is never selected automatically because Adobe is missing or
invalid. Switching back to Adobe preserves both configured paths.

A non-empty path must pass structural validation before **Speichern** succeeds.
This prevents accidentally replacing a valid configuration with a broken path.
Clearing a path and saving is a deliberate user action.

## Open flow

1. Locate the SWF:
   - a SWF passed by a shell association or `--open <path>` (used directly), or
   - the most recent export path from this session, or
   - for a saved project that has not been exported this session, a sibling
     `<project-stem>.swf` if it exists.
2. Verify that the path exists, is readable, is a file, has the `.swf`
   extension, and starts with a SWF signature (`FWS`, `CWS` or `ZWS`).
3. Select the runtime from the saved configuration:
   - **Adobe Projector:** require `adobe_projector_path`;
   - **Ruffle:** require `ruffle_path`.
4. Validate the selected executable/bundle structurally.
5. Start the runtime with the SWF as one separate process argument:
   `Command::new(executable).arg(swf)`.
6. On success, log the runtime and PID, and state clearly that **the process was
   started but actual playback was not verified**.
7. On failure, show a dialog with the exact error, the SWF path, and a pointer to
   **Flash Player → Runtime-Einstellungen …**.

NØVA never uses the operating system's current `.swf` association as a runtime
fallback. The Windows installer can optionally associate `.swf` with NØVA
itself (see below), but that is a play request to NØVA, not a handoff to a
system-default player. The runtime must be Adobe or Ruffle and must be selected
explicitly.

If no SWF has been exported, NØVA explains that **SWF exportieren** must be used
first. It does not launch a blank player or fabricate a preview.

## Configuration file

Settings are stored outside individual projects in the per-user configuration
directory:

| OS | Path |
| --- | --- |
| Windows | `%APPDATA%\NOVA Flash Studio\flash-player.json` |
| macOS | `~/Library/Application Support/NOVA Flash Studio/flash-player.json` |
| Linux | `$XDG_CONFIG_HOME/nova-flash-studio/flash-player.json`, falling back to `~/.config/nova-flash-studio/flash-player.json` |

`NOVA_FLASH_STUDIO_CONFIG_DIR` overrides the directory for tests and diagnostics.

Current shape:

```json
{
  "default_runtime": "adobe_projector",
  "adobe_projector_path": "C:\\Program Files\\Flash Player\\flashplayer.exe",
  "ruffle_path": null,
  "first_launch_setup_done": true,
  "last_validation": {
    "runtime": "adobe_projector",
    "path": "C:\\Program Files\\Flash Player\\flashplayer.exe",
    "ok": true,
    "message": "Adobe Flash Player Projector erkannt: Windows-EXE (…). Strukturprüfung bestanden; Prozessstart und Wiedergabe nicht geprüft.",
    "checked_at_unix": 1794510911
  }
}
```

The file is written to a temporary sibling and renamed. On Windows the previous
file is removed before the rename because `std::fs::rename` cannot replace an
existing file there; the temporary file still prevents a truncated settings file
if writing fails. A UTF-8 BOM is tolerated when reading.

### Legacy migration

The legacy pre-Milestone-7 file used:

```json
{
  "use_system_default": false,
  "projector_path": "C:\\Tools\\Ruffle\\ruffle.exe"
}
```

On load:

- `use_system_default: true` with no manual path becomes Adobe default with
  first-launch setup required;
- a manual path whose file name contains `ruffle` becomes an explicit Ruffle
  selection and is preserved in `ruffle_path`;
- other manual paths are preserved as `adobe_projector_path`;
- the legacy file is not rewritten until the user saves the Runtime settings.

## Platform mechanisms and safety

| OS | Adobe / Ruffle launch |
| --- | --- |
| Windows | `Command::new(executable).arg(swf)` |
| macOS | `Command::new(executable).arg(swf)`, or `open -a <app> <swf>` for a bundle |
| Linux | `Command::new(executable).arg(swf)` |

No shell command string is assembled from a file path. The SWF and runtime paths
are separate process arguments. This is important for names containing spaces,
quotes, `&`, `%`, non-ASCII characters or leading dashes. Automated tests cover
spaces and Unicode in both the SWF and runtime paths.

NØVA never constructs `cmd /C start`, never uses `ShellExecuteW` for the runtime
launch, and never reports a process start as a successful playback.

## Validation layers

Keep these separate in diagnostics and bug reports:

| Layer | What it proves | What it does not prove |
| --- | --- | --- |
| SWF path validation | File exists, is readable, is a file, has `.swf`, starts with `FWS`/`CWS`/`ZWS` | That the runtime can execute it |
| Runtime path validation | File exists, is a native executable/bundle, passes the structural check | That it is the intended Adobe/Ruffle build, or that it starts |
| Process launch validation | The OS created the runtime process; NØVA has a PID | That the SWF loaded, executed, or rendered |
| Playback validation | The runtime log or a captured runtime frame shows the SWF loaded and, where applicable, script/timeline activity without fatal errors | Full visual, timing, audio, or Adobe compatibility |

The **Prüfen** button performs runtime path validation only. **Open in Flash
Player** performs process launch validation. Actual playback requires inspecting
the runtime itself; NØVA does not claim it automatically.

Structural checks:

- Windows: `MZ` header with an `.exe` extension.
- macOS: Mach-O executable magic, or a directory ending in `.app` containing
  `Contents/Info.plist`.
- Linux: ELF magic with at least one executable permission bit.

## Error behavior

Examples:

- **No export:** “Es wurde noch keine SWF-Datei exportiert.”
- **Missing file:** the path is named in the dialog.
- **Wrong extension or signature:** the target is rejected as not being a valid
  SWF. The check reads the first three bytes, so a renamed or truncated file is
  caught before the runtime is launched.
- **No Adobe Projector configured:** “Kein Adobe Flash Player Projector
  konfiguriert. Öffne Flash Player → Runtime-Einstellungen … und wähle den Projector aus – oder
  wechsle dort ausdrücklich zu Ruffle.”
- **Ruffle selected without a Ruffle path:** the dialog points to the Runtime
  settings.
- **Missing runtime:** “Die ausgewählte Runtime existiert nicht.”
- **Wrong executable type:** the file-type check fails with a clear message.
- **Spawn failure:** the OS error is included and copied to the console.

The settings file being absent is not an error; defaults are used. A malformed
settings file is logged and shown as a splash warning while the app starts with
defaults and first-launch setup.

## `.swf` file associations

The application itself does **not** silently change `.swf` associations. NØVA is
an authoring tool, not a runtime, and taking over an existing Flash association
without consent would be surprising and incorrect.

The **Windows installer** offers explicit association options. In the setup
wizard, `.nova` is enabled by default and `.swf` is default-off; headless
installs create no associations unless `--assoc-nova` or `--assoc-swf` is
passed. If the user selects the `.swf` association, Windows passes the SWF path
to NØVA as a startup argument. NØVA recognises the `.swf` extension, does not
try to parse it as a project, and opens it through the configured runtime. If no runtime is configured, it shows
the normal configuration error instead of falling back to Ruffle or a
system-default handler. This is an explicit installer choice, not an automatic
association change.

The macOS bundle and Linux desktop entry do not register a `.swf` handler.
Possible future cross-platform opt-in designs, only after an explicit
confirmation dialog:

- **macOS:** use LaunchServices (`LSSetDefaultRoleHandlerForContentType`) for
  `public.film`/the SWF UTI, after showing the current handler. A bundle would
  need a document type declaration; the current `Info.plist` intentionally has
  none.
- **Linux:** ship a desktop entry with a MIME type and run
  `xdg-mime default nova-flash-studio.desktop <mime>` only after confirmation.
  The current desktop entry intentionally has no `MimeType` line.

The in-app Runtime settings section explains this policy rather than presenting a
nonfunctional toggle. The Windows installer's association is visible in the
wizard and is not applied unless the user checks it or passes `--assoc-swf` to
the headless installer.

## Tested runtimes

| Runtime | Status in this environment |
| --- | --- |
| **Ruffle Desktop 0.7.1** | **Playback PASS for the regenerated `as3-example` SWF via the app launch path**, both from the toolbar action and from a `.swf` startup argument. The app started `ruffle.exe` with the SWF as one argument; Ruffle logged `Opening file:///…/as3-example.swf`, `Loaded SWF version 10, resolution 640x360 @ 24 FPS`, and `avm_trace: Box gestartet` with no AVM2 errors. The app status bar reported “Ruffle gestartet; Wiedergabe nicht verifiziert”. M7.3 additionally captured visual playback of `validation.swf`: red document-class marker, cyan linked-sprite rotation and orange tween motion. |
| **Adobe Flash Player Projector 32.0.0.465** | **PROCESS/WINDOW LAUNCH PASS; PLAYBACK NOT VERIFIED.** A local debug projector was configured through per-user Runtime settings. `--startup-check` reported the projector as valid, and `--open samples/as3-example.swf` started `flashplayer_32_sa_debug` with a main window titled `Adobe Flash Player 32`. In M7.3 the window opened but the SWF area rendered blank for the validation and sample SWFs, and no `flashlog.txt` trace appeared. No Adobe binary was downloaded or bundled, and no Adobe playback claim is made. |

The checked-in `samples/as3-example.swf` predates the AVM2 fixes and is not a
valid playback baseline. Regenerate it with `nova export samples/as3-example.nova`
before using it for runtime testing.

### Milestone 7.3 visual evidence

The disposable `validation.swf` (875 bytes, 24 frames, 2 ABC scripts,
SymbolClass `(0, Main)`/`(4, Added)`) was opened directly in Ruffle 0.7.1. A
PrintWindow capture showed the red document-class marker, the cyan linked sprite
rotating and the orange tween moving; timeline labels executed. Ruffle logged a
recurring non-fatal `ReferenceError: Error #1056` for named timeline children.

The same file opened an Adobe debug projector window, but the content area was
blank and no trace file was written. Adobe visual playback is therefore
**NOT VERIFIED**, not PASS. A one-frame SymbolClass fixture executed its linked
class before and after import/re-export; the multi-frame rebuilt SWF preserved
the structural links but did not visibly execute the linked class across the
rebuilt frames (**PARTIAL**). See `docs/SWF_RUNTIME_COMPATIBILITY.md`.

## Manual QA matrix

| Case | Expected | Result |
| --- | --- | --- |
| First launch, no configuration | First-launch dialog with explanation, browse, Prüfen, save, later | PASS (GUI screenshot) |
| First launch, valid Adobe path | No first-launch dialog | PASS (automated model test + startup-check) |
| First launch, **Später einrichten** | Dialog closes, app usable, `first_launch_setup_done: true` persisted | PASS (GUI + config inspection) |
| Runtime settings, Adobe default | Adobe radio selected; status shows validated Windows-EXE for a fake projector | PASS (GUI screenshot) |
| Runtime settings, explicit Ruffle | Ruffle selected and saved; `default_runtime: "ruffle"` persisted | PASS (GUI + config inspection) |
| Switch back to Adobe | Adobe selected; both paths preserved | PASS (automated test) |
| Missing Adobe, Ruffle configured | Error dialog; no Ruffle process starts | PASS (GUI; process list empty) |
| Missing/deleted Adobe path | Status not ready; launch error names the path | PASS (automated tests) |
| Missing/deleted SWF | Clear “existiert nicht” error | PASS (automated test) |
| SWF opening via app | Ruffle process starts with the exact SWF; status says playback not verified | PASS (Ruffle GUI + log) |
| `.swf` passed as startup path (`--open` / association) | Treated as a play request, not parsed as a project; configured runtime starts | PASS (GUI + Ruffle log; `--open` used) |
| Spaces/Unicode paths | No truncation or shell splitting | PASS (automated test) |
| Invalid settings JSON | App starts with defaults and warning | PASS (automated test) |
| Settings save/restart | Configuration restored | PASS (automated round-trip + GUI) |
| Windows `.swf` association (explicit, default-off) | Not applied unless the user checks it; if selected, the SWF is passed to NØVA and uses the configured runtime | PASS (installer code; M7 startup path tested via `--open`; fresh registry install not rerun in M7) |
| M7.3 Ruffle visual playback of `validation.swf` | Document-class marker, linked sprite, tween and labels visible | PASS (PrintWindow capture) |
| M7.3 Adobe debug projector | SWF loads and renders | PARTIAL / NOT VERIFIED — window opens, content blank, no trace |
| M7.3 one-frame SymbolClass import/re-export | Linked class still executes | PASS (Ruffle visual) |
| M7.3 multi-frame SymbolClass import/re-export | Linked class still executes across frames | PARTIAL — structural links only |
| M7.3 current `.swf` association inspection | NØVA not silently registered | PASS (inspection only: HKCR Winamp, HKCU Ruffle/Adobe entries; no change made) |

## Limitations

- Actual Adobe Flash Player playback is unverified. A local projector starts a
  window, but the tested SWFs rendered blank and no trace log was written.
  Adobe load, rendering and script execution are **NOT VERIFIED**.
- NØVA cannot verify that a selected executable is genuinely Adobe rather than
  another native program. Structural validation only checks the file type.
- The app cannot confirm playback merely from a successful `spawn()`; the UI
  intentionally says “Wiedergabe nicht verifiziert”.
- Ruffle compatibility is evidence for the tested SWFs, not a guarantee for the
  full AS3 subset. See `docs/SWF_RUNTIME_COMPATIBILITY.md`.
- Ruffle logs a non-fatal `Error #1056` for named timeline children on the
  document class; playback continues and no `dynamic` workaround was added.
- Multi-frame SWF import/re-export preserves structural SymbolClass links but
  does not visibly execute the linked class across rebuilt frames (**PARTIAL**).
- The current `.swf` association state was inspected, not changed; NØVA is not
  registered as a handler in this environment.
