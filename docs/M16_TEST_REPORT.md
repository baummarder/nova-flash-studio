# M16 Test Report — Visual Authoring & Chain IK

## Unit tests (`nova-core::model::bones`)

| Test | Ergebnis |
|---|---|
| `fk_chain_reaches_expected_positions` | ✅ |
| `rest_pose_uses_bone_rotation` | ✅ |
| `two_bone_ik_reaches_target` | ✅ |
| `ik_unreachable_returns_none` | ✅ |
| `clip_loops_and_clamps` | ✅ |
| `fabrik_reaches_target` | ✅ Tip (60,-50) erreicht, Root gepinnt |
| `fabrik_unreachable_stretches` | ✅ Kette faltet Richtung Target |
| `pole_mirrors_chain_side` | ✅ Pole vs. Gegenpole → unterschiedliche Biegung |
| `chain_ik_three_bones_reaches_target` | ✅ 3-Bone-Kette + Pole, Tip ≈ (60,-60) |
| `chain_ik_stops_at_animated_ancestor` | ✅ animierter Vorfahre bleibt Kettenwurzel |

## Integration tests (`nova-ai/tests/m16_runtime.rs`)

| Test | Ergebnis |
|---|---|
| `chain_ik_moves_bound_parts_deterministically` | ✅ 14 Ops bauen Rig; EndPart-Transform Delta >5px Frame 0→6; identisch bei Wiederholung |
| `chain_ik_bakes_into_sprite_frames` | ✅ ≥10 Sprite-Frames; PlaceObject-Matrizen Frame 0 vs 6 nachweislich verschieden |

## Workspace

`cargo test --workspace`: alle Suites grün (0 failed), inkl. M12–M15
Regressionen (`m15_runtime`, `m14_runtime`, `swf_scripts`, Bones).

## Manuell / UI

- Stift-Werkzeug: interaktive Anker-Erstellung (Klick/Ecke,
  Ziehen/Kurve, Klick-auf-Start=schließen, Enter=offen, Esc=abbruch)
  — Code geprüft, egui-Interaktion nicht automatisch testbar.
- PathEdit: Knoten einfügen (Doppelklick auf Segment) und löschen
  (Rechtsklick, MoveTo = ganzer Teilpfad) implementiert.
- Bone-Overlay: Skelett-Linien + Gelenke + Namen im Stage; Ziehen
  eines Gelenks aktualisiert `bone.x`/`bone.y` (Eltern-Raum-korrekt).
- Pencil/Brush-Doppeloffset-Bug behoben (lokale Pfadkommandos).

## Ruffle runtime

- `samples/m15-skeletal-animation.nova` → SWF v13, 0 Errors,
  `M15DONE` (Regression nach M16-Änderungen).
- FABRIK-Kette: via `m16_runtime` strukturell im SWF belegt
  (Matrizen-Delta); kein dedizierter Ruffle-Lauf mit IK-Clip als
  aktivem Clip — `Greifen`-Clip ist im Demo vorhanden, aber
  `Schwung` bleibt aktiv (FK). Ehrlich dokumentiert.

## Known limitations

- Ruffle AVM2 `getChildAt`/`getChildByName` → `null` für Kinder
  AVM1-artiger Sprites (M15-Doku).
- Kein Mesh-Skinning, keine Gradients, keine Font/Button/MorphShape-
  Tags, AS3-Teilmenge unverändert.
