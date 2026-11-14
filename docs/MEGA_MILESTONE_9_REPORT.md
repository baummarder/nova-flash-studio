# Mega Milestone 9 Report — `0.5.0-rc1`

M9 turns NØVA Flash Studio into a symbol- and asset-driven authoring tool:
real symbol timelines, nested MovieClips, embedded bitmap/audio assets, mask,
guide and sound layers, shape tweens, drawing and transform tools, a larger
AI operation set, and six demo projects. This report states what was actually
implemented and verified. It does not claim completeness, Adobe compatibility
or runtime proof that was not obtained.

Result categories: **PASS**, **FAIL**, **PARTIAL**, **NOT VERIFIED**,
**NOT AVAILABLE**.

## Baseline and release decision

- Baseline: `0.4.0-rc1`, preserved in `dist/archive/0.4.0-rc1/` together with its
  portable ZIP, Setup.exe and `SHA256SUMS.txt`. `dist/archive/0.3.0-rc2/` is
  untouched.
- M9 version: `0.5.0-rc1` (`[workspace.package] version`).
- `.nova` format version: `2`. Version 1 documents migrate additively; legacy
  shape symbols are rebuilt as real one-layer timelines
  (`version_1_symbol_migrates_to_a_real_timeline` test).
- Binaries are unsigned. No signing certificate was available or used.

## Implemented

### Document model and architecture

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Symbol kinds Graphic / MovieClip / Button | **PASS (model+export)** | `SymbolKind`, validation; Button exports as MovieClip-like sprite with warning — button states/hit tests **NOT AVAILABLE** |
| Stable UUID symbol identity | **PASS** | `Symbol.id`; names are labels only |
| Independent symbol timelines | **PASS** | `Symbol.timeline: Scene`; `Project::edit_scene(Option<Uuid>)` selects main scene or symbol timeline |
| Nested symbol instances | **PASS** | `ObjectKind::Symbol` resolution is recursive in `resolve_scene`; cycles rejected by validation |
| Guarded symbol deletion | **PASS** | `symbol_users` / `symbol_delete_blockers`; app blocks delete while instances reference the symbol |
| Symbol duplicate / rename | **PASS** | `Project::duplicate_symbol`; library UI + AI ops |
| `AssetKind::Bitmap/Audio/Font`, embedded bytes | **PASS** | Self-contained `.nova`; provenance path retained |
| `ObjectKind::Bitmap` with `BitmapCrop` | **PASS** | Normalised crop fractions; SWF export converts to pixels |
| `LayerKind::Guide/Mask/Sound` + `Frame.sound` | **PASS** | Model + validation; guides/folders/sound layers skipped by `resolve_scene` |
| `TweenKind::Shape` | **PASS** | Export bakes interpolated shapes; incompatible pairs rejected by validation |
| Format v1 → v2 migration | **PASS** | Additive serde defaults + `from_legacy_shape` rebuild; test coverage |

### SWF export (`nova-build`)

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Recursive `DefineSprite` for symbol timelines | **PASS** | Nested sprites verified structurally; 2 sprites in `m9-nested-symbols.swf` |
| Per-frame `PlaceObject2`/`RemoveObject2`/`ShowFrame` | **PASS** | Verified in export stats |
| Bitmap tags JPEG2/Lossless2 + crop | **PASS** | `m9-bitmap-assets.swf` exports DefineBits |
| `DefineSound` + `StartSound` | **PASS (event-like)** | True `SoundStream` **NOT AVAILABLE**; exporter warns |
| Mask `clipDepth` | **PASS** | Mask clips layer at `index-1`; warning when no visible target |
| Shape tween baking | **PASS** | `m9-masks-tweens.swf`: 51 shapes baked across 48 frames |
| Button states/hit tests | **NOT AVAILABLE** | Exported as clip with warning |

### XFL (`nova-formats`, `formats/fla.rs`)

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Import: symbols, nested instances, layers, keyframes, tweens, assets, scenes, sounds | **PASS** | Existing importer extended for M9 model |
| Export: experimental `.xfl` ZIP | **PASS** | README marks package experimental; **NOT tested in Adobe Animate** |
| Structural roundtrip XFL → NØVA → XFL → NØVA | **PASS** | `m9_structural_roundtrip_preserves_symbols_assets_and_layers` covers symbols/assets/layer kinds/sound cues |
| Native `.fla` compatibility | **NOT AVAILABLE** | Explicitly not claimed |

### Application (`nova-app`)

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Symbol library panel (kind, create MC, duplicate, rename, guarded delete, enter) | **PASS** | `tree.rs` |
| Symbol editing mode + breadcrumb back to scene | **PASS** | `edit_symbol` state, stage header shows `Symbol: <name>` |
| Shared edits affect all instances | **PASS** | Instances resolve through the shared symbol timeline |
| Asset browser + bitmap preview textures | **PASS** | `asset_texture` decode cache, `Selection::Asset` inspector |
| Bitmap import/placement | **PASS** | `ImportBitmap`/`AddBitmapAsset` actions; centre-based transform |
| Audio import + sound cue placement | **PASS** | WAV/MP3 import, `PlaceSoundCue`-equivalent UI action |
| Drawing tools: Line, Pencil, Brush | **PASS** | Draft stroke → filled `PathShape`/rotated rect; SWF has no stroke width so pencil/brush bake to fills |
| Eraser / Fill tools | **PASS** | Hit-test delete / flood fill colour set |
| Path node editing | **PASS** | `PathEdit` tool drags MoveTo/LineTo/CurveTo nodes |
| Transform handles (scale + rotate) | **PASS** | Corner/edge handles, centre-pivot uniform scale, rotation ring; free aspect ratio stays in properties |
| Timeline sound markers, tween-kind colours, mask/guide badges | **PASS** | `timeline_view.rs` |
| Layer kind selector incl. Sound | **PASS** | Properties panel |
| Custom pivot/origin editing | **NOT AVAILABLE** | Pivot stays object centre; documented |

### AI operations (`nova-ai`)

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Typed ops: symbols (kind), symbol timelines, bitmap/audio import, place bitmap, sound cue, layer kind | **PASS** | `preview_operations_with`/`apply_operations_with` with workspace-confined `read_asset`; transactional + rollback preserved |
| Symbol timeline targeting (`symbol` selector on layer/object/keyframe/tween ops) | **PASS** | `timeline_mut` resolves symbol timeline by name |
| Path confinement for asset reads | **PASS** | `Workspace::read_bytes` reuses canonicalised root confinement; unsafe paths rejected before read |
| Provider prompt documents new ops | **PASS** | `agent.rs` op list extended |
| Cloud sharing / credentials | **PASS (unchanged)** | No keys in logs/project; DPAPI model untouched |

### Tests and demos

| Area | Result | Evidence / limit |
| --- | --- | --- |
| `cargo fmt --check` / `cargo check --workspace` | **PASS** | Clean (4 dead-code warnings in nova-app state, non-blocking) |
| `cargo test --workspace` | **PASS** | ~290 tests passed, 4 ignored (live-network tests) |
| Six demo projects | **PASS** | `samples/m9-*.nova`, all `validate()` clean; generator `nova-core/examples/gen_demos.rs` |
| SWF export of all six demos | **PASS** | `samples/swf/m9-*.swf`; mask warning resolved by layer order |

## Runtime verification

| Runtime | Result | Evidence |
| --- | --- | --- |
| Ruffle | **NOT AVAILABLE** | No Ruffle binary on this machine; prior M7/M8 Ruffle evidence does not cover M9 features |
| Adobe Flash Player debug projector (`flashplayer_32_sa_debug.exe`, already configured locally) | **PARTIAL** | `m9-nested-symbols.swf` ran for 6 s without process exit/crash; visual playback and frame-accurate behaviour **NOT VERIFIED** (no screen capture / trace log taken) |
| SWF structural inspection (`nova probe`) | **PASS** | Tag/frame/sprite counts reported per demo SWF |

## Packaging

| Artifact | Result |
| --- | --- |
| `NOVA-Flash-Studio-0.5.0-rc1-windows-x64.zip` | **PASS** — built by `package-windows.ps1`, payload smoke test OK |
| `NOVA-Flash-Studio-0.5.0-rc1-windows-x64-Setup.exe` (78.5 MB) | **PASS** — `package-installer.ps1 -UseExistingPayload`, payload SHA verified |
| Headless install + uninstall | **PASS (PARTIAL scope)** | `--install <dir> --quiet` wrote exe + uninstaller; `--uninstall --quiet` completed; no GUI interaction tested |
| Portable ZIP verification | **PASS** | EXE launches `--version`/smoke path inside packaged folder |
| `dist/windows/SHA256SUMS.txt` | **PASS** | EXE, ZIP and Setup hashes recorded |

SHA-256 (see `dist/windows/SHA256SUMS.txt` for authoritative values):
ZIP `E37D1C9F…D53D40`, Setup `E1888EC0…3593` — rebuilt after the M9.1
AVM1/AVM2 bitmap fix described in the addendum below; earlier hashes remain
in `SHA256SUMS.txt` history.

## Known limitations and honest gaps

- **Ruffle M9 playback: NOT VERIFIED** — requires a local Ruffle binary; none present.
- Button symbols export as MovieClips; no button states or hit areas.
- Timeline sounds export as `StartSound` events; no streaming `SoundStream`.
- XFL export is experimental; not validated in Adobe Animate; native `.fla` unsupported.
- Shape tweens bake at export; incompatible endpoint pairs are rejected by
  validation rather than morphing silently.
- Drawing tools produce filled shapes only (SWF shapes have no stroke width);
  pencil/brush width is baked into geometry.
- Transform gizmo uses centre pivot; arbitrary pivot editing is not implemented.
- AI asset ops require the AI workspace (project folder) to resolve paths;
  without it they fail validation, never silently skip.
- 4 compiler warnings remain (unused state fields/methods kept for upcoming UI
  polish); no functional impact.

## Remaining manual tests

- Interactive GUI walkthrough of symbol editing, asset import and drawing tools.
- Ruffle playback matrix once a Ruffle build is available.
- Adobe Animate open of exported `.xfl` packages.
- UAC-denied all-users install path (unchanged from M6; headless per-user path
  verified here).

## Post-report addendum (M9.1 runtime pass)

After the report above was written, Ruffle Desktop 0.7.1 was located at
`C:\Program Files\Ruffle\bin\ruffle.exe` and all six M9 demo SWFs were
re-captured (`samples/ruffle-captures/`). This pass found and fixed one real
runtime defect.

### Defect: bitmaps invisible in AVM2-flagged SWFs under Ruffle

Every `.nova` project carries the default `scripts/Main.as` stub, so every
export was AVM2-flagged (`FileAttributes.action_script_3`) even though the ABC
was never linked through `SymbolClass` and could not execute. Under Ruffle
0.7.1, `DefineBitsLossless2`/`DefineBitsJPEG2` characters placed on the
timeline rendered nothing in that state, while shapes and sprites rendered
normally. The same file with the AS3 flag cleared rendered the bitmap
correctly, proving the tag encoding, pixel data and placement were valid.

Reproducer evidence (local captures, not committed):

- `m9-bitmap-assets.swf` (AVM2): zero saturated pixels on stage.
- Same file minus DoABC2/AS3 flag (AVM1): checkerboard visible.
- `m9-bitmap-rect.swf`: red shape rendered, bitmap did not — isolating the
  failure to bitmap characters only.

### Fix

`nova-build` now only embeds `DoABC2`/`SymbolClass` and keeps the AS3 flag
when at least one class link exists (`symbol_links` non-empty). Scriptless or
unlinked projects are emitted with `action_script_3 = false` and a warning
("Keine Skript-Verknüpfung gefunden; ..."). Timeline content is identical;
the file simply stops claiming an AVM2 runtime it cannot use. Linked AS3
projects (e.g. `samples/as3-example.nova`) still export DoABC2 + SymbolClass
and remain AVM2 — verified by `nova probe`.

### Verification after fix

- `cargo test --workspace`: all tests pass, 0 failures.
- All six M9 SWFs re-exported and re-captured in Ruffle 0.7.1; each shows its
  expected stage content, including the previously invisible bitmap checker
  (`72,120,192` / `240,240,240` pixels present).
- `samples/swf/m9-bitmap-assets.swf` probe: `DefineBitsLossless2` 64x64,
  `w*h*4` zlib payload verified byte-exact.

### Remaining honest gaps

- AVM2 movies **with** linked scripts that also place bitmaps on the timeline
  may still hit the same Ruffle 0.7.1 limitation; not separately verified.
- Whether real Adobe Flash Player renders bitmaps in unlinked-AVM2 files is
  unknown; the AVM1 downgrade is safe either way because the ABC was inert.
- Audio remains structural evidence only (`DefineSound`/`StartSound` tags);
  no audible playback check was performed.
