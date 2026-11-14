# World Building & Visual Game Creation (M11)

Mega-Milestone 11 evolves NØVA Flash Studio from an animation editor with game
support into a visual 2D game-creation environment. Everything below is backed
by the project model (`formatVersion` 4), the deterministic game runtime and
the SWF/XFL exporters — nothing here is decorative UI.

## Tilemaps & Tilesets

- `ObjectKind::Tilemap(TilemapData)` is a first-class timeline object: grid
  dimensions, tile size, multiple layers with visibility/lock/opacity and
  stable cell storage (`u16` tile indices, `0` = empty).
- `Project.tilesets` stores `Tileset` records that reference existing bitmap
  assets by ID — image bytes are never duplicated. Tilesets carry per-tile
  names and a `solid` flag list used by preview collision.
- The bottom **Kacheln** tab is a real painting surface: it renders the actual
  tileset texture (UV sub-rects of the shared bitmap) and supports Stift,
  Radierer, Rechteck, Füllen (flood), Pipette and Ersetzen, plus NxN brush
  stamps, per-layer visibility/lock/opacity and auto-fit zoom. Edits go
  through the same `nova_core::tilemap` operations the AI uses and are covered
  by the project undo history.
- SWF export bakes every visible layer into one lossless bitmap
  (`DefineBitsLossless2`) with alpha compositing and layer opacity. Missing
  tilesets/assets produce explicit build warnings; the `.nova` data is
  preserved. XFL export emits a compatibility warning instead of writing
  tilemap data.

## Level Building

- Scene game config gained `WorldConfig` (world width/height, spawn point) —
  enforced by the preview runtime's body clamping.
- `CameraConfig` gained follow smoothing, a rectangular dead-zone and a
  maximum shake amplitude; all are honoured by the preview camera.
- `CollisionShape::Polygon { points }` adds arbitrary convex/concave polygons
  (validated: 3–64 points, finite coordinates, non-degenerate bounds) to the
  existing rectangle/circle shapes. `CollisionSpec` gained `layer`/`mask`
  bits; pairs only collide when `(a.layer & b.mask) != 0 && (b.layer & a.mask) != 0`.
- `ObjectInstance.particles: Option<ParticleEmitter>` attaches a deterministic
  emitter to any object. Nine presets (Custom, Rauch, Funken, Feuer, Regen,
  Schnee, Staub, Magie, Explosion) cover rate, burst, lifetime, speed range,
  direction/spread, gravity, drag, size/opacity/colour over life, rotation
  speed, local/world space, max particle count and a fixed seed — identical
  results on every run.
- The inspector exposes all emitter fields; the stage draws live particles
  during game preview only. Particles are intentionally **not** exported to
  SWF/XFL (explicit warnings are emitted).
- Tilemap solid tiles generate static tile colliders in the preview runtime
  (`attach_tile_colliders`); tilemap objects become implicit static bodies, so
  no game metadata is required.

## AI Operations

New typed, transactional operations (all validated, all-or-nothing):

| Operation | Purpose |
|---|---|
| `create_tileset` | Slice an existing bitmap asset into a tileset (auto columns, solid list, names) |
| `create_tilemap` | Place a tilemap object on a scene layer |
| `modify_tilemap` | Resize/re-tile/re-tileset an existing tilemap |
| `paint_tiles` | pencil/eraser/rect/flood/eyedrop/replace strokes |
| `create_collision_shape` | Set/update/remove rect, circle or validated polygon collision |
| `modify_camera` | Merge follow/bounds/zoom/smoothing/dead-zone/shake |
| `create_particle_emitter` / `configure_particle_emitter` | Upsert or patch an emitter (configure requires an existing one) |
| `create_level_group` | Folder layer for grouping |
| `place_game_object` | Rect + optional collision/physics in one step |
| `import_project_asset` | Extension-detected bitmap/audio import |
| `create_game_level` | Composite: scene + world + tileset + painted tilemap + objects + camera in one transaction |

## Project Format v4

- `formatVersion` is now **4**. Versions 1–3 load unchanged (all new fields
  are serde-defaulted); `formatVersion` is normalised on load.
- New serialised data: `Project.tilesets`, `ObjectKind::Tilemap`,
  `ObjectInstance.particles`, `CollisionShape::Polygon`, collision
  `layer`/`mask`, camera `follow_smoothing`/`dead_zone`/`shake`, and
  `GameSceneConfig.world`.
- Tileset `solid`/`names` vectors are length-agnostic; tile indices beyond
  the vector are non-solid/unnamed.

## Known Limitations (honest list)

- Particle emitters, polygon detail and tile collision are **preview-runtime
  only**. SWF export bakes tilemaps to bitmaps; emitters are skipped with a
  warning. XFL cannot represent tilemaps, particles or game metadata — all
  produce warnings and remain in `.nova`.
- Tile collision ignores per-layer collision masks (tileset solidity is
  binary); layer/mask filtering applies to regular object pairs only.
- Polygon collision uses the bounding-box + closest-point approximation in
  the preview solver, not a full SAT/clip implementation.
- The tilemap editor canvas auto-fits the whole map; there is no manual pan/zoom
  yet (the stage view itself has full zoom/pan).
- Folder layers provide grouping; persistent nested *object* groups are not
  part of the format.
- Tileset creation is exposed through AI ops and the inspector; there is no
  dedicated tileset-slicing dialog yet.
