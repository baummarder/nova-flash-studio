# NØVA Flash Studio

A native desktop prototype that combines a small ActionScript 3 code editor with a
Flash-style animation editor — and exports a real, binary **SWF** file.

NØVA Flash Studio is written in Rust. The UI uses `egui`/`eframe` (wgpu backend),
the project format is versioned JSON, and the SWF writer is implemented directly
against the published SWF file-format specification. Tools and extensions run in
an embedded QuickJS context; ActionScript is **not** executed by QuickJS.

> **Status: Milestones 2-7.3 implemented — release candidate `0.3.0-rc2`, not
> stable.** The SWF container, shapes, sprites and frame-by-frame animation
> export work and are
> covered by tests. The editor has undo/redo, multi-select, tween authoring,
> keyframe workflow and autosave with crash recovery. A documented AS3 subset is
> type-checked and compiled to real AVM2 ABC, embedded with `DoABC2` and linked
> with `SymbolClass`. Unsupported ActionScript is rejected with source
> diagnostics instead of being mistranslated. Milestones 5–6 added the branded
> Windows installer, portable distribution, licensing files, and runtime fixes
> found by executing generated SWFs in Ruffle. **Milestone 7 makes Adobe Flash
> Player Projector the default external runtime**, adds first-launch setup and a
> dedicated **Runtime** settings section, and keeps **Ruffle optional and
> explicit-only**. There is no silent system-default or Ruffle fallback.
> Milestone 7.1 adds real `.nova`, `.fla`/XFL and `.swf` import paths, editable
> reconstruction and compatibility reports. Native `.fla` export is refused; the
> experimental XFL ZIP is explicitly unverified.
> **Milestone 7.2 adds the integrated AI Agent Editor**, local/cloud provider
> management with DPAPI-protected credentials, explicit cloud consent, bounded
> user-selected context, diff-confirmed and project-root-confined file changes,
> build diagnostics and `SymbolClass` reconstruction. **Milestone 7.3** adds
> live local-Ollama provider and AI Agent backend validation, a security audit,
> Ruffle visual playback of a document-class/SymbolClass/tween fixture, XFL
> round-trip checks, installer/portable rc2 verification and performance
> measurements. A local Adobe debug projector is configured and starts a window,
> but the tested SWF area rendered blank and no trace log appeared; **visual
> Adobe playback remains NOT VERIFIED**. Ruffle Desktop 0.7.1 visually played the
> M7.3 fixture (marker, linked sprite, tween and labels). The rc2 Windows ZIP and
> Setup.exe are produced and smoke-tested by the packaging scripts; the multi-frame
> SymbolClass round-trip and XFL tween fidelity remain **PARTIAL**. macOS and
> Linux packages are scripted and structurally checked but were not built on
> their native hosts here. See
> [Limitations](#limitations), [docs/AI_AGENT_EDITOR.md](docs/AI_AGENT_EDITOR.md),
> [docs/AI_PROVIDER_GUIDE.md](docs/AI_PROVIDER_GUIDE.md),
> [docs/FORMAT_COMPATIBILITY.md](docs/FORMAT_COMPATIBILITY.md),
> [docs/PACKAGING.md](docs/PACKAGING.md),
> [docs/FLASH_PLAYER_INTEGRATION.md](docs/FLASH_PLAYER_INTEGRATION.md),
> [docs/MILESTONE_7_2_REPORT.md](docs/MILESTONE_7_2_REPORT.md), and
> [docs/MILESTONE_7_3_REPORT.md](docs/MILESTONE_7_3_REPORT.md).

---

## Universal format compatibility

The editor and the `nova` CLI can open three real formats through one shared
project model:

| Format | Import | Export | Important limit |
| --- | --- | --- | --- |
| `.nova` | Full native project | Full native project | NØVA JSON is not an Adobe exchange format |
| `.fla` (ZIP/XFL) | Documented subset | Native `.fla` export refused | Complex XFL geometry, nested symbol timelines, text, bitmap and sound are partial or preserved-only |
| `.swf` | Header, dimensions, FPS, frames, layers, shapes, sprites, placements, labels, ABC and metadata | Real SWF from `.nova`, imported FLA or imported SWF | Reconstruction is not byte-identical; sprites are flattened; preserved ABC is not source |

Every import and export produces a compatibility report. Unsupported source data
is kept in the original file and named in the report; it is never silently
discarded. See [docs/FORMAT_COMPATIBILITY.md](docs/FORMAT_COMPATIBILITY.md).

### Format commands

```powershell
# Import any supported file and save a native .nova project
cargo run -p nova-cli -- import samples/as3-example.swf imported.nova

# Analyze structure, code provenance and compatibility
cargo run -p nova-cli -- analyze samples/as3-example.swf

# Export to SWF, an XFL compatibility ZIP, or back to .nova
cargo run -p nova-cli -- export imported.nova reconstructed.swf
cargo run -p nova-cli -- export imported.nova package.zip
cargo run -p nova-cli -- export imported.nova copy.nova
```

Native `.fla` export is deliberately unavailable. The `.zip` XFL package is a
clearly labelled compatibility package, not a `.fla` file, and has not been
tested in Adobe Animate.

---

## Download, install and platforms

The application is distributed as a native package; target machines do not need
Rust.

| Platform | Package | Status in this repository |
| --- | --- | --- |
| Windows 10/11 x64 | `NOVA-Flash-Studio-<version>-windows-x64-Setup.exe` installer, or the portable `NØVA Flash Studio.exe` in the ZIP under `dist/windows/` | Rebuilt and smoke-tested after M7.2; headless install/uninstall verified in M5/M6 |
| macOS 10.15+ | `NØVA Flash Studio.app` ZIP | Bundle script and metadata prepared; not built here |
| Linux x86_64 | `NØVA-Flash-Studio.AppImage` | AppImage recipe prepared; not built here |

The Windows installer is a self-contained setup wizard that installs **for the
current user by default** (`%LOCALAPPDATA%\Programs\NØVA Flash Studio`, no
admin rights, no UAC prompt). "Install for all users" is an explicit choice
(`C:\Program Files\NØVA Flash Studio` + HKLM, standard UAC elevation, custom
directory as an option, optional `.nova` (wizard default) / `.swf` (default-off)
associations,
scope-aware uninstaller, upgrade handling that never mixes per-user and
machine-wide installs). It needs no product key or activation and works
fully offline. The ZIP remains the portable distribution — independent of any
installed copy. Legal texts ship in both: `LICENSE-MIT.txt`,
`LICENSE-APACHE-2.0.txt`, `THIRD-PARTY-NOTICES.md`, the German
`EULA-DRAFT.md` (draft, pending owner/legal review — see
[docs/LICENSING.md](docs/LICENSING.md)) and `LICENSES/` SPDX texts — see
`legal/`.

Build a package from source:

```powershell
# Windows: tests + release build + branded EXE + optional ZIP
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1

# Windows installer (needs .NET 8 SDK; embeds the ZIP above)
powershell -ExecutionPolicy Bypass -File scripts/package-installer.ps1

# Everything at once (Windows payload + installer)
powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1 -Target windows
```

```bash
# macOS: .app bundle, optional signing/notarization via environment variables
bash scripts/package-macos.sh

# Linux: AppDir + AppImage; appimagetool must be installed
bash scripts/package-linux.sh
```

The current Windows artifacts are produced from workspace version `0.3.0-rc2`:
the portable ZIP, the staged `NØVA Flash Studio.exe`, and the self-contained
`NOVA-Flash-Studio-0.3.0-rc2-windows-x64-Setup.exe`. Sizes and SHA-256 values are
listed in `dist/windows/SHA256SUMS.txt`, which is authoritative even if a
documentation-only rebuild changes a ZIP or Setup hash. The artifacts are
unsigned.

See [docs/PACKAGING.md](docs/PACKAGING.md) for prerequisites, signing,
notarization, AppImage dependencies, CI and the exact artifact layout.
`scripts/verify-packaging.py` checks icon containers, `Info.plist`, AppStream
metadata and desktop-entry keys without claiming a foreign-platform build.

**Development requirements**

- Rust 1.88 or newer (tested with Rust 1.99.1, edition 2024)
- A Vulkan/DX12/Metal-capable GPU and desktop session for the GUI
- Windows was used for development; `eframe` is cross-platform but macOS/Linux
  have not been tested natively in this workspace

```powershell
# Desktop application (loads the built-in demo project)
cargo run -p nova-app

# Run the whole test suite
cargo test --workspace

# Build the sample SWF headlessly
cargo run -p nova-cli -- demo samples/nova-demo.swf

# Build the checked-in AS3 example project
cargo run -p nova-cli -- export samples/as3-example.nova samples/as3-example.swf

# Inspect the result with the independent tag reader
cargo run -p nova-cli -- probe samples/nova-demo.swf
```

The demo build currently reports:

```text
SWF geschrieben: samples/nova-demo.swf (1411 Bytes, 48 Frames, 145 Platzierungen, 4 Shapes, 2 Sprites)
Verifikation: SWF v10 · 550×400 px · 24 fps · 48 Frames · 346 Tags · zlib
```

### CLI

The `nova` binary is a headless companion; it uses the same project model and
build pipeline as the GUI.

| Command | Purpose |
| --- | --- |
| `nova demo [out.swf]` | Build the built-in demo (default `samples/nova-demo.swf`) |
| `nova import <file> [out.nova]` | Import `.nova`, `.fla`/XFL or `.swf` and save a native project |
| `nova analyze <file>` | Print structure, code provenance and compatibility report as JSON/text |
| `nova export <file> [out]` | Export to SWF, an XFL `.zip` package, or `.nova` |
| `nova probe <file.swf>` | Print header, declared/actual size and tag list |
| `nova new <project.nova>` | Create an empty project |
| `nova check <file>` | Validate a NØVA/FLA/SWF project and print a summary |
| `nova build <file> [out.swf]` | Build with compiler/build diagnostics and write the SWF |
| `nova as-check <file.as>` | Type-check AS3 and emit ABC to memory (reports byte count) |
| `nova ai-status` | Print provider diagnostics without API-key values |
| `nova help` | Usage |

---

## Open in Flash Player

After exporting a scene, click **Open in Flash Player** in the toolbar, in the
**Flash Player** menu, or in the export-complete dialog. NØVA validates the SWF
and starts the runtime selected in **Runtime** settings.

- **Adobe Flash Player Projector is the default.**
- **Ruffle is optional** and is used only after an explicit selection.
- There is **no system-default handler** and **no silent fallback** to Ruffle if
  Adobe is missing or invalid.

On first launch, if no valid Adobe path is configured, NØVA shows **Flash Player
Projector einrichten**. The dialog explains why a runtime is needed, states that
NØVA does not download or bundle Adobe binaries, and offers a path field,
**Durchsuchen …**, **Prüfen**, **Speichern und fortfahren**, and **Später
einrichten**. Choosing “Später” keeps the app usable and persists
`first_launch_setup_done: true`; opening a SWF then shows a configuration error
until a runtime is selected.

Open **Flash Player → Runtime-Einstellungen …** to:

- select **Adobe Flash Player Projector (empfohlen)** or **Ruffle (nur nach
  ausdrücklicher Auswahl)**;
- enter or browse to the Adobe projector executable (or a macOS `.app`);
- enter or browse to an optional Ruffle executable;
- run a structural **Prüfen**, inspect live status and the last validation
  result, reset the draft, copy diagnostics and save for the next launch.

No shell command is built from a path. Windows and Linux use
`Command::new(executable).arg(swf)`; macOS uses the same form for an executable
or `open -a <app> <swf>` for a bundle. This preserves spaces, quotes, Unicode and
other special characters. A successful process start is reported as
“gestartet; Wiedergabe nicht verifiziert” — NØVA never claims playback merely
because `spawn()` succeeded.

NØVA does not silently change `.swf` associations. The Windows installer offers
an explicit, **default-off** `.swf` association and a wizard-default `.nova`
association. If the `.swf` association is selected, Windows
passes the SWF path to NØVA as a startup argument; NØVA recognises it, does not
parse it as a project, and opens it through the configured runtime. macOS and
Linux packages do not register a `.swf` handler. See
[docs/FLASH_PLAYER_INTEGRATION.md](docs/FLASH_PLAYER_INTEGRATION.md) for the
association analysis, diagnostics and validation layers.

---

## The application

The window is a dark, four-region authoring layout:

```text
┌──────────────────────────────────────────────────────────────────────┐
│ Datei  Bearbeiten  Einfügen  Ansicht  Hilfe        [Code|Animation]  │
├───────────────┬──────────────────────────────────────┬───────────────┤
│ Projekt       │                                      │ Eigenschaften │
│  Szenen       │        Bühne (Animation mode)        │  Szene        │
│  Ebenen       │        or AS3 editor (Code mode)     │  Ebene        │
│  Symbole      │                                      │  Objekt       │
│  Skripte      │                                      │  Symbol       │
│  Assets       │                                      │  Export       │
├───────────────┴──────────────────────────────────────┴───────────────┤
│ Timeline / Konsole / Probleme                                        │
├──────────────────────────────────────────────────────────────────────┤
│ Status: Frame, Auswahl, Zoom, Meldungen                              │
└──────────────────────────────────────────────────────────────────────┘
```

UI labels are German; identifiers and source code are English. Two modes share
one project:

- **Animation** — stage with zoom/pan, grid, onion skin, selection handles,
  direct dragging, a multi-layer timeline with keyframes, held frames, motion
  tweens, labels and a playhead, plus playback at the scene FPS.
- **Code** — project scripts as tabs, a syntax-highlighted AS3 editor with line
  numbers, diagnostics, and a problems list that jumps to a source position.

The console dock is a persistent QuickJS context. It exposes `nova.version`,
`nova.log(...)` and a read-only `nova.project` snapshot. It is for editor tools
and extensions — not an ActionScript runtime.

The **AI Agent Editor** is an additional integrated workspace, opened from the
toolbar, the **KI** menu or `Ctrl+Shift+A`. It replaces the central authoring
area with resizable project/context, source, chat and output panels while
preserving the normal editor state. It uses the same project model, compiler and
undo history; it is not a second editor. See
[docs/AI_AGENT_EDITOR.md](docs/AI_AGENT_EDITOR.md) and
[docs/AI_PROVIDER_GUIDE.md](docs/AI_PROVIDER_GUIDE.md).

### Keyboard shortcuts

| Keys | Action |
| --- | --- |
| `Ctrl+N` / `Ctrl+O` | New / open project |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / save as |
| `Ctrl+E` | Export film (SWF or XFL package) |
| `Ctrl+Shift+A` | Open/close the integrated AI Agent Editor |
| `Ctrl+Z` / `Ctrl+Y` / `Ctrl+Shift+Z` | Undo / redo |
| `Ctrl+C` / `Ctrl+V` / `Ctrl+D` | Copy / paste / duplicate selected keyframes |
| `Space` | Play/pause (Animation mode) |
| `F5` / `Shift+F5` | Insert / remove frame |
| `F6` / `Shift+F6` | Insert keyframe / blank keyframe |
| `Delete` | Delete selection |
| `Escape` | Clear selection |
| `←` / `→` | Previous / next frame |
| `Home` / `End` | First / last frame |
| Shift-click / drag empty stage | Add to selection / marquee select |
| Mouse wheel over stage | Zoom around pointer |
| Drag empty stage / middle-drag | Pan camera |
| Drag object | Move selection at the governing keyframe |

Clicking or dragging the timeline ruler scrubs the playhead. Timeline frame cells
have context menus for keyframe operations.

---

## What works

- **Project model** — scenes, layers, keyframed object placements, held frames,
  motion tweens with easing, frame labels, graphic symbols, linked scripts,
  assets metadata, export configuration.
- **Versioned `.nova` format** — JSON envelope with a format tag and version,
  validation, future-version rejection, migration hook, atomic save.
- **Stage editor** — fit/zoom/pan camera, grid, onion skin, hit-testing including
  rotation and ellipses, drag-to-move with keyframe targeting, shift-click and
  marquee multi-select, group move/scale/rotate around the shared center.
- **Timeline** — custom-painted layer lanes, keyframe/held-frame distinction,
  tween indicators, labels, playhead, ruler scrubbing, per-cell menus, zoom,
  keyframe range marking, copy/paste/duplicate, cross-layer moves, frame insert
  and remove.
- **Authoring history** — snapshot-based undo/redo with transaction grouping,
  labels, redo invalidation and a bounded history; it covers object creation,
  deletion, transforms, keyframes, layers and timeline edits.
- **Tween authoring** — create/remove a motion tween between two keyframes,
  choose linear/ease-in/ease-out/ease-in-out, edit duration; the same easing
  model is used by the stage, playback and exporter.
- **Autosave** — configurable interval, dirty detection, atomic snapshots next to
  the project or in the temp recovery directory, freshness check, recovery prompt,
  discard action and cleanup after a normal save.
- **Playback** — frame stepping and FPS-accurate playback with looping.
- **SWF export** — FWS/CWS, SWF 10 AVM2 header, `FileAttributes`,
  `SetBackgroundColor`, `DefineShape3` rectangles/ellipses/rounded rectangles and
  imported paths, one-frame `DefineSprite` symbols, per-frame
  `PlaceObject2`/`RemoveObject2`, frame labels, zlib compression, names and IDs
  deduplicated. Imported ABC is embedded unchanged.
- **Universal format layer** — `.nova`, ZIP/XFL `.fla` and `.swf` share one
  project model. Import/export reports name unsupported tags, approximated
  geometry, lost editability and preserved bytecode. Native `.fla` export is
  refused; an explicitly unverified XFL ZIP is available instead. See
  [docs/FORMAT_COMPATIBILITY.md](docs/FORMAT_COMPATIBILITY.md).
- **AS3 compiler** — hand-written lexer/parser/AST, symbol and type checking,
  source diagnostics, and a real AVM2 ABC writer: constant pools, multinames,
  methods, traits, instance/class/script initializers, constructors, control flow
  and verifier-oriented bytecode. Supported source compiles on export.
- **SWF script linking** — `DoABC2`, `SymbolClass`, document-class linking and
  one-frame sprite wrappers for object scripts; SWF version is raised to at least
  10 when scripts are embedded. Imported `SymbolClass` character links are
  collected and re-emitted where the referenced object can be reconstructed;
  unlinked or unmappable links are reported instead of being invented.
- **Inspection** — an independent tag reader in `nova-swf` plus a CLI `probe`
  command; emitted SWF and ABC are cross-checked with Ruffle's external `swf`
  crate, including its AVM2 ABC reader.
- **QuickJS console** — persistent context, captured logs, time/memory limits,
  interrupt guard, project JSON snapshot.
- **Desktop shell** — menus, mode switch, properties inspector, file tree,
  open/save/export dialogs, unsaved-changes prompts, help/about/export dialogs.
- **Branding** — shared cyan/violet NØVA icon in ICO/ICNS/PNG formats, native
  window icon, branded animated splash with version, skip and fade, and readable
  startup failure diagnostics.
- **External runtime integration** — exact **Open in Flash Player** action,
  Adobe-default/Ruffle-explicit runtime selection, first-launch setup, SWF and
  runtime validation, per-user JSON settings, safe separate-argument launch,
  `.swf` startup handling, diagnostics and clear error dialogs.
- **AI Agent Editor** — integrated workspace with project/context selection,
  source editor, chat, build output, diagnostics, diff review and file undo.
  Local Ollama, Ollama Cloud, Groq and OpenAI-compatible providers; model
  discovery, connection tests, enable/disable, default and per-session provider
  selection. API keys use Windows DPAPI (with an explicitly labelled fallback
  elsewhere), never provider JSON or logs. Cloud requests require explicit
  consent. File changes are root-confined, atomic, previewed as a diff and
  applied only after confirmation; preserved ABC cannot be replaced or deleted.
- **Packaging** — custom Windows icon/version resource embedding, Windows ZIP
  script, a self-contained branded setup wizard with per-user/all-users
  install, custom directory, optional file associations, uninstaller and
  upgrade handling, bundled license/EULA/third-party notices, macOS `.app`
  bundle template with signing/notarization hooks, Linux AppDir/AppImage
  recipe, versioned release scripts and an optional GitHub Actions matrix.

## Limitations

These are deliberate, visible limits of the current prototype — not hidden
fallbacks:

- **The AS3 compiler supports a documented subset, not full ActionScript 3.**
  Unsupported constructs are rejected with file/line diagnostics; they are never
  silently translated. See [docs/AS3_COMPILER.md](docs/AS3_COMPILER.md).
- **Adobe playback remains unverified; Ruffle visual playback is verified only
  for the M7.3 fixtures.** Ruffle Desktop 0.7.1 executed a regenerated
  `as3-example` SWF through NØVA's launch path (`avm_trace: Box gestartet`, no
  AVM2 errors) and rendered the M7.3 `validation.swf` with the document-class
  marker, linked sprite rotation, tween motion and timeline labels. A local Adobe
  Flash Player debug projector is configured in this development environment;
  its process and window start, but the SWF area rendered blank for the tested
  files and no `flashlog.txt` trace appeared. Adobe visual playback and Adobe
  compatibility are therefore **NOT VERIFIED**. NØVA does not ship a runtime,
  and a successful process start is not reported as playback. The checked-in
  `samples/as3-example.swf` predates the AVM2 fixes; regenerate it before runtime
  testing.
- **FLA support is a documented XFL subset.** `.fla` import reads document
  settings, timelines, layers, frames, labels, basic shapes, symbol instances and
  `scripts/*.as`. Complex edge geometry, nested symbol timelines, text, bitmaps,
  sounds and filters are partial or preserved-only. Native `.fla` export is
  refused; the experimental XFL ZIP has not been opened in Adobe Animate/Flash.
  The M7.3 XFL round-trip preserved scripts and document settings but lost the
  authored tween and left frames 1–22 empty; this is a documented limitation, not
  Adobe compatibility.
- **SWF reconstruction is lossy.** Sprites are flattened, strokes/gradients and
  unsupported tags are reported rather than fully editable, and preserved ABC is
  bytecode, not source. `SymbolClass` character links are reconstructed only when
  the referenced character can be mapped to an imported object; links to
  unsupported, missing or ambiguous characters are reported and not invented.
  A re-exported scripted SWF can therefore still differ from the original. In
  M7.3, a one-frame `SymbolClass` fixture survived import/re-export and still
  executed its linked class in Ruffle, but a multi-frame rebuilt SWF preserved the
  structural links without visibly executing the linked class across rebuilt
  frames.
- **The AI Agent Editor is not a full autonomous coding agent.** It sends one
  bounded, user-selected context per request; it does not upload the project by
  default, execute shell commands or apply file changes without a confirmed diff.
  Cloud use requires explicit per-session consent. Responses are not streamed,
  and the model can propose incorrect code. Preserved ABC is never replaced by
  reconstructed source. Provider availability and data retention are outside
  NØVA's control. M7.3 validated the backend workflow live against local Ollama
  with `qwen2.5-coder:7b` (explanation, reject, accept, build and file undo), but
  the full interactive GUI workflow and cloud/Groq providers remain unverified.
  See [docs/AI_AGENT_EDITOR.md](docs/AI_AGENT_EDITOR.md).
- **macOS/Linux packages were not built here.** Their scripts and metadata are
  checked structurally; a native build on each platform (or the prepared CI
  matrix) is still required before calling those artifacts tested.
- **Scripted objects are code-owned after the first placement.** The exporter
  places them once at a stable depth above the baked timeline; their authored
  timeline tweens are not re-applied every frame. Non-scripted objects are still
  baked frame by frame.
- **External Flash APIs are mostly dynamic surfaces.** `MovieClip`/`Sprite`
  inheritance, `addEventListener`, `Event`, display properties and similar calls
  are emitted as dynamic multiname accesses. Only `trace`, `getTimer`, `isNaN`,
  `isFinite`, `parseInt` and `parseFloat` are recognized as built-in free
  functions. There is no complete `playerglobal` declaration set.
- **Only the active scene is exported.** Other scenes are ignored with a warning.
- **Graphics only.** No text, bitmaps, sounds, fonts, video or filters. Assets
  exist in the model but are not embedded.
- **Symbols are one-frame sprites.** Nested symbol timelines, symbol editing
  modes, masks and guide layers are not implemented (the data model has layer
  kinds, but the editor/exporter do not act on masks/guides).
- **Layer management is incomplete.** Rename, visibility and lock exist, but
  reordering/folder semantics are not a complete workflow.
- **No HTML5 export.** It is an optional later target, not implemented.
- **Character-ID ceiling.** At most 65,535 SWF character IDs; export fails
  cleanly beyond that.

## Manual projector test

Until this passes on a real Adobe Projector, treat Adobe playback as unverified.
Ruffle playback of the regenerated example is already verified (see
`docs/SWF_RUNTIME_COMPATIBILITY.md`).

1. Build the samples:
   `cargo run -p nova-cli -- demo samples/nova-demo.swf` and
   `cargo run -p nova-cli -- export samples/as3-example.nova samples/as3-example.swf`.
2. Obtain a standalone **Adobe Flash Player Projector** build for your platform
   (the projector, not the browser plug-in). Flash Player is end-of-life and
   unsupported; use an offline machine and only files you trust. NØVA does not
   download or bundle it.
3. Start NØVA. If the first-launch dialog appears, browse to the projector and
   use **Prüfen** and **Speichern und fortfahren**. Alternatively, open
   **Flash Player → Runtime-Einstellungen …**, keep Adobe selected (the default),
   and set the projector path. Ruffle is optional and must be selected
   explicitly; it is never chosen automatically when Adobe is missing.
4. Use **Open in Flash Player** after exporting, or double-click a `.swf` if the
   optional Windows association was installed. Opening the file directly in the
   player is equivalent for a playback smoke test.
5. Expected result for the demo:
   - stage 550 × 400 px, dark blue-grey background, 24 fps;
   - 48 frames, looping;
   - the amber title bar is linked to `Main` and moves horizontally via its
     `ENTER_FRAME` script, wrapping at the right edge;
   - a cyan dot travels down the right side;
   - an orange emblem instance rotates 180° and moves upward.
6. Open a freshly exported `as3-example.swf`: a rotating amber box should move
   left/right and reverse direction at the stage edges.
   `trace("Box gestartet")` should appear in a debug/player log if the projector
   exposes `trace`.
7. If a file does not open or behaves differently, record the projector build,
   OS, exact error, `nova probe` output, and whether path validation, process
   launch and actual playback each succeeded. Do not describe the exporter as
   Adobe-projector-verified until this test succeeds. A started process alone is
   not playback evidence.

## Repository layout

```text
crates/
  nova-core/    project model, demo, validation, JSON persistence, timeline resolution
  nova-swf/     spec-based bit writer, shapes, tags, movie assembly, inspector
  nova-build/   Project -> SWF baking, script compilation, DoABC2/SymbolClass
  nova-as/      AS3 lexer, parser, AST, type checker, ABC emitter, diagnostics
  nova-script/  QuickJS host for tools/extensions
  nova-ai/      providers, DPAPI credentials, context, safe edits, build diagnostics
  nova-app/     eframe desktop UI, branding, splash, player integration and AI workspace
  nova-cli/     headless build/inspect/check companion
assets/
  branding/     editable SVG master and rendered PNG sizes
  icons/        Windows ICO, macOS ICNS, Linux hicolor PNGs
packaging/
  windows/      installer resources + installer/ (WinForms setup wizard)
  macos/        Info.plist and PkgInfo templates
  linux/        desktop entry, AppRun and AppStream metadata
legal/
  LICENSE-*.txt            project license texts (mirror Cargo.toml)
  THIRD-PARTY-NOTICES.md   generated dependency/license inventory
  EULA-DRAFT.md            draft EULA, pending owner/legal review
scripts/
  generate-icons.py       deterministic PNG/ICO/ICNS generation
  verify-packaging.py     icon, metadata, legal and installer checks
  gen-third-party-notices.py  regenerate legal/THIRD-PARTY-NOTICES.md
  package-windows.ps1     native Windows build + ZIP + smoke tests
  package-installer.ps1   Windows setup wizard exe (dotnet publish)
  package-macos.sh        .app bundle + optional signing/notarization
  package-linux.sh        AppDir + AppImage
  build-release.ps1       release target dispatcher
samples/
  nova-demo.swf      generated, independently parsed demo
  as3-example.nova   checked-in project with class, fields and ENTER_FRAME method
  as3-example.swf    generated example with DoABC2/SymbolClass
docs/
  FEASIBILITY.md     what is proven, what is not, and runtime risk
  ARCHITECTURE.md    crate boundaries, data flow, formats and UI design
  ROADMAP.md         milestones from this prototype to a usable studio
  AS3_COMPILER.md    supported language subset and diagnostics
  AVM2_BYTECODE.md   ABC layout and bytecode emission details
  SWF_EXPORT.md      tag pipeline, script linking and validation
  BRANDING.md        icon system, splash behavior and asset regeneration
  PACKAGING.md       Windows/macOS/Linux builds, signing and CI
  FLASH_PLAYER_INTEGRATION.md  Adobe/Ruffle runtime policy, safety, associations
  SWF_RUNTIME_COMPATIBILITY.md structural vs runtime evidence
  AI_PROVIDER_GUIDE.md   provider setup, DPAPI credentials, privacy and diagnostics
  AI_AGENT_EDITOR.md     integrated AI workspace, context and diff-confirmed edits
  TESTING.md         automated and manual QA results
  MILESTONE_7_REPORT.md  runtime integration report
  MILESTONE_7_1_REPORT.md  format layer report
  MILESTONE_7_2_REPORT.md  AI Agent Editor, provider and SymbolClass report
  MILESTONE_7_3_REPORT.md  real-world validation, security, performance and RC2 report
```

## Documentation

- [Feasibility](docs/FEASIBILITY.md) — honest assessment of projector-compatible
  SWF and real AS3 support.
- [Architecture](docs/ARCHITECTURE.md) — how the crates, model, exporter and UI
  fit together.
- [AS3 compiler](docs/AS3_COMPILER.md) — supported subset, checks and errors.
- [AVM2 bytecode](docs/AVM2_BYTECODE.md) — ABC pools, traits, methods and bodies.
- [SWF export](docs/SWF_EXPORT.md) — baked timeline, `DoABC2`, `SymbolClass` and
  validation.
- [Format compatibility](docs/FORMAT_COMPATIBILITY.md) — supported FLA/XFL and
  SWF subsets, conversion results, code provenance and explicit non-claims.
- [Branding](docs/BRANDING.md) — icon design, formats, splash timing and
  regeneration.
- [Packaging](docs/PACKAGING.md) — reproducible Windows/macOS/Linux packaging,
  signing, notarization, AppImage and CI.
- [Flash Player integration](docs/FLASH_PLAYER_INTEGRATION.md) — Adobe-default
  and Ruffle-explicit runtime policy, first launch, Runtime settings, safe
  process launching, validation layers and file associations.
- [SWF runtime compatibility](docs/SWF_RUNTIME_COMPATIBILITY.md) — structural,
  process-launch and actual-playback evidence, including Ruffle and the Adobe
  limitation.
- [AI Agent Editor](docs/AI_AGENT_EDITOR.md) — integrated workspace, context
  selection, diff review, safe file operations and provenance rules.
- [AI provider guide](docs/AI_PROVIDER_GUIDE.md) — local/cloud providers,
  DPAPI credentials, privacy model, model discovery and diagnostics.
- [Testing & QA](docs/TESTING.md) — automated tests, CLI checks, GUI runtime
  integration evidence and known untested areas.
- [Milestone 7 report](docs/MILESTONE_7_REPORT.md) — runtime integration changes,
  evidence and remaining verification work.
- [Milestone 7.1 report](docs/MILESTONE_7_1_REPORT.md) — universal format
  architecture, FLA/SWF subsets, conversion evidence and non-claims.
- [Milestone 7.2 report](docs/MILESTONE_7_2_REPORT.md) — AI Agent Editor,
  provider/credential layer, safe edits, SymbolClass reconstruction and test
  evidence.
- [Milestone 7.3 report](docs/MILESTONE_7_3_REPORT.md) — live Ollama validation,
  security audit, Ruffle visual playback, XFL/installer verification, performance
  and the rc2 release-candidate evidence.
- [Licensing](docs/LICENSING.md) — license texts, third-party notices, EULA
  draft status and the no-activation/no-telemetry policy.
- [Roadmap](docs/ROADMAP.md) — what comes next and what "done" means.

## License

MIT OR Apache-2.0 at your option, matching the workspace `license` field in
`Cargo.toml`; full texts are in `legal/` and shipped in every package — see
[docs/LICENSING.md](docs/LICENSING.md). No product key, activation or account
is required and the application works fully offline. The bundled third-party
Rust dependencies are listed with their license expressions in
`legal/THIRD-PARTY-NOTICES.md` with SPDX texts under `LICENSES/`.
`legal/EULA-DRAFT.md` is a **German draft** end-user agreement that requires
owner/legal review before release — it is not a finalized license decision
and does not restrict the open-source license rights. Flash and Adobe are
trademarks of Adobe Inc.; this project is not affiliated with or endorsed by
Adobe.
