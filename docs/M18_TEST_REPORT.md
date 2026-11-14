# M18 Test Report — Flash Compatibility & Workflow

**Version:** `0.14.0-rc1` · **Stand:** nach M18-Implementierung

## Automatisierte Tests

`cargo test --workspace` — alle Suites grün, keine Fehler:

| Crate/Suite | Ergebnis |
|---|---|
| nova-core (Modell, Bones, BoolOp, Skinning) | grün (109 + Unit-Tests) |
| nova-as (Parser/Checker/Emitter/ABC) | grün, inkl. 2 neuer `switch`-Tests |
| nova-ai `m18_runtime` | **9/9** |
| nova-ai `m17_runtime` + weitere | grün (Regression) |
| nova-formats (SWF/XFL/FLA, nova.rs) | grün (34 + 32) |
| nova-build (SWF-Export, Gradient-Parser-Test) | grün |
| nova-app / nova-cli / nova-script | grün |

## M18-spezifische Tests (`crates/nova-ai/tests/m18_runtime.rs`)

1. `rename_and_move_asset_keep_references` — Umbenennen + Ordner-Move;
   Objektreferenz per Uuid bleibt gültig.
2. `audit_assets_reports_unused_and_broken` — Audit-Zähler und
   Problemliste.
3. `analyze_project_compatibility_reports_content` — Bericht enthält
   Inventar und Grenzhinweise.
4. `replace_asset_rejects_unsafe_path` — `../escape.png` → Fehler.
5. `xfl_import_decodes_edge_strings_and_gradients` — XFL-ZIP mit
   `!`-Edge-String → Pfad mit 4+ Commands; `LinearGradient` +
   `GradientEntry` → `PathShape.gradient`; `DOMText` → `TextObject`
   mit bold/farbe/richtigem Inhalt.
6. `ole2_binary_fla_is_rejected_with_guidance` — OLE2-Signatur →
   `FormatKind::Fla` → verständliche Fehlermeldung.
7. `xfl_directory_imports` — lose XFL-Dateien + `.xfl`-Marker →
   Projekt mit korrekten Maßen/FPS.
8. `zip_slip_entries_are_rejected` — `../evil.xml` → harter Fehler.
9. `asset_kind_used_in_fixture` — Fixture-Sanity.

## AS3-`switch` (nova-as)

- `compiles_switch_with_fallthrough_and_break` — ABC parst sauber.
- `switch_case_expressions_type_check` — Case-Werte typechecken.

## Ruffle-Lauf (`tools/ruffle-runtime-test.py`)

`python tools/ruffle-runtime-test.py samples/m18-switch-compat.nova
--expect SW1=30 --expect SW2=20 --expect SW3=30 --expect SW4=30
--expect SW5=-1 --expect M18DONE --seconds 8`

Ergebnis: **6 Traces, 0 Errors, alle Marker PASS.**

- `SW1=30` beweist Fall-through (case 1 → case 2: 10+20).
- `SW3=30`/`SW4=30` beweisen Multi-Value-Case (`case 3, 4:`).
- `SW5=-1` beweist `default`.
- `M18DONE` am Frame 21; Gradient-Shape renderte fehlerfrei.

## Regression

Alle M12–M17-Demos und Tests unverändert grün; M17-SWF-Export,
Skinning, Boolean, Gradients nicht regressiert (Workspace-Suite).

## Nicht abgedeckt / dokumentierte Lücken

- Kein Test gegen echte Adobe-Animate-XFL-Dateien (nur synthetische
  Fixtures im dokumentierten Subset).
- XFL-Export-XML ist strukturell valide, aber nicht in Animate
  geöffnet/gegen ein Referenz-Korpus geprüft.
- Pre-CS5-FLA-Ablehnung ist detection-getestet, nicht import-getestet.
