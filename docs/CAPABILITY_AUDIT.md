# Capability Audit — NØVA Flash Studio 0.9.0-rc1 (post-M13)

Status legend: ✅ fully implemented & tested · 🟡 partially implemented ·
🔵 implemented but insufficiently tested · 🔴 not implemented.

"Tested" means automated Rust tests, `nova check`/export round-trips, or
Ruffle trace-assertion runs (`tools/ruffle-runtime-test.py`). Nothing here
is claimed from documentation alone.

## 1. Project management & file formats — ✅

- `.nova` JSON format v4 (scenes, layers, frames, objects, symbols,
  assets, tilesets, scripts, game metadata). Round-trip serde tested.
- Backwards compatibility: v1–v3 projects load (format migration tested
  since M9).
- Autosave, project validation (`nova check`), undo/redo history,
  transactional AI file operations.

## 2. Drawing tools & shapes — ✅

- Rectangle (corner radius), ellipse, imported vector paths (`Path` with
  `PathShape` fills/strokes), symbol instances, bitmap objects, editable
  text (baked to paths on export), tilemaps.
- Selection/marquee, property editing, fill/stroke editing — editor-level,
  exercised daily by the milestone demos.

## 3. Animation & timeline — ✅

- Keyframes per object per frame, tweening (position/scale/rotation/
  alpha/color), motion paths, frame labels, playhead control, onion-skin
  style previews, audio sync markers.
- SWF export bakes keyframes to PlaceObject/DefineSprite timelines.
- Shape tweens (rect/ellipse/path vertex interpolation + easing) baked
  per frame — deterministic, no MorphShape tags (M9).
- **M15**: skeletal animation — bone hierarchies, FK clips, two-bone IK,
  rigid part binding, per-frame baked matrices in the exported sprite,
  editor skeleton panel, AI bone ops. No mesh skinning/weight painting.
- **M16**: multi-bone FABRIK chain IK + pole targets (`IkKey.poleX/Y`),
  `add_ik_key` AI op; visual pen tool, path node insert/delete,
  bone overlay with draggable joints in the stage view.

## 4. Scenes & MovieClips — ✅

- Multi-scene projects, nested symbols (sprite instances), scene frame
  ranges, concatenated multi-scene SWF export with labels, scene
  transitions via `game.NovaGame` (API + configured keys), per-scene
  state restore (M13).

## 5. Layers, masks & symbols — 🟡

- Layer stack with visibility/lock/opacity, layer kinds (normal/guides/
  masks), symbol library with nested instances.
- Layer masks exist in the model; mask *authoring UX* is thinner than
  Flash's and mask rendering in export is partial — treat as 🟡.

## 6. Text & typography — 🟡

- Editable text objects with font/size/color, baked to vector outlines on
  SWF export (device-font independent — actually a strength vs Flash).
- **M14**: runtime `flash.text.TextField`/`TextFormat` work natively in
  Ruffle at SWF v13 (dynamic text verified); `DefineEditText`/font-embedding
  *tags* for authored dynamic fields are still not exported.
- No input text, no TLF.

## 7. Bitmap & audio import — ✅

- PNG/JPEG/WAV import embedded as base64 in `.nova` (self-contained),
  asset library, audio cues on timeline, preview audio via rodio,
  SWF DefineBits/DefineSound export. M9–M10 verified.

## 8. SWF import — 🟡

- Header/tag parsing, shape/sprite extraction to `.nova` geometry,
  ABC disassembly view, capability reporting (`nova-formats`).
- Lossy by design: gradients, filters, masks, video, fonts are reported
  as unsupported rather than imported. Real-world SWF round-trip is
  "inspection + partial reconstruction", not faithful re-authoring.

## 9. SWF export — ✅

- AVM2/SWF10 zlib, shapes/sprites/buttons?, text-as-outlines, bitmaps,
  sounds, frame scripts + document class, SymbolClass linking,
  `exportAllScenes` concatenation, `NovaGameData` game metadata.
- Ruffle-verified across all milestone demos.

## 10. XFL/FLA — 🔵/🟡

- XFL package read/write (DOMTimeline XML), FLA capability detection and
  honest unsupported reporting; tilemap→PNG bitmap export in XFL.
- Not a full Animate-compatible round-trip: filters, motion tweens as XML,
  IK, components are not reproduced. 🔵 tested structurally, not against
  real Animate output corpus.

## 11. ActionScript 3 & compiler — 🟡

- Real recursive-descent AS3 subset → AVM2 ABC: classes, inheritance,
  packages, static/instance members, control flow, typed arithmetic,
  events. M10 fixed MultinameL/labels/dynamic-class bugs.
- **M14**: script projects auto-export at SWF v13, unlocking the
  playerglobal API surface in Ruffle — JSON, ByteArray+Endian,
  Dictionary, Timer, TextField/TextFormat, SimpleButton, Graphics,
  BitmapData/Bitmap, LoaderInfo, Point/Rectangle/Matrix/ColorTransform,
  full String/Array methods (20/20 probes, `m14-api-surface`).
- **Subset limits**: no interfaces, `switch`, `try/catch`, `for-in`,
  closures as values, `Vector.<T>` syntax, package-level functions.
  Untested against large real-world AS3 codebases.

## 12. Game engine & runtime — ✅

- `game.NovaGame` bridge: scenes/transitions, event bus, input, camera,
  particles, pause/resume, fixed-step determinism, state restore.
- `GameRuntime` preview equivalent with typed events and lifecycle.
- Verified in Ruffle via trace assertions (5 demo projects, 0 errors).

## 13. Physics & collision — ✅

- Rect/circle/convex-polygon SAT, layer/mask bit filtering, enter/exit
  events, positional correction + restitution/friction, static/dynamic/
  kinematic bodies, tilemap solid-cell grids, concave→hull approximation.
- Limits: no joints/constraints/raycasting, kinematic bodies don't push
  dynamics, >8192-cell maps unsupported, sub-2px colliders can tunnel.

## 14. Tilemap & level editor — ✅

- Tilesets with slicing/margins/spacing, per-tile solid flags, multi-layer
  painting (pencil/eraser/rect/flood/eyedrop/replace), region ops,
  flip/move, grid/zoom/pan, collision overlay, SWF bitmap baking +
  M13 solid-cell collision export. Verified in `m13-runtime-features`.

## 15. Camera & world — ✅

- Follow target, smoothing, dead-zone, zoom, world bounds with
  small-world centering, deterministic shake — preview and SWF (via
  `NovaGame`). Verified in `m12-collision-demo` (screen-centred target).

## 16. AI agent editor — ✅

- Transactional operations (create/modify scenes, objects, symbols,
  scripts, tile layers, game config, composite `create_game_level`),
  undo integration, diff-safe file ops, ~40 operations across M8–M12.
- Provider abstraction with model discovery, chat, privacy
  classification, credential storage (DPAPI on Windows).

## 17. AI providers — 🟡

- OpenAI-compatible + Ollama/local endpoints, API-key handling, streaming.
- Live-verified against local Ollama during development; cloud providers
  structurally supported but not exhaustively re-tested per release.

## 18. Preview & debugging — ✅

- Real-time game preview (`GameRuntime`), transport controls, status
  readout, console panel, per-frame stepping, volume, deterministic
  seed (`srand`/`Random`), `debugInfo`/`particleCount` probes,
  Ruffle harness for exported SWFs.

## 19. Ruffle compatibility — ✅

- Primary validation target; all milestone SWFs run with 0 ERROR lines.
- Deterministic fixed-step design specifically accommodates Ruffle's
  uncapped frame ticking.

## 20. Windows installer & portable — ✅

- `package-windows.ps1`: release build → staged payload → archive-safe
  swap → versioned ZIP + SHA256SUMS + BUILD-INFO.json.
- `package-installer.ps1`: .NET single-file installer with payload
  hashing, EULA, smoke test. Archive-before-delete with checksum
  verification (`archive-releases.ps1` + 15 tests).
- Unsigned binaries (no code-signing cert — documented, not claimed).

## Cross-cutting gaps (honest)

- No IK, shape tweens beyond basic, no bone rigging, no 3D, no video.
- No marketplace/plugin SDK (extensions = gamekit scripts + AI ops).
- AS3 subset is intentionally small; porting real Flash games requires
  rewriting scripts to the subset.
- Collaborative editing, version control UI, asset CDN — absent.
