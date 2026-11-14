# Mega Milestone 9 — Implementation Plan

Target: `0.5.0-rc1`  
Baseline: `0.4.0-rc1` (`25fc64ad6279641e49c2173c1d6e6c434cd697da`)  
Status of this document: implementation plan, updated as work lands.

## 1. Baseline and safety rules

- Preserve `dist/archive/0.3.0-rc2/` and archive the complete `0.4.0-rc1`
  artifacts under `dist/archive/0.4.0-rc1/` before packaging M9.
- Keep the existing project shell, AS3 compiler, SWF importer, AI privacy model
  and DPAPI credential handling. Do not rewrite working subsystems for novelty.
- Every document-format change gets a version bump and a migration. Existing
  `.nova` version 1 documents must load without loss of text, folders, scenes,
  keyframes, tweens, scripts, symbols or assets.
- New document fields are optional/defaulted in serde and additionally
  normalised after deserialisation so hand-written and migrated documents are
  treated identically.
- Nothing is reported as PASS without a test, a structural verification or a
  runtime observation. Unsupported features are marked NOT AVAILABLE or
  PARTIAL in the report.

## 2. M8 regression gate (must pass before new work is trusted)

Add reproducible fixtures and tests for:

- text objects and baked text geometry,
- layer folders / nested layers,
- multiple scenes and scene-boundary labels,
- keyframes, held frames and motion tweens,
- AS3 arrays, objects and array/object index access,
- SWF import → NØVA → SWF export,
- XFL export → import → export structural round trip.

Runtime evidence remains Ruffle-first. A compiler success alone is not runtime
evidence. The M8 regression suite is run before and after M9 changes; confirmed
regressions are fixed rather than documented away.

## 3. Model migration (format 2)

Bump `.nova` format version from 1 to 2. Version 1 documents are migrated by
setting `formatVersion = 2` and normalising defaults:

- `Symbol.kind`, `Symbol.timeline` (a real `Scene` with layers/keyframes),
- `Symbol` legacy shape converted into a one-layer timeline,
- `LayerKind::Sound`, optional `Frame.sound`,
- optional bitmap placement/crop fields,
- optional embedded asset bytes and media metadata,
- optional tween kind (`motion` default, `shape` opt-in).

The migration is additive. The semantic content of a v1 project is preserved;
old symbols still look and export like their original filled shapes.

## 4. Symbol library and nested MovieClips

### Model

- Stable UUID symbol IDs; names are labels, never identity.
- Symbol kinds: Graphic, MovieClip, Button.
- Each symbol owns a real timeline (`Scene` value) with layers, frames, keyframes,
  tweens, labels and nested instances.
- Recursive dependency discovery and cycle validation across the library.
- Validation rejects dangling references, duplicate IDs, cycles and invalid
  timelines.

### Editor

- Library CRUD: create Graphic/MovieClip, rename, duplicate, delete guarded by
  reference checks, place instance, swap instance's symbol.
- Dependency inspection (direct and transitive users).
- Enter/leave symbol editing context with a prominent breadcrumb. The timeline,
  stage, layer and keyframe operations target the symbol timeline while editing.
- Changes to a symbol affect all instances; no flattened copy is created.
- Recursive dependency guard prevents a symbol from containing itself.

### SWF

- Recursive real `DefineSprite` definitions with their own timelines.
- Per-frame `PlaceObject2` / `RemoveObject2` / `ShowFrame` boundaries.
- Nested symbols use the character id of their target symbol sprite.
- Deterministic definition order, cycle check before writing.
- Graphic and MovieClip are real sprite characters. Button is represented as a
  MovieClip-like sprite in this milestone; button hit/over/down states are not
  claimed as implemented.

## 5. Bitmap pipeline

- Import PNG, JPEG and GIF via the `image` crate; keep original bytes in the
  asset and store dimensions/mime.
- Asset library shows thumbnails, size and format; rename/delete with reference
  guard; detect unused assets.
- Place bitmap instances on stage, move/scale/rotate with existing transform
  system, preserve aspect ratio with Shift, show transparency.
- Optional crop rectangle stored on the bitmap instance and applied in editor
  and exporter.
- SWF: `DefineBitsLossless2` for decoded PNG/GIF/alpha content;
  `DefineBitsJPEG2` for original JPEG data. No bitmap is editor-only.
- Tests parse the emitted tags independently and verify ids, dimensions and
  payload presence. Ruffle visual fixture where the runtime is available.

## 6. Audio pipeline

- Import WAV (PCM 16-bit) and MP3. Store original bytes, sample rate, channels,
  duration and mime.
- Audio assets in the library, preview where a real OS decoder is available
  (WAV via Windows `PlaySound`), otherwise explicitly NOT AVAILABLE.
- Dedicated sound layers and sound cues with start frame, offset, duration,
  loop count and event/timeline distinction.
- SWF: `DefineSound` for WAV/MP3 and `StartSound` at cue frames. Event sounds are
  the reliable path. Timeline (streaming) sound is marked PARTIAL unless
  `SoundStreamHead`/`SoundStreamBlock` can be proven in Ruffle.
- No fake waveform or silent-button playback.

## 7. Drawing and transforms

- Tools: Select, Line, Pencil, Brush, Eraser, Fill, Path edit.
- Freehand tools produce real `Path` geometry; line tool produces a filled line
  mesh; eraser removes hit objects; fill changes the hit object's fill/path
  fills; path-edit mode exposes and drags path nodes.
- Visible scale handles and a rotation handle on the selection bounds.
- Shift preserves aspect ratio; multi-selection scales/rotates around the group
  centre; existing move, align, distribute and snap-to-grid remain.
- Centre-based transform origin is retained; the pivot is displayed. Custom
  pivot offsets are documented as not implemented unless the export matrix is
  extended and tested.

## 8. Timeline, shape tweens, masks and guides

- Symbol timelines are first-class timeline targets.
- Sound layers show cues and asset names.
- Keyframe/tween editing keeps existing F6/F5, clipboard and undo behaviour.
- Shape tweens: opt-in tween kind, deterministic geometry interpolation for
  compatible primitives/paths, editor preview, frame-baked SWF export. If a pair
  is incompatible, validation/creation reports it instead of silently morphing.
- Masks: a Mask layer masks the next lower visible layer (documented convention).
  Timeline shows the relationship; editor preview clips rectangle masks and
  clearly marks unsupported arbitrary-shape clipping; SWF export uses
  `PlaceObject2.clipDepth`.
- Guides: visibly marked, skipped by playback and export, never silently shown.

## 9. XFL

- Import/export symbol library files with symbol timelines, nested instances,
  layers, keyframes, labels, tweens, assets (bitmaps/sounds) and scene structure.
- XFL round trip: `XFL → NØVA → XFL → NØVA`, structural comparison.
- Native `.fla` export remains explicitly unsupported; byte output is an
  experimental `.xfl.zip` and is not labelled as Adobe-authored FLA.

## 10. Runtime and SWF QA

- Structural SWF tests for sprite nesting, frame boundaries, bitmap/audio tags,
  masks and shape baking.
- Ruffle runtime projects: nested MovieClips, bitmap assets, multiple scenes,
  text, animation, sound where implemented, masks where implemented, AS3 arrays
  and objects.
- Verify framewise behaviour/instance presence, not just "process started".
- Investigate confirmed `Error #1056` causes. Do not apply speculative changes.

## 11. AI typed operations

Extend the existing transactional operation/preview/confirm/undo model with:

- symbol creation/update and instance placement,
- symbol timeline layers/keyframes/tweens,
- bitmap/audio import (asset registration from bounded workspace paths only),
- bitmap placement and sound cues,
- masks and sound layers,
- existing scene/layer/keyframe operations.

All operations are validated on a clone first, applied atomically, and recorded
by the undo snapshot. Path traversal and cloud-consent restrictions remain.

## 12. UI integration

- Symbol library with kind badges and CRUD controls.
- Symbol editing breadcrumb and return action.
- Asset browser with bitmap thumbnails and audio metadata; preview and placement.
- Sound layer/cue inspector.
- Drawing tool strip and transform gizmos.
- Timeline badges for symbols, sounds, masks, guides and tween kinds.
- Tooltips and keyboard shortcuts; no empty buttons.

## 13. Examples, tests and release gates

Examples: Nested Symbols, MovieClip Animation, Bitmap Assets, Audio Timeline,
Masks and Tweens (where supported), Multi-Scene Regression. Additional format,
SWF, XFL, AI, runtime and migration tests are required.

Release gates:

1. `cargo fmt --check`
2. `cargo check --workspace`
3. `cargo test --workspace --no-fail-fast`
4. release build
5. Windows packaging
6. installer smoke test
7. portable ZIP verification
8. archive and hash verification
9. Ruffle visual fixtures for the M9 runtime matrix

If a gate fails, `0.5.0-rc1` is not claimed as released. The report records the
actual state.

## 14. Documentation deliverables

- `docs/MEGA_MILESTONE_9_REPORT.md`
- architecture, user/README, format, shortcut, limitation and release-note
  updates
- exact test statistics and ignored tests
- preserved 0.3.0-rc2 and 0.4.0-rc1 artifact inventory
