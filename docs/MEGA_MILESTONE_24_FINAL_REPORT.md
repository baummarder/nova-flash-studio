# MEGA MILESTONE 24+25 — Final Report

**Installer Completion (i18n wizard) & AI Agent Project Access**
Branch `main` · Basis `394fe3d` (post-M23) · Deliverables: vollständig
lokalisierter Setup-Assistent mit Sprachauswahl und allen fünf
Sprachen, plus kontrollierter Projektzugriff für den AI-Agent mit
Berechtigungsmodell, isoliertem Review-Workspace und sicherem Apply.

## 1. Installer — implementierte Funktionen (M24)

| Anforderung | Umsetzung |
|---|---|
| Nav-Bug | Footer neu aufgebaut: `TableLayoutPanel`+`FlowLayoutPanel` (RightToLeft), auf Content kann keine Buttons überdecken; `_content`-Padding war doppelt (64+56) und wurde auf symmetrische 8 px korrigiert |
| Navigation | Zurück / Weiter / **Installieren** (nur Summary-Seite) / Abbrechen→„Schließen" auf Finish; Back auf Progress/Finish verborgen, Lizenz-Gate dynamisch |
| Sprachauswahl | `LanguagePage` beim ersten Start ohne gespeicherte Sprache; Header-ComboBox auf **jeder** Seite → Live-Re-Lokalisierung aller Controls via `RefreshText()` |
| Sprachen | de-DE, en-GB, da-DK, fr-FR, pl-PL — native Namen, eingebettete `i18n/*.json`-Ressourcen, en-GB-Fallback, keine hartcodierten neuen Texte |
| Persistenz | `HKCU\Software\<UninstallKey>\Installer\Language` pro Edition; `--set-language`/`--get-language` CLI |
| Ablauf | Sprache → Willkommen → Lizenz → Installationsort → Optionen → **Zusammenfassung** (neu) → Fortschritt → Fertig |
| Abbruch | `CancellationToken` + Bestätigungsdialog bei laufender Installation; keine Daten werden gelöscht |
| Abschluss | Erfolgsbestätigung, „Programm starten"-Checkbox, „Dokumentation öffnen", funktionierender Schließen-Button |

Engine (`InstallEngine.cs`) unverändert: Per-User/All-Users+UAC,
Manifest-Upgrades, Safe-Uninstall, keine `UserChoice`-Eingriffe.

## 2. AI Agent — implementierte Funktionen (M25)

**Berechtigungsdialog beim ersten Öffnen** (`access_dialogs` in
`ai_editor.rs`): Option A (nativer Picker für Dateien/Ordner via `rfd`),
Option B (ganzer Projektbaum), Option C (nur lesen), „Später
entscheiden" → Chat bleibt nutzbar, Dateizugriff = keiner.

**Berechtigungsmodell** (`nova-ai/src/access.rs`):
- `AccessStore` → `ai-access.json` in der Editions-Config, keyed per
  kanonischem Projekt-Root → keine Übertragung auf andere Projekte.
- `Grant{path,level}`: `Read` oder `ReadWrite`; tiefstes Grant gewinnt;
  Projekt-Flag `read_only` deckelt alles auf Lesen.
- Widerruf pro Grant, alle Grants, oder Read-only-Umschaltung —
  alles in der Grants-Dialog-UI („Berechtigungen…") und als
  ZUGRIFF-Sektion im Explorer sichtbar.

**Dateisystem-Sicherheit**: `GuardedWorkspace` ist die einzige
Agent-Dateischnittstelle (Explorer-Liste, Editor-Lesen, Asset-Import,
Apply). `Workspace::resolve` blockt Traversal/Absolute/Symlinks;
`is_sensitive` denied `.env*`, Keys/Zertifikate, `id_rsa*`,
`credentials`, `.netrc`, `.ssh/.aws/.gnupg/.git` — **auch innerhalb
von RW-Grants**. Keine Shell-Ausführung.

**Agent Workspace** (`nova-ai/src/review.rs`): `ReviewSession` staged
jedes Proposal in ein isoliertes Verzeichnis mit Original-Byte-
Snapshot; Originale werden nie direkt beschrieben. Diff pro Datei.

**Review-System**: Diff-Tab mit Checkbox pro Datei (Löschen default
aus), „Alle bestätigen" wendet nur akzeptierte Items an, „Verwerfen"
löscht Staging. Komplett-Vorschau für neue Dateien, Löschen rot
markiert.

**Apply** (`ReviewSession::apply`): Snapshot-Vergleich → **Konflikt**
blockiert nur diese Datei; Grant-Re-Check unmittelbar vor dem
Schreiben; atomare Writes; Fehler → Rollback aller bereits
geschriebenen Dateien; Agent kann nie selbst annehmen.

## 3. Geänderte/neue Dateien

- `crates/nova-ai/src/access.rs` (neu, ~580 Z.) — Grants, Store,
  Sensitive-Filter, GuardedWorkspace + 5 Tests
- `crates/nova-ai/src/review.rs` (neu, ~460 Z.) — ReviewSession,
  Staging, Konflikte, Apply/Rollback + 6 Tests
- `crates/nova-ai/src/lib.rs` — Exports
- `crates/nova-app/src/ai_editor.rs` — Permission-Dialog, Grants-UI,
  Explorer-Filterung, Review-Staging, per-File-Akzeptanz
- `packaging/windows/installer/WizardForm.cs` — Rewrite (i18n,
  LanguagePage, SummaryPage, Footer/TableLayoutPanel)
- `packaging/windows/installer/InstallerText.cs` (neu) — i18n-Engine
- `packaging/windows/installer/i18n/*.json` (neu, 5 Sprachen)
- `packaging/windows/installer/Program.cs` — `--set-language`/
  `--get-language`
- `packaging/windows/installer/NovaFlashStudio.Setup.csproj` —
  i18n-Resources
- `scripts/test-installer.ps1` — + Sprach-Persistenz-Checks
- `packaging/windows/installer/README.md`, `docs/AI_AGENT_EDITOR.md`,
  `README.md`, `CHANGELOG.md`

## 4. Testergebnisse

- `cargo test --workspace`: **alle Suites PASS** (0 failed;
  inkl. 11 neuer access/review-Tests: Grants-Isolation,
  Read-only-Cap, Sensitive-Denylist, Traversal, Staging ohne
  Original-Zugriff, Per-File-Akzeptanz, Konflikt-Block,
  Grant-Widerruf vor Apply, Delete-Opt-in)
- `cargo fmt --all -- --check` + `cargo clippy --workspace
  --all-targets -- -D warnings`: clean
- `dotnet build` Installer: 0 Warnungen/Fehler
- `scripts/test-installer.ps1`: **alle Checks PASS** für user/edu/
  enterprise inkl. neuem Sprach-Persistenz-Test (set de-DE → get
  de-DE → reset en-GB)
- `test-edition-packaging.ps1` + `test-archive-releases.ps1`: PASS
  (Archive mit `-Force` aktualisiert)

## 5. Bekannte Einschränkungen

- **Engine/Headless-Fortschrittsmeldungen** des Installers bleiben
  deutsch (Wizard-UI vollständig lokalisiert).
- Wizard-Interaktion (Buttons, Drag, Sprachwechsel live) nur per
  CLI/Registry getestet — **manuelle GUI-Prüfung empfohlen**.
- Sensible-Datei-Liste ist bewusst konservativ; neue Muster können
  ergänzt werden.
- Review-Staging ist dateibasiert; Binärdateien werden als UTF-8
  behandelt (fehlende Bytes → Konflikt statt stiller Korruption).
- KI-Agent-Dateizugriff greift erst nach Grant; Projektoperationen
  (Dokumentmodell) benötigen weiterhin keinen FS-Zugriff.

## 6. Manuelle GUI-Tests (offen)

1. Setup-Exe starten → Sprachseite → alle Seiten in gewählter Sprache,
   Header-Combo wechselt live, Neustart behält Sprache.
2. Nav-Buttons in allen Seiten/Fenstergrößen sichtbar; Summary zeigt
   alle Optionen; Abbrechen während Progress → Bestätigung.
3. AI Agent öffnen → Berechtigungsdialog → A/B/C testen; Explorer zeigt
   nur freigegebene Dateien; Diff-Tab Checkboxen; Konflikt durch
   externe Änderung simulieren.

## 7. Build-Artefakte (`0.17.0-rc1`, SHA-256)

| Artefakt | SHA-256 |
|---|---|
| user `NOVA-User-Portable.zip` | `4E757E0F0AB9562791E043E58B7FC6D19C06A8A4620119FAA3E8343E4E0A8DF3` |
| user `NOVA-User-Setup.exe` | `82DCA1A01AB51AD711C591E23A2624556F8050BD5D30495B4FCF09341B4E3C4A` |
| user App-EXE | `7A86789D71153078C1D24D9BC11B922ADDBCB9D9D7AFE47037A7DA3CF3996E79` |
| edu `NOVA-Edu-Portable.zip` | `37A7EAC120660BBF4A05F3D0CA5B8FC3DF4B5D8C0789DC0767C16FE731135CA8` |
| edu `NOVA-Edu-Setup.exe` | `EE1AE4917EC755874471F28237698B1F98305294AEEAA8EF4A4A1EE5A44E051B` |
| edu App-EXE | `F70A6DEF8A82918A982410703773CDD0CE33EF56AE75974358CE1AB1A9F7B77B` |
| enterprise `NOVA-Enterprise-Portable.zip` | `31140AF06CE8B2D0A3D658DA9D024D602B836D17D83A2F7CC68AE2CFA4DAC91A` |
| enterprise `NOVA-Enterprise-Setup.exe` | `BE0553BD59601DD133B6F2CCD1B83FFC3ADEE5767405C6B0811B3697822D0D33` |
| enterprise App-EXE | `38BB1145E08FCF506344FAD5D9498B6DB0E8B55DAAB6829FC76340F0DE8A6A14` |

## 8. Git-Commits

- `6082fdb` — M24+M25 Implementierung (Co-Authored-By: Devin AI,
  Trailer verifiziert)
- Dieser Commit — Report + Doku
