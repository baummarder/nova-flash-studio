# Mega Milestone 11.1 — World Building Polish & Runtime Improvements

**Release:** `0.7.1-rc1` (release candidate, not stable)
**Baseline:** `0.7.0-rc1` (commit `926f3f8`)
**Scope:** close every documented M11 limitation — real polygon collision,
tile-collider layer/mask filtering, tileset margins/spacing, a practical
tilemap editor (zoom/pan/grid/selection/clipboard/flip), a dedicated tileset
slicing dialog, edit-mode particle preview, camera edge cases, AI coverage,
XFL tilemap export, performance measurements and full re-verification.

---

## 1. What changed, by subsystem

### Collision (`nova-core/src/collision.rs`, new module)

- Narrow-phase moved out of `game.rs` into a dedicated deterministic module.
- SAT implementation: polygon↔polygon over both polygons' edge normals,
  minimum-translation vector producing contact normal (B→A) and depth.
  Circle↔polygon uses closest-point-on-boundary with edge normals for
  interior penetration; circle↔circle stays analytic. Rectangles and
  circles are normalised into the same solver — rect↔polygon is now real
  polygon geometry, not a bounding box.
- `is_convex` validation: polygons with <3 points, non-finite coordinates or
  zero-area bounds are rejected; **concave or self-intersecting input
  collides as its convex hull** (`convex_hull`), never as a bounding box.
  Arbitrary concave decomposition is a documented non-goal.
- `resolve_pair`/`resolve_tiles` feed the same contacts into positional
  correction, restitution and the grounded flag; `GameRuntime`'s
  enter/exit tracking uses the same `collision::overlaps` geometry.

### Collision layer/mask

- Tile colliders now carry `layer`/`mask` (inherited from the tilemap
  object's collision spec, defaults 1/MAX so old projects behave identically).
- `specs_interact` in `game/runtime.rs` filters enter/exit pairs through
  `CollisionSpec::interacts_with`, so the event bus and the solver agree —
  including tilemap↔object pairs.
- Tests: physics mask filtering, tile-collider mask filtering, event-bus
  mask filtering (`collision_events_respect_layer_mask`), polygon
  integration and invalid-input handling.

### Model (`nova-core/src/model/tilemap.rs`)

- `Tileset` gained `margin_x/y` and `spacing_x/y` (serde-defaulted, skipped
  when zero → M11 files load byte-identically).
- `stride_x/stride_y`, `columns_for`, `rows_for`, `tile_count`, `tile_rect`
  (bounds-checked), `is_solid`, `tile_name` — every consumer (editor
  palette, stage renderer, SWF bake, XFL bake) resolves pixels through
  `tile_rect`, so slicing metadata is honoured identically everywhere.

### Tilemap core (`nova-core/src/tilemap.rs`)

- New region ops: `copy_region` (clamped, returns `TileRegion`),
  `paste_region` (overwrites including empty cells, respects locked layers
  and bounds), `clear_region`, `flip_region` (horizontal/vertical, locked
  layers skipped). `PaintTool::Rect` kept; selection move reuses copy/paste
  so one history entry covers the gesture.
- Tests for each op including bounds clamping and lock respect.

### Tilemap editor (`nova-app/src/tilemap_view.rs`, rewritten)

- Tools: Stift, Radierer, Rechteck, Füllen, Pipette, Ersetzen, **Auswahl**.
- View: mouse-wheel zoom around cursor, middle-mouse/Space+drag pan,
  Einpassen/100% buttons, configurable grid (on/off + opacity) and a
  solid-cell **collision overlay** (red tint).
- Selection: drag-select, drag-inside to move (paste-ghost preview),
  Kopieren/Ausschneiden/Einfügen/Löschen/Aufheben, ghost preview on paste,
  ↔/↕ flip toggles applied to stamps/pastes but not moves.
- **Tileset dialog**: asset picker, tile size/margins/spacing/name prefix,
  live sliced-grid preview over the bitmap, click-to-toggle solidity,
  validation (≥1 tile fits, dims sane, unique name). Creates a real
  `Tileset`; the bitmap stays a single shared asset.
- All mutations go through the existing `pending_before`/`History`
  coalescing — one undo entry per gesture, no parallel undo system.

### Particles

- Edit-mode preview in `stage_view.rs`: per-object `ParticleSystem`s stepped
  with real frame delta outside game preview; recreated on spec change,
  pruned on object removal. Pause/restart semantics are inherited from
  `GameRuntime` (systems rebuilt on `restart()`, frozen on `pause()`) and
  covered by existing tests.
- Still preview-only: SWF/XFL emit explicit warnings (documented).

### Camera (`nova-core/src/game.rs`)

- Small-world clamp: worlds smaller than the viewport centre instead of
  pinning to a degenerate range.
- Missing follow target falls back to stage centre.
- Tests for both edge cases.

### AI operations (`nova-ai/src/operations.rs`)

- `create_tileset` accepts `margin_x/y`, `spacing_x/y`, `solid`, `names`.
- New ops: `add_tile_layer`, `configure_tile_layer`, `flip_tile_region`,
  `move_tile_region` — all validated, transactional (failed batches leave
  the project untouched).
- Polygon collision ops validate point count, finiteness, extents and
  convexity.
- Tests: `tile_layer_region_and_slicing_operations` and transactional
  rejection coverage.

### Export

- SWF unchanged in shape: `define_tilemap` now delegates to the new public
  `nova_build::bake_tilemap`, which honours `tile_rect` (margins/spacing).
- **XFL**: `bake_tilemaps` bakes every scene/symbol tilemap to RGBA→PNG via
  the shared `bake_tilemap`, writes `LIBRARY/Tilemap_*.png`, adds
  `DOMBitmapItem`s and emits `DOMBitmapInstance` at the authored transform.
  Unbakeable maps fall back to an explicit warning.
- Test: `xfl_exports_tilemap_as_baked_bitmap` verifies the DOMBitmapInstance,
  library entry and PNG payload.

### Performance (`crates/nova-core/tests/perf.rs`, new)

Ignored by default; run with
`cargo test -p nova-core --release --test perf -- --ignored --nocapture`.
Release-build measurements on the dev machine:

| Measurement | Result |
|---|---|
| Paint rect 512×512 | ~0.27 ms |
| Flood fill 512×512 | ~4.2 ms |
| Copy 256×256 region | ~0.10 ms |
| Clear+paste 256×256 | ~1.1 ms |
| Flip 512×512 region | ~0.56 ms |
| 200 physics bodies × 120 steps | ~12 ms (~0.1 ms/step) |
| 50 emitters × 600 steps | ~6 ms |
| Project clone (undo snapshot, 512×512 map) | ~0.39 ms |

## 2. Verification evidence

- `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace`:
  **365 tests, 0 failures** (was ~330 at M11).
- New focused tests: SAT pairs/normals/depth, convex-hull fallback, degenerate
  rejection, tile layer/mask (physics + events), camera edge cases, region
  ops, tileset math, XFL bake, AI ops, perf suite.
- Serialization: all `samples/*.nova` (M9–M11, 21 files) pass `nova check`;
  an M11 project survives `.nova` export→check→SWF roundtrip. New Tileset
  fields are serde-defaulted; M11 files are unchanged.
- **Ruffle 0.7.1** (desktop, ~6 s per file, stderr captured): all seven
  `samples/m10-*.nova` SWFs and all five `samples/m11-*.nova` demo SWFs —
  **0 ERROR lines each**. Startup-log evidence only; no frame-verified
  visual playback was performed.

## 3. Honest limitations (unchanged or new)

- Concave polygons collide as their convex hull — true concave decomposition
  is not implemented.
- Per-cell tile collision uses the minimum-axis solver per cell; bodies on
  tile seams can be pushed along the minimum axis (same semantics as the old
  AABB solver, not a regression).
- Particles remain preview-only; SWF/XFL warn explicitly.
- XFL tilemap export produces a baked bitmap instance — layer/collision data
  is not exportable; the package stays experimental and untested in Adobe
  Animate.
- Per-tile rename UI does not exist (names come from slicing prefix/AI).
- Audible Ruffle/preview audio remains unverified headless (unchanged).
- Adobe Flash playback unverified; Ruffle is the validation target.

## 4. Release

- Workspace version `0.7.1-rc1`; `.nova` format stays **v4** (backwards
  compatible, no bump needed).
- Windows artifacts under `dist/windows/`:
  `NOVA-Flash-Studio-0.7.1-rc1-windows-x64.zip` (11.5 MB) and
  `NOVA-Flash-Studio-0.7.1-rc1-windows-x64-Setup.exe` (79.5 MB);
  SHA-256 in `dist/windows/SHA256SUMS.txt`.
  - zip: `87BDEF8BDA465474E14F225BDCCA027579ADEB8C3E1FC08B9B526C06D7B8E5FF`
  - setup: `B85A3CC726FE5484CC9357F9A7E5B83CC0F20A2D64499DEEA8175B1D8B0F6FB0`
- **0.7.0-rc1 archive:** `scripts/package-windows.ps1` wipes
  `dist/windows` before staging, which deleted the 0.7.0-rc1 artifacts
  before they were archived (dist is gitignored). They were rebuilt from
  the `926f3f8` release commit into `dist/archive/0.7.0-rc1/` (ZIP 11.4 MB,
  Setup 83.3 MB; SHA-256 in that folder's `SHA256SUMS.txt`). Because the
  rebuild is from identical source but not the original binaries, treat it
  as a faithful reconstruction — the original published checksums were lost
  with the files.
- macOS/Linux packaging scripts unchanged; still require native hosts.
