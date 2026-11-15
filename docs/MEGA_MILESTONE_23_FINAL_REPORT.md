# MEGA MILESTONE 23 — Final Report

**Integrated Code Workspace & Windows Installer Redesign**
Branch `main` · Basis `e510c3b` (post-M22) · Deliverables:
drei Editor-Layouts mit Persistenz, klassischer Installer-Assistent,
Headless-Installer-Tests für alle drei Editionen.

## 1. Architektur-Änderungen

### Workspace-Layout (M23.1)

- Neues `EditorLayout { Stage, Split, Code }` in `state.rs` +
  `UiAction::SetLayout`.
- `AppState.split_code: bool` — `Mode::Animation` + Flag =
  Stage+Code; `Mode::Code` = Code Only (unverändert).
- `NovaApp.code_split: f32` — Divider-Position (Code-Anteil,
  0.2–0.8), plus `editor_layout()`/`persist_layout()` Helfer.
- `app.rs::split_workspace()` — horizontaler Split: Stage links,
  Code rechts, ziehbarer Classic-Divider (5 px, Hover-Hervorhebung);
  beide Panes bekommen eigene `Ui`-Kinder → volle Interaktivität
  (Werkzeuge, Editor-Shortcuts, Timeline unverändert).
- Toolbar: kompakte dreifach-Segmentierung **Bühne / Bühne+Code /
  Code** (i18n-Keys `layout.*`, alle 5 Sprachen) +
  `menu.view.split`-Eintrag; Classroom/Enterprise/AI unverändert.
- Persistenz: `app-settings.json` (pro Edition) erhält `layout` +
  `split_ratio`; `i18n.rs` refactored auf `read_settings()`/
  `write_settings()` — Sprachfeld bleibt erhalten, kein
  `.nova`-Format-Change, kein neues Config-Verzeichnis.

### Installer (M23.3/M23.4)

- `WizardTheme` (neu, `WizardForm.cs`): geteilte Classic-Palette —
  `#26282B` Hintergrund, `#303236` Panels, `#4C9BE0` Akzent,
  Warn/Info-Farben; `MakeGroup()` für geätzte Gruppenrahmen.
- `HeaderPanel` mit vertikalem `LinearGradientBrush` + Haarlinie.
- Footer-Buttons: `FlatStyle.System` (Back/Cancel), Akzent-Next mit
  Border — klassisches 2009-Erscheinungsbild.
- Finish-Seite: **„Dokumentation öffnen"** → öffnet `<InstallDir>\docs`
  (Payload enthält `docs/*.md` + README).
- `InstallEngine.cs` **unverändert** — Audit ergab bereits korrekte
  Implementierung (siehe §3).

## 2. Neue Funktionen

| Feature | Beschreibung |
|---|---|
| Stage+Code-Split | Stage und Editor gleichzeitig, Drag-Divider, beide bedienbar |
| Layout-Persistenz | pro Edition in `app-settings.json`; Restore beim Start |
| Layout-i18n | `layout.stage/split/code`, `menu.view.split` in 5 Sprachen |
| Installer-Theme | WizardTheme-Palette, Gradient-Header, Classic-Buttons |
| Docs-Button | Finish-Seite öffnet mitgelieferte Dokumentation |
| Installer-Tests | `scripts/test-installer.ps1` — 39 Headless-Checks |

## 3. Installer-Audit & Verhalten (M23.2/M23.4)

| Aspekt | Implementierung |
|---|---|
| System | Custom .NET 8 WinForms (`WizardForm`+`InstallEngine`), Single-File-Setup-Exe, kein WiX/Inno |
| Editionen | `EditionInfo.cs` pro Edition generiert; eigene Namen/Keys/ProgIds → Side-by-Side |
| Pfade | Per-User: `%LOCALAPPDATA%\Programs`; All-Users: `Program Files` |
| UAC | All-Users → `runas`-Relaunch; Ablehnung = Abbruch ohne Seiteneffekte |
| Upgrade | Manifest-basierter In-Place-Update; stale Files entfernt; Konflikte nur mit Migrationsbestätigung |
| Deinstallation | Nur manifest-recorded Artefakte; `PreviousDefault`/`ProgIdPreExisting` restauriert; **kein UserChoice-Eingriff**; `%APPDATA%` bleibt immer |
| Dateizuordnungen | ProgId + OpenWith + Capabilities; nur eigene Einträge werden entfernt |

## 4. Testergebnisse

- `cargo test --workspace`: **496 passed / 0 failed / 8 ignored**
  (neu: `workspace_layout_persists_and_preserves_language`)
- `cargo fmt --all -- --check`: clean · `cargo clippy --workspace
  --all-targets -- -D warnings`: clean
- `scripts/test-installer.ps1`: **39/39 PASS** — pro Edition
  Smoke-Test, Headless-Install, Payload/Manifest/Registry/Docs-
  Prüfung, Uninstaller-Kopie, Headless-Uninstall, Entfernung,
  **Benutzerdaten-Erhalt** (Sentinel in `%APPDATA%\<edition>`)
- `test-edition-packaging.ps1` + `test-archive-releases.ps1`: PASS
- PE-Subsystem aller drei App-EXEs: **2 (GUI)** — kein Konsolenfenster
- Manuell zu prüfen: Wizard-Aussehen, Split-Dragging, Layout-Restore

## 5. Bekannte Einschränkungen

- Split-Code-Editor zeigt die volle `code_view` inkl. Script-Tabs/
  ActionBar — bei sehr schmalen Fenstern (< ~500 px Mitte) wird der
  Divider auf 240 px Minimum geklemmt.
- Layout gilt nur für den Standard-Editor; Classroom/Enterprise/AI
  behalten Vollflächen-Views (by design).
- Wizard-Interaktion (Buttons, Drag) nur per Headless-CLI getestet;
  visuelle Prüfung manuell empfohlen.
- `UserChoice`-Verhalten von Windows 10/11 bleibt OS-kontrolliert
  (nur OpenWith-Registrierung, kein Default-Claim).

## 6. Manuelle Testanleitung

**Workspace:** App starten → Toolbar-Segmente „Bühne / Bühne+Code /
Code" → Divider ziehen → App neu starten → Layout wiederhergestellt.
Menü „Ansicht → Layout Bühne + Code" als Alternativpfad.

**Installer:** `dist\windows\<edition>\NOVA-<Edition>-Setup.exe`
starten → Welcome (Edition+Version) → Lizenz → Installationsort
(per-user Default; all-users zeigt UAC-Hinweis) → Optionen
(Desktop/Startmenü/.nova/.swf) → Fortschritt → Fertig
(„Programm starten" + „Dokumentation öffnen").
Upgrade: Setup erneut auf bestehenden Installationsort laufen lassen.
Deinstallation: Windows „Apps" oder `<InstallDir>\* Uninstall.exe`.

## 7. Build-Artefakte (`0.17.0-rc1`, SHA-256)

| Artefakt | SHA-256 |
|---|---|
| user `NOVA-User-Portable.zip` | `25A7FE3D0778C0C77BB8D942B6FD863A1E58BE016EC540E18E48C4101A7EC629` |
| user `NOVA-User-Setup.exe` | `B157E4A1DC124492CCECC8CC223AEA991F70EDCB38641ED29CAFC22333FAFFB8` |
| user App-EXE | `41086C85B4D9AC9C3999C1C15CF16D5F065E941A20B6DEB7634A92C472A219D6` |
| edu `NOVA-Edu-Portable.zip` | `163F52D47ED3965F8B4F52A664619D7B6AC4E6754591D8ACAEBAD2978F17B246` |
| edu `NOVA-Edu-Setup.exe` | `208D466F64196DF419FCF6FFFA45964A845D9FCB1C386047D397FCBAB4C3A577` |
| edu App-EXE | `F4DB135CD0BA037DEA70E21527F1EF8F10AAB3FC2C08BA20F0C5EE27A886D1F0` |
| enterprise `NOVA-Enterprise-Portable.zip` | `1C5D861973552D8B2DB7A67DE3D235D283322438F1EFE5EC0C50B5F9CE9B6107` |
| enterprise `NOVA-Enterprise-Setup.exe` | `F7625788BCDEC296EB0D642BEB8A877F0C56B46F1B6ED93C73FE7ED2E5ADBBC1` |
| enterprise App-EXE | `B9DD68BD5FA6BDEB1ADD65C0DA41E27FB02A7CE599BC8665C6E4807DA0EDE6DB` |

## 8. Git-Commits

- `43866b0` — M23.1+M23.3+M23.5: workspace layouts, classic wizard,
  installer test script (Co-Authored-By: Devin AI, verifiziert)
- Dieser Commit — Plan/Report/Doku + finale Hashes

## 9. Screenshots

Nicht automatisch erstellt (kein GUI-Screenshot-Tool im Repo);
manuelle Prüfung via portable EXE / Setup-Exe empfohlen.
