# `game.NovaGame` — exported-SWF runtime bridge (M12)

`NovaGame` is the ActionScript 3 runtime that makes game metadata authored in
the editor actually execute inside an exported `.swf`. It is installed as a
gamekit script (`scripts/game/NovaGame.as`) and paired with a generated data
class (`game.NovaGameData`) that the SWF exporter emits whenever the project
contains multiple scenes, scene game config, object collision/physics
metadata, or particle emitters.

## Usage

```ascript
package {
    import flash.display.MovieClip;
    import flash.events.Event;
    import game.NovaGame;

    public dynamic class Main extends MovieClip {
        public function Main() {
            NovaGame.init(this);          // call once, first thing
            addEventListener(Event.ENTER_FRAME, onTick);
        }
        private function onTick(e:Event):void {
            // game code here
        }
    }
}
```

`init(root)` hooks `Event.ENTER_FRAME` on the document root and begins the
fixed-step simulation (one tick = one authored frame at the SWF frame rate).

## Public API

| Member | Signature | Notes |
|---|---|---|
| `init` | `(root:DisplayObject):void` | idempotent; emits `nova:gameStart` |
| `pause` / `resume` / `restart` | `():void` | lifecycle control + events |
| `isPaused` | `():Boolean` | |
| `getTime` / `getDeltaTime` | `():Number` | seconds since start / fixed step |
| `currentScene` / `sceneCount` / `sceneIndex` | `():String/int` | active scene info |
| `gotoScene` | `(name:String):Boolean` | false + `nova:sceneError` for unknown names |
| `emit` | `(type:String, data):void` | dispatch a custom `nova:*` event |
| `on` / `off` | `(type, listener)` | AVM2 `add/removeEventListener` aliases |
| `eventInfo` | `Object` | `{kind, scene, a, b, key, data}` for the last event |
| `droppedEventCount` | `():int` | events dropped by the recursion guard |
| `particleCount` | `():int` | live particles in the current scene |
| `setBodyPosition` | `(name, x, y):Boolean` | teleport a body (test/debug hook) |
| `setBodyVelocity` | `(name, vx, vy):Boolean` | drive kinematic/dynamic bodies (M13) |
| `getBodyVelocity` | `(name):Object` | `{x, y}` or null (M13) |
| `debugSetKey` | `(keyCode, down):void` | inject synthetic key state; fires `nova:keyPressed`/`Released` and drives `Input.down`/`pressed` incl. configured transitions (M13) |
| `clearSceneStates` | `():void` | drop per-scene body snapshots (M13) |
| `debugInfo` | `():String` | diagnostic dump |

## Events (`flash.events.Event` on the root)

`nova:gameStart`, `nova:gamePause`, `nova:gameResume`, `nova:gameStop`,
`nova:gameRestart`, `nova:sceneLoad`, `nova:sceneUnload`,
`nova:sceneTransition`, `nova:sceneError`, `nova:collisionEnter`,
`nova:collisionExit`, `nova:keyPressed`, `nova:keyReleased`, plus any custom
`nova:*` type dispatched via `emit`.

Multiple listeners per type run in registration order; listeners may be
removed during dispatch; recursion is capped (depth 8) with drops counted in
`droppedEventCount()`.

`eventInfo` fields: `kind` (event family), `scene`, `a`/`b` (sorted object
names for collision pairs), `key` (AVM2 keyCode for input events), `data`
(custom payload).

## What the bridge simulates

- **Scenes**: `NovaGameData` stores per-scene frame ranges; the bridge owns
  the playhead inside the active range (`gotoAndStop` stepping) so the
  concatenated timeline cannot drift into neighbouring scenes. Multi-frame
  scenes loop. `gotoScene` and configured `scene.transitions` (key → target)
  both work; returning to earlier scenes is supported. Body position and
  velocity are snapshotted per scene on exit and restored on revisit
  (preview `GameRuntime` parity); `clearSceneStates()` resets memory.
  Stale display-object bindings are re-resolved every tick because
  multi-frame scenes replace instances on `gotoAndStop`.
- **Collision**: rectangle, circle and convex polygon bodies with SAT,
  layer/mask bit filtering, enter/exit contact tracking on pre-resolution
  positions, and positional correction + restitution/friction for dynamic
  bodies. Concave polygons are approximated by their convex hull at export
  (same as the preview, with a warning). Tilemap objects export solid-cell
  grids (shape-4 specs, ≤8192 cells) expanded into static rect runs sharing
  the map object's name — one contact pair per map, matching preview
  semantics. Static bodies keep authored positions; kinematic bodies
  integrate `setBodyVelocity` velocity without forces. Physics runs 8
  substeps per frame to limit tunneling.
- **Camera**: follow target (world position resolved through body records),
  smoothing, dead zone, zoom, world-bound clamping with small-world
  centering, deterministic sine shake. The camera writes display transforms
  for every named top-level child; external writes are detected via last
  screen-position comparison.
- **Particles**: emitters attached to named objects — rate/burst, lifetime,
  size/opacity ranges, colour lerp, speed/direction/spread, gravity, drag,
  rotation, local/world space, per-emitter pool capped at 512 live particles.
  RNG is a Park-Miller LCG (deterministic per scene, different sequence than
  the preview's xorshift — distribution equivalent, not bit-identical).
- **Time**: fixed step `1/FPS` per `ENTER_FRAME`, clamped to 0.1 s —
  deterministic regardless of renderer frame rate.

## Limitations

- `Math.*` is provided by the `scripts/Math.as` gamekit shim (approximate
  series implementations, deterministic LCG `random`); uncommon functions
  (`imul`, `fround`, `clz32`, `sign`, `cbrt`, `expm1`, `log1p`, `hypot`)
  are missing. Other Flash builtins (`ByteArray`, `Vector`, `JSON`, …)
  remain unavailable.
- Tilemap collision covers solid cells only; tile flip/rotation and
  per-layer masks are not modelled.
- Kinematic bodies have velocity but produce no collision response.
- `maxParticles` is clamped to 512 in SWF.
- Key-triggered transitions are configured in project metadata
  (`scene.game.transitions[].key`) using egui-style key names; unsupported
  names produce an export warning and are ignored.
