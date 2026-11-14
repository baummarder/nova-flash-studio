# M17 Test Report — Vector & Animation Pro

## Unit tests (`nova-core`)

| Bereich | Test | Ergebnis |
|---|---|---|
| Boolean | `union_of_overlapping_rects_is_single_shape` | ✅ 1 Shape, ≥8 Punkte |
| Boolean | `subtract_creates_smaller_shape` | ✅ |
| Boolean | `intersect_of_disjoint_rects_is_empty` | ✅ |
| Boolean | `xor_of_identical_rects_is_empty` | ✅ |
| Boolean | `subtract_preserves_hole_contours` | ✅ ≥2 Subpfade |
| Boolean | `curves_flatten_for_boolean` | ✅ kein Panic |
| Skinning | `skin_vertex_single_bone_follows_pose` | ✅ |
| Skinning | `skin_vertex_blends_two_bones` | ✅ Mittelpunkt-Mix |
| Skinning | `skin_vertex_missing_bone_is_identity` | ✅ |
| Skinning | `normalize_weights_scales_to_one` | ✅ |
| Skinning | `auto_skin_assigns_nearest_bone` | ✅ |
| Bones (M15/M16 Regression) | 10 bestehende Tests | ✅ alle grün |

## SWF-Export (`nova-build` lib tests)

| Test | Ergebnis |
|---|---|
| `gradient_path_exports_native_gradient_fill` | ✅ `swf`-Parser findet `FillStyle::LinearGradient` im DefineShape-Tag |

## AI-Integration (`nova-ai/tests/m17_runtime.rs`)

| Test | Ergebnis |
|---|---|
| `boolean_union_via_ai_op` | ✅ Objective-Transforms berücksichtigt, L-Outline |
| `gradient_via_ai_op_roundtrips` | ✅ Modell + serde-Roundtrip |
| `mesh_binding_and_vertex_weight_via_ai_ops` | ✅ Auto-Bind + normalisierte Vertex-Gewichte |
| `clip_management_via_ai_ops` | ✅ Duplikat + Umbenennung |

## Workspace

`cargo test --workspace`: alle Suites grün (0 failed), inkl.
M12–M16-Regressionen (`m15_runtime`, `m16_runtime`, `m14_runtime`,
`swf_scripts`, Bones, Boolean, Skinning).

## Ruffle runtime

- `samples/m17-vector-skinning.nova` → SWF v13, 640×360, 24 fps,
  1488 Bytes, 37 Tags, zlib.
- **28 Shape-Definitionen** (Mesh-Deformation pro Frame als eigene
  Varianten) + Gradient-Card.
- `M17DONE` Trace, **0 Errors**.

## Manuell / UI (Code geprüft)

- Stift, PathEdit (Knoten einfügen/löschen/ziehen), Cubic-Handles mit
  Smooth/Alt-Modus.
- „Vektor"-Sektion: Gradient-Editor (Typ/Winkel/Radial/Stops),
  Boolean-Operand + 4 Ops, Mesh-Bindung, Knochen-/Radiuswahl.
- „Gewichte"-Werkzeug: Weight Painting mit Normalisierung und
  Knochenfarben-Visualisierung.

## Known limitations

- Boolean-Ergebnisse sind Polygone (Kurven vorher geflattet).
- Skinning nur auf Path-Objekten; keine Constraints/Blends.
- Ruffle-Child-Zugriff-Limitation (M15) unverändert.
