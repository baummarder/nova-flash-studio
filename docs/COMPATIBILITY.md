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
| Collision: concave polygon | yes (decomposed in preview) | **no** — exported as authored polygon; SAT assumes convex, results undefined | n/a | not claimed |
| Layer/mask filtering | yes | yes | n/a | yes (`m12-collision-demo` ghost pair) |
| Enter/exit contact events | yes | yes | n/a | yes (`m12-event-bus`) |
| Penetration resolution (positional) | yes | yes (approximation) | n/a | yes |
| Physics: gravity/damping/bounce | yes | yes | n/a | yes |
| Physics: kinematic bodies | yes | velocity-only, no drive API | n/a | partial |
| Tilemap collision | yes | **no** — tilemap bakes to bitmap; colliders not exported | n/a | not claimed |
| Camera follow/bounds/zoom/dead-zone | yes | yes via `game.NovaGame` | n/a | yes (`m12-collision-demo`) |
| Camera shake | yes (decaying) | yes (sine approximation, deterministic) | n/a | not separately asserted |
| Pause/resume | yes | yes (`NovaGame.pause/resume`) | n/a | partial |
| Particles | yes | yes via `game.NovaGame` emitter bridge | n/a | yes (`m12-particle-runtime`) |
| Particle colour-end lerp | yes | yes (per-frame graphics redraw) | n/a | yes |
| Particle `maxParticles` > 512 | yes | clamped to 512 pool | n/a | documented limit |
| Input (keyboard state/edges) | yes | yes via `game.Input` + `game.NovaGame` bindings | n/a | partial (no synthetic key injection in harness) |
| Scene state persistence | yes | **no** — SWF scenes reload fresh; `gotoScene` does not restore per-scene object state | n/a | documented difference |
| `exportAllScenes` concatenation | n/a | yes (frame labels = scene names) | n/a | yes |
| AS3 document class (`Main.as`) | n/a | yes | yes | yes |
| AS3 subset (no `Math`, `for-in`, interfaces, `switch`, `try`, closures) | n/a | compiler-enforced | n/a | yes |
| AVM1 target | n/a | yes (no AS3/gamekit) | n/a | yes |
| Audio (SoundCue) | yes | exported | n/a | M9/M10 verified |
| SWF signing | n/a | **no** — artifacts are unsigned | n/a | not claimed |
| Windows installer | n/a | yes (Inno-less .NET installer, unsigned) | n/a | packaged, not code-signed |

## Known limitations (explicit)

- **Concave polygons**: the AS3 bridge uses SAT on the authored vertex list.
  Convex polygons are correct; concave input is *not* decomposed — results
  are undefined. The exporter does not reject them; treat concave colliders
  as unsupported in SWF.
- **Tilemap collision**: tile layers bake to a bitmap on export; per-tile
  collision is preview-only.
- **Scene state restore**: the preview persists per-scene body state across
  transitions; the SWF bridge reloads scene objects fresh (documented
  difference, `GameRuntime` parity gap).
- **Kinematic drive**: kinematic bodies integrate `vx`/`vy` but no public
  API sets them yet.
- **`Math`**: unavailable in the tested Ruffle/AVM2 path; `NovaGame` uses
  local helpers (`fSin`, `fSqrt`, …).
- **Dynamic class fields**: `#1056 Cannot create property` errors appear if
  a document class is not `dynamic` while the timeline places named
  children — use `public dynamic class Main`.
