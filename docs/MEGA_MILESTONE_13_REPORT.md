# Mega Milestone 13 — Runtime Parity Completion & Capability Audit

Baseline: `0.8.0-rc1` / `f2a1a9d` → Target: `0.9.0-rc1`

M13 closed the remaining documented M12 runtime gaps in the exported-SWF
bridge, fixed a significant stale-instance bug, and produced a full
capability audit plus an honest Animate comparison.

## Delivered

### `Math.*` support (AS3 subset)

- New gamekit script `scripts/Math.as`: a top-level `Math` class with
  `abs, sqrt, floor, ceil, round, trunc, min, max, pow, sin, cos, tan,
  asin, acos, atan, atan2, exp, log, random, setSeed` and the standard
  constants. Deterministic series implementations + LCG `random`.
- Compiler change (`nova-as/src/compiler/check.rs`): the identifier
  `Math` now resolves as an external class reference (`ClassRef`) in both
  class-member and script scope, so `Math.sqrt(...)` emits
  `getlex Math` + `callproperty` instead of a `this.Math` lookup that
  always failed with `Error #1010`.
- Verified in Ruffle: `sqrt(2)=1.41421356`, `sin(1)=0.84147098`,
  `floor(-1.5)=-2`, `pow(2,10)=1024`, `atan2(1,1)=0.78539816`,
  `log(E)=1` — `M13 math ok=true`, zero errors.

### Kinematic body drive API

- `NovaGame.setBodyVelocity(name, vx, vy)` / `getBodyVelocity(name)`.
- Ruffle-verified: kinematic body moved 90 px/s → display x=190
  (`M13 kin x=190 ok=true`).

### Per-scene state save/restore

- Body position+velocity snapshots per scene on exit, restored on
  revisit — parity with the preview `GameRuntime` state memory.
- `clearSceneStates()` resets. Ruffle-verified: hero landed at y=300,
  left and re-entered the scene, restored to y=300 (`ok=true`).

### Key injection (deterministic input tests)

- `Input.debugSetKey(code, down)` + `NovaGame.debugSetKey` which also
  emits `nova:keyPressed`/`nova:keyReleased`. Injected keys flow through
  the real `Input.down`/`pressed` path — a configured `Space → Level`
  transition fired end-to-end in Ruffle (`M13 scene=Level ok=true`).

### Tilemap collision export

- `nova-build` emits shape-4 "tile grid" collider specs for tilemap
  objects with solid tiles (≤8192 cells, mirrors
  `attach_tile_colliders` solidity logic). The bridge expands contiguous
  solid runs into static rect bodies sharing the map object's name —
  one contact pair per map object, matching preview semantics.
- Ruffle-verified: dynamic hero landed on solid tile rows at y=300 with
  `nova:collisionEnter` (`Hero<>Terrain`) — `M13 tile enter=true`.

### Concave polygon parity

- Export now applies `nova_core::collision::convex_hull` to non-convex
  polygon colliders — the same approximation the preview uses — with an
  explicit warning instead of undefined SAT behaviour. Unit-tested.

### Bug fixes found via Ruffle validation

- **Stale body→display binding (major)**: multi-frame scenes re-place
  objects on every `gotoAndStop`, replacing DisplayObject instances.
  Bodies kept writing physics to removed instances — dynamic bodies
  moved internally but never on screen unless the camera masked it.
  `refreshBodyObjects()` now re-resolves `body.o` every tick.
- **Tunneling**: physics now runs 8 substeps/frame so fast bodies cannot
  skip thin colliders between frames.
- Debug info now reports real extents (was `undefinedxundefined`).

### Demo & tests

- `tools/gen-m13-demo.py` → `samples/m13-runtime-features.nova`
  (Boot/Level scenes covering all of the above).
- `crates/nova-build/tests/m13_runtime.rs`: 3 tests (Math script
  presence + DoABC2 counts, tile-grid emission, concave-hull warning).
- All four M12 demos re-verified green after the runtime changes.

## M12 gap closure status

| M12 gap | Status |
|---|---|
| Concave polygon collision | ✅ hull approximation (parity) |
| Tilemap collision export | ✅ solid-cell grid specs |
| Per-scene state restore | ✅ snapshots + restore |
| Kinematic body API | ✅ set/getBodyVelocity |
| Key injection tests | ✅ debugSetKey |
| `Math.*` support | ✅ shim class |
| Code signing | 🔴 still unsigned (no cert) |

## Documentation

- `docs/CAPABILITY_AUDIT.md` — 20-category feature matrix.
- `docs/FLASH_COMPARISON.md` — honest Animate comparison.
- `docs/COMPATIBILITY.md`, `docs/NOVAGAME_API.md` — updated.

## Not claimed

- True concave decomposition, tile flip/rotation collision, kinematic
  collision response, `Math.imul`/`fround`/`clz32`/`sign`/`cbrt`/`expm1`/
  `log1p`/`hypot`, code signing, plugin SDK, real-AS3-codebase
  compatibility.
