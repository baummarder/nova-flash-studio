# Mega Milestone 17 — Professional Vector & Animation

Baseline: `0.12.0-rc1` / `90cbd6a` → Target: `0.13.0-rc1`

M17 liefert die drei Kernbereiche des Milestones als echte,
getestete Funktionen: kubische Bézier-Segmente mit
Smooth/Independent-Handles, native SWF-Gradient-Fills, echte
Polygon-Boolean-Operationen (Union/Intersect/Subtract/XOR inkl.
Löcher) und Mesh-Skinning mit Weight Painting.

## Delivered

### Vector Pro

- **`PathCommand::CubicTo`** — kubische Bézier mit zwei Kontrollpunkten
  (`c1`, `c2`). Serde-additiv, rückwärtskompatibel.
  - Rendering: kubische Flattening in Stage-Vorschau und Knoten-Editor.
  - PathEdit: drei Knoten pro Segment (c1, c2, Ende); **Smooth-Modus**
    spiegelt den gegenüberliegenden Handle automatisch um den Anker,
    **Alt = unabhängige Handles**.
  - SWF-Export: Cubic → adaptive Quadratic-Ketten (de Casteljau,
    Flatness <0.6px, max. Tiefe 5) — SWF-SHAPE-Records können nur
    quadratische Kurven.
  - Shape-Tweens interpolieren CubicTo-Felder.
- **Gradienten** — `Gradient { kind, stops, angle, center, radius }`
  auf `PathShape` (serde `gradient`, optional).
  - Linear (Winkel) und Radial (Mitte/Radius), mehrere Stops mit
    Position 0..1 und Farbe.
  - Stage-Vorschau: vertex-colorierter Dreiecks-Fan (gleiche
    Convex-Einschränkung wie Solid-Fill).
  - **SWF-Export: native Gradient-Fills** — nova-swf `FillStyle::
    LinearGradient`/`RadialGradient` (Typen 0x10/0x12), Gradient-Matrix
    aus Winkel/Bounds-Projektion, GradientRecords mit Ratio+RGBA.
    Mit dem Referenz-`swf`-Parser verifiziert (`FillStyle::
    LinearGradient` im DefineShape-Tag).
  - Editor: „Vektor"-Sektion in den Eigenschaften — Typ, Winkel,
    Radial-Parameter, Stop-Liste (Position/Farbe/Hinzufügen/Löschen).
- **Boolean/Pathfinder** — `nova_core::boolean_paths` +
  `boolean_path_objects` auf `i_overlay` 9.1 (deterministisch,
  EvenOdd, unterstützt Löcher und mehrere Konturen).
  - Union, Intersect, Subtract, Exclude/XOR; kurvenbasierte Pfade
    werden vor der Operation geflattet (Ergebnis = Polygone).
  - `boolean_path_objects` bildet beide Operanden über ihre
    Transform-Chain in den gemeinsamen Raum ab und das Ergebnis
    exakt zurück in die lokalen Koordinaten des Ziels
    (skaliert/unrotiert/verschoben berücksichtigigt).
  - Editor: „Vektor"-Sektion mit Operanden-Combo + 4 Op-Buttons.
  - AI: `boolean_path_operation`.

### Character Animation Pro

- **Mesh Skinning** — `PathShape.skin: Option<SkinData>` mit
  `SkinData.weights[i]` = `Vec<SkinWeight{bone, weight}>` parallel zu
  `PathShape::skinning_points()` (Anker + Kurvenkontrollpunkte,
  reihenfolgetreu zur Deformation).
  - `resolve_symbol_frame` deformiert skinned Pfade pro Frame:
    LBS über `skin_vertex` (bind→current Delta pro Bone,
    `p' = joint_cur + R(Δrot)·(p − joint_bind)`), Gewichte werden
    defensiv normalisiert.
  - Objekt-Transform wird in Skeleton-Raum und zurückgerechnet
    (exakte Inverse, auch bei nicht-uniformem Scale + Rotation).
  - Export: deformierte Pfade werden pro Frame als eigene
    DefineShape-Varianten gebacken (kein neuer SWF-Tag nötig).
  - `auto_skin_weights`: nächster Knochen pro Vertex (Auto-Bind).
  - `normalize_weights`: Skalen auf Summe 1.
- **Weight Painting** — neues Werkzeug **„Gewichte"**: Ziehen auf
  dem Pfad malt `state.skin_bone` mit `state.skin_radius` auf alle
  Skinning-Punkte im Radius; Shift entfernt; pro Vertex
  normalisiert; Vertex-Punkte werden als Knochenfarben-Dots
  visualisiert. Knochen-/Radiuswahl in den Eigenschaften.
- **Clip-Verwaltung** — AI-Ops `duplicate_bone_clip`,
  `rename_bone_clip` (active_clip folgt der Umbenennung).
- **AI-Ops** — `set_path_gradient`, `boolean_path_operation`,
  `bind_mesh_to_skeleton`, `set_vertex_weight` (mit Validierung:
  endliche Werte, Stop-Positionen 0..1, ≥2 Stops, Vertex-/Knochen-
  Existenz, Pfadindex).

### Format- & Runtime-Integration

- `.nova`: alle neuen Felder serde-additiv (`#[serde(default)]` +
  `skip_serializing_if`); alte Projekte laden unverändert.
- SWF: Gradient = native FillStyle; Cubic = quadratische Näherung
  (ehrlich dokumentiert); Skinning = gebackene Frame-Varianten.
- XFL: keine neuen Felder (dokumentierte Lücke wie bisher).
- Ruffle: Demo `samples/m17-vector-skinning.nova` → SWF v13,
  1488 Bytes, 37 Tags, 98 Platzierungen, **28 Shapes** (Mesh-
  Deformation erzeugt pro Frame eigene Shape-Definitionen),
  `M17DONE`, 0 Errors.

## Honest limitations

- **Kein MorphShape** — Shape-Tweens werden wie gehabt gebacken.
- **Kein Focal-Gradient** (SWF 0x13) — nur linear/radial.
- **Boolean-Ergebnis = Polygone** — Kurven werden vor der Operation
  geflattet; das Ergebnis enthält nur LineTo-Segmente.
- **Skinning nur auf Path-Objekten** — Rect/Ellipse bleiben starr
  gebunden; keine Textur-/Multi-Influence-Constraints.
- **Weight Painting auf Skinning-Punkten** (inkl. Kontrollpunkte),
  nicht auf gerenderte Kurvenpunkte.
- **Vorschau-Gradient = Fan-Näherung** (convex), SWF ist exakt.
- **Constraints** (Position/Rotation/Parent Limits) und
  Animations-Blend-Bäume bleiben offen.
- **Knoten einfügen** weiterhin nur auf geraden Segmenten.

## Scope decision

`0.13.0-rc1`: Cubic-Handles, native Gradients, echte Booleans und
Mesh-Skinning mit Weight Painting sind vollständig implementiert,
serialisiert, exportiert und getestet — das „vollständig funktionierender
RC"-Kriterium ist erfüllt. 1.0.0 bleibt an Constraints, Cubic im
SWF-Nativformat (unmöglich — Format-Limit), MorphShapes, XFL-Tiefe
und dem vollen AS3-Umfang gekoppelt.
