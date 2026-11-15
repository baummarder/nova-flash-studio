# MEGA MILESTONE 23 — Plan

**Integrierter Code-Workspace & Windows-Installer-Redesign**

## M23.0 — Audit-Befunde

### Workspace-Architektur
- `Mode::{Code, Animation, Classroom, Enterprise}` in `state.rs`;
  zentrale View-Auswahl in `app.rs` (`CentralPanel`-Match).
- `code_view::show(ui, project, state)` ist bereits ein eigenständiges,
  in `Ui` einbettbares Modul (Script-Tabs, ActionBar, Editor,
  Diagnostics) — keine Aufteilung in Monolithen nötig.
- `stage_view::show` analog; Timeline/Bottom-Dock/Side-Docks sind
  Panels außerhalb des CentralPanel und bleiben unverändert.
- Klassisches 2009-Theme (`theme.rs`), Toolbox (`toolbox.rs`),
  Right-Dock (Properties + Library, `right_split`) aus M22.2.

### Installer-Architektur (vor M23)
- Custom .NET 8 WinForms-Assistent (`packaging/windows/installer/`):
  `WizardForm.cs` (6 Seiten), `InstallEngine.cs` (Manifest-basiert),
  `Program.cs` (CLI: `--install/--uninstall/--smoke-test/--version`).
- `EditionInfo.cs` wird pro Edition von `package-installer.ps1`
  regeneriert (Produktname, EXE, Uninstall-Key, ProgIds).
- Install-Scope: Per-User (`%LOCALAPPDATA%`, HKCU, kein UAC) und
  All-Users (Program Files, HKLM, UAC-Relaunch via `runas`).
- Upgrade: `install-manifest.json` im Installationsordner;
  vorherige Artefakte (Files, Shortcuts, Assoziationen,
  Capabilities/RegisteredApplications) werden gezielt entfernt;
  abweichende Installationen nur nach expliziter Migrationsbestätigung.
- Deinstallation: Manifest-gesteuert, nur eigene Dateien/Einträge;
  Association-Records speichern `PreviousDefault` und
  `ProgIdPreExisting` → UserChoice und fremde Zuordnungen bleiben
  unangetastet; `%APPDATA%`-Benutzerdaten werden nie gelöscht.
- Headless-Pfad (`--install DIR`, `--uninstall --quiet`) ist
  vollständig automatisierbar → Grundlage für M23.5-Tests.

### Bekannte Einschränkungen (vor M23)
- Wizard-Visual war generisches dunkles Theme, nicht 2009-klassisch.
- Keine „Dokumentation öffnen“-Aktion auf der Abschlussseite.
- Kein automatisierter Installer-Test (nur Packaging-/Archiv-Tests).

## Untermeilensteine

| MS | Inhalt | Status |
|---|---|---|
| M23.0 | Audit + Plan | ✅ |
| M23.1 | Workspace-Layouts Stage / Stage+Code / Code + Persistenz | ✅ |
| M23.2 | Installer-Audit (s. oben) | ✅ |
| M23.3 | Wizard-Redesign (WizardTheme, Gradient-Header, Bevels, Docs-Button) | ✅ |
| M23.4 | Upgrade/Deinstallations-Verifikation + Doku | ✅ |
| M23.5 | `scripts/test-installer.ps1` (Headless-Zyklus, 3 Editionen) | ✅ |
| M23.6 | Regression: fmt/clippy/Tests, 3 Edition-Builds | ✅ |
| M23.7 | Doku, Report, Artefakt-Hashes, Commits | 🔄 |

## Design-Entscheidungen

- **Layout-Modell:** `Mode::Code` bleibt „Code Only“, `Mode::Animation`
  + `split_code`-Flag ergibt „Stage+Code“ — minimale Invasion, alle
  bestehenden `Mode::`-Checks (Shortcuts, Playback) unverändert.
- **Persistenz:** `app-settings.json` (pro Edition) bekommt
  `layout` + `split_ratio`; Shared-Read/Write-Refactor in `i18n.rs`,
  Sprachfeld bleibt erhalten. Kein `.nova`-Format-Change.
- **Installer:** bestehende Engine beibehalten (bereits
  manifest-basiert, upgrade-sicher, UAC-korrekt); nur visuelle
  Überarbeitung + Docs-Button. Keine neue Installer-Technologie.
