# Mega Milestone 10 — Game Development — Report

Status: **release candidate `0.6.0-rc1`, not stable.**

M10 turns NØVA from an animation authoring tool into the first stage of a
2D game-development environment. It deliberately reuses the existing
architecture: the `.nova` document model owns optional game metadata, the
editor preview simulates it, AS3 game logic stays in project scripts and is
compiled to ABC exactly like before, and SWF export is unchanged for scenes
without game metadata.

## What was built

### Model (`nova-core`, format v3)

`crates/nova-core/src/model/game.rs` adds optional, serde-defaulted game
metadata:

- `GameObjectConfig` on every `ObjectInstance`: `CollisionSpec`
  (rectangle/circle, pixel extents) and `PhysicsSpec` (static/dynamic/
  kinematic body, gravity scale, friction, bounce, controlled flag).
- `GameSceneConfig` on every `Scene`: `CameraConfig` (enable, follow object
  name, world bounds, zoom) and `InputBindings` (egui-style key names for
  left/right/up/down/jump/action).

Both are `Option` fields with `#[serde(default)]`, so v1/v2 documents load
unchanged and serialize identically when unused. A migration test strips all
game fields from a serialized project and verifies it loads as v3.

### Preview simulation (`nova-core::game`)

`crates/nova-core/src/game.rs` implements the editor-side simulation:

- `collect_bodies` merges game objects across frames (last keyframe wins,
  matching timeline semantics).
- `step` integrates gravity, damping, controlled acceleration, jump with
  ground detection, and resolves dynamic-vs-static collisions with
  restitution. `dt` is clamped to 100 ms.
- `step_camera` lerps a follow camera toward a named object (or stage
  centre) and clamps it to configured world bounds.

This simulation drives **only the editor preview**. It is not exported to
SWF; runtime behaviour in Ruffle/Flash comes from AS3 scripts.

### Game kit and templates

`crates/nova-core/src/gamekit.rs` ships installable AS3 helpers
(`scripts/game/{Input,Collision,Camera,Random,Physics}.as`) that compile
with the `nova-as` subset; `install` is idempotent and never overwrites.

`crates/nova-core/src/games.rs` + `assets/games/*/Main.as` define seven
templates — `blank`, `pong`, `breakout`, `platformer`, `topdown`, `racing`,
`menu` — each generating a complete `.nova` project with document class,
game-kit scripts, placed objects carrying collision/physics metadata, and
scene camera/input configuration where relevant. `gen_games` regenerates
`samples/m10-*.nova`.

### Editor UI (`nova-app`)

- Object inspector gained a **Spiel** section: enable game object, collision
  on/off with shape + width/height, physics on/off with body type,
  gravity/friction/bounce and a "controlled" checkbox.
- Scene inspector gained a **Spiel** section: enable scene game config,
  camera (follow-object picker, zoom, bounds) and the six input bindings.
- Pressing Play on a scene with game bodies runs the `nova-core` simulation
  in real time: WASD/arrows/Space drive controlled bodies via the scene
  bindings, the camera follows, simulated transforms replace authored
  positions while playing, and collision outlines are overlaid in green.
  Stop resets the simulation. Scenes without game metadata keep classic
  frame playback.

### AI operations (`nova-ai`)

Three new typed, previewable, transactional operations:

- `SetGameObject` — set/clear collision and physics on a named object with
  validation (positive finite extents, sane physics ranges).
- `SetSceneGame` — set/clear camera and input bindings with validation.
- `InstallGameKit` — installs the AS3 helpers idempotently.

## AS3 compiler fixes required by game logic

Game templates exposed three latent AVM2 emission bugs, all fixed and
verified under Ruffle 0.7.1:

1. **MultinameL namespace set** — `a[i]` indexing serialized namespace-set
   index 0, which the verifier rejects (`VerifyError #1032`). A real
   namespace set containing the public package namespace is now interned.
2. **Missing `label` ops** — AVM2 requires backward-jump targets to be
   `label` instructions (`VerifyError #1021`). `while`/`do…while`/`for`
   heads now emit `op::LABEL`.
3. **`dynamic` classes rejected** — the checker rejected the modifier even
   though the emitter already maps it to the sealed class flag. Document
   classes are now `public dynamic class Main`, which Ruffle requires for
   named timeline children (`Plat1` etc.) and ad-hoc properties.

Additionally `tools/as3-ruffle-test.py` strips UTF-8 BOMs from generated
sources; the embedded template `.as` files no longer carry one.

## Verification evidence

- `cargo test` green across the workspace: nova-core 40, nova-as 64,
  nova-build (incl. gamekit + template compile/roundtrip tests), nova-ai 44.
- All seven templates export to SWF (`SWF v10`, DoABC2 embedded) and run in
  Ruffle 0.7.1 with **zero ERROR lines** on stderr.
- Live `trace()` inside a running template reports `ball.x` changing and
  `flash.utils.getTimer()`/dt advancing at ~30 fps — the game loop, delta
  time and event dispatch work end to end.
- Editor-side simulation is covered by `nova-core::game` unit tests
  (integration, collision, camera clamp) and exercised through the preview.

## Known limitations (honest list)

- **Scene transitions are not implemented** in the preview or in AS3. SWF
  export concatenates scenes as labelled frames (M9 behaviour), but there is
  no `gotoAndStop`-style navigation in the AS3 subset and no transition UI.
- **Game events** (collision-enter/exit callbacks) exist only implicitly in
  the preview simulation; there is no event bus or AS3 event surface yet.
- **Audio in the preview** is not played; audio assets and sound cues export
  to SWF as before (M9), and templates do not yet use them.
- The preview simulation is intentionally simple (AABB/circle, one controlled
  body per scene is typical but not enforced, no tilemap/pathfinding).
- AS3 subset limits still apply (see `docs/AS3_COMPATIBILITY.md`): no
  inheritance beyond one level, no interfaces, limited display-list API.
- Ruffle verification covered the seven templates plus targeted bytecode
  probes; it is not a claim of general AS3 compatibility.

## Files of interest

- `crates/nova-core/src/model/game.rs`, `game.rs`, `gamekit.rs`, `games.rs`
- `crates/nova-core/assets/games/*/Main.as`
- `crates/nova-app/src/properties.rs` (Spiel sections),
  `crates/nova-app/src/app.rs` (`start_game_preview`, `step_game_preview`,
  `binding_pressed`), `crates/nova-app/src/stage_view.rs` (simulation
  overlay), `crates/nova-app/src/state.rs` (`GamePreview`)
- `crates/nova-ai/src/operations.rs` (game ops)
- `crates/nova-as/src/compiler/{abc,emit,check}.rs` (AVM2 fixes)
- `tools/as3-ruffle-test.py`, `crates/nova-as/examples/{disasm,emit_abc}.rs`
- `samples/m10-*.nova`, `samples/ruffle-captures/m10-*.png`
