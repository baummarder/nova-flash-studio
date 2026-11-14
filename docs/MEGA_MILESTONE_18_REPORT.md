# MEGA MILESTONE 18 — Flash Compatibility & Workflow

**Version:** `0.14.0-rc1` · **Base:** `0.13.0-rc1` (M17, commit `0dde918`)

M18 vertieft die Interoperabilität mit der Flash-Welt: ehrliche
FLA-Erkennung, echte XFL-Verbesserungen, SWF-Gradienten-Import,
AS3-`switch`, Asset-Workflows, ein Projekt-Kompatibilitätsbericht und
gehärtete Archivverarbeitung. guiding principle: nichts wird als
unterstützt ausgegeben, was nicht wirklich funktioniert.

## 1. FLA — Format-Realität und ehrliche Erkennung

Untersuchtes Formatwissen (verifiziert gegen Adobe-/Community-
Dokumentation):

| Container | Inhalt | NØVA-Status |
|---|---|---|
| `.fla` CS5+ (ZIP/XFL) | ZIP mit `DOMDocument.xml`, `LIBRARY/`, `bin/`, `mimetype` | **Import** via XFL-Pfad |
| XFL-Ordner (unkomprimiert) | lose Dateien + `.xfl`-Marker oder `DOMDocument.xml` | **Import** (neu) |
| `.fla` ≤ CS4 (OLE2) | Microsoft Compound Document, undokumentiertes binäres Format | **Erkennung + klare Ablehnung** |

Implementiert:

- `detect_format` erkennt die OLE2-Signatur (`D0 CF 11 E0 …`) als FLA
  und routet sie zum FLA-Importer, der eine verständliche Fehlermeldung
  mit Konvertierungshinweis („Speichern unter → XFL") liefert — statt
  eines kryptischen ZIP-Fehlers.
- `fla::import_dir` importiert unkomprimierte XFL-Projektordner
  (rekursiv, mit Größen-/Datei-Obergrenzen). `load_path` akzeptiert
  Verzeichnisse mit `DOMDocument.xml` oder `.xfl`-Marker.
- **Archiv-Härtung (Sicherheit):** ZIP-Eintragsnamen mit `..`,
  Absolutpfaden, Laufwerksbuchstaben oder NUL werden abgelehnt;
  Obergrenzen 4096 Einträge / 512 MB Gesamt / 128 MB pro Eintrag;
  XFL-Ordner 8192 Dateien / 512 MB. Es gibt keinen stillen
  Datenverlust: unbekannte ZIP-Einträge werden weiter als
  „Nicht übernommene Einträge" gezählt.

**Nicht implementiert (ehrlich):** Schreiben von `.fla` (kein nativer
FLA-Writer), Import binärer Pre-CS5-FLAs, vollständiges XFL-Schema
(Motion-Tweens als XML, Filter, Komponenten, IK-Daten).

## 2. XFL — Interoperabilität

`crates/nova-formats/src/formats/fla.rs`:

- **DOMShape-Edge-Dekodierung (neu):** `<Edge edges="…">` wird in
  echte `PathCommand`s übersetzt — sowohl das kompakte
  `!x y|x y`-Punktlistenformat (Twips) als auch SVG-artige
  `M/L/C/Q/Z`-Kommandos (inkl. `CubicTo` für C). Koordinaten werden
  auf die Bounds-Mitte zentriert wie andere Pfadobjekte.
- **FillStyles (neu):** `<FillStyle>` mit `SolidColor` (inkl. Alpha)
  und `LinearGradient`/`RadialGradient` + `GradientEntry`-Stops wird
  in `PathShape.fill`/`PathShape.gradient` importiert. Export schreibt
  Gradients als echte XFL-Gradient-Fills (Start/End-Point aus dem
  Winkel, Center/Radius für radial).
- **DOMText (neu):** Textobjekte exportieren jetzt als editierbarer
  XFL-Text (`<DOMText>` mit `<p>/<span>` für Größe, Schrift, Farbe,
  bold/italic, Align) statt als eingebrannte Umrisse; der Import dekodiert
  `DOMText` zurück in `TextObject` (vorher Platzhalter-Rechteck für
  `DOMTextInstance`, unverändert als Warnung).
- Bisheriges bleibt: DOMSymbolInstance, DOMRectangleObject,
  DOMEllipseObject, DOMBitmapInstance, DOMSoundItem, DOMLayer/DOMFrame/
  DOMSymbolItem, Skripte, Assets, Warnungen bei unbekannten Elementen.

## 3. SWF — Gradienten-Import

`crates/nova-formats/src/formats/swf.rs`:

- `FillStyle::LinearGradient`/`RadialGradient`/`FocalGradient` werden
  nicht mehr nur auf die erste Farbe vereinfacht, sondern in
  `PathShape.gradient` überführt (Ratio→Position, RGBA→Stops).
- Gradient-Matrizen werden approximiert: Linear → `angle` aus der
  x-Achse des Gradientenraums; radial → Mitte/Radius aus
  Translations-/Skalierungstermen (dokumentierte Näherung).
- `Bitmap`-Füllungen bleiben Grau + Warnung (kein stiller Verlust).

Kein neuer Tag-Export: SWF v13 bleibt Basis; bestehende Tags
(DefineShape3/4, PlaceObject, Sprite, DefineBits, DefineSound,
DoABC2, SymbolClass, FrameLabel, FileAttributes) unverändert.

## 4. AS3 — `switch`-Statement

`nova-as` (Parser → Checker → Emitter → ABC):

- `switch (expr) { case v[, v…]: … default: … }` mit **strikter
  Gleichheit** (`strictequals`), **mehreren Werten pro Case**,
  **Fall-through** (Kontrollfluss fällt ohne `break` in den nächsten
  Arm — bodies werden in Reihenfolge emittiert) und `default`.
- Der Selektor wird einmal in einem Temp-Local gehalten; `break`
  springt zum Switch-Ende, `continue` delegiert an die umgebende
  Schleife (korrekte LoopLabel-Weitergabe).
- Verifizierte Semantik in Ruffle: `SW1=30` (Fall-through 10+20),
  `SW2=20`, `SW3=30`/`SW4=30` (Multi-Value), `SW5=-1` (Default),
  `M18DONE`, 0 Errors — `samples/m18-switch-compat.nova`.

**Weiterhin nicht unterstützt:** `try/catch/finally`, `for-in`,
Interfaces/`implements`, Getter/Setter, `dynamic`/`native`-Member,
Closures als Werte, `Vector.<T>`, `Array<T>`, `delete`, `in`,
`with`, `throw`-Nutzung ohne try.

## 5. Asset-Workflow

- **Asset-Ordner:** `Asset.folder` (serde-kompatibel, `"/"` = Wurzel,
  z. B. `UI/Sprites`) — in den Objekteigenschaften editierbar, in der
  Bibliotheksliste als `Ordner/Name` sortiert angezeigt.
- **Stabile Referenzen:** Objekte/Tilesets/Sound-Cues referenzieren
  Assets per `Uuid`; Umbenennen/Verschieben kann keine Referenz
  brechen.
- **Neue AI-Ops** (`nova-ai`, alle validiert & transaktional):
  `rename_asset`, `move_asset`, `replace_asset` (Bytes+Metadaten aus
  workspace-konfiniertem Pfad, Bild-/Audio-Validierung, Art-Beibehaltung
  der ID), `audit_assets` (leere Payloads, reine Dateireferenzen,
  ungenutzte Assets), `analyze_project_compatibility` (Inhalts-
  Inventar + dokumentierte Grenzen).
- **UI:** Datei-Menü → „Projekt-Kompatibilität …" öffnet den
  bestehenden CompatibilityReport-Dialog mit dem Projektbericht
  (`nova_ai::project_compatibility_report`, shared mit dem AI-Op).

## 6. Kompatibilitätszentrum

- `nova_formats::analyze_path/analyze_project` (vorhanden) analysiert
  Dateien; der neue projektbasierte Bericht deckt den aktuellen
  Editor-Zustand ab: Szenen/Symbole/Skripte/Assets, Gradient-/Kubic-
  /Skin/Text-Statistiken, Game-/Tileset-/Script-Flags und eine
  klare Liste bekannter Export-Grenzen.
- Import/Export-Dialoge zeigen weiterhin den vollen
  CompatibilityReport (erkannte/nicht übernommene Einträge, Warnungen).

## 7. Sicherheit & Dateiverarbeitung

- ZIP Slip: Entry-Namen mit `..`/Absolute/Drive/NUL → harter Fehler.
- Größenlimits: 4096 Einträge, 512 MB Gesamt, 128 MB/Eintrag,
  8192 Dateien/512 MB für Ordner.
- `read_asset`/`validate_asset_path` (vorhanden) bleiben die einzige
  FS-Grenze für AI-Ops; `replace_asset` nutzt dieselbe Konfinierung.
- Importierte AS3-Skripte werden **nicht** ausgeführt — Skripte landen
  als `original_source` im Projekt und kompilieren erst beim Export.

## 8. Tests

- `crates/nova-ai/tests/m18_runtime.rs` — 9 Tests: Asset-Rename/Move
  mit stabilen Referenzen, Audit, Kompatibilitätsbericht,
  ReplaceAsset-Pfadablehnung, XFL-Import mit Edge-String + Gradient +
  DOMText, OLE2-Ablehnung, XFL-Ordnerimport, ZIP-Slip-Ablehnung,
  Asset-Kind-Fixture.
- `crates/nova-as/tests/compiler.rs` — `switch`-Compile +
  Typecheck-Tests.
- `cargo test --workspace`: alle Suites grün inkl. M12–M17.
- Ruffle: `samples/m18-switch-compat.nova` → 6 Traces, 0 Errors,
  alle Switch-Marker korrekt.

## 9. Bekannte Einschränkungen (ehrlich)

- Pre-CS5 binäre FLA: nicht importierbar (kein OLE2-Parser).
- XFL: Motion-Tween-XML, Filter, Komponenten, Symbole mit
  AS3-Verknüpfung (`linkageExport`) nicht reproduziert; Edge-Formate
  außerhalb `!`/`|` und M/L/C/Q/Z fallen auf die Bounding-Box
  zurück (mit Warnung).
- XFL-Gradient-Matrizen: Näherung über Winkel/Mitte/Radius.
- AS3: `try/catch`, `for-in`, Interfaces, Getter/Setter, Closures,
  `Vector.<T>` weiterhin außerhalb der Subset — der Compiler meldet
  sie explizit als nicht unterstützt.
- Kein `.fla`-Schreiben: XFL-Paket ist der dokumentierte Austauschweg
  und in Adobe Animate nicht final gegen ein reales Animate-Korpus
  validiert.
