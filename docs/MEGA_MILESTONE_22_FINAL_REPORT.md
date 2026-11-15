# MEGA MILESTONE 22 — Final Report

**UI Redesign & Internationalization** · Branch `main` · Basis
`48e2fa7` (post-M21-Remediation) · Deliverables: mode rail, i18n
system (5 locales), first-run language wizard, per-edition language
persistence, Windows console-window removal.

## 1. Implementierte Funktionen

- **M22.1 UI-Redesign (erste Ausbaustufe):** neue schlanke
  Modus-Leiste links (`Panel::left "mode_rail"`, 56 px, Animate-
  Workflow-Position): Code / Animation / Klassenzimmer* /
  Enterprise* + AI-Agent-Umschalter. Die Leiste ist in allen Modi
  sichtbar, damit Classroom/Enterprise/AI-Modi zurückwechseln
  können. Toolbar behält Projektname, Dirty-Marker und
  Export/Save/Open-in-Player; Moduswahl wurde aus der Toolbar
  entfernt (Menü „Ansicht“ bleibt als zweiter Pfad). Panels bleiben
  resizable; Dark-Theme unverändert konsistent.
  - *Edition-Gating unverändert:* `has_active_module("classroom")`
    bzw. `("administration")` — kein Cross-Edition-Leak.
- **M22.2 Terminal-Fenster beseitigt:** Ursache = fehlendes
  `windows_subsystem`; Binaries waren Konsolen-Subsystem. Jetzt
  `#![windows_subsystem = "windows"]` → PE-Subsystem **2 (GUI)** in
  allen drei Editionsexes (verifiziert). CLI bleibt funktionsfähig:
  `attach_parent_console_for_cli()` bindet bei `--version`/`--help`/
  `--startup-check` die Parent-Konsole an (**getestet: Ausgabe
  erscheint**). Panics landen in `<edition-config>/nova-crash.log`
  (kein stiller Verlust). `windows-sys` Feature
  `Win32_System_Console` ergänzt.
- **M22.3 i18n-System:** `i18n/{de-DE,en-GB,da-DK,fr-FR,pl-PL}.json`
  (UTF-8 ohne BOM, `include_str!`, offline). `i18n.rs`: `Lang`,
  `t()`, `tf()` mit `{name}`-Platzhaltern, Fallback
  aktiv→en-GB→Key, Missing-Key-Tracking. Persistenz pro Edition in
  `app-settings.json` (isolierte Config-Dirs, atomarer Write,
  korrupt→en-GB).
- **M22.4 First-Run-Wizard:** `language_wizard.rs` — gebrandeter
  Vollbild-Screen vor dem Splash, wenn keine Sprache gespeichert ist;
 .native Namen (Deutsch / English (UK) / Dansk / Français / Polski),
  Live-Vorschau in der gewählten Sprache, Bestätigung persistiert.
- **M22.5 Sprachwechsel:** ComboBox in den Einstellungen
  (Allgemein→Sprache, sofort wirksam) + „Sprachassistent erneut
  öffnen“ in Hilfe und in den Einstellungen.
- **M22.6 Edition-Integration:** gemeinsames System für alle drei
  Editionen; Edu- und Enterprise-Tab-Labels übersetzt
  (`edu.tab.*`, `ent.tab.*`).

## 2. Geänderte Architektur

- Neues Modul `i18n.rs` + `language_wizard.rs`; 5 Locale-Dateien
  unter `crates/nova-app/i18n/`.
- `app.rs`: `wizard: Option<Lang>`-State, Wizard-Branch in `ui()`,
  `mode_rail()`, Toolbar-Bereinigung, i18n-Aufrufe in Menüs,
  Toolbar, Dock, Statusleiste, About/Help/NewProject/Unsaved-
  Dialogen.
- `edu_state.rs`/`ent_state.rs`: `label()` → `String` via `i18n::t`.
- `main.rs`: `windows_subsystem`, `AttachConsole`, Panic-Log,
  `i18n::load_language()`.
- `Cargo.toml`: `Win32_System_Console` Feature.
- Keine Änderungen an `.nova`-Format, Projektmodell, Edition-
  Registry oder Domain-Crates.

## 3. Neue/geänderte Dateien

Neu: `i18n.rs`, `language_wizard.rs`, `i18n/*.json` (5),
`docs/I18N.md`, `docs/MEGA_MILESTONE_22_PLAN.md`, dieser Report.
Geändert: `main.rs`, `app.rs`, `edu_state.rs`, `ent_state.rs`,
`Cargo.toml`, `README.md`.

## 4. Übersetzungsstatus je Sprache

| Sprache | Keys | Status |
|---|---|---|
| de-DE | 164 | vollständig (Referenz) |
| en-GB | 164 | vollständig (Fallback) |
| da-DK | 164 | vollständig |
| fr-FR | 164 | vollständig |
| pl-PL | 164 | vollständig |

Abgedeckt: Menüleiste, Toolbar, Modus-Leiste, Dock-Tabs,
Statusleiste, About/Help/New-Project/Unsaved-Dialoge,
Einstellungs-Sprache, Wizard, Edu/Enterprise-Tab-Labels.
**Einschränkung:** tiefe Feature-Panel-Texte (properties, stage,
timeline, code, ai_editor, Klassenzimmer-/Enterprise-Tab-Inhalte)
und deutsche Domain-Fehlermeldungen (`nova-ent`/`nova-edu`) sind
noch Literal — dokumentiert in `docs/I18N.md`, nächster Schritt
ohne Architekturänderung.

## 5. Terminal-Ursache und Lösung

- **Ursache:** `Cargo.toml`/`main.rs` ohne
  `windows_subsystem` → Standard = Konsolen-Subsystem →
  Konsolenfenster bei jedem GUI-Start (auch per Installer/Link,
  da das EXE selbst das Subsystem trägt).
- **Lösung:** `#![windows_subsystem = "windows"]` (PE-Subsystem 2,
  verifiziert). CLI-Ausgaben bleiben via `AttachConsole
  (ATTACH_PARENT_PROCESS)`; Panics → `nova-crash.log`.
- **Verifiziert:** `--version` und `--startup-check` auf dem
  User-Release-EXE geben korrekte Ausgabe; Subsystem-Bytes geprüft.

## 6. Testergebnisse

- `cargo test --workspace`: **495 passed / 0 failed / 8 ignored**
  (neu: 7 i18n-Tests — Key-Parität, UTF-8/no-BOM, Platzhalter-
  Konsistenz, Fallback, Substitution, Locale-Parsing,
  per-Edition-Persistenz inkl. korrupt→en-GB).
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `test-edition-packaging.ps1`: alle PASS.
- `test-archive-releases.ps1`: alle PASS.

## 7. Bekannte Einschränkungen

- UI-Text-Abdeckung: Chrome/Dialoge/Tab-Labels in 5 Sprachen;
  tiefe Panel-Inhalte + Domain-Fehlermeldungen noch Deutsch
  (Literal-Fallback, kein Key-Verlust).
- Panels sind resizable, nicht frei verschiebbar (egui-Standard;
  `egui_dock` wäre ein späterer Schritt).
- Wizard hat keine Skip-Option (Sprachwahl ist Pflicht laut Spec);
  Settings-Combo erlaubt Wechsel ohne Neustart.
- GUI-Interaktion (Wizard-Klick, Sprachwechsel live, Rail-Klicks)
  nur per Code-/Testpfad verifiziert — **MANUAL CHECK REQUIRED**.
- `archive-releases.ps1`: Edition-App-Ordner im Archiv sind
  verify-only; nach Rebuilds erneut manuell entfernt+rearchiviert
  (bekannte Script-Einschränkung, nicht M22-Neuheit).

## 8. Build-Artefakte (`0.17.0-rc1`, SHA-256)

| Artefakt | SHA-256 |
|---|---|
| user `NOVA-User-Portable.zip` | `292CD9925FBDF96749656D7B10FE5E060B2BAA4432918FB4EBF5C37A223FADDD` |
| user `NOVA-User-Setup.exe` | `6FB9B1276E7DD10E97938063AF2A07F4F75B7905C70891EA66D23E4CB7E29B0B` |
| user App-EXE | `652291A272049F12B280CD5BA732B17CC6F12BEAFFCA303C3D6FD63FD11E45D4` |
| edu `NOVA-Edu-Portable.zip` | `456BD47CE052435BC4421B0D84972E57442349DBC9206D699DA2E77EE03E14D7` |
| edu `NOVA-Edu-Setup.exe` | `4ECF58BB578033589E786F622A1CAD29790A963AC0101414C12389C0CA8DFDDD` |
| edu App-EXE | `44F944845B34437FC95D8DC1493656BEE17AD12C7D7C9D04F658A75D0E8C4EC6` |
| enterprise `NOVA-Enterprise-Portable.zip` | `538CB1559834D85BF5F39136D89CB934605E183628DFE0101E26E903139C6159` |
| enterprise `NOVA-Enterprise-Setup.exe` | `B94B178984261F485F52255815D26F0CBA5C559C1095C1495C7ADE7E4B3001FA` |
| enterprise App-EXE | `3EA0D6625D879B92E4C0766FCF23ED5DFA7A8598A5513573812BB84F95D7173E` |

Archive `dist/archive/0.17.0-rc1/<edition>/` aktualisiert und per
`test-edition-packaging.ps1` verifiziert.

## 9. Git-Commits

- `M22 core` — i18n, wizard, rail, terminal fix, plan, tests
  (Co-Authored-By: Devin AI Trailer verifiziert).
- `M22 docs+artifacts` — README, I18N.md, Report, neue Hashes
  (dieser Commit).

## 10. Verbleibende manuelle Tests

- GUI: Wizard bei Erststart, Sprachwechsel live, Rail in allen drei
  Editionen, Klassenzimmer/Enterprise-Modi.
- Installer-Start ohne Konsolenfenster (Subsystem verifiziert,
  echter Explorer-Start recommended).
- `.nova`-Projekt-Öffnen nach M22 unverändert.
