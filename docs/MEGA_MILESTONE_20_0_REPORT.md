# MEGA MILESTONE 20.0 — Stabilization, CI & Documentation

**Target:** `0.16.0-rc1` preparation · **Base:** `d4b9e5a` (M20 prep) on
`926e11e` (M19) · **Status:** stabilization only — **no** classroom code,
**no** `nova-edu` crate, `.nova` stays v4.

## 1. Ausgangszustand

- M19 (`926e11e`) + M20-Vorbereitung (`d4b9e5a`) vorhanden; Arbeitskopie
  enthielt uncommittete Packaging-/Doku-Arbeiten (Editionen-Manifest,
  Testskripte, ROADMAP/README).
- CI-Clippy (`cargo clippy --workspace --all-targets -- -D warnings`)
  war rot (mind. `nova-swf`, `nova-as`, `nova-core`).
- In-App-Hilfe/README enthielten veraltete Funktionsangaben
  (Bitmaps/Sounds/Masken/Shape-Tweens als fehlend).
- `dist/archive/0.15.0-rc1/` fehlte zunächst das `enterprise/`-Verzeichnis.

## 2. Behobene Clippy-Probleme

Commit `091f6bf` (vom Nutzer, OpenCode/DeepSeek-Vorarbeit): alle
`-D warnings`-Fehler workspace-weit behoben — Tuple-/Struct-Refactors
lose Parameter, Dead-Code-/Import-Cleanup, `let`-Chains, kein
verhaltensändernder API-Bruch. **Verifiziert:**

```
cargo clippy --workspace --all-targets -- -D warnings  → sauber
cargo fmt --all -- --check                            → sauber
cargo test --workspace --quiet                        → 428 passed, 0 failed
```

## 3. Exakte Testergebnisse

| Prüfung | Ergebnis |
|---|---|
| `cargo test --workspace` | **428 passed / 0 failed / 8 ignored** (32 Suites) |
| `cargo fmt --all -- --check` | **PASS** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** |
| `scripts/test-edition-packaging.ps1` | **PASS** (Manifest, EditionInfo.cs, alle 3 Editionen: Artefakte, Checksummen, ZIP-Inhalt, Setup-VersionInfo, Archiv-Vollständigkeit) |
| `scripts/test-archive-releases.ps1` | **PASS** (inkl. neuem Drei-Editionen-Archiv-Test, Idempotenz, -Force-Schutz) |

## 4. In-App-Hilfe

`app.rs` About/Help (Commit `091f6bf` + Korrektur in diesem Lauf):
- Bitmaps, Sounds, verschachtelte Symbol-Timelines, Masken,
  Shape-Tweens und Skelett-Animation werden als **implementiert**
  gelistet.
- „Bewusste Grenzen“ nennt jetzt korrekt: AS3-Teilmenge,
  Script-Objekt-Timeline-Einschränkung, Vektor-Schrift ohne
  Systemfonts/Laufzeittext, Audio-Vorschau ohne Offset-/Dauer-Trimming,
  strukturelle (nicht Runtime-) Validierung, fehlende
  Classroom-/Admin-Module und die **nicht verifizierte** `.nova`-
  Dateizuordnung.
- About zeigt editions-spezifische Module und markiert reservierte
  Module explizit als „Geplant, noch nicht implementiert“.

## 5. README- und ROADMAP-Änderungen

- `README.md`: M20.0-Statusblock, Editions-Artefakt-Tabelle
  (`dist/windows/{user,edu,enterprise}/`), `0.15.0-rc1`-Windows-Stand,
  korrigierte Feature-Grenzen (Gradienten, Masken, verschachtelte
  Timelines als vorhanden).
- `docs/ROADMAP.md`: aktueller Status (M9–M19-Tabelle, M20.0 als done,
  M20.1–M20.3 offen), historischer M4-Abschnitt als überholt markiert,
  Verweis auf die neue Assoziations-Spezifikation.
- `docs/PACKAGING.md`: Editions-Abschnitt, `editions.json` als
  Single-Source-of-Truth, Layout `dist/windows/<edition>/`,
  pro-edition Config-Verzeichnisse.
- `crates/nova-core/tests/editions.rs`: präzisiert — prüft jetzt
  strukturell auf `edition`/`editionId`-Felder statt Substring-Suche
  (robust gegen legitime Namen) und validiert `packaging/editions.json`
  gegen `nova_core::Edition`.

## 6. Packaging- und Archivkorrekturen

- `packaging/editions.json` (neu): Single Source of Truth für
  Produktname, Config-/Recovery-Dirs, Bundle-ID, AppExe,
  Uninstall-Exe/-Key, ProgIds, Setup-/Portable-Namen.
- `package-windows.ps1` / `package-installer.ps1`: hartcodierte
  Edition-Tabellen durch `editions.json` ersetzt; `EditionInfo.cs` wird
  pro Edition generiert und auf User-Defaults restauriert.
- `archive-releases.ps1`: Archiviert Editions-Verzeichnisse
  (`BUILD-INFO.json`-Version) nach `dist/archive/<version>/<edition>/`,
  mit Payload-Exe- und SHA256SUMS-Verifikation; `-Force`-Schutz.
- **Archiv-Lücke geschlossen:** `dist/archive/0.15.0-rc1/` enthält jetzt
  `user/`, `edu/` und `enterprise/` — verifiziert per
  `test-edition-packaging.ps1` (Archiv-ZIPs stimmen hash-genau mit den
  aktuellen Artefakten überein).
- `test-edition-packaging.ps1` (neu): 90+ Assertions über Manifest,
  Installer-Identität, Artefakt-Vollständigkeit, Prüfsummen,
  Koexistenz und Archiv.
- `test-archive-releases.ps1`: um Drei-Editionen-Szenarien erweitert.

## 7. Neue und verbesserte Tests

- `nova-core/tests/editions.rs`: +Manifest-Konsistenztests
  (`editions.json` ↔ `Edition`), präzisierte Projekt-Feld-Prüfung,
  Edition-Loop im Roundtrip.
- `scripts/test-edition-packaging.ps1`: vollständige Editions- und
  Archivprüfung.
- `scripts/test-archive-releases.ps1`: Edition-Archiv-Szenarien
  (vollständig, idempotent, -Force-Verweigerung).
- Alle M19-Tests unverändert grün.

## 8. Windows-Dateizuordnungs-Spezifikation

`docs/WINDOWS_FILE_ASSOCIATION.md` (neu, **nur Spezifikation**):

- Bestehende Mechanik dokumentiert: `AssociateNova` →
  `RegisterAssociation(".nova", EditionInfo.ProgIdNova, …)` mit
  Snapshot/Restore in `install-manifest.json`.
- Pro-Edition ProgIds (`NOVAFlashStudio[Edu|Enterprise].Project/.SWF`).
- Fehlende M20.2-Lücke identifiziert: `Capabilities\FileAssociations` +
  `RegisteredApplications` (für moderne „Öffnen mit“-Listen) und
  `OpenWithProgids`.
- UserChoice-Regeln: nie schreiben, nur dokumentieren; Standard-App
  bleibt Windows-Settings-Sache.
- Deinstallations-/Koexistenz-Matrix, Portable-Opt-in-Design,
  M20.2-Checkliste.

## 9. Bekannte Einschränkungen

- Keine echte Installation der Setup-EXEs auf einem frischen System
  getestet (nur `--version`/`--smoke-test` und VersionInfo).
- `Capabilities`/`RegisteredApplications` noch nicht implementiert
  (M20.2).
- macOS/Linux-Packaging weiterhin uneditioniert.
- In-App-Hilfe nennt die `.nova`-Zuordnung als „nicht verifiziert“ —
  bewusst ehrlich, da die Installer-Registrierung code-seitig existiert,
  aber kein frischer Installationslauf dokumentiert ist.

## 10. Lokale Commits

- `091f6bf` — M20.0: repair Rust CI and correct stale in-app help
  (Vorarbeit, Nutzer/OpenCode).
- Dieser Abschluss-Commit (siehe `git log`).

## 11. Devin-Co-Author-Trailer

`091f6bf` wurde vom Nutzer ohne Devin-Beteiligung erstellt und trägt
korrekterweise **keinen** Devin-Trailer (Commit-Body dokumentiert die
OpenCode/DeepSeek-Beteiligung als Prosa). Der Abschluss-Commit dieses
Berichts enthält Devin-Beiträge (Spezifikation, PACKAGING.md,
Hilfe-Korrektur, Report) und trägt den verpflichtenden Trailer —
verifiziert via `git log -1 --format=%B`.

## 12. Bestätigung

**Nichts wurde gepusht.** Kein Release, kein Tag, keine GitHub-Aktion.
Der Benutzer übernimmt Pushes und Releases selbst.
