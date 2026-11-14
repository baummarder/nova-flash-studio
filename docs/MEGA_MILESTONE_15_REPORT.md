# Mega Milestone 15 — Skeletal Animation & Authoring Foundations

Baseline: `0.10.0-rc1` / `9d0a6ac` → Target: `0.11.0-rc1`

M15 wurde bewusst auf **ein vollständiges, verifiziertes System**
zugeschnitten statt vier halbfertige Gruppen: das 2D-Skelettsystem
(FK + Two-Bone-IK, starre Teile-Bindung, deterministisches Baking in
die SWF-Timeline). Vector-Editor-Feinschliff, Weight Painting,
Morph-Tags und Font-/Button-Tag-Export bleiben ehrlich dokumentierte
Lücken.

## Delivered

### Bone/Skeleton model (format v4, `.nova`-serialisierbar)

- `Skeleton`/`Bone` (id, name, parent, joint offset x/y, length,
  rest rotation) an `Symbol`.
- `BoneClip` (name, frames, looping, tracks) mit `BoneTrack`
  (FK `BoneKey` rotation keys + `IkKey` two-bone target keys).
- `ObjectInstance.bone: Option<Uuid>` — starre Bindung: der lokale
  Transform eines Objekts wird mit der Welt-Pose seines Knochens
  komponiert (`bind_transform`).
- `Symbol.active_clip` wählt den Clip für Preview/Export; `None` =
  Ruhepose (gebundene Objekte werden trotzdem unter ihre Knochen
  gehängt).

### FK + IK evaluation (`nova-core::model::bones`)

- `Skeleton::pose_at(clip, frame)` — deterministische Hierarchieberechnung,
  Clip-Frame-Looping/Klemmen, Key-Interpolation (linear).
- `solve_two_bone` — analytischer Two-Bone-IK-Solver (Cosinus-Satz),
  Deric-Targets in Skeleton-Space;_fk_-Fallback bei unerreichbaren
  Targets (Solver gibt `None` zurück → FK-Keys gelten).
- 5 Unit-Tests (FK-Kette, Ruhepose, IK-Reichweite, Loop/Clamp).

### Integration

- **Export**: `nova-build` nutzt `resolve_symbol_frame` — gebackene
  Knochen-Posen werden pro Frame in die `PlaceObject2`-Matrizen der
  Symbol-Sprite-Frames geschrieben. Keine neuen SWF-Tags nötig;
  Ruffle spielt die Animation nativ ab. Sprite-Länge deckt die
  aktive Clip-Länge ab (`frame_count = max(timeline, clip.frames)`).
- **Preview**: `stage_view` löst rigged Symbol-Timelines über
  `resolve_symbol_frame` auf — Skelette sind beim Scrubben sichtbar.
- **Editor**: Skelett-Sektion im Symbol-Inspektor (`properties.rs`):
  Knochenliste mit Parent/Koordinaten/Länge/Rotation, hinzufügen,
  löschen (mit Re-Parenting + Track-Cleanup), Clip-Auswahl/-Anlage.
- **AI**: 6 neue Operationen — `add_bone`, `set_bone_parent`
  (mit Zyklusprüfung), `add_bone_clip`, `add_bone_key`,
  `bind_object_to_bone`, `set_symbol_clip`; System-Prompt aktualisiert.
- **SWF-Qualität**: Symbol-interne Platzierungen erhalten jetzt
  Instanznamen (`.named`) — nötig für `getChildByName`-Zugriffe und
  Flash-Parität (szene-seitige Namen waren bereits vorhanden).

### Demo

`samples/m15-skeletal-animation.nova` (+ `tools/gen-m15-demo.py`):
3-Knochen-Arm-Rig (Torso→Oberarm→Unterarm), 3 starre Teile, zwei
Clips („Schwung“ FK, „Greifen“ IK), auf der Bühne platziert, mit
ActionScript-Probe.

## Verified

- Rust: 3 M15-Integrationstests (Model, Pose-Delta, gebackene
  Frame-Matrizen unterscheiden sich nachweislich zwischen Frame 0
  und 12), 5 Bone-Unit-Tests, 1 AI-Ops-Test (inkl. Zyklus-Ablehnung).
- Ruffle 0.7.1: SWF exportiert fehlerfrei (73 Platzierungen =
  24 Frames × 3 Teile + Szene), Runtime läuft ohne Fehler,
  `M15DONE` emittiert.

## Honest limitations (documented)

- **Kein Mesh-Skinning / Weight Painting** — Bindung ist starr pro
  Objekt. Verformbare Characters benötigen weiterhin einzelne
  MovieClip-Teile oder zukünftige Mesh-Unterstützung.
- **Ruffle zeigt AVM2 `getChildAt`/`getChildByName` für Kinder
  AVM1-artiger Sprites nicht** — die gebackenen Kind-Objekte sind im
  Display-List nicht per Skript erreichbar (verifiziert: `numChildren`
  zählt sie, `getChildAt` liefert `null`). Animation ist visuell
  korrekt; Skript-Zugriff auf Kind-Teile eines rigged Symbols ist
  eine bekannte Ruffle-Grenze. Szene-seitige Instanzen bleiben
  skript-bar.
- **IK**: nur Two-Bone-Ketten (Bone + Elternteil); längere Ketten
  nutzen FK. Pole-Vector/Constraint-Typen fehlen.
- **Editor**: Bone-Editor ist funktional kompakt (Liste + Clips);
  kein visueller Bone-Overlay im Stage-View, kein Keyframe-Editing
  pro Frame im Editor (Keys via AI/.nova).
- **Shape Tweens** existierten bereits (M9): Rect/Ellipse/Path-
  Vertex-Interpolation mit Easing, gebacken — keine neuen
  MorphShape-Tags (SWF `DefineMorphShape` bleibt ungenutzt; baking
  ist deterministischer und portabel).
- **Vector-Editor-Feinschliff** (Pen-Werkzeug mit Bézier-Handles,
  Gradients, Boolean-Ops) und **Font-/Button-Tag-Export** bleiben
  offen — Datenmodell (`PathShape` mit MoveTo/LineTo/CurveTo) und
  Editor-Rendering sind vorhanden, die Werkzeug-UI nicht.

## Scope decision

`0.11.0-rc1`: das Skelettsystem ist vollständig implementiert,
testbar und exportiert — ein reales Alleinstellungsmerkmal.
1.0.0 bleibt an der vollständigen dokumentierten Vision gekoppelt.
