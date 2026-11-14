# Mega Milestone 12 — Runtime Parity & Release Engineering

Baseline: `0.7.1-rc1` / `0e5dff0` → Target: `0.8.0-rc1`

M12 moves supported gameplay behaviour from preview-only operation into
exported SWF runtime behaviour, validates it in Ruffle, and makes release
packaging safe and reproducible.

## Delivered

### Release engineering (packaging safety)

- `scripts/archive-releases.ps1` — archives `dist/windows` artifacts into
  `dist/archive/<version>/` before any destructive replacement. Verifies
  `SHA256SUMS.txt` entries, refuses differing archive content without
  `-Force`, supports `-WhatIfArchive`, preserves corrupt artifacts while
  warning on checksum mismatch, and is idempotent.
- `scripts/package-windows.ps1` — stage-then-swap build into a staging
  directory, `BUILD-INFO.json` metadata, archive-before-delete, versioned
  ZIP + installer + `SHA256SUMS.txt`.
- `scripts/test-archive-releases.ps1` — 15 automated packaging tests
  (all passing).

### AS3 runtime bridge (`game.NovaGame`)

- New gamekit class `scripts/game/NovaGame.as` (~1400 lines) covering:
  runtime init/cleanup, fixed-step time/delta, input state + edges,
  object position/velocity access (`setBodyPosition`, `debugInfo`), scene
  identification, `gotoScene` + configured key transitions, public
  `emit`/listener API, lifecycle/scene/collision/input event types,
  collision (rect/circle/convex polygon SAT, layer/mask, enter/exit,
  penetration resolution), camera (follow/bounds/zoom/smoothing/dead-zone/
  shake), pause/resume/restart, and a particle emitter runtime.
- Event bus: multiple listeners, registration-order dispatch, safe dispatch
  under mutation, recursion depth cap (8) with `droppedEventCount()`.
- Deterministic fixed step: one `ENTER_FRAME` = one authored frame
  (`1/FPS`), independent of renderer rate — required because Ruffle ticks
  uncapped and wall-clock dt made behaviour non-deterministic.

### SWF export integration

- `nova-build` generates `scripts/game/NovaGameData.as` whenever the export
  contains multiple scenes, scene game config, object collision/physics, or
  particle emitters. It encodes scene frame ranges, per-scene collider
  specs, camera/world/input/transition tables, and emitter specs.
- Transitions configured in project metadata now auto-enable
  `exportAllScenes` (concatenated timeline) with an explicit warning-free
  note; scene names remain frame labels.
- Key-name → AVM2 `keyCode` mapping with warnings for unsupported names.
- Particle warning reworded: emitters run only under the `NovaGame` bridge.

### Ruffle runtime harness

- `tools/ruffle-runtime-test.py` — exports a `.nova` (or takes `.swf`),
  runs Ruffle, captures `avm_trace` + ERROR/WARN lines, evaluates
  `--expect`/`--forbid` trace assertions, emits a JSON report. Pass =
  markers emitted AND zero ERROR lines — runtime behaviour, not startup.

### Four M12 demo projects (`samples/`)

Generated reproducibly by `tools/gen-m12-demos.py`:

| Demo | Verified in Ruffle |
|---|---|
| `m12-scene-transitions.nova` | 3 scenes, `gotoScene` API, invalid target → `false`, load/unload/transition counts, return to earlier scene |
| `m12-event-bus.nova` | collision enter/exit pair, two ordered listeners, listener removal, custom payload, recursion guard (`dropped>0`) |
| `m12-collision-demo.nova` | rect+circle+convex-polygon landings, layer/mask ghost pair produces no events, camera follow centres target |
| `m12-particle-runtime.nova` | `particleCount()` > 0 and growing across emitters |

All four: `nova check` clean, SWF export clean, Ruffle run with **0 ERROR
lines** and all trace assertions passing.

## Bugs found & fixed during validation

- `circlePoly` built polygon vertices from `poly.pts[i]` which is empty for
  rect colliders → `TypeError` every frame → zero contacts. Now reuses
  `worldPoints`.
- `worldPoints` caches (`ptsA`/`ptsB`/`ptsWorld`) went stale after
  integration → bodies fell through colliders. Caches invalidated before
  the contact-detection pass.
- Contact tracking measured POST-resolution positions → resting contacts
  (depth ≈ 0) never registered enter/exit. Detection moved to a pre-resolution
  pass mirroring preview semantics.
- Scene playhead drifted through concatenated scenes → `gotoAndStop`
  stepping inside the active range.
- Static bodies adopted camera screen-positions as world positions →
  collider corruption. Static `px` now authored-only; `setBodyPosition` is
  the move API.
- Camera follow read the display transform it had just written (feedback
  loop) → `worldPosOf` resolves world position through body records +
  external-write detection.
- Wall-clock dt made sim rate depend on renderer fps → fixed-step
  `1/FPS` per `ENTER_FRAME`.

## Not claimed / unsupported

- Concave polygon decomposition in SWF (SAT assumes convex).
- Tilemap collision export.
- Per-scene object state restoration across transitions.
- Kinematic drive API.
- `Math.*` in user scripts (unavailable in tested AVM2 path).
- Code signing of binaries/installer; Ruffle pixel-accurate validation.

See `docs/COMPATIBILITY.md` for the full matrix and
`docs/NOVAGAME_API.md` for the bridge API.
