# M14 Test Report — AVM2 API Surface

All runtime evidence from Ruffle 0.7.1 via `tools/ruffle-runtime-test.py`
(logs in `%TEMP%\m14api*.json` / `*.swf.log`).

## Ruffle runtime tests

### `samples/m14-api-surface.nova` (SWF v13) — 20 traces, 0 errors

| Probe | Result |
|---|---|
| `json` (parse object+array+string) | true |
| `jsonStr` (stringify contains `"x":7`) | true |
| `ba` (ByteArray int+UTF roundtrip, LITTLE_ENDIAN) | true |
| `baLen` (length==6) | true |
| `dict` (Dictionary dynamic key) | true |
| `point` (Point clone/fields) | true |
| `rect` (Rectangle contains) | true |
| `matrix` (transformPoint) | true |
| `colorXform` (redMultiplier) | true |
| `str` (charAt/indexOf/substr/toUpperCase/split) | true |
| `array` (push/sort/indexOf/join) | true |
| `text` (TextField+TextFormat, textWidth>0) | true |
| `button` (SimpleButton up/over/down states + hitTestPoint) | true |
| `graphics` (moveTo/lineTo/curveTo) | true |
| `bitmapData` (fillRect+getPixel) | true |
| `loaderInfo` (width==640) | true |
| `timer` ×2 (Timer 40ms ×2 repeatCount=2) | true |
| `M14DONE` | emitted |

Ruffle ERROR lines during the run: **0** (after fixing `hitArea`→removed
and `fillRect` signature `(Rectangle, color)`).

### Regression suite (all at auto-raised SWF v13)

| Demo | Markers | Errors |
|---|---|---|
| m12-scene-transitions | M12DONE, loads/unloads/trans ok | 0 |
| m12-event-bus | M12DONE, enter/exit/custom/removed/recurse | 0 |
| m12-collision-demo | M12DONE, rect/circle/poly/ghost/cam | 0 |
| m12-particle-runtime | M12DONE, ok30/ok | 0 |
| m13-runtime-features | M13DONE, math/kin/scene/tile/state ok | 0 |

## Rust tests

- `nova-build/tests/m14_runtime.rs` — 2 tests ✅
  - `script_projects_raise_swf_version_to_13` (v10 fixture → warning)
  - `m14_api_surface_demo_builds` (1 DoABC2)
- `nova-build/tests/m13_runtime.rs` — 3 tests ✅ (regression)
- `nova-build/tests/m12_runtime.rs` — 4 tests ✅ (regression)
- Full `cargo test --workspace`: see release section (all suites ok).

## Negative findings (verified, documented)

- `JSON` at forced SWF v10 → `ReferenceError #1065` (version gate
  confirmed; auto-raise fixes it).
- `SimpleButton.hitArea` → `ReferenceError #1056` (Ruffle: setter
  unimplemented).
- `BitmapData.fillRect(x,y,w,h,color)` → `ArgumentError #1063`
  (correct signature is `(Rectangle, color)`).
- `import flash.utils.getQualifiedClassName` → compile error
  (package-level functions unsupported).
- `for-in` → compile error (subset limit).
- `Vector.<int>` syntax → not parseable (no generic type args).

## Manual verification notes

- TextField/SimpleButton/Graphics render visually in Ruffle desktop —
  traces prove object construction and state, pixels were spot-checked in
  the Ruffle window during development.
