# Compatibility matrix — NØVA Flash Studio 0.8.0

Evidence-based support levels. "Preview" = editor simulation (`GameRuntime`),
"SWF" = exported `.swf` playback, "XFL" = round-trip import/export,
"Ruffle" = verified in the Ruffle desktop runtime via
`tools/ruffle-runtime-test.py` trace assertions.

| Feature | Preview | SWF export | XFL | Ruffle-verified |
|---|---|---|---|---|
| Scenes & multi-scene timelines | yes | yes (concatenated frames + labels) | partial | yes |
| Scene transitions (key/API) | yes | yes via `game.NovaGame` | metadata only | yes (`m12-scene-transitions`) |
| Game event bus (typed events) | yes | yes via `game.NovaGame` | n/a | yes (`m12-event-bus`) |
| Collision: rectangle | yes | yes | n/a | yes (`m12-collision-demo`) |
| Collision: circle | yes | yes | n/a | yes (`m12-collision-demo`) |
| Collision: convex polygon | yes | yes | n/a | yes (`m12-collision-demo`) |
| Collision: concave polygon | yes (convex hull) | yes (same convex-hull approximation at export, warned) | n/a | partial (hull path covered by unit test) |
| Layer/mask filtering | yes | yes | n/a | yes (`m12-collision-demo` ghost pair) |
| Enter/exit contact events | yes | yes | n/a | yes (`m12-event-bus`) |
| Penetration resolution (positional) | yes | yes (approximation) | n/a | yes |
| Physics: gravity/damping/bounce | yes | yes | n/a | yes |
| Physics: kinematic bodies | yes | yes — `setBodyVelocity`/`getBodyVelocity` drive API | n/a | yes (`m13-runtime-features`) |
| Tilemap collision | yes | yes — solid cells export as static rect runs (shape-4 grid specs, ≤8192 cells/map) | n/a | yes (`m13-runtime-features`) |
| Camera follow/bounds/zoom/dead-zone | yes | yes via `game.NovaGame` | n/a | yes (`m12-collision-demo`) |
| Camera shake | yes (decaying) | yes (sine approximation, deterministic) | n/a | not separately asserted |
| Pause/resume | yes | yes (`NovaGame.pause/resume`) | n/a | partial |
| Particles | yes | yes via `game.NovaGame` emitter bridge | n/a | yes (`m12-particle-runtime`) |
| Particle colour-end lerp | yes | yes (per-frame graphics redraw) | n/a | yes |
| Particle `maxParticles` > 512 | yes | clamped to 512 pool | n/a | documented limit |
| Input (keyboard state/edges) | yes | yes via `game.Input` + `game.NovaGame` bindings; `debugSetKey` injects synthetic keys | n/a | yes (`m13-runtime-features` key-driven transition) |
| `Math.*` in AS3 | n/a | yes — `scripts/Math.as` shim (sqrt/sin/cos/tan/asin/acos/atan/atan2/exp/log/pow/floor/ceil/round/trunc/abs/min/max/random + constants) | n/a | yes (`m13-runtime-features`) |
| Scene state persistence | yes | yes — per-scene body snapshots restored on revisit (`clearSceneStates` resets) | n/a | yes (`m13-runtime-features`) |
| `exportAllScenes` concatenation | n/a | yes (frame labels = scene names) | n/a | yes |
| Skeletal animation (bone hierarchy, FK clips, rigid binding) | yes | yes — baked per-frame matrices in symbol sprites | metadata only | partial — matrices verified structurally; Ruffle plays the sprite but AVM2 cannot reach AVM1-style sprite children (`m15-skeletal-animation`) |
| Two-bone IK | yes (solver) | yes (IK keys baked to rotations) | n/a | unit-tested (`solve_two_bone`) |
| Multi-bone chain IK (FABRIK + pole) | yes | yes (IK keys baked to rotations) | n/a | unit-tested (`fabrik_solve`, `chain_ik_*`) |
| AS3 document class (`Main.as`) | n/a | yes | yes | yes |
| AS3 subset (no `for-in`, interfaces, `switch`, `try`, closures, `Vector.<T>`; `Math`/`JSON` resolve) | n/a | compiler-enforced | n/a | yes |
| Playerglobal APIs at SWF v13+ (JSON, ByteArray, Dictionary, Timer, TextField, SimpleButton, Graphics, BitmapData, Point/Rectangle/Matrix/ColorTransform, LoaderInfo) | n/a | n/a | yes | yes (`m14-api-surface`, 20/20 probes) |
| AVM1 target | n/a | yes (no AS3/gamekit) | n/a | yes |
| Audio (SoundCue) | yes | exported | n/a | M9/M10 verified |
| SWF signing | n/a | **no** — artifacts are unsigned | n/a | not claimed |
| Windows installer | n/a | yes (Inno-less .NET installer, unsigned) | n/a | packaged, not code-signed |

## Known limitations (explicit)

- **Concave polygons**: both preview and SWF approximate concave input by
  its convex hull (export warns). True decomposition is not implemented.
- **Tilemap collision**: solid cells export as per-row rect runs
  (≤8192 cells/map); contacts merge into one pair per map object. Tile
  rotation/flip and per-layer collision masks are not modelled.
- **`Math.*`**: shim covers common functions with approximations
  (series-based trig/exp/log, deterministic LCG `random`/`setSeed`) plus
  `imul`, `sign`, `clz32`, `cbrt`, `hypot`, `expm1`, `log1p`, `fround`
  (M14); it is not bit-identical to Flash's `Math`.
- **Playerglobal APIs**: available only at SWF v13+ (script projects are
  auto-raised). Verified in Ruffle: JSON, ByteArray+Endian, Dictionary,
  Timer/TimerEvent, TextField/TextFormat, SimpleButton (states;
  `hitArea` unsupported), Graphics drawing, BitmapData/Bitmap,
  LoaderInfo, Point/Rectangle/Matrix/ColorTransform, String/Array methods.
  NOT available: `Vector.<T>` syntax, package-level functions
  (`getQualifiedClassName`, …), `try/catch`, `for-in`, closures.
- **Kinematic bodies**: `setBodyVelocity`/`getBodyVelocity` exist; kinematic
  bodies still produce no collision *response* on dynamics.
- **Substepping**: physics runs 8 substeps/frame; colliders thinner than
  ~2 px can still be skipped at extreme speeds.
- **Dynamic class fields**: `#1056 Cannot create property` errors appear if
  a document class is not `dynamic` while the timeline places named
  children — use `public dynamic class Main`.
