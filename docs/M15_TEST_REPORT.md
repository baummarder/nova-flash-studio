# M15 Test Report — Skeletal Animation

## Rust unit/integration tests

| Test | Ergebnis |
|---|---|
| `bones::fk_chain_reaches_expected_positions` | ✅ Oberarm 90° → Unterarm-Gelenk (0,40), Rot 90° |
| `bones::rest_pose_uses_bone_rotation` | ✅ |
| `bones::two_bone_ik_reaches_target` | ✅ Target (0,-60) mit 40+40-Gliedern erreicht |
| `bones::ik_unreachable_returns_none` | ✅ |
| `bones::clip_loops_and_clamps` | ✅ Loop frame 8 == frame 4; Clamp bei 99 |
| `m15_runtime::bone_model_serializes_and_resolves` | ✅ 3 Bones, 3 gebundene Objekte, aktiver Clip |
| `m15_runtime::skeletal_pose_changes_bound_object_transforms` | ✅ OberarmSegment-Transform unterscheidet sich Frame 0 vs 12 |
| `m15_runtime::m15_demo_bakes_skeletal_frames_into_sprite` | ✅ ≥20 Sprite-Frames, Matrizen Frame 0 vs 12 verschieden |
| `nova-ai::bone_operations_build_a_working_rig` | ✅ 6 Ops, Clip-Länge → Timeline ≥12, Zyklus + unbekannter Clip abgelehnt |

## Ruffle runtime (0.7.1, `tools/ruffle-runtime-test.py`)

`$TEMP\m15demo.swf` (exportiert aus `samples/m15-skeletal-animation.nova`):

- SWF: v13, 640×360, 24fps, 73 Platzierungen (24 Frames × 3 Teile +
  1 Szenen-Instanz), 1 DefineSprite, 3 DefineShapes.
- Runtime: **0 Errors**, `M15DONE` emittiert.
- Nachweis-Limitation: `hero.numChildren` = 3, aber
  `getChildAt`/`getChildByName` liefern `null` für die Kinder des
  AVM1-artigen Sprites (Ruffle-Display-List-Grenze, verifiziert).
  Die Animations-Korrektheit wird stattdessen durch den
  Rust-Test `m15_demo_bakes_skeletal_frames_into_sprite` belegt
  (Frame-Matrizen im SWF unterscheiden sich nachweislich).

## Regression

| Demo | Ergebnis |
|---|---|
| m13-runtime-features | ✅ M13DONE, 0 Errors |
| m14-api-surface | ✅ M14DONE, 0 Errors |
| m12 (4 Demos) | ✅ (M14-Lauf); M15-Änderungen betreffen nur Symbol-interne Namen + resolve_symbol_frame — Scene-Pfad unverändert |

## Negative/known findings

- `getChildAt`/`getChildByName` auf Sprite-Kindern: `null` (Ruffle,
  AVM1-artige Sprite-Kinder ohne eigenes ABC).
- `Vector.<T>`, `try/catch`, `for-in`, Package-Functions: weiterhin
  nicht Teil der AS3-Teilmenge.
- Weight Painting / Mesh-Skinning: nicht implementiert (starr).
- IK: Two-Bone only; keine Pole-Constraints.
