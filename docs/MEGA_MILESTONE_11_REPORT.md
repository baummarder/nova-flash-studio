# Mega Milestone 11 Report — World Building & Visual Game Creation

**Version:** `0.7.0-rc1` (release candidate, not stable)
**Baseline:** `0.6.0-rc2` (M10.1)
**Format:** `.nova` `formatVersion` **4** (v1–v3 documents load unchanged)

M11 evolves NØVA Flash Studio from an animation editor with game support into
a visual 2D game-creation environment. The guiding constraint held throughout:
every feature is backed by the serialisable project model, the deterministic
preview runtime, the SWF/XFL exporters or explicit, visible warnings — no fake
controls, no silent degradation.

## 1. What was built

### Tilemaps & tilesets (model v4)

- `ObjectKind::Tilemap(TilemapData)` — grid dimensions, tile size, multiple
  layers with visibility/lock/opacity, `u16` cell values (`0` = empty).
- `Project.tilesets: Vec<Tileset>` — bitmap-asset-backed shared tilesets with
  tile size, column count, per-tile names and a `solid` flag list. Image data
  is referenced, never duplicated.
- `nova_core::tilemap` — single shared implementation of pencil, eraser,
  rectangle, flood fill, eyedropper and replace tools plus NxN brush stamps,
  used identically by the editor UI, AI operations and tests.

### Level building

- `GameSceneConfig.world: Option<WorldConfig>` — world size + spawn, enforced
  by preview-runtime body clamping.
- `CameraConfig` — follow smoothing, rectangular dead-zone, max shake
  amplitude; all honoured by `step_camera` in the preview.
- `CollisionShape::Polygon { points }` — validated polygon collision
  (3–64 points, finite coords, non-degenerate bounds) alongside
  rectangle/circle; `CollisionSpec.layer`/`mask` bit filtering for object
  pairs.
- `ObjectInstance.particles: Option<ParticleEmitter>` — deterministic
  emitters (seeded, capped, local/world space) with nine presets; simulated
  by `GameRuntime` and rendered as `ParticleView`s on the stage during game
  preview only.
- Tilemap solid cells become `TileCollider` grids via
  `attach_tile_colliders`; tilemap objects are implicit static bodies (no game
  metadata required). Tile collision ignores layer/mask — documented.

### Editor UI

- New bottom-dock **Kacheln** tab (`tilemap_view.rs`): tile palette rendered
  from the real tileset texture (per-tile UVs and names), all six paint
  tools, brush size, flood/eyedrop/replace parameters, layer
  visibility/lock/opacity controls and zoom-to-fit. Every edit flows through
  the existing history/undo pipeline.
- Inspector: polygon point editor, collision layer/mask, camera
  smoothing/dead-zone/shake, world spawn, full emitter form (preset + all
  fields).
- Stage: tilemaps render cell-by-cell with texture UVs and per-layer opacity;
  particles draw during preview; collision overlays include polygons.

### AI operations (12 new, all transactional)

`create_tileset`, `create_tilemap`, `modify_tilemap`, `paint_tiles`,
`create_collision_shape`, `modify_camera`,
`create_particle_emitter`/`configure_particle_emitter`,
`create_level_group`, `place_game_object`, `import_project_asset`,
`create_game_level` (composite scene+world+tileset+tilemap+objects+camera in
one validated transaction).

### Templates & demos

- Three new game templates: `tilemap-platformer` (painted parallax
  tilemap + proven platformer AS3), `tilemap-dungeon` (tiled floor/walls +
  topdown AS3 + torch emitters), `particle-showcase` (eight presets).
- Four demo projects: `m11-platformer.nova`, `m11-topdown-adventure.nova`,
  `m11-particle-showcase.nova`, `m11-tilemap-editing.nova` (script-free
  editing demo with polygon colliders, extended camera, solid metadata).

## 2. Export behaviour

- **SWF:** `define_tilemap` bakes all visible layers into one
  `DefineBitsLossless2` bitmap (alpha-composited, layer opacity honoured,
  8192 px cap). Missing tilesets/assets produce explicit warnings. Particles
  warn once per object. Verified: `DefineBitsLossless2` tag present in the
  built SWF.
- **XFL:** tilemaps, particles and game metadata warn per object; the data
  stays in `.nova`.
- **Ruffle 0.7.1:** all four M11 demo SWFs run with zero ERROR lines (only
  font-fallback warnings). This is startup-log evidence, not frame-verified
  visual playback.

## 3. Verification evidence

- `cargo test --workspace`: all green, including new tilemap paint/flood/
  eyedrop/replace tests, particle determinism tests, collision layer/mask
  tests, camera extension tests, format-v4 round-trip, AI
  `ai_builds_playable_tilemap_level` and `ai_rejects_invalid_tilemap_ops_
  transactionally` tests, and `tilemap_export` build tests
  (bake/warning/empty cases).
- `every_game_template_compiles`: all ten templates produce SWFs.
- Ruffle: 0 ERROR lines on `m11-platformer.swf`, `m11-dungeon.swf`,
  `m11-particles.swf`, `m11-editing.swf`.

## 4. Honest limitations

- Particle emitters, tile collision and polygon detail are **preview-runtime
  only**; SWF gameplay still comes from the AS3 document class.
- Tilemaps bake to a single bitmap per object — no per-tile editability in
  the exported SWF; tile collision does not exist in exported SWFs.
- XFL cannot represent tilemaps/particles/game metadata — warnings only.
- Tile collision ignores per-layer masks (binary tileset solidity).
- Polygon collision uses bounding-box + closest-point approximation, not
  full SAT.
- The tilemap editor canvas auto-fits; manual pan/zoom exists only on the
  stage view.
- Folder layers group content; persistent nested *object* groups are not in
  the format.
- Tileset slicing is exposed via AI ops; there is no dedicated slicing dialog.
- Audible Ruffle/preview audio remains unverified headless (unchanged from
  M10.1).

## 5. Release

- Workspace version `0.7.0-rc1`; Windows ZIP + installer + SHA-256 sums under
  `dist/windows/` (see `dist/windows/SHA256SUMS.txt`).
- macOS/Linux packaging scripts unchanged; still require native hosts.

## 6. M11.1 addendum (0.7.1-rc1)

World Building Polish closed the M11 limitations listed above: real SAT
polygon collision (convex hull fallback for concave input), collision
layer/mask filtering for tile colliders and the event bus, tilemap canvas
zoom/pan/grid/collision overlay, region select/copy/paste/move/flip with
undo, a dedicated tileset slicing dialog, tileset margins/spacing honoured
everywhere, edit-mode particle preview, camera small-world and
missing-target fixes, new AI ops (add_tile_layer,
configure_tile_layer, flip_tile_region, move_tile_region,
slicing fields on create_tileset), XFL tilemap-to-bitmap export and a
repeatable perf suite. See `docs/MEGA_MILESTONE_11_1_REPORT.md` for the
full report and `docs/WORLD_BUILDING.md` for the current documentation.

