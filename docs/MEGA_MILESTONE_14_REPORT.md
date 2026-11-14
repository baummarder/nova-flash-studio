# Mega Milestone 14 — AVM2 API Surface Activation

Baseline: `0.9.0-rc1` / `edf8aa3` → Target: `0.10.0-rc1`

M14 untersuchte den tatsächlichen Abdeckungsgrad der AS3-Laufzeitumgebung
und stiess auf eine zentrale Erkenntnis: **Der nova-as-Compiler emittiert
bereits korrekte Multinamen für importierte Flash-Klassen — der Flaschen-
Hals war die SWF-Version.**_playerglobal_-APIs (JSON, ByteArray,
flash.utils.*, flash.text.*, BitmapData, SimpleButton, Graphics, …) sind in
Ruffles playerglobals versions-gated; bei SWF v10 fehlten sie, bei SWF v13
funktionieren sie nativ.

## Delivered

### SWF v13 auto-raise for script projects

- `nova-build` raises `swf_version` to ≥13 whenever DoABC2 tags are
  emitted (warning issued). `ExportConfig` default is now 13 for new
  projects. AVM1 path unchanged.
- Verified: `samples/m14-api-surface.nova` (SWF v13) runs **20/20 API
  probes with zero Ruffle errors**.

### Newly verified working playerglobal APIs (Ruffle 0.7.1)

| API | Status | Evidence |
|---|---|---|
| `JSON.parse` / `JSON.stringify` | ✅ native | `m14-api-surface` |
| `flash.utils.ByteArray` + `Endian` | ✅ native | readInt/writeInt/readUTFBytes/writeUTFBytes/position/length |
| `flash.utils.Dictionary` | ✅ native | dynamic keys |
| `flash.utils.Timer` + `TimerEvent` | ✅ native | 2 timed ticks observed |
| `flash.geom.Point/Rectangle/Matrix/ColorTransform` | ✅ native | transformPoint, contains, multipliers |
| `flash.text.TextField` + `TextFormat` | ✅ native | textWidth>0, styled text renders |
| `flash.display.SimpleButton` (up/over/down) | ✅ partial | states work; `hitArea` setter unsupported in Ruffle |
| `flash.display.Graphics` (moveTo/lineTo/curveTo/beginFill) | ✅ native | runtime vector drawing |
| `flash.display.BitmapData` + `Bitmap` | ✅ native | fillRect/getPixel verified |
| `flash.display.LoaderInfo` | ✅ native | width/height |
| `String` methods (charAt/indexOf/substr/toUpperCase/split) | ✅ native | AVM2 String |
| `Array` methods (push/sort/indexOf/join) | ✅ native | AVM2 Array |
| `Math.*` | ✅ shim + playerglobal | extended shim below |

### Extended `Math` shim

Added `imul` (int32 wrap), `sign`, `clz32`, `cbrt`, `hypot`, `expm1`,
`log1p`, `fround` to `scripts/Math.as` (deterministic implementations).

### Compiler

- `playerglobal_class()` in `check.rs` generalises the M13 `Math`
  special-case: top-level `Math` and `JSON` resolve as `ClassRef`
  multinames (JSON resolves to the playerglobal class at SWF v13+).

## Verified NOT working / unsupported (documented)

- `Vector.<T>` generic syntax — parser has no type-argument syntax.
- `flash.utils.getQualifiedClassName` and other package-level *functions*
  — imports of functions are not supported by the compiler ("Klasse kann
  nicht als Funktion aufgerufen werden").
- `try/catch/throw`, `for-in`, closures-as-values, interfaces, `switch` —
  compiler subset limits (unchanged).
- `SimpleButton.hitArea` — Ruffle does not implement the setter
  (states work, custom hit shape does not).
- Character animation (bones/IK/skinning), shape tweens/morph tags,
  DefineFont/DefineEditText tags, FLA binary — unchanged, deferred.

## Regression

- All M12 (4) + M13 demos re-verified in Ruffle at auto-raised SWF v13:
  zero errors, all markers pass.
- `m14_runtime.rs`: 2 tests (v13 auto-raise warning, demo build).

## Version decision

Released as **`0.10.0-rc1`**, not `1.0.0-rc1`: the requested full feature
expansion (vector editor, skeletal animation, shape tweens, button/font
tags, FLA) is only partially realised — the AS3 *runtime* surface is now
broad, but authoring-side systems remain. 1.0 is reserved for a release
where the documented scope is actually met.
