# World Building & Visual Game Creation (M11 / M11.1)

Mega-Milestone 11 evolved NØVA Flash Studio from an animation editor with game
support into a visual 2D game-creation environment; M11.1 ("World Building
Polish & Runtime Improvements", `0.7.1-rc1`) closed the documented M11
limitations. Everything below is backed by the project model (`formatVersion`
4), the deterministic game runtime and the SWF/XFL exporters — nothing here is
decorative UI.

## Collision

- `crates/nova-core/src/collision.rs` is a dedicated, deterministic 2D
  narrow-phase module. Supported pairs:
  - circle ↔ circle (analytic),
  - circle ↔ convex polygon (closest-point on boundary, edge normals for
    interior penetration),
  - convex polygon ↔ convex polygon (SAT over both polygons' edge normals,
    minimum-translation vector with contact normal and depth).
- Rectangles and circles are converted to their polygon/circle equivalents, so
  rect↔polygon uses real polygon geometry — no bounding-box shortcut remains.
- Concave or self-intersecting polygons are **not** silently approximated:
  `is_convex` validation plus `convex_hull` fallback means a concave input
  collides as its convex hull. Degenerate polygons (<3 points, zero-area)
  produce no collider. The supported contract is: *convex polygons collide
  exactly; concave polygons collide as their convex hull.* Arbitrary concave
  decomposition is a documented non-goal.
- `resolve_pair`/`resolve_tiles` in `nova-core::game` use these contacts for
  positional correction and restitution; `GameRuntime`'s event bus uses the
  same geometry for enter/exit pairs.
- `CollisionSpec.layer`/`mask` are honoured everywhere: object↔object pairs,
  object↔tilemap pairs **and** the enter/exit event tracking. Tile colliders
  inherit the tilemap object's collision spec (defaults layer 1, mask all),
  so older projects behave exactly as before.

## Tilemaps & Tilesets

- `ObjectKind::Tilemap(TilemapData)` is a first-class timeline object: grid
  dimensions, tile size, multiple layers with visibility/lock/opacity and
  stable cell storage (`u16` tile indices, `0` = empty).
- `Project.tilesets` stores `Tileset` records that reference existing bitmap
  assets by ID — image bytes are never duplicated. Tilesets carry per-tile
  names, a `solid` flag list and M11.1 slicing metadata:
  `margin_x/y` (edge padding) and `spacing_x/y` (gap between tiles). All
  consumers — editor palette, stage renderer, SWF bake, XFL bake — resolve
  tile pixels through `Tileset::tile_rect`, so margins/spacing behave
  identically everywhere.
- The bottom **Kacheln** tab is a real painting surface:
  - tools: Stift, Radierer, Rechteck, Füllen (flood), Pipette, Ersetzen and
    **Auswahl** (region select);
  - NxN brush stamps, per-layer visibility/lock/opacity;
  - **Mausrad zoom around the cursor**, middle-mouse or Space+drag pan,
    "Einpassen"/"100%" view buttons;
  - configurable grid (visibility + opacity), tile-boundary grid and an
    optional **collision overlay** that tints solid cells red;
  - **selection**: drag to select, drag inside a selection to move it,
    Kopieren/Ausschneiden/Einfügen/Löschen/Aufheben buttons, paste ghost
    preview, and ↔/↕ flip toggles applied to brush stamps and pasted regions
    (moves preserve cells verbatim);
  - every gesture is one project-history entry, so undo/redo covers strokes,
    fills, selections, moves, flips and pastes without per-cell snapshots.
- Flood fill, replace and brush stamps share the same bounded core
  (`nova_core::tilemap`) used by the AI ops; out-of-range cells are clipped,
  never wrap.
- **Tileset dialog** ("Tileset…" in the Kacheln toolbar): pick a bitmap
  asset, set tile size/margins/spacing/name prefix, preview the sliced grid
  live, click tiles to toggle solidity, and create the `Tileset` — all with
  live validation (dimensions, grid fit, duplicate names). The source bitmap
  stays a single shared asset.
- SWF export bakes every visible layer into one lossless bitmap
  (`DefineBitsLossless2`) honouring margins/spacing via `tile_rect`, alpha
  compositing and layer opacity. Missing tilesets/assets produce explicit
  build warnings; the `.nova` data is preserved.
- **XFL export** now bakes each tilemap to a PNG (`nova_build::bake_tilemap`,
  shared with the SWF path), writes it under `LIBRARY/` and places a
  `DOMBitmapInstance` at the authored transform. Unbakeable tilemaps fall back
  to an explicit warning. Layer/collision data remains editor metadata.

## Level Building

- Scene game config has `WorldConfig` (world width/height, spawn point) —
  enforced by the preview runtime's body clamping.
- `CameraConfig` has follow smoothing, dead-zone, max shake and M11.1 fixes:
  worlds smaller than the viewport centre correctly instead of clamping to a
  degenerate range, and a missing/renamed follow target falls back to the
  stage centre rather than freezing the camera.
- `ObjectInstance.particles: Option<ParticleEmitter>` attaches a deterministic
  emitter (nine presets + Custom) with rate, burst, lifetime, speed, direction/
  spread, gravity, drag, size/opacity/colour over life, rotation, space and
  max count. `ParticleSystem` is a seeded xorshift simulation — identical
  results every run; systems are rebuilt on scene load/restart and paused
  with the runtime, so restart/pause semantics are exact.
- M11.1 adds an **edit-mode particle preview** on the stage (outside game
  preview): emitters simulate with real frame delta against authored object
  transforms, recreate when the spec changes and are pruned when the object
  disappears.
- Particles remain **preview-only**: SWF and XFL export emit explicit warnings
  and keep the data in `.nova`. This is a documented limitation, not an
  oversight.
- Tilemap solid tiles generate static per-cell colliders
  (`attach_tile_colliders`); layer/mask filtering applies to them. Tile
  collision resolves per-cell with SAT; bodies landing exactly on tile seams
  can be pushed along the minimum axis (same semantics as the previous AABB
  solver — documented behaviour, not a regression).

## AI Operations

Typed, transactional, validated, all-or-nothing operations. M11 additions plus
M11.1 extensions:

| Operation | Purpose |
|---|---|
| `create_tileset` | Slice a bitmap asset; now accepts `margin_x/y`, `spacing_x/y`, solid list, names |
| `create_tilemap` / `modify_tilemap` | Place/resize/re-tile tilemap objects |
| `paint_tiles` | pencil/eraser/rect/flood/eyedrop/replace strokes |
| `add_tile_layer` | Append a named empty tile layer |
| `configure_tile_layer` | Name/visibility/lock/opacity of one layer |
| `flip_tile_region` | Mirror a cell region horizontally/vertically |
| `move_tile_region` | Move a cell region by (dx,dy) in one gesture |
| `create_collision_shape` | Rect/circle/validated polygon + layer/mask |
| `modify_camera` | Follow/bounds/zoom/smoothing/dead-zone/shake |
| `create_particle_emitter` / `configure_particle_emitter` | Upsert/patch emitters |
| `create_level_group`, `place_game_object`, `import_project_asset`, `create_game_level` | Composite building blocks |

`create_game_level` remains the single-transaction "generate a whole level"
path; the new layer/region ops let agents refine generated maps afterwards.

## Project Format v4

- `formatVersion` is **4**. Versions 1–3 load unchanged; every M11/M11.1
  field is serde-defaulted (Tileset margins/spacing skip-serialize when zero,
  so M11 files are byte-compatible).
- New serialised data: `Project.tilesets`, `ObjectKind::Tilemap`,
  `ObjectInstance.particles`, `CollisionShape::Polygon`, collision
  `layer`/`mask`, camera smoothing/dead-zone/shake, `GameSceneConfig.world`,
  Tileset `margin_*`/`spacing_*`.

## Performance (release build, `cargo test -p nova-core --release --test perf`)

Measured on the dev machine; rerun with
`cargo test -p nova-core --release --test perf -- --ignored --nocapture`:

| Measurement | Result |
|---|---|
| Paint rect on 512×512 tilemap | ~0.27 ms |
| Flood fill 512×512 | ~4.2 ms |
| Copy 256×256 region | ~0.10 ms |
| Clear+paste 256×256 | ~1.1 ms |
| Flip 512×512 region | ~0.56 ms |
| 200 physics bodies × 120 steps | ~12 ms total (~0.1 ms/step) |
| 50 particle emitters × 600 steps | ~6 ms total |
| Project clone (512×512 tilemap) for undo | ~0.39 ms |

Undo history stores project snapshots; a 512×512 tilemap clone costs well
under a millisecond, so per-gesture snapshots are cheap. The tilemap canvas
culls off-screen cells before rasterising.

## Known Limitations (honest list)

- Concave polygons collide as their convex hull; true concave decomposition
  is not implemented.
- Per-cell tile collision uses the minimum-axis solver per cell; bodies on
  tile seams can slide along the minimum axis (same as the old AABB solver).
- Particles are preview-runtime only — no SWF/XFL representation.
- Tilemaps bake to a single bitmap for SWF/XFL; per-layer objects, tile
  animation and per-tile collision are not exportable.
- XFL tilemap export produces a baked bitmap instance, not editable tile
  layers; the XFL package remains experimental and untested in Adobe Animate.
- Folder layers provide grouping; persistent nested *object* groups are not
  part of the format.
- Tileset names are editable via the slicing dialog prefix; there is no
  per-tile rename UI yet.
