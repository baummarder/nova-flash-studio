# Architecture

NØVA Flash Studio is a Cargo workspace with a strict separation between the
document model, the SWF writer, the AS3 compiler, the script host and the desktop
UI. The central rule is: **the stage, the timeline, playback and the exporter all
use the same resolved-timeline function**. There is no second, "export-only"
animation interpretation.

A second rule applies to scripts: **the compiler either produces complete ABC or
returns diagnostics; it never emits a partial script and never translates AS3 to
JavaScript.**

## 1. Crate graph

```text
                    ┌──────────────┐
                    │  nova-core   │  model, validation, JSON, timeline
                    └──────┬───────┘
                           │
          ┌────────────────┼─────────────────┐
          │                │                 │
   ┌──────▼──────┐  ┌──────▼──────┐   ┌──────▼──────┐
   │  nova-build │  │   nova-as   │   │ nova-script │
   │ baking +    │  │ AS3 compiler│   │ QuickJS host│
   │ script link │  │ → ABC       │   │ (tools)     │
   └──────┬──────┘  └──────┬──────┘   └──────┬──────┘
          │                │                 │
   ┌──────▼──────┐         │                 │
   │  nova-swf   │         │                 │
   │ writer +    │         │                 │
   │ inspector   │         │                 │
   └──────┬──────┘         │                 │
          │                │                 │
          └────────────────┼─────────────────┘
                           │
                    ┌──────▼───────┐
                    │  nova-ai     │  providers, context, safe edits
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │  nova-app    │  eframe/egui desktop UI
                    └──────────────┘
                    ┌──────────────┐
                    │  nova-cli    │  headless build/inspect/check
                    └──────────────┘
```

- **`nova-core`** has no UI or SWF dependency. It owns `Project`, scenes,
  layers, frames, objects, symbols, scripts, assets, export config, validation and
  persistence. Both the GUI and CLI consume it.
- **`nova-swf`** knows nothing about projects. It writes and reads SWF bytes.
- **`nova-build`** is the only crate that translates the document model into SWF
  tags. It depends on `nova-core`, `nova-as` and `nova-swf`, compiles scripts,
  links classes to characters and bakes the timeline.
- **`nova-formats`** owns the shared import/export layer for `.nova`, `.fla`/XFL
  and `.swf`. It depends on `nova-core` and `nova-build`, keeps format-specific
  code in separate modules and produces compatibility reports and analysis data.
- **`nova-as`** is a self-contained AS3 compiler: lexer, parser, AST, symbol and
  type checker, ABC emitter and diagnostics. It does not depend on the editor or
  the SWF writer.
- **`nova-script`** embeds QuickJS for tools/extensions only. It is deliberately
  not wired into animation or AS3 execution.
- **`nova-ai`** is UI-independent: provider configuration, HTTP transport
  abstraction, DPAPI/RestrictedFile credentials, bounded project context,
  cloud-consent enforcement, root-confined file previews/atomic apply/undo and
  build diagnostics. It depends on `nova-core`, `nova-as` and `nova-build`; it
  never depends on `egui`.
- **`nova-app`** is an `eframe` binary (`nova-flash-studio`) that composes the
  panels and maps user intent to `UiAction`s. Its `ai_editor`/`ai_providers`
  modules render the `nova-ai` state and execute prepared requests on a worker.
- **`nova-cli`** (`nova`) exposes the same model and build pipeline without a
  window, which keeps export testable in CI; `nova ai-status` and
  `nova build` expose AI-provider and build diagnostics.

Dependencies are one-directional. In particular, `nova-core` never imports
`egui`; the UI converts model colors and geometry in `nova-app/src/util.rs`.

## 2. Document model

### Project

```text
Project
├── metadata        name, author, description, timestamps
├── scenes[]        named stages with size, background, fps
│   └── layers[]    ordered bottom-to-top
│       └── frames[] one slot per frame index
│           ├── keyframe: bool
│           ├── objects[]: ObjectInstance
│           ├── tween?: Easing
│           └── label?
├── symbols[]       reusable single-shape graphics
├── scripts[]       virtual AS3 files (path + source + optional object link)
├── assets[]        metadata for future bitmap/audio/font embedding
├── export          target runtime, SWF version, compression, script flag
└── activeScene
```

`ObjectInstance` has a stable UUID, name, kind (`Rect`, `Ellipse`, `Symbol`),
transform, fill and optional script link. The transform origin is the **object
center**, which matches the SWF placement matrix and the stage manipulation
handles.

### Frame semantics

- Layers are stored bottom-to-top. `resolve_scene` returns objects in that order,
  and `nova-build` uses the resulting index as SWF depth (bottom = depth 1).
- Only keyframes carry objects. A held frame resolves the most recent keyframe at
  or before the requested frame.
- A keyframe with a `tween` interpolates towards the **next keyframe**, but only
  for objects with the **same UUID** in both keyframes. Objects without a match
  are held.
- Easing is applied to the normalized span before transform and fill
  interpolation. `EaseIn`, `EaseOut`, `EaseInOut` and `Linear` are implemented.
- Hidden layers are skipped. Folder layers are skipped as render containers; mask
  and guide kinds are currently treated as normal layers by the renderer/exporter.
- `Scene::frame_count()` is the longest layer, minimum 1. Frames are zero-based
  internally and one-based in the UI.

### Invariants and validation

`Project::validate()` runs after loading and before saving/exporting. It rejects:

- empty project names, missing scenes, invalid stage size (1–8192 px) or FPS
  (1–120);
- duplicate scene/layer/symbol IDs (IDs are checked globally, not only per list);
- duplicate object IDs inside the same frame;
- symbol or script references that do not exist;
- non-finite transforms, empty layers, invalid symbol dimensions;
- an active-scene ID that does not exist.

The validation is structural, not semantic. It does not check whether an AS3
script compiles or whether a tween has matching endpoints.

## 3. Project file (`.nova`)

The file is UTF-8 JSON with a versioned envelope:

```json
{
  "format": "nova-flash-studio",
  "formatVersion": 1,
  "project": {
    "id": "…",
    "metadata": { "name": "NØVA Demo", "author": "", "…": "…" },
    "scenes": [ … ],
    "symbols": [ … ],
    "assets": [ … ],
    "scripts": [ … ],
    "export": {
      "target": "avm2",
      "swfVersion": 10,
      "compression": "zlib",
      "includeScripts": true
    },
    "activeScene": "…"
  }
}
```

- `FORMAT_TAG = "nova-flash-studio"`, `CURRENT_FORMAT_VERSION = 1`.
- A different tag produces `NotAProject`; a newer version produces
  `UnsupportedVersion`. Older versions go through `migrate()`; version 1 has no
  migration yet, but the hook exists so future changes never guess silently.
- Saving validates first, writes to `*.nova.tmp`, then replaces the target with a
  rename. This avoids leaving a half-written project.
- Unknown fields are ignored by serde, which gives forward-compatible reads for
  additive changes; structural changes must bump the format version.

## 4. UI architecture

`nova-app` is an immediate-mode `egui` application on the `eframe`/wgpu backend.
`main.rs` owns the native window; `app.rs` owns the `NovaApp` struct:

```text
NovaApp
├── project: Project
├── state: AppState        UI-only state (selection, camera, dock, dialogs…)
├── engine: Option<ScriptEngine>
├── history: History       bounded project snapshots + transaction labels
├── ai: AiEditor           provider/session/workspace state for the AI workspace
└── pending_before: Option<Project>   pre-transaction snapshot
```

Undo/redo uses whole-project snapshots. A UI interaction starts a transaction,
several edits are coalesced into one history entry, and the entry receives a
human-readable label (for example "Objekt verschieben" or "Schlüsselbild
einfügen"). Snapshots are bounded so long sessions do not grow without limit.

### Panels and modules

| Module | Responsibility |
| --- | --- |
| `app.rs` | menus, mode switch, toolbar, dock, status bar, shortcuts, dialogs, file/export flows, history/autosave flow |
| `history.rs` | bounded snapshot undo/redo with labels and transaction grouping |
| `autosave.rs` | atomic autosave, recovery discovery/freshness checks, discard |
| `tree.rs` | project tree: scenes, layers, symbols, scripts, assets |
| `stage_view.rs` | stage camera, grid, onion skin, object painting, hit-testing, drag, shift-click/marquee selection |
| `timeline_view.rs` | custom-painted layer lanes, cells, keyframe/tween/label markers, playhead, ruler scrub |
| `properties.rs` | scene/export/layer/object/symbol inspector and edits |
| `code_view.rs` | script tabs, highlighted editor, diagnostics, problem list |
| `ai_editor.rs` | integrated AI workspace: context selection, source, chat, build/diagnostics/diff/activity, apply and undo |
| `ai_providers.rs` | provider add/edit/enable/default/model/test/key/remove dialog and credential status |
| `console.rs` | QuickJS console UI and project context |
| `highlight.rs` | pure AS3 tokenizer/highlighter with line-offset mapping |
| `state.rs` | `AppState`, `Mode`, `Selection`, `Dialog`, `PendingAction`, `UiAction` |
| `util.rs` | color conversion, object bounds/hit-testing, keyframe-governing lookup |
| `theme.rs` | dark palette and egui style setup |

### Intent flow

Widgets do not borrow the project mutably across the UI and do not perform global
actions inline. They return `Vec<UiAction>`; `NovaApp::handle_actions` applies
them after all panels have been drawn. This keeps borrows local and makes actions
testable and reusable from menus, shortcuts and buttons.

`AppState` is UI-only: selection, current frame, playback accumulator, zoom,
camera center, onion/grid toggles, bottom tab, console lines, dirty flag, path,
dialog, drag state, status text.

Keyboard shortcuts are suppressed while a modal is open and while text input has
focus (except file shortcuts).

### Stage camera

`stage_view::View` maps between stage pixels and screen pixels:

```text
screen = rect.center + (stage - camera_center) * zoom
stage  = camera_center + (screen - rect.center) / zoom
```

- First display fits the stage with a 36 px margin, clamped to zoom 0.05–4.0.
- Wheel zoom keeps the stage point under the cursor pinned.
- Dragging empty space or middle-dragging pans.
- Hit-testing converts the pointer to stage space, iterates resolved objects
  front-to-back, and tests local coordinates. The object center is the origin;
  rotation is undone and scale is divided out. Ellipses use the normalized
  ellipse equation; other shapes use a rectangle.
- Dragging edits `util::governing_object_mut(scene, layer_id, frame, id)`, not
  `Project::find_object`'s first match. This matters because a tween's start and
  end keyframes deliberately share an object ID; editing the wrong one would not
  affect the visible frame.
- Shift-click adds/toggles objects in the selection. Dragging empty stage draws a
  marquee; objects whose transformed bounds intersect it are selected.
- Multi-selection transforms are applied around the group center. The drag start
  stores every object's original transform to avoid accumulating float drift.

### Timeline

The timeline is painted rather than assembled from thousands of widgets. Each
cell registers an interaction rect for click/context menus. The ruler supports
click/drag scrubbing through the pure `frame_at_x` helper. Playback advances with
`stable_dt`, accumulating fractional frames at the scene FPS and looping or
stopping at the end.

Keyframe operations are model-level actions: select one or more cells, copy
(`Ctrl+C`), paste (`Ctrl+V`) or duplicate (`Ctrl+D`), move across layers, insert a
frame (`F5`) or remove one (`Shift+F5`). Multi-layer copies are rejected with a
warning rather than silently guessing a mapping. Tween creation/removal and
easing/duration edits are regular `UiAction`s, so they participate in undo/redo.

### Properties

The inspector edits the governing keyframe for object properties, and the scene
or layer directly for their properties. It never edits a resolved copy. Symbol
and export settings are edited on the project. With a multi-object selection it
shows shared transform controls and applies the delta around the group center.
Edits set `dirty` and update the modification timestamp.

### Autosave

Autosave is driven from the app update loop. It writes to
`<project>.nova.autosave`, or to
`%TEMP%\nova-flash-studio\untitled.nova.autosave` for unsaved projects. The write
is atomic (temporary file + replace). Startup checks recovery files and offers
restore/discard. A recovery snapshot carries a project revision; a snapshot older
than the currently loaded project is rejected so it cannot overwrite newer work.
Normal saving clears the corresponding autosave.

### AS3 editor

`highlight.rs` is a pure tokenizer over the source; `code_view.rs` turns tokens
into an `egui` layout job and maps diagnostics to character offsets (the editor's
cursor index is a character index, not a byte offset, so Unicode is handled
correctly). Diagnostics carry line/column and render a caret via
`Diagnostic::render`.

The editor's automatic check runs the parser/sema path for fast feedback. Export
runs the full compiler (`nova_as::compile_with_options`); compiler errors are
rendered with the script path and abort the build. The UI never presents the
QuickJS console as AS3 execution.

### AI Agent Editor

The AI workspace is an application-level surface, not a second document model.
`ai_editor.rs` borrows the open `Project` and `AppState`; `ai_providers.rs`
edits `ProviderManager` settings. The flow is:

```text
user message
  -> AgentSession::prepare (UI thread)
       provider selection, cloud consent, credential read,
       bounded context builder, system + recent messages
  -> PreparedRequest
  -> worker thread: execute_prepared(ReqwestTransport)
  -> AgentOutcome (message, proposed changes, optional build action)
  -> preview every change against Workspace (root-confined)
  -> user reviews diff
  -> accept: atomic writes + project-model update (with snapshot rollback)
  -> reject: no writes
```

`PreparedRequest` keeps the API key out of UI state after preparation. The
worker result is delivered through an `mpsc` channel and polled from the update
loop; the UI stays responsive. `Workspace` rejects absolute paths, drive
prefixes, `..`, symlink escapes, create-over-existing and modify-missing. It
keeps previous content for file undo. Preserved ABC scripts are blocked from
modify/delete before a preview is offered. Project-model updates happen only
after disk writes succeed; a project snapshot is restored on a model error and
already-written files are rolled back.

Cloud classification is endpoint-based (`localhost`, loopback and single-label
hosts are local; everything else is cloud). A cloud request is rejected unless
the session has explicit consent. Context is opt-in per script and bounded, so
the whole project is not uploaded by default. See
[AI_AGENT_EDITOR.md](AI_AGENT_EDITOR.md) and
[AI_PROVIDER_GUIDE.md](AI_PROVIDER_GUIDE.md).

## 5. SWF export pipeline

`nova_build::build_swf(project)` performs these steps:

1. **Validate** the project, select the active scene.
2. **Collect warnings** for ignored scenes, assets and the AVM1 target. Warnings
   are returned, not silently dropped.
3. **Compile scripts** when `export.include_scripts` is enabled and the target is
   AVM2. Every script goes through `nova_as::compile_with_options`. Error
   diagnostics abort the build with `BuildError::ActionScript`; warnings are
   collected.
4. **Choose the SWF version.** Embedded scripts force version ≥ 10 because
   `DoABC2` is a SWF 10 tag.
5. **Create the movie** with `Movie::new_avm2` (or `new_avm1`), which installs
   `FileAttributes` and `SetBackgroundColor`.
6. **Resolve every frame** with `nova_core::resolve_scene`, producing a
   `Vec<Vec<ResolvedObject>>`. This is the same data the stage draws.
7. **Define library symbols** as one-frame sprites: each symbol gets a
   `DefineShape3` plus a `DefineSprite` containing that shape, `ShowFrame`, and
   the sprite's own `End`.
8. **Deduplicate scene shapes** by `(geometry, fill)` with geometry quantized to
   1/100 px. Each unique key becomes one `DefineShape3`; instances map to the
   character ID.
9. **Link scripted objects.** A display class attached to an object gets a
   one-frame wrapper `DefineSprite`; the object's character mapping points to the
   wrapper. `SymbolClass` maps the wrapper to the class name. Unlinked
   `MovieClip`/`Sprite` scripts can become the document class linked to character
   0. Ambiguous or impossible links produce warnings.
10. **Emit `DoABC2`** for every compiled script, with a deduplicated script name
    and the complete ABC payload.
11. **Bake each frame**:
    - `RemoveObject2` for every non-scripted depth used in the previous frame;
    - optional `FrameLabel` (first label found at this frame);
    - one `PlaceObject2` for each non-scripted object, carrying the character ID,
      matrix and a sanitized unique name;
    - scripted objects are placed once at reserved depths above the baked
      timeline, removed when they disappear and not re-placed while visible;
    - `ShowFrame`.
12. **Serialize** with the configured compression (`CWS` + zlib by default,
    `FWS` for uncompressed), computing the declared length from the uncompressed
    body.
13. **Return** bytes, warnings and `BuildStats` (frames, placements, shapes,
    sprites, embedded scripts, ABC bytes, file bytes).

The baking strategy is intentionally simple and deterministic. It is not an
optimized keyframe-diff exporter, and the docs say so. Scripted-object placement
is a deliberate exception to per-frame baking: re-placing the object would reset
the transform that ActionScript just changed.

### Coordinate and binary details

- Stage units are pixels; `Movie` converts the stage rectangle to twips
  (1 px = 20 twips) and writes a bit-packed `RECT`.
- Frame rate is written as 8.8 fixed point; frame count is the number of
  `ShowFrame` tags.
- Objects are placed with `Matrix::from_transform(scale, rotation, x, y)`;
  translation is in twips, and shapes are centered on their local origin.
- `DefineShape3` uses RGBA fill and line styles; rectangles are split into edges
  when a coordinate exceeds the 15-bit edge limit.
- Tag lengths use the short form when possible and the long form otherwise.
- `DefineSprite` inner tags include their own `End`.
- Names are sanitized to ASCII alphanumerics/underscores, prefixed if they start
  with a digit, and deduplicated per frame.

### Inspector

`nova-swf::inspect` is a separate reader used by tests and `nova probe`. It
checks signature, version, declared vs. actual length, zlib decompression, tag
headers, and the presence of `End`. It reports warnings for suspicious structure
and a `SwfInfo` summary. It is not a full SWF parser, but it is independent of the
writer's serialization path.

## 6. ActionScript compiler

`nova-as` contains both the front-end and the ABC back-end:

```text
source
  → lexer           tokens, comments skipped, unterminated-string diagnostics
  → parser          recursive descent → CompilationUnit (AST)
  → check           symbols, scopes, types, call resolution, unsupported features
  → emit            AVM2 bytecode and trait/method tables
  → abc             constant pools and binary serialization
  → diagnostics     severity, message, span (line/column)
```

Module roles:

| Module | Responsibility |
| --- | --- |
| `lexer.rs`, `token.rs` | AS3 tokens, literals, operators, comments, lexical errors |
| `parser.rs`, `ast.rs` | declarations, statements, expressions, spans |
| `sema.rs` | fast declaration-level checks used by the editor |
| `compiler/types.rs` | type world, class/method/field symbols, scopes, assignability |
| `compiler/check.rs` | full type checking, call/ref classification, unsupported features |
| `compiler/emit.rs` | method bodies, initializers, control flow, calls, coercions |
| `compiler/abc.rs` | constant pools, multinames, MethodInfo, traits, infos, serializer |
| `compiler/opcode.rs` | opcode constants and u30/i32 encoders |
| `diagnostic.rs` | error/warning spans and rendered source snippets |

The supported subset, built-ins and rejected constructs are documented in
[AS3_COMPILER.md](AS3_COMPILER.md); the binary layout is in
[AVM2_BYTECODE.md](AVM2_BYTECODE.md).

Key semantic rules:

- local classes, single inheritance and external Flash bases are resolved;
- the checker distinguishes globals, fields, instance methods, static methods,
  constructors, class references, dynamic members and unsupported function
  values;
- calls through arbitrary function values, interfaces, `dynamic`/`native`,
  getters/setters, arrays/indexing, object literals, `switch`, exceptions and
  `for-in` are rejected instead of guessed;
- only `trace`, `getTimer`, `isNaN`, `isFinite`, `parseInt` and `parseFloat` are
  known free functions; other Flash APIs are dynamic surfaces.

The compiler emits a complete ABC block or no ABC. `nova-build` embeds the block
in `DoABC2` and links classes with `SymbolClass`. There is no AS3-to-JavaScript
translation and no editor-side AS3 execution.

## 7. QuickJS host

`nova-script::ScriptEngine` owns a persistent `rquickjs` context. It:

- exposes `nova.version`, `nova.log(...)` and a read-only `nova.project` JSON
  snapshot through `set_project_context`;
- captures console/log output and the completion value;
- enforces a 64 MiB memory limit and a 1 MiB stack;
- installs a wall-clock interrupt handler so infinite loops return a timeout
  instead of freezing the UI;
- returns typed errors that distinguish evaluation errors from timeouts, with
  the log lines preserved;
- escapes JSON when injecting the project, so quotes/newlines/Unicode cannot
  break the host script.

The engine is created lazily on first console use and reset when a new project is
loaded. It is a tooling runtime, not an AS3 interpreter.

## 8. Errors and honesty

- Library crates use `thiserror` error enums (`ProjectError`, `SwfError`,
  `BuildError`, `ScriptError`) with context, not panics.
- UI failures open an error dialog and also log to the console; they never fail
  silently.
- Export reports omitted assets and ignored scenes. Scripts are either compiled
  and embedded or cause a build error; they are not silently skipped when script
  export is enabled.
- Unsupported AS3 is a compiler error with file/line/column. It is never silently
  translated or dropped.
- The UI repeats the key limits in the help dialog and console labels, so a user
  cannot reasonably mistake the QuickJS console for AS3 execution.
- Unknown or invalid SWF/ABC structures are rejected where possible; structural
  parser acceptance is not presented as runtime verification.

## 9. Testing

The workspace has **254 tests** (unit, integration and one doctest) at the time
of writing:

- **nova-core** (17): timeline resolution, tweens/easing, held frames, hidden
  layers, color/transform math, JSON round-trips, migrations/version rejection,
  validation, atomic disk save/load.
- **nova-swf** (21 + 3 independent): bit packing (including negatives and partial
  bits), tag headers, RECT/matrix encoding, shape geometry and edge splitting,
  sprite/tag structure, header consistency, compression, inspector
  truncation/unknown signature; Ruffle parses emitted SWF, `DoABC2`/`SymbolClass`
  and animated movies.
- **nova-build** (4 + 5): demo/uncompressed export, shape/name dedup, invalid
  project rejection, script embedding/linking, document class, disabled scripts,
  compiler-error propagation, scripted placement count and the checked-in AS3
  example.
- **nova-as** (34 + 29): lexer/parser/sema; type checking; unsupported-feature
  diagnostics; ABC constant pools, multinames, method bodies, scope depths,
  initializer shapes, traits, constructors, calls, `new`, loops, branches,
  `is`/`as`/`instanceof`; every emitted ABC is parsed independently.
- **nova-script** (13): persistence, console capture, JSON escaping, project
  context, thrown values, syntax/runtime errors, timeout/interrupt recovery.
- **nova-formats** (20): format detection, `.nova` round trip, FWS/CWS/ZWS,
  SWF 9/10/13, sprites/layers/labels/ABC, unknown tags, corrupt SWF, XFL
  containers, rejected FLA inputs, experimental XFL export and conversion round
  trips.
- **nova-app** (74): highlighter offsets/Unicode, camera mapping, hit-testing
  (rotation/ellipses), governing-keyframe edits, timeline mapping, marquee and
  shared transforms, history/redo/limit behavior, autosave paths/freshness/disk
  recovery, runtime settings and first-launch behavior, AI preview project-model
  create/modify/delete and preserved-ABC rejection.
- **nova-ai** (33): provider JSON privacy and round-trip, model discovery,
  authenticated headers, rejected credentials without key leakage, chat/agent
  reply parsing, cloud consent and per-session overrides, context inclusion and
  ABC handling, root confinement/traversal, create-overwrite and modify-missing
  protection, diff preview/apply/undo and build diagnostics.

`cargo test --workspace` must stay green; `cargo build --release` is the release
gate. Structural parser acceptance is explicitly not counted as runtime
validation.

## 10. Extension points

- **AI providers**: add streaming, token accounting and provider-specific
  capability metadata behind the existing `HttpTransport`/`ProviderKind` layer;
  keep keys in `CredentialStore` and keep cloud consent explicit.
- **Runtime verification**: run the emitted SWF in a Flash Player Projector or
  Ruffle, add runtime smoke tests and only then claim playback compatibility.
- **Flash API layer**: add a declaration surface for `flash.display`,
  `flash.events`, `flash.text` and friends so more calls become type-checked and
  fewer are dynamic.
- **More language features**: interfaces, `super(...)`, `switch`, `try`/`catch`,
  arrays/indexing, object literals, function values/closures, metadata.
- **Assets**: `Project.assets` already carries kind/path metadata; add embedders
  in `nova-build` and import UI in `nova-app`.
- **More shapes/text**: extend `ObjectKind` and `nova_swf::shape`, then the shape
  dedup key and stage painting. Keep validation and docs in sync.
- **HTML5**: a separate renderer over `resolve_scene`, reusing the document model
  but not the SWF writer.
- **Plug-ins**: QuickJS already has a project snapshot; a future API can expose
  safe commands instead of raw mutation.

## 11. Module map

```text
crates/nova-core/src/
  model/project.rs      Project, metadata, scripts, assets, export config, save/load
  model/scene.rs        Scene, Layer, layer kinds, frame count
  model/timeline.rs     Frame, Tween, Easing, resolve_scene
  model/object.rs       ObjectInstance, ObjectKind, Symbol (incl. skeleton,
                        bone_clips, active_clip)
  model/bones.rs        M15/M16 rig model: Bone/Skeleton/BoneClip/BoneTrack,
                        FK+IK pose evaluation, bind_transform, two-bone IK,
                        FABRIK chain solver + pole mirroring
  model/transform.rs    center-origin transform + matrix/lerp
  model/color.rs        RGBA, hex, lerp
  model/game.rs         game metadata (collision, physics, camera, input,
                        scene transitions)
  game.rs               preview simulation primitives (bodies, step, camera)
  game/runtime.rs       GameRuntime: scene lifecycle, event bus, transitions
  gamekit.rs            installable AS3 game-kit scripts (incl. game.NovaGame
                        runtime bridge, generated game.NovaGameData specs,
                        top-level Math shim, substepped physics)
  games.rs              built-in game template definitions
  time.rs               frame/time conversion and playback stepping
  error.rs              ProjectError
  lib.rs                public re-exports

crates/nova-swf/src/
  bits.rs               MSB-first bit writer/reader
  types.rs              Rect, Matrix, Color, PlaceObject, ColorTransform
  shape.rs              DefineShape3 paths/fills/strokes
  tags.rs               tag codes and serialization
  movie.rs              header, FWS/CWS, frame count, compression
  inspect.rs            independent reader and SwfInfo
  error.rs              SwfError

crates/nova-build/src/
  lib.rs                compile, link, bake, stats, warnings, generated
                        game.NovaGameData (scene ranges, collider/camera/
                        emitter/tile-grid specs, convex-hull polygons),
                        transition-aware all-scene export, SWF v13 minimum
                        for script projects (playerglobal API gating)
  error.rs              BuildError

crates/nova-as/src/
  token.rs lexer.rs     tokens and lexing
  ast.rs parser.rs      AST and recursive-descent parser
  sema.rs               declaration checks for the editor
  diagnostic.rs         severity/span/rendering
  compiler/types.rs     type world, symbols, scopes
  compiler/check.rs     type checking and unsupported diagnostics
  compiler/emit.rs      AVM2 bytecode emission
  compiler/abc.rs       ABC constant pools and serializer
  compiler/opcode.rs    opcodes and operand encoding
  lib.rs                parse/analyze/compile entry points

crates/nova-ai/src/
  provider.rs           provider settings, model discovery, chat, privacy classification
  credentials.rs        DPAPI, restricted-file fallback and memory test store
  http.rs               HTTP transport trait + reqwest implementation
  agent.rs              context builder, consent, prepared request, reply parser
  files.rs              root confinement, diff preview, atomic apply/undo
  diff.rs               line-oriented diff
  build.rs              compiler/exporter build diagnostics
  error.rs              AI error types

crates/nova-app/src/
  main.rs               window + eframe entry
  app.rs                shell, actions, dialogs, file/export flows, history/autosave
  history.rs            snapshot undo/redo
  autosave.rs           autosave and crash recovery
  state.rs              UI state and action vocabulary
  ai_editor.rs          integrated AI workspace
  ai_providers.rs       provider settings dialog
  stage_view.rs         stage rendering/camera/manipulation/multi-select
  timeline_view.rs      timeline rendering/interaction
  properties.rs         inspectors
  tree.rs               project tree
  code_view.rs          editor and problems
  highlight.rs          AS3 highlighting
  console.rs            QuickJS console
  audio.rs              rodio preview audio engine and cue collection
  theme.rs util.rs      styling and geometry helpers
```

## 12. Mega Milestone 8 additions (0.4.0-rc1)

M8 keeps the existing crate boundaries. The document model remains the single
source of truth; UI, build, format and AI layers only mutate it through explicit
operations and validation.

### Document model

- `ObjectKind::Text(TextObject)` stores text, font size, alignment and colour.
  `text.rs` owns the built-in ASCII/German 5×7 vector glyph table, metrics and
  outline/path generation. Text is rendered by the stage and baked into SWF and
  XFL as ordinary shapes. There is no `DefineText` and no system-font path.
- `Layer` gained `parent` and `expanded`. Folder layers have no exported
  content; `resolve_scene` skips them and descendants of hidden folders.
  Validation rejects missing parents and cycles, and `repair_folder_links`
  repairs imported/broken hierarchies without discarding layers.
- `Project::scenes` and `ExportConfig::export_all_scenes` support multiple
  scenes. The default remains active-scene-only. When enabled, the build layer
  concatenates scene timelines and emits a scene boundary label before each
  non-first scene. Scene names and frame labels are not unique identifiers.
- `template.rs` defines four constructors (`Empty`, `AnimationShort`,
  `Platformer`, `UiKit`). Templates create real scenes, layers, objects,
  tweens, symbols and scripts; they are shared by the GUI wizard and
  `nova new --template`.
- New serialized fields use `#[serde(default)]`; the `.nova` format version
  stays `1`. Older projects load unchanged, and unknown future versions are
  still rejected.

### UI and editing

- `AppState` owns an object clipboard, snap-to-grid state, alignment/distribution
  helpers and the code-search state. `UiAction` remains the intent vocabulary;
  app methods perform edits and snapshot history exactly once per user action.
- Stage dragging honours grid snapping. Multi-selection can be aligned
  (left/centre/right/top/middle/bottom) or distributed horizontally/vertically.
  Copy/paste/duplicate works on selected objects and preserves object identity
  rules; keyframe copy/paste/duplicate remains a separate path.
- The code view supports incremental find, case-sensitive toggle, replace and
  project-wide search results. Search does not rewrite files automatically.
- The New Project dialog lists the four templates and validates the project
  name before creating a real project.

### AS3 compiler

The checker now accepts array literals, object literals and index access
(`a[i]`, nested index assignment and increment/decrement). The emitter adds a
late-bound multiname (`MultinameL`) and uses `newarray`, `newobject`,
`getproperty`/`setproperty` and `getproperty`/`setproperty` with dynamic names.
This is structurally validated ABC, not a runtime-verified collection
implementation.

### AI structured operations

`nova-ai::operations` is UI-independent and network-free. The agent can propose
up to 32 operations tagged with `op`: rename project, set stage, add scene,
add layer, add/update object, add keyframe, set frame label, add tween, add
symbol, place symbol and add script. A plan is validated against the current
project and shown as human-readable summaries before application. Application
is transactional: any error rolls back the whole model, and the app records a
single undo snapshot. Operations never delete content. File diffs and project
operations share the same confirmation boundary but remain different data
types.

### Verification boundary

`cargo test --workspace` covers model validation, text export, multi-scene and
folder export, template builds, AS3 ABC structure and AI operation transactions.
Ruffle 0.7.1 remains the only visually verified external runtime. Multi-frame
`SymbolClass` execution, XFL tween fidelity, Adobe playback, bitmap/audio,
nested symbol timelines and the full Flash API remain partial or unverified.

## 13. Mega Milestone 9 additions (0.5.0-rc1)

M9 keeps the same crate boundaries: `nova-core` owns the document model,
`nova-build`/`nova-swf` own SWF emission, `nova-formats` owns XFL/FLA and `.nova`
IO, `nova-ai` owns provider/privacy/operations and `nova-app` is the egui shell.

### Document model (format version 2)

- `Symbol` gained `kind` (`Graphic`/`MovieClip`/`Button`) and `timeline: Scene`.
  The legacy shape fields (`width`, `height`, `fill`, `corner_radius`) remain for
  v1 compatibility; `Symbol::from_legacy_shape` rebuilds the timeline on load.
- `ObjectKind::Bitmap` references an `Asset` UUID and carries an optional
  normalised `BitmapCrop`. `ObjectKind::Path` stores imported vector geometry.
- `Asset` embeds bytes plus mime/dimension/duration metadata; `AssetKind` covers
  Bitmap, Audio and Font. Assets are self-contained in the `.nova` document.
- `LayerKind` gained `Guide`, `Mask` and `Sound`; `Frame` gained `sound:
  Option<SoundCue>` (offset, duration, event flag, loop count) and
  `Tween.kind` (`Motion` default, `Shape` opt-in).
- `Project::edit_scene(Option<Uuid>)` / `edit_scene_mut` select either the
  active scene or a symbol timeline, so every timeline-aware subsystem
  (editor, validation, export, AI ops) works on symbol timelines unchanged.
- Validation rejects duplicate IDs, dangling asset/symbol references, symbol
  dependency cycles, folder cycles, invalid sound cues and incompatible
  shape-tween pairs.

### Export

- SWF: symbol timelines become recursive `DefineSprite`s; masks use
  `PlaceObject2.clipDepth` against the layer below; bitmaps emit JPEG2/
  Lossless2; sounds emit `DefineSound`/`StartSound` (event-like, no streaming);
  shape tweens are baked to interpolated shapes. Buttons export as
  MovieClip-like sprites with a warning.
- XFL: the experimental `.xfl` ZIP importer/exporter round-trips symbols,
  nested instances, layers, keyframes, tweens, assets and sound references;
  a structural XFL?N�VA?XFL?N�VA test guards the M9 fields. It is not validated
  in Adobe Animate and native `.fla` remains unsupported.

### Application

- The library panel manages symbols (kind, duplicate, rename, guarded delete,
  enter/exit editing mode). Editing a symbol edits the shared timeline; every
  instance updates.
- Stage tools: select (with scale/rotate handles), line, pencil, brush,
  eraser, fill and path-node editing. Pencil/brush bake freehand strokes to
  filled shapes because SWF shapes carry no stroke width.
- The timeline shows sound cues, shape-vs-motion tween colouring and
  mask/guide/sound badges; the properties panel edits layer kind and tween kind.

### AI operations

`nova-ai::operations` gained symbol-kind, symbol-timeline, asset-import,
bitmap-placement, sound-cue and layer-kind operations. Timeline-targeting ops
accept a `symbol` name selector. Asset imports never touch the filesystem
directly: `preview_operations_with`/`apply_operations_with` take a
workspace-confined `read_asset` callback (`Workspace::read_bytes`), so path
traversal is rejected before any read and the batch stays transactional.

## 14. Mega Milestones 10/10.1 additions (0.6.0-rc2)

M10/M10.1 keep the crate boundaries: `nova-core` owns all game runtime state,
`nova-app` is the egui shell that drives it, `nova-ai` mutates game metadata
through typed operations only.

### Game document model (format version 3)

- `ObjectInstance.game: Option<GameObjectConfig>` carries optional collision
  (`CollisionSpec`: rect/circle) and physics (`PhysicsSpec`: static/dynamic/
  kinematic, gravity scale, friction, bounce, controlled flag).
- `Scene.game: Option<GameSceneConfig>` carries camera (`CameraConfig`),
  input bindings (`InputBindings`) and, since M10.1, `transitions:
  Vec<SceneTransition>` (key name + target scene name, rising-edge trigger).
  All fields are serde-defaulted; v2 projects load unchanged.

### Game runtime (`nova_core::game::runtime`)

- `GameRuntime` snapshots the active scene (bodies via `collect_bodies`,
  per-body `BodyState`, `CameraState`, scene config) and owns the lifecycle:
  `start`/`pause`/`resume`/`stop`/`restart`/`reset_scene`/`goto_scene`.
- `goto_scene` resolves targets by name or id, refuses unknown targets with
  `RuntimeError::SceneNotFound` without touching state, persists per-scene
  body states (returning to a scene restores positions), and dispatches
  `SceneUnload`/`SceneTransition`/`SceneLoad` events.
- `EventBus` is a typed multi-listener registry keyed by `GameEventKind`
  (lifecycle, scene, `CollisionEnter`/`CollisionExit`, `InputPressed`).
  Listeners are `Fn(&GameEvent)` callbacks returning opaque `ListenerId`s;
  `remove` detaches exactly one registration. Dispatch snapshots the
  listener list, allows safe register/remove during dispatch and caps
  re-entrant depth at `MAX_DISPATCH_DEPTH` (8) counting dropped events.
- Collision events are computed from pre-integration positions each step
  (enter/exit edges); sustained contacts re-fire only after a real gap.
- `step` clamps `dt` to 100 ms (tab-switch safe), integrates via the M10
  `game::step`/`step_camera` primitives, emits input rising-edge events and
  evaluates configured key-triggered scene transitions (at most one per
  step; failed targets dispatch a `SceneTransition` event with the error
  and keep running).

### Application

- The egui shell owns one `GameRuntime` per play session
  (`AppState::game_preview`). Transport buttons map to lifecycle calls:
  play/pause toggles `start`/`pause`, "Neu" calls `restart`, "Stop" calls
  `stop`. Scene-tree selection during play routes through `goto_scene`.
- The stage overlay shows active scene name, simulation time and runtime
  status while a preview runs.
- `audio.rs` wraps rodio (`DeviceSinkBuilder::open_default_sink`,
  per-sound `Player`): embedded asset bytes decode via symphonia
  (WAV/MP3/FLAC/OGG), multiple instances overlap, a master volume slider
  lives in the timeline transport, and pause/resume/stop follow playback.
  `play_sound_cues` fires `SoundCue`s when the playhead enters a frame;
  missing assets and decode failures are logged as warnings.
- Properties inspector gained a per-scene "Szenenwechsel" editor (key +
  target scene combo + remove).

### AI operations

`SetSceneTransition { key, target, scene?, remove? }` adds, updates or
removes a scene transition with validation (non-empty bounded key, existing
target scene, no duplicates per key). Audio cues and game objects were
already covered by M9/M10 operations.

### Honest scope

- Scene transitions exist in the editor preview runtime only. AS3 cannot
  call into the host, so exported SWFs implement navigation themselves;
  `gotoAndStop`-style timeline jumps are not part of the AS3 subset.
- Event listeners are Rust-side (editor/tests). No AS3 event API exists.
- Preview audio plays each cue once per frame entry; `loop_count`,
  `offset` and `duration` are honoured by the SWF exporter, not the preview.
- Ruffle validation covers SWF export regression; it does not exercise the
  editor runtime. Actual audible output was not verified headless.


## 15. Mega Milestone 11 additions (0.7.0-rc1)

M11 turns the editor into a visual 2D game-creation environment while keeping
the crate boundaries: `nova-core` owns tilemap logic (`tilemap.rs`), particles
(`particles.rs`) and all game runtime state; `nova-app` adds the painting UI;
`nova-ai` exposes the same operations as typed, transactional edits.

### Document model (format version 4)

- `ObjectKind::Tilemap(TilemapData)`: columns/rows grid, per-layer
  visibility/lock/opacity, `u16` cell values (`0` = empty).
- `Project.tilesets: Vec<Tileset>`: bitmap-asset-backed tile sets with tile
  size, column count, per-tile names and `solid` flags. Tilesets are shared
  across scenes; image data is never duplicated.
- `ObjectInstance.particles: Option<ParticleEmitter>` and
  `CollisionShape::Polygon { points }` (3–64 points, validated non-degenerate)
  plus `CollisionSpec.layer`/`mask` bit filtering.
- `GameSceneConfig.world: Option<WorldConfig>` (size + spawn), camera
  `follow_smoothing`/`dead_zone`/`shake`.
- All additions serde-defaulted; format v1–v3 documents load unchanged and
  are normalised to v4 on save. `Project::validate` checks tileset ids/names,
  tile dimensions and asset kind.

### Core logic

- `nova_core::tilemap` is the single implementation of paint tools
  (pencil/eraser/rect/flood-fill/eyedrop/replace), brush stamps, flood-fill
  connectivity and tile index validation — shared by the UI, AI operations
  and tests.
- `nova_core::particles` is a deterministic particle system (seeded RNG,
  fixed-point-free f32 math, capped `max_particles`, local/world space) with
  `ParticleView` render data consumed by the stage overlay.

### Runtime integration

- `nova_core::collision` is the shared narrow-phase module: SAT for
  convex-polygon pairs (rect/circle normalised into the same solver),
  closest-point circle↔polygon, analytic circle↔circle. Concave input is
  validated and resolved as its convex hull — never silently treated as a
  bounding box.
- `collect_bodies` now emits implicit static bodies for tilemap objects;
  `attach_tile_colliders` converts tileset-solid cells into a `TileCollider`
  grid resolved per cell by `resolve_tiles` in `game::step` (same SAT
  contacts, restitution, grounded flag). Tile colliders carry the tilemap
  object's `layer`/`mask`, and `GameRuntime`'s enter/exit tracking filters
  pairs through the same `interacts_with` predicate — tile layer/mask
  filtering is consistent between physics and events.
- `GameRuntime` collects `EmitterRuntime`s per scene (object id →
  `ParticleSystem`), follows objects across frames and exposes
  `particle_views()` for the stage renderer. Emitter simulation is stepped
  inside `GameRuntime::step` with the same clamped dt; restart rebuilds the
  systems so state never leaks between runs.
- `GameRuntime` applies world bounds for body clamping and the extended
  camera (follow smoothing, dead-zone, shake) via `step_camera`; worlds
  smaller than the viewport centre correctly and a missing follow target
  falls back to the stage centre.

### Application

- Bottom dock tab **Kacheln** (`tilemap_view.rs`): tile palette rendered from
  the real tileset texture honouring `tile_rect` (margins/spacing), all seven
  tools including region **Auswahl**, NxN brush stamps with ↔/↕ flip
  toggles, selection copy/cut/paste/move with ghost previews, wheel zoom
  around the cursor, middle-mouse/Space+drag pan, configurable grid +
  opacity, solid-cell collision overlay and per-layer
  visibility/lock/opacity. The **Tileset…** dialog slices a bitmap asset
  (tile size, margins, spacing, solidity toggles, name prefix) into a
  `Tileset` without duplicating image data. All mutations go through the
  history/undo pipeline — one entry per gesture.
- Edit-mode particle preview: `stage_view` keeps a per-object
  `ParticleSystem` map stepped with real frame delta whenever the game
  preview is not running; systems recreate on spec change and prune on
  object removal.
- Inspector (`properties.rs`): polygon collision point editor, collision
  layer/mask, camera smoothing/dead-zone/shake, world spawn, and the full
  particle emitter form (preset + every field).
- Stage (`stage_view.rs`): tilemaps render cell-by-cell with real texture
  UVs and per-layer opacity; particles draw during game preview **and** in
  the edit-mode emitter preview; collision overlays include polygons.

### AI operations

`create_tileset` (now with margins/spacing), `create_tilemap`,
`modify_tilemap`, `paint_tiles`, `add_tile_layer`,
`configure_tile_layer`, `flip_tile_region`, `move_tile_region`,
`create_collision_shape`, `modify_camera`,
`create_particle_emitter`/`configure_particle_emitter`,
`create_level_group`, `place_game_object`, `import_project_asset` and the
composite `create_game_level` are transactional and validated; the
`ai_builds_playable_tilemap_level`, `tile_layer_region_and_slicing_operations`
and `ai_rejects_invalid_tilemap_ops_transactionally` tests exercise the
pipeline.

### Export

- SWF: `nova_build::bake_tilemap` bakes visible layers into RGBA honouring
  `tile_rect` (margins/spacing), alpha-composited with layer opacity and an
  8192px cap, then `define_tilemap` wraps it in `DefineBitsLossless2`.
  Missing tilesets/assets warn explicitly. Particles warn once per object.
- XFL: tilemaps are baked through the same `bake_tilemap` into PNGs under
  `LIBRARY/` with `DOMBitmapInstance` placements; bake failures fall back to
  an explicit warning. Particles and game metadata still warn per object;
  their data stays in `.nova`.

### Verification boundary

- Unit tests: SAT collision pairs/normals/depth, invalid-polygon handling,
  collision layer/mask (objects and tiles, enter/exit events), camera
  small-world/missing-target, tilemap paint/flood/eyedrop/replace,
  copy/paste/clear/flip region ops, tileset slicing math, format-v4
  roundtrip, AI level workflow and new layer/region ops.
- Perf: `crates/nova-core/tests/perf.rs` (ignored by default) measures large
  tilemap ops, 200-body collision steps, 50-emitter particle sim and undo
  snapshot cost — run with `--release -- --ignored`.
- Build tests: tilemap→`DefineBitsLossless2` bake, XFL tilemap→PNG bake,
  missing-tileset warning, every game template compiles to SWF.
- Ruffle 0.7.1 runs all seven M10 template SWFs and all five M11 demo SWFs
  with zero ERROR lines (font fallback warnings only). Audible/visual frame
  inspection was not performed headless; claims are limited to clean startup
  logs.
