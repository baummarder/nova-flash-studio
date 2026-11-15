# Roadmap

The roadmap is ordered by risk and user value. The single biggest risk is not the
editor UI — it is the gap between "AS3 source editor" and "real AVM2 execution".
Projector verification comes before feature breadth.

Status legend: **done**, **in progress**, **next**, **planned**, **not planned
for now**.

---

## Where the project is

| Area | Status |
| --- | --- |
| Workspace, model, validation, `.nova` persistence (format v4) | **done** |
| Spec-based SWF writer (shapes, sprites, timeline, compression, gradients) | **done** |
| Independent parser validation of emitted SWF | **done** |
| Desktop shell, dark layout, two modes | **done** |
| Stage camera, hit-testing, drag editing, onion skin | **done** |
| Multi-select (shift-click/marquee), shared transforms | **done** |
| Timeline keyframes, tweens, labels, ruler scrub | **done** |
| Tween creation/easing/duration through the UI (motion and shape) | **done** |
| Keyframe copy/paste/duplicate, cross-layer moves, insert/remove | **done** |
| Undo/redo with transaction grouping | **done** |
| Autosave and crash recovery | **done** |
| Playback at scene FPS | **done** |
| AS3 lexer, parser, AST, declaration checks | **done** |
| QuickJS console for tools | **done** |
| AS3 type checking / ABC / `DoABC2` / `SymbolClass` | **done for the documented subset** |
| AS3 `switch` and strict-equality lowering | **done (M18)** |
| AS3 interfaces, `super`, try/catch/finally, full API declarations | **next** |
| Ruffle playback and app launch path | **done for tested sample exports** |
| Adobe Flash Player Projector path and process launch | **done in this development environment; visual playback NOT VERIFIED** |
| Adobe-default runtime setup, first launch, explicit Ruffle, persisted settings | **done** |
| AI Agent Editor, provider manager, DPAPI credentials, safe edits | **done; mocked tests, no live provider required** |
| AS3 arrays, object literals, index access | **done structurally; runtime semantics NOT VERIFIED** |
| `.nova`, `.fla`/XFL and `.swf` import/export with compatibility reports | **done for documented subsets** |
| `SymbolClass` character-link reconstruction for preserved ABC | **done where the character maps to a reconstructed object; unmappable links reported** |
| Native `.fla` export validated in Adobe Animate/Flash | **blocked: no Adobe authoring app available** |
| Layer folders, reorder, project-tree editing, editor find/replace, project search | **done** |
| Static text authoring (built-in vector font, shapes export) | **done** |
| Bitmaps, sounds, nested symbol timelines, masks, shape tweens | **done for the documented subset (M9/M12/M17)** |
| System fonts and editable runtime text | **unimplemented; planned** |
| Skeletal animation, mesh skinning and weight painting | **done (M15/M17)** |
| Tilemaps, particles, game preview with physics | **done (M10/M11)** |
| Three independently distributable editions (User/Edu/Enterprise) | **done (M19)** |
| M20.0 CI/help/README/roadmap/archive stabilization | **done** |
| M20.0 Windows `.nova` file-association specification | **done (spec only)** |
| M20.1 Education implementation (`nova-edu`, classroom mode) | **not started** |
| Native `.nova` file association on Windows | **spec only; implementation planned M20.1** |
| HTML5 export | **planned, secondary** |

---

## Mega Milestones 9–19 — delivered baseline (`0.15.0-rc1`)

The sections below preserve the historical M0–M8 plans. The actual project moved
well past them; the release reports are authoritative:

| Milestone | Release | Delivered focus |
| --- | --- | --- |
| M9 | `0.5.0-rc1` | Bitmaps, sounds, nested symbol timelines, masks, shape tweens, editor depth |
| M10 / M10.1 | `0.6.0-rc1` | Game metadata, preview physics, game kit, templates, runtime lifecycle/event bus |
| M11 / M11.1 | `0.7.1-rc1` | Tilemaps, tilesets, particles, world building, SAT collision, region editing |
| M12 | `0.8.0-rc1` | Exported AS3 runtime bridge (`game.NovaGame`), release archive safety |
| M13 | `0.9.0-rc1` | `Math` shim, body drive API, tilemap collision export, Ruffle runtime parity |
| M14 | `0.10.0-rc1` | AVM2 playerglobal surface, SWF v13 script exports |
| M15 | `0.11.0-rc1` | Skeletal animation, FK/IK, baked sprite matrices |
| M16 | `0.12.0-rc1` | Pen tool, path nodes, bone overlay, FABRIK chains |
| M17 | `0.13.0-rc1` | Cubic Béziers, native SWF gradients, Boolean operations, mesh skinning |
| M18 | `0.14.0-rc1` | FLA/XFL honesty, SWF gradient import, AS3 `switch`, asset workflow |
| M19 | `0.15.0-rc1` | Three editions, isolated config dirs/installers, shared `.nova` compatibility |

See `docs/MEGA_MILESTONE_*_REPORT.md` and the per-milestone test reports.

## M20 — Education Edition and stabilization

Architectural basis: `docs/MEGA_MILESTONE_20_PREPARATION.md`. The `.nova` format
stays at **v4** and remains edition-neutral; Education data will live in
sidecars, and classroom UI will be gated behind the new `nova-edu` crate.

### M20.0 — Stabilization (done)

- [x] Repair CI: `cargo fmt --check` and
      `cargo clippy --workspace --all-targets -- -D warnings` are green.
- [x] Fix stale in-app About/Help text (bitmaps, sounds, nested symbols, masks,
      shape tweens and skeletal animation are documented as implemented).
- [x] Align README and roadmap with the actual M19/M20 state.
- [x] Verify and complete the three-edition release archive without overwriting
      unverified artifacts; edition artifacts carry `BUILD-INFO.json` and
      `SHA256SUMS.txt`.
- [x] Add precise edition/packaging regression tests (config directories,
      installer identity, coexistence, portable archives, SHA-256, cross-edition
      `.nova`).
- [x] Document a Windows `.nova` file-association specification (no registry
      writes in M20.0) — see `docs/WINDOWS_FILE_ASSOCIATION.md`.

### M20.1 — Education core (not started)

- [ ] New `nova-edu` crate and Edu-only `Mode::Classroom` registration.
- [ ] Teacher/student roles, class roster and assignment sidecar data.
- [ ] Classroom UI gated by the Edu edition; User and Enterprise editions keep
      the existing two modes.
- [ ] `.nova` v4 compatibility tests across all three editions stay green.

### M20.2 — File association and packaging follow-up (not started)

- [ ] Implement the reviewed `.nova` association spec (HKCU, UserChoice-aware,
      per-edition ProgIds, portable opt-in).
- [ ] Rebuild all six Windows edition artifacts and archive them with verified
      checksums.

### M20.3 — Release (not started)

- [ ] Release `0.16.0-rc1` only after M20.1/M20.2 tests pass.
- [ ] Do not advertise Edu/Enterprise classroom or administration features before
      they exist.

---

## Mega Milestone 8 — Development platform (implemented subset, `0.4.0-rc1`)

M8 broadened the editor without replacing the working M7.3 subsystems. The
release candidate is buildable and tested, but it is not a complete Flash IDE.

### Delivered

- [x] Static text objects, alignment, size, colour and built-in vector font.
- [x] Text baked to SWF/XFL shapes; no fake `DefineText`.
- [x] Layer folders, recursive visibility/lock, cycle validation and repair.
- [x] Multiple scenes, opt-in concatenated export and scene boundary labels.
- [x] Four real templates shared by GUI and CLI.
- [x] Object copy/paste/duplicate, align/distribute and grid snapping.
- [x] Code find/replace, case toggle and project-wide text search.
- [x] AS3 array literals, object literals and index access.
- [x] Structured AI project operations with preview, validation, transaction and
      undo.
- [x] M8 regression tests and documentation.

### Still next or unavailable

- [ ] Inheritance, interfaces, `super`, switch, try/catch/finally, exception
      semantics. These remain compiler errors.
- [ ] Runtime verification of AS3 collection/index behavior in Ruffle/Adobe.
- [ ] Nested MovieClip timelines, symbol editing mode, masks and shape tweens.
- [ ] Bitmap, audio, font embedding and editable runtime text.
- [ ] Full `flash.*` API and practical game-helper library.
- [ ] Multi-frame `SymbolClass` execution after import/re-export.
- [ ] XFL tween/keyframe round-trip fidelity; Adobe Animate compatibility.
- [ ] Adobe visual playback.
- [ ] Live cloud AI provider verification.
- [ ] Native macOS/Linux build verification.

---

## Milestone 7 — Runtime integration (implemented, RC)

This is the runtime work delivered for the current release candidate; it is
independent of the older M4/M5 feature groupings below.

- [x] Adobe Flash Player Projector is the default external runtime.
- [x] Ruffle is optional and used only after explicit selection; no silent
      fallback to Ruffle or a system-default handler.
- [x] First-launch setup dialog with explanation, browse, structural
      validation, persistence and “Später einrichten”.
- [x] Dedicated **Runtime** settings: default runtime, Adobe path, browse,
      validate, live status, last validation, optional Ruffle path, reset,
      diagnostics.
- [x] Per-user configuration outside projects, legacy settings migration, UTF-8
      BOM tolerance and safe replacement on Windows.
- [x] Safe separate-argument launch, spaces/Unicode support, missing
      file/runtime errors and honest process-launch reporting.
- [x] `.swf` passed as a startup argument is treated as a play request, not
      parsed as a `.nova` project.
- [x] Tests for first launch, valid/invalid/missing runtimes, persistence,
      explicit Ruffle, switching back to Adobe, SWF validation, spaces/Unicode
      and no silent fallback.
- [x] Ruffle 0.7.1 playback verified through the app launch path for a
      regenerated `as3-example` SWF.
- [x] Adobe Flash Player Projector path and process launch — verified in this
      development environment with a local debug projector.
- [ ] Adobe Flash Player Projector visual playback — **NOT VERIFIED**; process
      start is not playback evidence. The projector is not bundled.
- [ ] Interactive UAC-denial click-through and broader GUI automation — still
      manual/not run.

See `docs/FLASH_PLAYER_INTEGRATION.md`, `docs/SWF_RUNTIME_COMPATIBILITY.md`,
`docs/TESTING.md` and `docs/MILESTONE_7_REPORT.md`.

## Milestone 7.1 — Universal format compatibility (implemented subset, RC)

- [x] Shared `nova-formats` layer over the existing `Project` model.
- [x] Native `.nova` import/export remains the default and stays compatible.
- [x] ZIP/XFL `.fla` import: document settings, timelines, layers, frames,
      labels, basic shapes, symbol instances and `scripts/*.as`.
- [x] SWF import: FWS/CWS/ZWS, SWF 9/10/13 headers, dimensions, FPS, frames,
      depth layers, shapes, flattened sprites, placements, labels and ABC.
- [x] Preserved ABC is embedded unchanged on re-export.
- [x] SWF export from native, imported-FLA and imported-SWF projects.
- [x] Compatibility reports for imports and exports; unsupported data named.
- [x] Open/Save/Export dialogs and CLI filters for `.nova`, `.fla`, `.swf`.
- [x] Native `.fla` export refused; experimental XFL `.zip` package only.
- [ ] Adobe Animate/Flash validation of generated FLA/XFL — **NOT VERIFIED**
      (no compatible authoring application available).
- [x] `SymbolClass` character-link reconstruction for preserved ABC where the
      character maps to a reconstructed object; unmappable links are reported.
- [ ] Full XFL edge geometry, nested symbol timelines, text, bitmaps, sound,
      filters and video.

See `docs/FORMAT_COMPATIBILITY.md` and `docs/MILESTONE_7_1_REPORT.md`.

## Milestone 7.2 — AI Agent Editor and SymbolClass reconstruction (implemented, RC)

- [x] Integrated AI workspace in the main app (toolbar, **KI** menu,
      `Ctrl+Shift+A`), not a second editor or project model.
- [x] Resizable project/context explorer, source editor, chat, build,
      diagnostics, diff and activity panels.
- [x] Providers: Ollama Local, Ollama Cloud, Groq and custom OpenAI-compatible.
- [x] Enable/disable, model discovery/selection, default provider, connection
      tests, edit/remove and per-session overrides.
- [x] API keys via Windows DPAPI; explicit restricted-file fallback elsewhere;
      no keys in provider JSON, logs or error messages.
- [x] Cloud classification and explicit per-session consent; no project upload
      without consent; user-selected bounded context.
- [x] Root-confined file create/modify/delete with diff preview, accept/reject,
      atomic writes, rollback and file undo.
- [x] Preserved ABC cannot be replaced or deleted through the agent.
- [x] `SymbolClass` character-link reconstruction for mapped objects.
- [x] CLI `nova build` with build diagnostics and `nova ai-status` without key
      values.
- [x] Mocked provider/agent tests, file-operation tests, format tests and app
      project-model apply tests.
- [ ] Live provider smoke tests against real endpoints — **NOT RUN**; the test
      suite uses mocked HTTP.
- [ ] Adobe playback of AI-modified or reconstructed output — **NOT VERIFIED**.
- [ ] Full Adobe FLA authoring compatibility — **NOT VERIFIED**.

See `docs/AI_AGENT_EDITOR.md`, `docs/AI_PROVIDER_GUIDE.md` and
`docs/MILESTONE_7_2_REPORT.md`.

---

## M0 — Foundations (done)

- [x] Cargo workspace with clear crate boundaries.
- [x] Project/scene/layer/frame/object/symbol/script/asset model.
- [x] Tween resolution and easing independent of the UI.
- [x] Versioned JSON envelope, validation, migrations, atomic save.
- [x] Demo project with four layers, two tweens and a linked script.
- [x] Error types and diagnostics instead of panics.

**Exit criteria:** model round-trips, invalid documents are rejected, `cargo test`
green. Met.

## M1 — Vertical slice (done)

- [x] Byte-level SWF writer: FWS/CWS, `DefineShape3`, `PlaceObject2`,
  `RemoveObject2`, `DefineSprite`, `ShowFrame`, `FrameLabel`, `End`.
- [x] Baking exporter with shape dedup, symbol sprites, sanitized names.
- [x] Independent reader/inspector and Ruffle parser cross-check.
- [x] `nova` CLI: `demo`, `export`, `probe`, `new`, `check`, `as-check`.
- [x] eframe desktop shell, dark theme, code/animation mode switch.
- [x] Stage camera, selection, drag-to-move, onion skin, grid.
- [x] Timeline rendering, keyframe operations, playback.
- [x] Properties inspector that edits the governing keyframe.
- [x] AS3 highlighter, diagnostics and problems list.
- [x] QuickJS console with limits and project context.
- [x] Open/save/export dialogs, unsaved-changes prompts, help/about dialogs.
- [x] Workspace tests (92 at M1 close; 153 after M2/M3).

**Exit criteria:** generated sample parses cleanly and the app runs. Met; Ruffle
playback is verified, while Adobe projector playback remains unverified.

**Immediate follow-up (blocking for Adobe claims):** obtain an Adobe Flash Player
Projector build, open a freshly exported `samples/nova-demo.swf` and
`as3-example.swf`, and record the result in a compatibility matrix. Ruffle 0.7.1
has already been tested. If Adobe fails, fix the binary issue before new
features.

## M2 — Authoring depth (mostly done)

Goal: the animation editor feels like a tool, not a demo.

- [ ] **Projector compatibility matrix** — Adobe projector builds (Windows and,
      if possible, one other OS), Ruffle versions, results, screenshots/hashes.
      Ruffle 0.7.1 evidence exists; Adobe remains blocked by the missing
      projector.
- [x] **Undo/redo** — snapshot-based edits for object transforms, properties,
      keyframes, layer/script/symbol changes; drags and multi-step actions are
      coalesced into one labeled transaction.
- [x] **Tween workflow** — create/remove a motion tween between keyframes, choose
      linear/ease-in/ease-out/ease-in-out, edit duration; tween spans are shown
      in the timeline.
- [x] **Selection tools** — shift-click, marquee multi-select, shared
      move/scale/rotate/delete around the group center, copy/paste/duplicate.
      Arrow-key nudge and align/distribute are not implemented yet.
- [ ] **Layer management** — rename, lock/visibility and add/delete exist;
      reordering and folder grouping semantics do not.
- [x] **Keyframe management** — copy/paste/duplicate, cross-layer moves, insert
      and remove frames, keyframe range marking, same-layer guard for multi-layer
      copies. Drag-keyframe and tween-link repair remain rough edges.
- [ ] **Project tree actions** — rename/delete scripts, symbols, scenes; create
      scenes; activate scenes; show linked objects.
- [x] **Autosave and crash recovery** — periodic snapshot, atomic write, recovery
      prompt, freshness check and discard.
- [ ] **Code editor depth** — find/replace, go-to-line, bracket matching, tab
      handling and larger-file performance are still missing.
- [x] **Export diagnostics** — warnings/statistics are returned by the build and
      shown in the export dialog and console. A richer structured report panel is
      still a future improvement.

**Exit criteria:** a user can build a short multi-layer tweened animation with
undo/redo, export it, and play it in a verified projector without editing JSON.
Authoring/export is met; the projector half is blocked.

## M3 — AS3 compiler subset (implemented subset; Ruffle-verified, Adobe blocked)

Goal: classes that extend `MovieClip` execute in a real AVM2 runtime.

The implemented pipeline is:

```text
lexer → parser → AST → symbols → type check
      → ABC emitter → DoABC2 + SymbolClass → SWF
```

- [x] **Name resolution** — packages, imports, local/imported classes,
      inheritance, member lookup, `this`, statics. Interfaces, `super` and full
      namespace/access enforcement are not implemented.
- [x] **Type checker** — primitive and class types, assignments, calls, returns,
      operators, property access, `is`/`as`/`instanceof`, source-located
      diagnostics. Arrays, vectors and function types are explicitly rejected.
- [ ] **Built-in declarations** — only six global free functions are known.
      `flash.*` imports are dynamic surfaces, not a declaration set.
- [x] **Control-flow lowering** — expressions, statements, `if`/`else`, `while`,
      `do while`, `for`, `break`, `continue`, assignments, short-circuit logic.
      `switch`, exceptions and closures are rejected.
- [x] **ABC encoder** — constant pools, QName/NamespaceSet multinames, method
      signatures/bodies, traits, instance/class/script infos, scope depths,
      coercions and opcode emission. Exception tables are empty; metadata is
      empty.
- [x] **SWF linking** — `DoABC2` with names, `SymbolClass` for linked classes and
      document classes, one-frame sprite wrappers, version bump to ≥10.
- [x] **Diagnostics and tooling** — readable file/line/column errors,
      `nova as-check` compiles to ABC and reports class/byte counts, and tests
      parse emitted ABC with Ruffle's independent reader.
- [ ] **Runtime conformance** — no golden ABC comparison against a reference
      compiler. Ruffle execution is verified for the regenerated sample; Adobe
      projector execution is not.
- [x] **First playable sample** — `samples/as3-example.nova` contains a class
      with constructor, fields and an `ENTER_FRAME` update method; it exports
      with embedded ABC and linked `SymbolClass`. Ruffle 0.7.1 executes the
      regenerated export (`avm_trace: Box gestartet`); Adobe playback is not
      verified.

**Exit criteria:** `samples/` contains a scripted SWF that executes correctly in
at least one Adobe projector, and the docs no longer say "scripts are omitted".
The docs/execution half is met for Ruffle; Adobe remains unverified.

**Non-goals for this milestone:** full AS3 language parity, E4X, the entire
`playerglobal` surface, performance parity with Adobe's compiler, AVM1 bytecode.

## M4 — Media and timeline depth (historical plan; superseded by M9–M17)

> Most items below shipped in M9–M17 (nested symbol timelines, bitmaps, sounds,
> gradients, masks, shape tweens, drawing tools, multi-scene export). This
> section is kept as the original plan, not as current status.

- [ ] Nested symbol timelines and a symbol edit mode.
- [ ] Bitmap import/embedding (`DefineBitsLossless2`/JPEG tags) and library
      management.
- [ ] Sound import/embedding (`DefineSound`, `SoundStreamHead`/`SoundStreamBlock`
      or event sounds) and a sound timeline.
- [ ] Text: static text first (`DefineText`/`DefineText2`), then dynamic text and
      font embedding.
- [ ] Drawing tools: pen/line, fill, stroke, gradients, rounded corners.
- [ ] Mask and guide layer semantics in renderer and exporter.
- [ ] Shape tweening (morph), motion path editing, custom easing curves.
- [ ] Optimized export: keyframe-diff placement instead of full re-bake, sprite
      reuse, optional shape optimization.
- [ ] Scene management UI and multi-scene export policy.

**Exit criteria:** common Flash-style rich-media animations export without manual
model edits, with a documented asset support matrix.

## M5 — Product hardening and secondary targets (historical plan; partially delivered)

> CI, packaging and installer work shipped in later milestones; see the
> M19/M20.0 sections and `docs/PACKAGING.md`. Localization, accessibility,
> HTML5 export and the QuickJS plug-in API remain future work.

- [ ] HTML5/Canvas exporter sharing `resolve_scene` (secondary, not a substitute
      for SWF).
- [ ] Plug-in API over QuickJS with explicit, capability-scoped commands.
- [ ] Preferences: theme density, autosave interval, default FPS/stage size,
      external editor integration.
- [ ] Localization (German/English) with a single string table.
- [ ] Accessibility: keyboard-only navigation, focus order, high-contrast theme.
- [ ] Performance budgets: 60 fps UI with 10k timeline cells, large projects,
      incremental validation.
- [ ] CI: Windows/macOS/Linux builds, `cargo fmt --check`, clippy, tests,
      release artifacts.
- [ ] Packaging: signed Windows installer, portable build, versioned release
      notes, crash logs (opt-in).
- [ ] Security review of QuickJS host limits and file import paths.

**Exit criteria:** repeatable releases, no known data-loss bugs, documented
compatibility matrix, and a clear support policy for the end-of-life Flash
runtime.

---

## Cross-cutting work

### Correctness

- Keep the stage, playback and exporter on the same resolver.
- Add property-based tests for binary encoders (lengths, bit widths, extreme
  coordinates).
- Compare emitted shapes against reference SWF files where legally possible.
- Fuzz the `.nova` loader and SWF inspector; never panic on untrusted input.

### Performance

- Avoid re-resolving the whole timeline on every UI frame if profiling shows it
  matters; cache per-frame resolution keyed by a project revision counter.
- Timeline painting should remain O(visible cells), not O(all frames × layers).
- Export should stream tag writing for large movies instead of holding every
  intermediate buffer.

### Documentation honesty

Every milestone updates:

- README "What works" and "Limitations";
- `docs/FEASIBILITY.md` compatibility evidence;
- `docs/ARCHITECTURE.md` crate/data-flow changes;
- in-app help text, so the UI never promises more than the build does.

---

## Explicit non-goals (for now)

- Opening or writing Adobe `.fla` files.
- AIR, mobile, or web deployment of the editor itself.
- A full AS3 standard library implementation.
- Emulating the Flash authoring IDE pixel-for-pixel.
- Shipping or bundling Adobe Flash Player.
- Claiming projector compatibility without a recorded test.

---

## Release gates

A build is releasable when:

1. `cargo test --workspace` passes and `cargo build --release` succeeds.
2. `nova demo` produces a file that `nova probe` accepts, and the file opens in
   the documented projector/Ruffle matrix.
3. No silent data loss: save/load round-trips every supported field, and invalid
   input is rejected with a message.
4. The UI and docs accurately describe unimplemented features.
5. A fresh clone can run the app and build the sample using only the README.
