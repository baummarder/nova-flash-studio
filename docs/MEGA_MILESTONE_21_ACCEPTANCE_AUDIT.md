# MEGA MILESTONE 21 — Final Acceptance Audit

**Release Candidate:** `0.17.0-rc1` · **Audited:** working tree @ `a3fda58`
(`main`) · **Basis:** M20 `8bbe2b8` · **Modus:** read-only audit;
keine Commits, kein Push, keine Artefakt-Änderung.

> **Kritischer Fund vorab:** Im committeten M21-Code wurde eine
> **UTF-8→CP1252-Mojibake-Korruption** in 5 Quelldateien festgestellt
> (deutsche UI- und Fehlermeldungen zeigen als `â€"`, `Ã¶` usw.).
> Details in §11/F1. Die kompilierten Artefakte enthalten diese
> Strings ebenfalls.

---

## 1. Git- und Commit-Status — PASS

- Branch: `main` · HEAD: `a3fda58` (M21.10) · `git status`: **clean**
- M21-Historie auf M20-Basis `8bbe2b8`:

| Commit | Subject | Files |
|---|---|---|
| `7230b75` | M21.0 audit + preparation doc | 1 (+121) |
| `855b937` | M21.1–M21.8 `nova-ent` domain crate | 16 (+4239) |
| `9d94791` | M21.9 Enterprise UI + gating | 8 (+1613/−10) |
| `a3fda58` | M21.10 `0.17.0-rc1`, docs, artifacts | 7 (+324/−32) |

Der im Final Report als `HEAD` genannte M21.10-Commit existiert
(`a3fda58`) und gehört zu `0.17.0-rc1` (Cargo.toml-Version im selben
Commit). Alle vier Commits sind lineare Nachfahren von `8bbe2b8` —
keine History-Rewrites.

## 2. Co-Author-Verifikation — PASS (mit Anmerkung)

`git log -1 --format=%B` für alle vier Commits enthält exakt:

```
Co-authored-by: Devin AI <158243242+devin-ai-integration[bot]@users.noreply.github.com>
```

Anmerkung: Die Commit-Subjects beginnen mit einem UTF-8-BOM
(`﻿M21.0: …`), weil die Commit-Nachrichten über `Set-Content
-Encoding UTF8` (BOM) erzeugt wurden. Kosmetisch, kein Blocker —
Empfehlung: künftige Messages ohne BOM schreiben.

## 3. Exakte Testverifikation — PASS

Erneut ausgeführt am Audit-Tree:

| Check | Ergebnis |
|---|---|
| `cargo fmt --all -- --check` | **PASS** (exit 0, keine Diffs) |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** (0 Warnings) |
| `cargo test --workspace` | **484 passed / 0 failed / 8 ignored** (36 Suites) |

Exakte Suite-Summen (Einzelergebnisse): 49+2+4+9+85+34+32+5+4+4+3+2+3+4
+8+3+113+5+33+22+22+13+21+3+1 = **484**; ignored = 3+4+1 = **8**.
Keine Suite mit FAIL.

`nova-ent` allein: **22 Tests, alle grün** (Workspace, Rollen/Matrix,
Registry, Assets, Versionen, Reviews, Builds, Integrationen, Paket,
Audit).

## 4. Edition-Gating — PASS (Code+Tests), GUI: MANUAL CHECK REQUIRED

- `edition.rs`: `administration` nur in `ENTERPRISE_MODULES` aktiv;
  `classroom` nur in `EDU_MODULES`. Gating-Tests asserten alle vier
  Kombinationen (admin×{User,Edu,Ent}, classroom×{Ent,Edu}).
- `app.rs`: Toolbar-Modusliste, `SetMode`-Guard und Central-Dispatch
  prüfen `has_active_module(...)` — dreifach gegated.
- `Mode::Enterprise` vs. `Mode::Classroom` sind disjunkt; ein
  Edition-Build kann den fremden Modus nicht einmal selectable
  darstellen.
- `--startup-check` aller drei Release-EXEs erfolgreich:
  - User → `NØVA Flash Studio 0.17.0-rc1`, Config
    `%APPDATA%\NOVA Flash Studio\`
  - Edu → `NØVA Flash Studio Edu 0.17.0-rc1`, Config
    `%APPDATA%\NOVA Flash Studio Edu\`
  - Enterprise → `NØVA Flash Studio Enterprise 0.17.0-rc1`, Config
    `%APPDATA%\NOVA Flash Studio Enterprise\`
- `.nova`-Format unverändert: `CURRENT_FORMAT_VERSION = 4`
  (`nova-core/src/model/project.rs:24`).

**MANUAL CHECK REQUIRED:** Interaktive GUI (Modus-Sichtbarkeit,
Enterprise-Tab-Nutzung, Klassenzimmer in Edu) wurde nicht per
Maus/Klick verifiziert — nur Code-Pfad, Unit-Tests und
Startup-Check.

## 5. Enterprise-Funktionstests — PASS (über `nova-ent` Unit-Tests)

| Bereich | Geprüft | Ergebnis |
|---|---|---|
| Workspace create/open, safe paths, `org.json`-Marker | tests | PASS |
| Projekt-Register, sha256-Baseline, `.nova`-Pfad-Validierung, Nicht-.nova-Reject | tests | PASS; Datei wird nur gelesen, nie geschrieben/verschoben (Registry speichert Pfad+Hash) |
| 7 Rollen, `default_permissions`, `member_can`, Projekt-Role-Overrides | tests | PASS |
| Asset-Import (Dedup→Version), Suche, Ordner/Tags, Verify, Tamper-Erkennung, Löschung unreferenzierter Blobs | tests | PASS |
| Snapshot→History→Diff→Restore, Safety-Snapshot, Tamper-Reject, Compare-Richtung | tests | PASS |
| Review-State-Machine (illegal Draft→Approved rejected), Reject/Changes ohne Begründung rejected, History, Anker-Kommentare, Orphan-Markierung | tests | PASS |
| Admin-Overview, Audit-Append, `audit_log_for`, CSV-Export (Code: `admin_tab`) | tests+review | PASS |
| Build-Profile CRUD, SWF-Build via `nova_build`, History+sha256 `verify_build_record` | tests | PASS; XFL-Pfad via `nova_formats::export_xfl` (Code-review; Test deckt SWF ab) |
| Integrationen: default disabled, URL-Validierung, Credentials-in-URL-Reject, `api_key_env` nur Name | tests | PASS |

**Explizit dokumentiert (korrekt):** Stage-Anker per Objektklick ist
NICHT implementiert — `ReviewAnchor` wird programmatisch gesetzt,
`anchor_resolves` prüft Auflösung; UI bietet nur Kategorie+Text.
Report §7 und ENTERPRISE.md beschreiben das ehrlich.

## 6. `.novaent`-Sicherheit — PASS

Ausgeführte Tests (Unit + temporärer Audit-Test, danach gelöscht):

| Angriff/Edge | Ergebnis |
|---|---|
| Roundtrip Export→Import (Metadata+Blobs) | PASS |
| `org.json` wird beim Import **nicht** übernommen (lokale Org bleibt) | PASS (verifiziert: Name unverändert) |
| Path Traversal (`../evil.txt`) | PASS — rejected |
| Falsche/fehlende SHA-256-Checksumme | PASS — rejected vor Schreiben |
| Ungültiges JSON-Manifest / kein Manifest | PASS — rejected |
| Garbage-Bytes / leere Datei | PASS — rejected |
| Unbekannte Dateien (`evil.sh`, `nested/deep.txt`) | PASS — Whitelist-Skip, nichts geschrieben |
| Vorhandene Zieldatei | PASS — wird **nicht** überschrieben (Bytes vorher/nachher identisch) |
| Verschlüsselte ZIP-Einträge | Code geprüft (`entry.encrypted()` → reject); **NOT TESTED** (kein AES-ZIP-Builder verfügbar) |
| Symlink-Einträge | Code geprüft: kein Zip-Extract, nur `fs::write` an geweiseten Pfad → Symlink-Attribute wirkungslos; **NOT TESTED** mit echter Symlink-Datei |
| >1 GiB Einträge / Gesamtgröße | Code geprüft (`MAX_TOTAL_BYTES`); **NOT TESTED** (unpragmatisch) |
| Kompressionsverhältnis >100 | Code geprüft; **NOT TESTED** |

Fehlerhafte Pakete hinterlassen keine Teil-Schreibungen: Checksummen
werden vor dem Commit aller Dateien verifiziert; `commit_files`
arbeitet pro Datei, Fehler werden reported (kein stiller Rollback
nötig, da nichts außerhalb der Whitelist schreiben kann).

**Bestätigt:** `.novaent` enthält nur Workspace-Metadaten
(team/projects/registry/assets-index/reviews/build-profiles/
integrations/audit-ausschluss + Blob-Dateien) — **keine** `.nova`-
Projektdateien; registrierte Pfade bleiben absolut/maschinenspezifisch
(ENTERPRISE.md dokumentiert das).

## 7. Release-Artefakte — PASS

`dist/archive/0.17.0-rc1/{user,edu,enterprise}/` — alle 6 Artefakte
vorhanden, Größe + SHA-256 **exakt** wie im Final Report:

| Artefakt | Größe | SHA-256 (verifiziert) |
|---|---|---|
| user `NOVA-User-Portable.zip` | 13.041.896 | `8E99C845…CAE4562` ✓ |
| user `NOVA-User-Setup.exe` | 84.956.901 | `2B940CDD…D3FE5` ✓ |
| edu `NOVA-Edu-Portable.zip` | 13.042.545 | `45CA6292…93EE27` ✓ |
| edu `NOVA-Edu-Setup.exe` | 84.957.407 | `C87E1A01…1DACD7` ✓ |
| enterprise `NOVA-Enterprise-Portable.zip` | 13.043.689 | `67B9C237…C77D46` ✓ |
| enterprise `NOVA-Enterprise-Setup.exe` | 84.958.985 | `421E53C1…9C20A8` ✓ |

- Hashes identisch mit `dist/windows/<edition>/` (Archive-Test PASS:
  „archived ZIP matches current artifact“).
- `scripts/test-edition-packaging.ps1`: **alle PASS**.
- `scripts/test-archive-releases.ps1`: **alle PASS**.
- Jeder Edition-Ordner enthält App-Ordner (EULA, Lizenzen, docs,
  `NØVA Flash Studio{, Edu, Enterprise}.exe`) + `BUILD-INFO.json` +
  `SHA256SUMS.txt`.
- Hinweis: Die gebündelten `docs/` enthalten `ENTERPRISE.md` auch in
  User/Edu-Paketen — Docs sind editionsneutral, kein Funktions-Leak.

## 8. Dokumentationskonsistenz — PASS (mit Anmerkungen)

- Version `0.17.0-rc1`: README, ROADMAP, Cargo.toml, Final Report —
  konsistent.
- ENTERPRISE.md: Rollen-als-Policy, kein Auth, `.novaent` =
  Metadaten-Backup, Integrationen config-only — alles code-konform.
- Final Report: Assertions zu Tests/Hashes/Commits stimmen mit dem
  Audit überein; Limitations-Sektion ehrlich (Stage-Anker-Klick,
  `.novaent`-Pfade, config-only Integrationen).
- **Anmerkung 1:** `CHANGELOG.md` existiert nicht (Repo-Konvention —
  nie vorhanden; kein M21-Defizit).
- **Anmerkung 2:** Report §6 listet M21.10-Commit als „HEAD“ ohne Hash;
  tatsächlich `a3fda58` — korrekt, nur unvollständig zitiert.
- **Anmerkung 3 (F1):** Mojibake-Dokumentation in ENTERPRISE.md und
  Code-Kommentaren teils ebenfalls betroffen? — Nein: Docs sind sauber
  (UTF-8), nur die 5 Source-Dateien in §11 betroffen.

## 9. Manuell offene Tests

- **MANUAL CHECK REQUIRED:** Interaktive GUI aller drei Editionen
  (Fenster, Modus-Tabs, Enterprise-Workspace-Anlage über Dialoge,
  Klassenzimmer in Edu, `.nova`-Öffnen).
- **MANUAL CHECK REQUIRED:** Echte Build-Ausführung SWF+XFL aus der
  Enterprise-UI (Domain-Pfad getestet, UI-Klickpfad nicht).
- **MANUAL CHECK REQUIRED:** `.novaent`-Export/Import über die UI-Buttons.
- Fresh-System-Installer-Lauf (nur Smoke-Test im Packaging-Skript).

## 10. Gefundene Fehler und Risiken

### F1 — KRITISCH: Mojibake-Korruption in committetem M21-Code

5 Dateien enthalten UTF-8→CP1252-korrumpierte deutsche Strings
(`â€"`, `Ã¶`, `Ã¼`, `Â·`, `â†'` usw.) **und** 3 davon ein UTF-8-BOM:

- `crates/nova-app/src/enterprise_view.rs` — praktisch alle
  deutschen UI-Labels, Buttons, Tooltips, Fehlermeldungen
- `crates/nova-ent/src/admin.rs` — Doc-Kommentare (BOM)
- `crates/nova-ent/src/builds.rs` — Fehlermeldung „nötig“→„nÃ¶tig",
  Kommentare (BOM)
- `crates/nova-ent/src/review.rs` — Statuslabels („In PrÃ¼fung“,
  „Ã„nderungen angefordert"), Fehlertexte, Kommentare (BOM)
- `crates/nova-ent/src/versions.rs` — Doc-/Fehlerstrings

Ursache: PowerShell-`Get-Content -Raw | Set-Content -Encoding UTF8`
Round-Trips lasen UTF-8 als CP1252 und schrieben Mojibake zurück
(mein Fehler während M21.9/M21.10-Edits). Kompiliert und getestet
fehlerfrei (gültiges UTF-8), aber **die Enterprise-UI zeigt
korrumpierte Texte** — im kompilierten 0.17.0-rc1-EXE enthalten.
`fmt`/`clippy`/`test` detektieren das nicht.

### F2 — Kosmetisch: BOM in Commit-Subjects

`s﻿M21.x:` Präfix in allen vier M21-Commits (siehe §2).

### F3 — Beobachtung: Archiv-Script-Reihenfolge

`package-editions.ps1` archiviert pro Edition vor dem nächsten
Staging-Swap; die Enterprise-Archivierung für `0.17.0.0-rc1` musste
manuell nachgezogen werden (in M20.8 bereits dokumentiert).
`test-edition-packaging.ps1` fing es diesmal ab — PASS nach
Re-Archiv. Empfehlung: Script so umbauen, dass Archiving nach allen
Editionen läuft.

### F4 — Risiko (niedrig): Paket-Import ohne Transaktion

`commit_files` schreibt Datei für Datei; ein Mitten-Drin-Fehler kann
teilimportierte Whitelist-Dateien hinterlassen (berichtigt, nicht
zurückgerollt). Akzeptabel für Metadaten-Import, dokumentieren.

## 11. Handlungsempfehlungen

1. **Vor Release zwingend:** Mojibake in den 5 Dateien reparieren
   (korrekte UTF-8-Strings wiederherstellen — z. B. betroffene
   String-Literale aus der Diff-Historie/`git show` der sauberen
   Versionen rekonstruieren oder gezielt neu setzen), dann
   `cargo fmt`, Tests, **Rebuild + Re-Package aller drei Editionen**
   und neue SHA-256. Bis dahin sind die aktuellen
   `dist/archive/0.17.0-rc1/`-Artefakte inhaltlich korrekt, aber mit
   defekter Enterprise-UI-Beschriftung.
2. Künftige Text-Edits nicht über PowerShell-`Get-Content`/`Set-Content`
   ausführen (Encoding-Falle); `edit`-Tool oder `-Encoding utf8NoBOM`
   verwenden. Optional: Repo-Hook/Lint auf `â€|Ã[äöüÄÖÜ]`-Muster.
3. Commit-Messages künftig ohne BOM (`git commit -F` mit
   `utf8NoBOM`-Datei oder `-m`).
4. `archive-releases.ps1`-Aufruf in `package-editions.ps1` ans Ende
   verschieben (F3).
5. Optional: `.novaent`-Import transaktional machen (Staging-Dir +
   Rename) — F4.
6. Manuelle GUI-Abnahme der drei Editionen vor finaler Freigabe.

## 12. Abschließende Bewertung

**BLOCK RELEASE** — nicht wegen fehlender Funktionalität (Umfang,
Tests, Gating, Packaging und Artefakte sind convincing), sondern
wegen **F1**: Der Release-Kandidat enthält nachweislich
korrumpierte deutsche Texte in der Enterprise-UI und in
`nova-ent`-Fehlermeldungen — ein sichtbarer Defekt im
Hauptfeature der Edition, zusätzlich in allen drei Binaries
kompiliert. Die Behebung ist klein (5 Dateien re-encoden + Rebuild +
Re-Archiv), danach ist auf Basis dieses Audits ein **ACCEPT WITH
LIMITATIONS** (Rollen=Policy, `.novaent`=Metadaten, Integrationen
config-only, Stage-Anker-Klick offen) gerechtfertigt.
