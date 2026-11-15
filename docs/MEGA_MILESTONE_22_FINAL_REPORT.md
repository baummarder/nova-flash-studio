# MEGA MILESTONE 22 — Final Report

**UI Redesign & Internationalization** · Branch `main` · Basis
`48e2fa7` (post-M21-Remediation) · Deliverables: classic-2009
workspace, i18n system (5 locales), first-run language wizard,
per-edition language persistence, Windows console-window removal.

> **UI-Korrektur (nach Nutzer-Feedback):** Die anfängliche
> Animate-Style-Mode-Leiste wurde durch die klassische
> 2009-Desktop-Richtung ersetzt — siehe Abschnitt 1a.

## 1a. UI-Korrektur — klassisches 2009-Design

Nach Nutzer-Feedback wurde die moderne/flache Richtung verworfen
und durch eine **Classic-Desktop-Ästhetik ca. 2008–2010** ersetzt
(Flash-CS4/CS5-Workflow-Referenz; keine Adobe-Assets):

- **Theme (`theme.rs`):** neue Charcoal-Palette — Pasteboard
  `#18191B`, Panels `#34363A`/`#3D3F43`, Inputs `#26282B`, Hover
  `#4C4F54`; Classic-Blue-Accent `#4C9BE0`; gedämpfte Statusfarben;
  2-px-Corner-Radius, kompaktere Abstände und kleinere Fonts
  (Body 12.5, Button 12, Small 11). Alle Views erben die Palette
  ohne Call-Site-Änderungen.
- **Neues `toolbox.rs`:** schmale vertikale Werkzeugleiste (48 px,
  nur im Animationsmodus) mit allen neun vorhandenen Tools
  (Select/Line/Pencil/Brush/Eraser/Fill/PathEdit/Pen/SkinPaint),
  als Icon-Buttons mit `theme::tool_icon()`-Primitive-Glyphen
  (16 px, keine Assets), Two-Tone-Bevels, Tooltips (Label+Hint).
- **Mode-Auswahl:** Leiste entfernt; kompakte Segmentierung in der
  Toolbar (Code/Animation/Klassenzimmer*/Enterprise*/AI Agent) +
  Menüs — Edition-Gating unverändert.
- **Rechtes Dock:** einzelnes `Panel::right "right_dock"` —
  Properties oben, Projektbaum als Bibliothek unten, mit
  ziehbarem Classic-Divider (`right_split`, 0.2–0.8).
- **Stage:** horizontaler Text-Tool-Strip entfernt; dunklere
  Pasteboard-Fläche. Toolbar 36 px mit dezenter Füllung +
  Haarlinie.
- Neue Helfer: `bevel()`, `bevel_frame()`, `tool_icon()`/
  `ToolIcon` — kein neue Dependency.

## 1. Implementierte Funktionen

- **M22.1 UI-Redesign:** klassisches 2009-Layout — Toolbox links,
  Stage/Code zentriert, Properties+Bibliothek rechts (teilbar),
  Timeline unten; kompakte Toolbar-Segmentierung für Modi.
  *Edition-Gating unverändert:* `has_active_module("classroom")`
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

Neu: `i18n.rs`, `language_wizard.rs`, `toolbox.rs`,
`i18n/*.json` (5), `docs/I18N.md`, `docs/MEGA_MILESTONE_22_PLAN.md`,
dieser Report. Geändert: `main.rs`, `app.rs`, `edu_state.rs`,
`ent_state.rs`, `theme.rs`, `stage_view.rs`, `Cargo.toml`,
`README.md`, `CHANGELOG.md`.

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
  (Literal-Fallback, kein Key-Verlust). Toolbox-Tooltips nutzen
  weiterhin die deutschen `Tool::label()`/`hint()`-Literale.
- Panels sind resizable (Properties/Bibliothek-Teilung per
  Divider), nicht frei verschiebbar (egui-Standard;
  `egui_dock` wäre ein späterer Schritt).
- Wizard hat keine Skip-Option (Sprachwahl ist Pflicht laut Spec);
  Settings-Combo erlaubt Wechsel ohne Neustart.
- GUI-Interaktion (Wizard-Klick, Sprachwechsel live, Rail-Klicks)
  nur per Code-/Testpfad verifiziert — **MANUAL CHECK REQUIRED**.
- `archive-releases.ps1`: Edition-App-Ordner im Archiv sind
  verify-only; nach Rebuilds erneut manuell entfernt+rearchiviert
  (bekannte Script-Einschränkung, nicht M22-Neuheit).

## 8. Build-Artefakte (`0.17.0-rc1`, SHA-256 — nach UI-Korrektur)

| Artefakt | SHA-256 |
|---|---|
| user `NOVA-User-Portable.zip` | `1688DAA83672F03BE040B5C2507E8592B5826ED8D04A155BD0584612C39F4AEC` |
| user `NOVA-User-Setup.exe` | `3A482DD0B308EE8BAAF9EADF3A2642910049857665C30B58D94AC625D3A5A761` |
| user App-EXE | `1CB3B177338A628156854573556659E21C034A6CA4BFB1E85C3F97163E3281D0` |
| edu `NOVA-Edu-Portable.zip` | `7E0C99F359013820B7405AA95CDECDBEA0D2E35DFFD9D9544C9533EBD8640C39` |
| edu `NOVA-Edu-Setup.exe` | `CF3136A1520D63DA81FD197065FC62F662E18F54C9459667CED593F247D32E7F` |
| edu App-EXE | `D2BBBAE8654A4B8D1C49DAF8F382C2F0EE1FF2E2A2A34CA84F320EC835A21039` |
| enterprise `NOVA-Enterprise-Portable.zip` | `F54869372F6AA82FE004576738878D2DE27C876B8A906D1125C8D0FB1DB334AB` |
| enterprise `NOVA-Enterprise-Setup.exe` | `42FAFADAD9C407469048A833D8BB9F81710EB0F357598B874FDC2344048240F4` |
| enterprise App-EXE | `05FEE4E7387597DC88F4B54020ABFE816270C597CDF0A3CB8D188A1A8FCE271D` |

Archive `dist/archive/0.17.0-rc1/<edition>/` aktualisiert und per
`test-edition-packaging.ps1` verifiziert.

## 9. Git-Commits

- `86950e6` — M22 core: i18n, wizard, rail, terminal fix, plan,
  tests (Co-Authored-By: Devin AI Trailer verifiziert).
- UI-Korrektur-Commit — klassisches 2009-Design (Theme, Toolbox,
  Right-Dock, Toolbar-Segmentierung).
- `10f5b27` — README, I18N.md, Report, erste Hashes.
- Dieser Commit — Report/Korrektur-Abschnitt + neue Hashes.

## 10. Verbleibende manuelle Tests

- GUI: Wizard bei Erststart, Sprachwechsel live, Rail in allen drei
  Editionen, Klassenzimmer/Enterprise-Modi.
- Installer-Start ohne Konsolenfenster (Subsystem verifiziert,
  echter Explorer-Start recommended).
- `.nova`-Projekt-Öffnen nach M22 unverändert.
