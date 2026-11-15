# M22 Plan — UI Redesign & Internationalization

**Basis:** `main` @ `48e2fa7` (post-M21-Remediation, `0.17.0-rc1`)
**Status:** Plan (M22.0 audit done)

## M22.0 — Audit-Ergebnisse

### Git
- Branch `main`, HEAD `48e2fa7`, Working Tree clean.
- M21 remediation abgeschlossen (`140aa94`, `48e2fa7`), Artefakte
  `0.17.0-rc1` neu gebaut + gehasht.

### Vorhandene Architektur (Ist-Zustand)
- **UI:** egui/eframe, `NovaApp` in `app.rs` (~190k). Layout bereits
  Animate-ähnlich: `Panel::top` menu_bar + toolbar (40px),
  `Panel::bottom` status_bar (22px) + bottom_dock (Timeline/Kacheln/
  Konsole/Probleme, resizable), `Panel::left` project_tree,
  `Panel::right` properties_panel, `CentralPanel` = stage/code/
  classroom/enterprise. Panels sind resizable, aber nicht frei
  verschiebbar (egui-Standard). Theme: dunkles "studio" Theme in
  `theme.rs`.
- **Modi:** `Mode::{Code, Animation, Classroom, Enterprise}` —
  Classroom nur Edu (`has_active_module("classroom")`), Enterprise nur
  Enterprise (`administration`). AI-Editor ersetzt Central-Area.
- **Config:** `flash.rs` → `settings_path()` =
  `<edition-config-dir>/flash-player.json`, Edition-Config-Dirs via
  `nova_core::Edition::config_dir()` (User = Legacy-Pfad,
  Edu/Enterprise isoliert). `NOVA_FLASH_STUDIO_CONFIG_DIR`-Override
  für Tests.
- **Terminal-Fenster (M22.2 Ursache):** `crates/nova-app/Cargo.toml`
  hat **kein `windows_subsystem = "windows"`** → Binaries werden als
  Konsolen-Subsystem gebaut → Windows öffnet beim GUI-Start ein
  Konsolenfenster. `--version`/`--help`/`--startup-check` in
  `main.rs::handle_cli` nutzen `println!` — nach dem Wechsel auf
  Windows-Subsystem muss die Parent-Konsole via
  `AttachConsole(ATTACH_PARENT_PROCESS)` angebunden werden, damit CLI-
  Ausgaben nicht verloren gehen. `windows-sys` ist bereits Dependency
  (Feature `Win32_System_Console` ergänzen).
- **i18n:** existiert nicht. UI-Texte sind deutsche Literale in
  `app.rs`, `classroom_view.rs`, `enterprise_view.rs`, `properties.rs`,
  `stage_view.rs`, `timeline_view.rs`, `tree.rs`, `code_view.rs`,
  `ai_editor.rs`, `splash.rs`, `main.rs`. Domain-Crates
  (`nova-ent`, `nova-edu`) liefern deutsche Fehlermeldungen — bleiben
  für M22 Literal (werden als `EntError`-Text angezeigt; dokumentierte
  Einschränkung).
- **Übersetzungs-Dateien:** keine vorhanden.

### Bekannte Einschränkungen (aus M21-Audit übernommen)
- Rollen = Policy, keine Auth; `.novaent` = Metadaten-Backup;
  Integrationen config-only; Stage-Anker-Klick offen.
- `archive-releases.ps1`: Edition-App-Ordner im Archiv sind
  verify-only (bekannte Script-Einschränkung).

## Untermeilensteine

| MS | Inhalt | Deliverable |
|---|---|---|
| M22.0 | Audit + Plan (dieses Dokument) | `docs/MEGA_MILESTONE_22_PLAN.md` |
| M22.1 | UI-Redesign: links Icon-Rail (Modi/AI), einheitliche Panel-Header, Tab-Styling, Theme-Feinschliff | Layout in `app.rs` + `theme.rs` |
| M22.2 | Terminal-Fix: `windows_subsystem`, `AttachConsole` für CLI | `Cargo.toml`, `main.rs` |
| M22.3 | i18n-Core: `i18n.rs`, 5 JSON-Locales, `t()`/`tf()`, Fallback en-GB, Missing-Key-Tracking, Tests | `i18n/` + Modul |
| M22.4 | First-Run Language Wizard | `language_wizard` im App-State |
| M22.5 | Sprachwahl in Einstellungen + Wiedereinstieg | Settings-Dialog |
| M22.6 | Edition-Integration (gemeinsames System, Gating unverändert) | — |
| M22.7 | Tests: alle Editionen, Gating, Locale-Integrität, Encoding | Test-Suiten |
| M22.8 | Docs + Final Report | README/CHANGELOG/ARCHITECTURE/I18N-Doku |

## i18n-Design

- `crates/nova-app/i18n/{de-DE,en-GB,da-DK,fr-FR,pl-PL}.json` —
  flache Key-Map, UTF-8 ohne BOM, `include_str!` → keine Runtime-
  Dateizugriffe nötig (offline-first).
- `i18n.rs`: `Lang` enum (`de-DE` Default? — Wizard erzwingt Wahl;
  Fallback bei fehlender Config = `en-GB`), `set_language`,
  `t(key) -> String`, `tf(key, &[(&str,&str)])` für Platzhalter
  `{name}`-Style.
  - Lookup: aktive Sprache → `en-GB` → Key selbst (sichtbar =
    dev-erkennbar). Missing keys werden in Debug über `log`/`eprintln`
    gezählt; `missing_keys()` für Tests.
- Persistenz: `app-settings.json` in Edition-Config-Dir
  (`{"language": "de-DE"}`), isoliert pro Edition, atomar geschrieben,
  kaputte Datei → `en-GB`.
- Migration: chrome-nahe Strings (Menüs, Toolbar, Dialoge, Statusleiste,
  Wizard, Settings, Classroom/Enterprise Tabs) werden auf `t()`
  umgestellt; tiefe Feature-Strings schrittweise — Abdeckung wird im
  Report ehrlich quantifiziert.

## UI-Redesign (M22.1) — konkret

- Links vertikale Icon-Rail (60px): Modi (Code/Animation/
  Klassenzimmer/Enterprise) + AI-Editor — Animate-Workflow.
- `Panel::left` project_tree bleibt (Library-Funktion), RIGHT panel
  bleibt Properties — Beschriftungen/Headers vereinheitlichen.
- bottom_dock: Timeline als Default-Tab, einheitliche Tab-Optik.
- Toolbar: Kontext-Optionen bleiben, Moduswahl wandert in Rail.
- Theme: subtile Akzent-Anpassung, konsistente Abstände — kein
  Funktionsverlust; alle bisherigen Aktionen bleiben erreichbar.

## Risiken

- Übersetzungsvolumen: App hat tausende deutsche Literale. System +
  Kernflächen vollständig, Rest dokumentiert.
- `AttachConsole` unter Windows nur — `#[cfg(windows)]`.
- Wizard muss vor Projekt-Laden greifen (State-Machine in `app.rs`).
