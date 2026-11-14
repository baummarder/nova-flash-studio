# Mega Milestone 16 — Visual Authoring: Pen Tool, Bone Overlay, Chain IK

Baseline: `0.11.0-rc1` / `68dca39` → Target: `0.12.0-rc1`

M16 wurde wieder auf **echte, im Editor sichtbare Workflows**
zugeschnitten: der Stift erzeugt interaktiv Pfade, das Skelett ist im
Stage-View sichtbar und editierbar, und IK unterstützt jetzt Ketten
von 3+ Knochen per FABRIK mit Pole-Targets. Die übrigen Phasen
(Gradients, Boolean-Operationen, Font-/Button-/MorphShape-Tags,
`try/catch`, `Vector.<T>`) bleiben ehrlich dokumentierte Lücken.

## Delivered

### Pen tool (visual vector authoring)

- Neues Werkzeug **„Stift"** in der Toolbar: Klick setzt Eck-Anker,
  Druck-Ziehen setzt Kurven-Anker (Ziehenposition = quadratischer
  Kontrollpunkt), Klick auf den Startanker (≥3 Anker) schließt den
  Pfad, Enter/Doppelklick beendet, Esc verwirft.
- Live-Vorschau: Anker-Polyline, Handle-Linien, Startanker-Markierung.
- Erzeugt echte `Path`-Objekte mit `PathShape` (Fill = aktuelle
  Objektfarbe), serialisierbar in `.nova`, exportierbar als SWF-Shape.
- Pfadkommandos werden korrekt in **objekt-lokalen, zentrierten
  Koordinaten** gespeichert (Konvention des Node-Editors + Exporter).

### Path-Edit erweitert

- **Knoten einfügen**: Doppelklick auf ein gerades Segment teilt es
  (LineTo wird vor dem Segment-Endkommando eingefügt).
- **Knoten löschen**: Rechtsklick auf einen Knoten; das Löschen eines
  MoveTo entfernt den ganzen Teilpfad; leere Pfade/Objekte werden
  aufgeräumt (Validierung bleibt grün).
- Vorher vorhanden und weiter nutzbar: Knoten/Griffe ziehen,
  CurveTo-Kontrollpunkte bearbeiten.

### Bugfix: Pencil/Brush-Doppeloffset

Pencil- und Brush-Striche schrieben Stage-Koordinaten in
objekt-lokale Pfadkommandos UND setzten den Transform auf die Mitte —
Doppel-Transformation beim Rendern und im SWF-Export. Korrigiert:
Kommandos werden jetzt relativ zur Objektmitte gespeichert.
(Bereits gespeicherte alt erstellte Striche behalten ihre alte
Geometrie — Datenmodell unverändert.)

### Bone-Overlay & Gelenk-Bearbeitung

- Beim Editieren eines rigged Symbols werden alle Knochen als
  Gelenk→Spitze-Linie plus Eltern-Verbindung und Namen im Stage
  gezeichnet (Pose = aktiver Clip, scrubbbar).
- Ziehen an einem Gelenk (Select-Tool, <9px Treffer) verschiebt
  `bone.x`/`bone.y` — inklusive korrekter Rückrechnung in den
  Eltern-Raum (Offset + Parent-Rotation).

### Multi-Bone-IK (FABRIK) + Pole Targets

- `IkKey` erhält optionale `pole_x`/`pole_y` (serde `poleX`/`poleY`,
  rückwärtskompatibel).
- `ik_chain`: IK-Kette = Effektor + alle Vorfahren ohne eigenen
  Animations-Track im Clip (animierte Vorfahren bleiben Kettenwurzel).
- Ketten ≥3 Knochen → **FABRIK**-Solver (`fabrik_solve`, 12
  Iterationen, deterministisch): gesperrtes Wurzelgelenk, alle
  Längen aus der FK-Pose, seg-weise Rotation-Neuberechnung.
- **Pole**: mittlere Kette wird nach dem FABRIK-Lauf auf die
  Pole-Seite der Wurzel→Ziel-Linie gespiegelt (2D-Pole-Constraint).
- Ketten ==2 behalten den analytischen `solve_two_bone`.
- Neue AI-Operation **`add_ik_key`** (x, y, optional pole_x/pole_y).

## Verified

- `bones.rs`: 10 Tests — inkl. `fabrik_reaches_target`,
  `fabrik_unreachable_stretches`, `pole_mirrors_chain_side`,
  `chain_ik_three_bones_reaches_target`,
  `chain_ik_stops_at_animated_ancestor`.
- `nova-ai/tests/m16_runtime.rs`: 2 Tests — komplette Rig nur über
  AI-Ops gebaut (3 Bones, 3 gebundene Teile, IK-Clip mit Pole),
  `resolve_symbol_frame` deterministisch und Frame-Deltas >5px;
  gebackene Sprite-Frame-Matrizen unterscheiden sich nachweislich.
- Ruffle: `m15-skeletal-animation` exportiert unverändert sauber
  (`M15DONE`, 0 Errors) — FABRIK-Pfad ist aktiv, sobald ein Clip
  ≥3-Knochen-Kette adressiert (Demo nutzt weiterhin 2er-Kette →
  Two-Bone-Pfad; `Greifen`-Clip hat jetzt Pole-Keys).

## Honest limitations

- **Kein Mesh-Skinning / Weight Painting** — Bindung bleibt starr
  pro Objekt; Verformung nur über Knochen-Anzahl + mehr Teile.
- **Kein Kanteneffekt für Poles bei 2-Bone** — Pole wirkt nur auf
  FABRIK-Ketten (≥3).
- **Stift**: quadratische Kurven (CurveTo hat einen Kontrollpunkt);
  Cubic-Handles werden als einzelner ControlPoint approximiert —
  kein korrekter Ein/Aus-Handle-Workflow wie in Illustrator.
- **PathEdit**: Knoten einfügen nur auf geraden Segmenten (kein
  Kurven-Split); kein Kantenzug-/Boolean/Pathfinder.
- **Gradients**: `PathShape` hat nur Vollton-Füllung; kein
  Linear/Radial-Gradient im Modell oder SWF-Export.
- **Font-/Button-/MorphShape-Tags**: unverändert nicht implementiert
  (Text wird als Shapes gebacken; Buttons als MovieClip-Sprites;
  Shape-Tweens werden deterministisch gebacken statt
  `DefineMorphShape`).
- **AS3**: `Vector.<T>`, `try/catch`, `for-in`, Package-Functions
  unverändert außerhalb der Teilmenge.
- **Ruffle-Grenze** (M15-Dokumentation gilt weiter): AVM2
  `getChildAt`/`getChildByName` erreichen Kinder AVM1-artiger Sprites
  nicht; Skelett-Teile sind visuell animiert, aber nicht per Skript
  erreichbar.

## Scope decision

`0.12.0-rc1`: Stift + PathEdit + Bone-Overlay + FABRIK-Ketten-IK sind
echte, bedienbare Workflows — das Kriterium „vollständig funktionierend
statt vier halbfertig" ist erfüllt. 1.0.0 bleibt an der vollständigen
Vision (Gradients, Skinning, Font/Button-Tags, voller AS3-Umfang) gekoppelt.
