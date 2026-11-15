# Mega Milestone 20 — Vorbereitung: Education Edition

**Stand:** 15. November 2026
**Basis:** `926e11e` — *M19: introduce three independent NØVA editions — User, Edu, Enterprise*
**Arbeitskopie:** sauber (`git status`: nothing to commit)
**Workspace-Version:** `0.15.0-rc1`
**Formatversion:** `.nova` v4 (unverändert)
**Status dieser Phase:** Analyse und Planung — **keine** Classroom-Implementierung,
keine Änderungen an Git-Konfiguration oder Contributor-Dokumentation.

---

## 0. Ergebnis in Kürze

| Frage | Ergebnis |
|---|---|
| Ist M19 vorhanden? | Ja, vollständig als Commit `926e11e` inkl. Edition-Architektur, Packaging, Tests und Dokumentation. |
| Gibt es bereits Classroom-Code? | Nein. Nur ein reservierter Modul-Slot `classroom` in `nova-app::edition`. |
| Ist die Basis grün? | Tests: **427 passed, 0 failed, 8 ignored** (32 Suites); `cargo fmt --check` sauber. |
| Ist CI grün? | **Nein.** `cargo clippy --workspace --all-targets -- -D warnings` schlägt lokal fehl (mind. `nova-swf`, `nova-as`, `nova-core`). |
| Empfohlene M20-Architektur | Neue Crate `nova-edu` (Domänenlogik, Paketformate, Speicher) + Edition-gesteuerter `Mode::Classroom` in `nova-app`. `.nova` bleibt unverändert. |
| Empfohlene M20-Struktur | 9 Teil-Milestones `M20.0`–`M20.8`, davon `M20.0` als Baseline-/Hygiene-Sprint. |
| OpenCode/DeepSeek als Co-Author? | **Noch nicht verifiziert.** Nur Devin besitzt eine belegte Identität. Keine erfundenen Trailer, keine Config-Änderung. |
| Nächster Schritt | Identitäten klären + `M20.0` freigeben. |

---

## 1. Prüfmethodik

Ausgeführt in dieser Phase (nur lesend bzw. testend):

```text
git status / git log / git show / git config --local --list / git remote -v
cargo test --workspace            -> 427 passed, 0 failed, 8 ignored, 32 Suites
cargo test --workspace --quiet    -> Exit 0
cargo fmt --all -- --check        -> Exit 0
cargo clippy --workspace --all-targets -- -D warnings
                                  -> Exit 101 (CI-Gate rot)
```

Zusätzlich gelesen: `AGENTS.md`, `README.md`, `docs/EDITIONS.md`,
`docs/EDITION_ARCHITECTURE.md`, `docs/ARCHITECTURE.md`,
`docs/CAPABILITY_AUDIT.md`, `docs/MEGA_MILESTONE_19_REPORT.md`,
`docs/M19_TEST_REPORT.md`, `docs/PACKAGING.md`, `docs/ROADMAP.md`,
`docs/TESTING.md`, `docs/RELEASE_CHECKLIST.md`, alle `Cargo.toml`-Dateien,
die Edition-/State-/App-/Autosave-/Flash-/CLI-Quellen, Packaging-Skripte,
GitHub-Workflows, Installer-Quellen und die vorhandenen Test-Suiten.

Nicht verifizierbar in dieser Umgebung: echte Installation der drei Setup-EXEs,
Adobe-Animate-/Projector-Playback, Ruffle-Läufe, macOS-/Linux-Builds,
zwei getrennte Rechner für einen Offline-Klassen-Workflow.

---

## 2. Repository- und Architekturübersicht

### 2.1 Historie und Git-Status

- 34 Commits, letzter: `926e11e` (M19), davor `d99960f` (M18, `0.14.0-rc1`).
- Remote: `https://github.com/baummarder/nova-flash-studio.git` (öffentlich),
  Branch `main` ist auf dem Stand von `origin/main`.
- **Keine Git-Tags**, keine GitHub-Releases.
- Arbeitskopie sauber; `dist/` und `packaging/**/bin|obj` sind ignoriert.
- Release-Archive vorhanden: `dist/archive/0.3.0-rc2` … `0.15.0-rc1`.
  **Lücke:** `dist/archive/0.15.0-rc1/` enthält `user/` und `edu/`, aber
  **nicht** `enterprise/`, obwohl `dist/windows/enterprise/` gebaut wurde
  (BUILD-INFO: 2026-11-14T23:37Z). Release-Hygiene für M20.0.

### 2.2 Crates

| Crate | Rolle | Für M20 relevant |
|---|---|---|
| `nova-core` | Dokumentmodell (`Project` v4), Validierung, JSON-Persistenz, Templates, Timeline-Auflösung, Spiel-/Tilemap-Modell, **`Edition`** | `Edition`, `ProjectTemplate`, Speicher-/ID-Muster |
| `nova-swf` | Bit-Writer, Shapes, Tags, Movie, unabhängiger Inspector | unverändert |
| `nova-build` | `Project` → SWF, AS3-Kompilierung, Symbol-/Bitmap-/Audio-Baking | unverändert |
| `nova-as` | AS3-Lexer/Parser/Typecheck/ABC-Emitter | unverändert |
| `nova-script` | QuickJS-Host | unverändert |
| `nova-formats` | Import/Export `.nova`/SWF/XFL/FLA | Paket-Import darf **nicht** hier landen (eigene Domäne) |
| `nova-ai` | Provider, DPAPI-Credentials, Kontext, Diff, sichere Dateioperationen | Muster für Root-Confinement + atomare Writes wiederverwenden |
| `nova-app` | egui/eframe-UI, Zustand, Aktionen, Dialoge, Datei-/Export-Flows | Classroom-Modus, Rollen-UI, Review-Modus |
| `nova-cli` | headless `nova`-Binary (build/check/probe/…) | optionale Admin-/Lehrer-Werkzeuge |

Kennzahlen: 113 Rust-Dateien, ~2,4 MB Quelltext; `nova-app/src/app.rs`
allein ~190 KB. **M20-UI darf `app.rs` nicht weiter aufblähen** — neue Module
sind Pflicht.

### 2.3 Datenfluss

```text
UI-Widgets -> Vec<UiAction> -> NovaApp::handle_actions -> Project-Mutation
                                              |
                                              +-> History (Snapshots, Undo/Redo)
                                              +-> Autosave (atomar)
                                              +-> Export (nova-build)

Dokument: Project (JSON, v4) = einzige Wahrheit für Szene/Timeline/Assets.
UI-Zustand: AppState = Auswahl, Frame, Kamera, Dialoge, Tool, Zwischenablage.
```

Wichtig für M20: **Classroom-Zustand darf nicht Teil des `Project`-Snapshots
werden.** Sonst würden Undo/Redo, Autosave und der `.nova`-Roundtrip
Classroom-Daten mitschleppen und die Editionsneutralität verletzen.

### 2.4 Persistenz und Verzeichnisse

- Projekt: `<name>.nova` (JSON, `formatVersion: 4`, `FORMAT_TAG`), atomarer
  Write in `Project::save` (tmp + remove + rename; kleines Fenster ohne
  atomaren Replace — Kandidat für M20.0-Härtung, aber nicht M20-Kern).
- Konfiguration pro Edition über `nova_core::Edition::config_dir()`:
  Windows `%APPDATA%\NOVA Flash Studio{, Edu, Enterprise}`, macOS analog,
  Linux `$XDG_CONFIG_HOME/nova-flash-studio-<edition>`.
- Überschreibungen: `NOVA_FLASH_STUDIO_CONFIG_DIR` (gesamtes Config-Dir),
  `NOVA_EDITION` (Compile-time gebacken, Runtime nur für Dev-Builds).
- Autosave/Recovery: `%TEMP%\nova-flash-studio-<edition>` bzw.
  `<projekt>.nova.autosave`.
- AI-Provider-Settings liegen im Editions-Config-Dir; Credentials via DPAPI.
- **Es gibt kein Schul-, Klassen-, Aufgaben- oder Abgabeverzeichnis.**

### 2.5 UI-Architektur (Anknüpfungspunkte)

| Ort | Inhalt | M20-Relevanz |
|---|---|---|
| `state.rs:14` | `Mode { Code, Animation }` | dritter Modus `Classroom` |
| `state.rs:30` | `BottomTab { Timeline, Tiles, Console, Problems }` | optionaler Classroom-Tab |
| `state.rs:180` | `Selection` | Classroom-Auswahl separat halten |
| `state.rs:367` | `Dialog` | neue Edu-Dialoge |
| `state.rs:430` | `UiAction` | neue `Edu*`-Aktionen |
| `state.rs:509` | `AppState` | `edu: EduState` ergänzen |
| `app.rs:172` | Menüleiste | Edu-Menü nur bei aktivem Modul |
| `app.rs:3414` | „Neues Projekt“-Dialog (nutzt `ProjectTemplate::ALL`) | Lehrvorlagen **nicht** hier einhängen |
| `app.rs:3554/3565/3644` | About-/Hilfe-Texte | **veraltet** (siehe 3.4) |
| `app.rs:4380` | `eframe::App::ui` | Classroom-View einhängen |
| `tree.rs` | Projektbaum | unverändert |
| `stage_view.rs` | Bühne, Auswahl, Drag | Kommentar-Overlay andocken |
| `autosave.rs` | atomare Snapshots | Muster für `nova-edu::store` |

### 2.6 Packaging und CI

- `package-windows.ps1 -Edition <id>` setzt `NOVA_EDITION`, baut Release,
  staged `dist/windows/<edition>/`, prüft ProductName/Version/`--version`/
  `--startup-check`, schreibt ZIP, `SHA256SUMS.txt`, `BUILD-INFO.json`.
- `package-installer.ps1 -Edition <id>` regeneriert `EditionInfo.cs`
  (Produktname, Exe, Uninstall-Key, ProgIds), baut das WinForms-Setup und
  stellt die Datei danach zurück.
- `package-editions.ps1` orchestriert alle drei Editionen.
- CI (`rust.yml`): `fmt --check`, `cargo check --workspace --all-targets`,
  **`clippy … -D warnings`**, Release-Build, `cargo test --workspace`.
- Release-Workflow baut Windows/macOS/Linux **ohne** Editionsmatrix;
  macOS/Linux-Packaging ist weiterhin nicht editioniert.

### 2.7 Tests

| Suite | Ergebnis |
|---|---|
| Gesamt | 427 passed / 0 failed / 8 ignored |
| Edition (core unit + integration) | 4 + 4 |
| `nova-app` Unit | 49 |
| `nova-core` Unit | 113 |
| `nova-formats` Unit | 22 |
| `nova-swf` Unit + unabhängiger Parser | 21 + 3 |
| `nova-script` Unit + Doc | 13 + 1 |
| Übrige Integrations-Suiten (M8–M18) | grün |
| Ignoriert | 8 (Perf/Live-Provider) |

---

## 3. M19-Bestandsaufnahme

### 3.1 In M19 implementiert

| Bereich | Umsetzung |
|---|---|
| Edition-Modell | `nova_core::Edition { User, Edu, Enterprise }`, `ALL`, `id`, `product_name`, `artifact_slug`, `config_dir_name`, `recovery_dir_name`, `parse`, `current` |
| Auflösung | Compile-time `option_env!("NOVA_EDITION")` → Runtime-Env → `User` |
| Konfiguration | editionsgetrennte Config-/Recovery-Verzeichnisse; User behält Legacy-Pfad; `NOVA_FLASH_STUDIO_CONFIG_DIR` |
| Profil/Registry | `nova-app::edition::{EditionProfile, EditionModule}`, `profile()`, `active_modules()`; Edu-Slot `classroom` (`reserved: true`), Enterprise-Slot `administration` |
| Branding | Fenstertitel, App-ID, Splash-Tagline/Audience, About, `build.rs`-Versionsresource |
| Neue-Projekt-Dialog | Hinweis auf reserviertes Modul der Edition |
| AI | Provider-Konfiguration automatisch pro Edition (Config-Dir-Delegation) |
| Projekte | `.nova` **ohne** Editionsfeld; byte-identischer Roundtrip; Cross-Edition-Tests |
| Packaging | 3 Editionen × (Portable-ZIP + Setup-EXE); eigene Uninstall-Keys/ProgIds/Ordner |
| Doku | `EDITIONS.md`, `EDITION_ARCHITECTURE.md`, M19-Report, Test-Report, README/Capability-Audit |
| Tests | Identität, Parse, Legacy-Pfad, Config-Override, keine Editionsmarker, isolierte Dirs, Profilvollständigkeit, genau 1 reservierter Slot |

### 3.2 Ausdrücklich **nicht** in M19

- Keine Classroom-Funktion (nur der reservierte Registry-Slot).
- Keine Rollen, Profile, Klassen, Gruppen, Aufgaben, Abgaben, Feedback,
  Fortschritt, Lehrvorlagen, Tutorials, Schulkonfiguration.
- Kein Lizenz-/DRM-/Aktivierungssystem (bewusst, bleibt so).
- Keine macOS-/Linux-Editionspakete.

### 3.3 Vorhandene M20-Grundlagen (wiederverwendbar)

1. **Editionsauflösung + getrennte Config-Dirs** — Edu-Daten können ohne
   neue Isolationsmechanik in `...\NOVA Flash Studio Edu\edu\` liegen.
2. **`EditionProfile.modules`** — genau der vorgesehene Registrierungspunkt
   für das Classroom-Modul.
3. **Stabile Projekt-IDs** — `ProjectId`, `SceneId`, `LayerId`, Objekt-`Uuid`
   existieren bereits und taugen als Feedback-Anker.
4. **Atomare Speicher-Muster** — `autosave.rs` und `nova-ai::files` liefern
   Temp-Datei+Replace bzw. Root-Confinement.
5. **`zip`-Infrastruktur** — bereits Workspace-Abhängigkeit (XFL/FLA);
   Import-Validierung (Pfad-Traversal, Größenlimits) existiert in
   `nova-formats` als Muster.
6. **UI-Intent-Muster** — `Vec<UiAction>` + zentraler Dispatch ist ideal für
   testbare Edu-Aktionen.
7. **Vier echte Projektvorlagen** — als Starter für Lehrvorlagen nutzbar.
8. **Deutsche UI-Konsistenz** — Edu-UI kann nahtlos deutsch bleiben.
9. **Installer-/Payload-Pipeline** — Edu-Inhalte (Vorlagen/Tutorials) können
   ohne neue Editionsmechanik mit ausgeliefert werden.
10. **Test-/Doku-Kultur** — jede Milestone liefert Report + ehrliche
    Nicht-verifiziert-Liste.

### 3.4 Konkrete Lücken (Bestandsaufnahme)

| M20-Bedarf | Ist-Zustand | Fehlt |
|---|---|---|
| Classroom Mode | `Mode` hat 2 Varianten | dritter Modus, Dashboard, Navigation |
| Lehrer-/Schülerrollen | nur Edition, keine Identität | lokales Profil, Rolle, Rollenwechsel, Review-Sperre |
| Klassen/Gruppen | nichts | Modell, Speicher, CRUD, Mitglieder |
| Aufgaben | `ProjectTemplate` ist geschlossenes Enum (4) | Assignment-Modell, Rubrik, Fälligkeit, Anweisungen |
| Starterprojekte | `.nova` öffnen/speichern | Verteilpaket, Manifest, Prüfsummen |
| Abgaben | nichts | Export/Import, Inbox, Duplikate, Status |
| Review | kein Read-only-Modus | schreibgeschützte Sitzung + „Kopie speichern“ |
| Feedback | nichts | Anker, Kommentare, Threads, Paket |
| Lernfortschritt | nichts | Status, Rubrikbewertung, Übersicht |
| Lehrvorlagen/Tutorials | 4 Compile-time-Templates | Katalog, Tutorial-Viewer, Unterrichtsmaterial |
| Offline-Betrieb | App ist offline-fähig | keine Classroom-Flows, kein Offline-Abnahmetest |
| Paket-I/O | ZIP nur für XFL/FLA | eigenes Paketformat + gehärtete Extraktion |
| Schulkonfiguration | nur Editions-Config | `school.json`, Policy, Präzedenz |
| Berichte | Exportbericht (Kompatibilität) | Klassen-/Fortschrittsberichte (CSV/MD/HTML) |
| CLI | `nova` ohne Edu-Befehle | optionale `nova edu …`-Befehle |

**Dokumentationsschulden, die M20 direkt treffen:**

- In-App-Hilfe und About behaupten weiterhin, Bitmaps, Sounds, verschachtelte
  Symbol-Timelines, Masken und Shape-Tweens fehlten (`app.rs:3565`, `:3644`).
  Das ist seit M9/M12 falsch und für Unterrichtsmaterial besonders schädlich.
- `docs/ROADMAP.md` endet inhaltlich bei M4/M5 und erwähnt M19 nicht.
- `README.md:261` nennt weiterhin `0.7.1-rc1` als aktuellen Windows-Build.
- `docs/PACKAGING.md` und `docs/RELEASE_CHECKLIST.md` enthalten **keinen**
  Editionsabschnitt.
- `--help` (`main.rs:86`) erwähnt `NOVA_EDITION` nicht.
- `crates/nova-core/tests/editions.rs:24` verbietet die Substrings
  `"user"`, `"edu"`, `"enterprise"` irgendwo im Projekt-JSON. Das ist als
  Schutz gedacht, aber fragil (ein Schülername „Edu“ würde den Test brechen).
  Für M20 präzisieren, ohne die Aussage abzuschwächen.

---

## 4. Vorgeschlagene M20-Architektur

### 4.1 Leitprinzipien

1. **Eine Codebasis, keine Forks.** Classroom ist ein Modul, keine Edition
   mit eigenem Codebaum.
2. **`.nova` bleibt unangetastet.** Format v4, kein Editionsfeld, keine
   Classroom-Metadaten im Projekt. Damit bleiben alle Kompatibilitäts- und
   Roundtrip-Tests gültig.
3. **Offline-first.** Kein Server, kein Account, keine Telemetrie, keine
   Lizenzschlüssel. Austausch ausschließlich über Dateien/Ordner.
4. **Rollen sind Komfort, keine Sicherheit.** Ohne Konten gibt es keine
   Authentifizierung; die UI und die Doku müssen das klar sagen.
5. **Edu darf User nicht einschränken.** User-Editions-Binary und
   `.nova`-Verhalten bleiben funktional identisch.
6. **Schreibpfade sind explizit und konfiniert.** Jede Dateioperation läuft
   über validierte relative Pfade innerhalb eines Workspace-Roots.

### 4.2 Schichtenmodell

```text
┌──────────────────────────────────────────────────────────────┐
│ nova-app                                                     │
│  edition.rs     Registry: classroom wird aktiv (Edu)         │
│  edu_state.rs   Rolle, Workspace, Auswahl, Import-Queue      │
│  classroom_view.rs / edu_dialogs.rs                         │
│  stage_view.rs  Kommentar-Overlay (nur lesend)               │
├──────────────────────────────────────────────────────────────┤
│ nova-edu  (NEU — Domänenlogik, keine UI, kein Netz)          │
│  workspace, school, classroom, assignment, submission,       │
│  feedback, progress, package, store, paths, report, error    │
├──────────────────────────────────────────────────────────────┤
│ nova-core  (unverändert: Project v4, Edition, IDs)           │
│ nova-formats / nova-build / nova-swf / nova-as / nova-ai     │
└──────────────────────────────────────────────────────────────┘
```

`nova-edu` hängt nur von `nova-core` (und technisch `serde`, `serde_json`,
`uuid`, `thiserror`, `zip`, `sha2`) ab. Es importiert **keine** UI- oder
Netzwerkbibliothek. Damit ist die gesamte Classroom-Logik headless testbar.

### 4.3 Modulzuschnitt `crates/nova-edu/src/`

| Modul | Inhalt |
|---|---|
| `lib.rs` | Re-Exports, `EDU_SCHEMA_VERSION` |
| `error.rs` | `EduError` (IO, Schema, Paket, Pfad, Integrität, Konflikt) |
| `paths.rs` | Root-Confinement, sichere relative Pfade, Dateinamen-Sanitizing |
| `store.rs` | atomare JSON-Writes, Lese-/Migrationshelfer, Advisory-Lock |
| `workspace.rs` | `Workspace`, Layout, Öffnen/Anlegen, Recent-Liste |
| `school.rs` | `SchoolConfig`, `SchoolPolicy`, Präzedenz, Validierung |
| `profile.rs` | `EduRole`, `LocalProfile` (Anzeigename/Pseudonym) |
| `classroom.rs` | `Classroom`, `Group`, `Member`, CRUD-Invarianten |
| `assignment.rs` | `Assignment`, `RubricCriterion`, `StarterRef`, Fälligkeit |
| `submission.rs` | `Submission`, Status, Revisionen, Duplikaterkennung |
| `feedback.rs` | `Comment`, `ProjectAnchor`, Kategorien, Auflösung |
| `progress.rs` | `ProgressEntry`, Rubrikstufen, Aggregation |
| `package.rs` | Manifest, ZIP-Write/Read, Prüfsummen, Limits, `verify` |
| `report.rs` | CSV/Markdown/HTML-Export (ohne externe Template-Engine) |
| `template.rs` | Lehrvorlagen-Katalog + Tutorial-Metadaten |
| `tests/` | Roundtrips, Migration, Zip-Slip, Limits, Anker, Reports |

### 4.4 Edition-Gating

- `EDU_MODULES`: `classroom` wird von `reserved: true` auf `reserved: false`
  gesetzt. Optional zusätzlich `assignments`/`progress` als eigene aktive
  Module — empfohlen ist **ein** Modul `classroom` plus interne
  `EduFeature`-Enum, damit die Registry nicht zum Feature-Flag-Friedhof wird.
- `CORE_MODULES` und `ENTERPRISE_MODULES` bleiben unverändert.
- Der Test `reserved_modules_are_marked_not_implemented` muss angepasst
  werden: Edu hat nach M20 **null** reservierte Module, Enterprise weiterhin
  genau eines. Zusätzlich neuer Test: **User enthält kein `classroom`**.
- `Edition::current()` bleibt Compile-time gebunden. Classroom-Logik nimmt
  `Edition`/Pfade als Parameter, damit Tests nicht auf Prozess-Env
  angewiesen sind (die bestehenden Env-Tests sind `unsafe` und global).
- UI-Zugriff ausschließlich über `profile().modules`:
  ```rust
  pub fn has_active_module(id: &str) -> bool {
      profile().modules.iter().any(|m| m.id == id && !m.reserved)
  }
  ```

### 4.5 Datenmodell (Skizze)

```rust
pub const EDU_SCHEMA_VERSION: u32 = 1;

pub enum EduRole { Teacher, Student, Assistant }

pub struct LocalProfile {
    pub id: Uuid,
    pub display_name: String,
    pub role: EduRole,
    /// Nur Anzeige; keine Authentifizierung, kein Passwort.
    pub pseudonym: Option<String>,
}

pub struct Classroom {
    pub id: Uuid,
    pub name: String,
    pub subject: String,
    pub term: String,
    pub groups: Vec<Group>,
    pub members: Vec<Member>,
}

pub struct Assignment {
    pub id: Uuid,
    pub classroom_id: Uuid,
    pub title: String,
    pub instructions_md: String,
    pub starter: StarterRef,          // Datei im Paket + SHA-256
    pub rubric: Vec<RubricCriterion>,
    pub due_unix: Option<u64>,
    pub allow_late: bool,
    pub required_exports: Vec<ExportKind>,
    pub tutorial: Option<String>,     // relativer Pfad im Paket
}

pub struct Submission {
    pub id: Uuid,
    pub assignment_id: Uuid,
    pub member_id: Uuid,
    pub revision: u32,
    pub submitted_unix: u64,
    pub project: String,              // relativer Pfad
    pub checksums: BTreeMap<String, String>,
    pub note: Option<String>,
    pub state: SubmissionState,       // Draft|Submitted|Returned|Reviewed|Graded
}

pub struct Comment {
    pub id: Uuid,
    pub author: AuthorRef,            // lokaler Anzeigename + Rolle
    pub created_unix: u64,
    pub anchor: Option<ProjectAnchor>,
    pub category: CommentCategory,    // Praise|Issue|Question|Requirement
    pub text: String,
    pub resolved: bool,
}

pub struct ProjectAnchor {
    pub scene_id: Uuid,
    pub layer_id: Option<Uuid>,
    pub object_id: Option<Uuid>,
    pub frame: Option<usize>,
    pub label: Option<String>,        // Fallback für Menschen
}
```

**Kein** Feld davon liegt in `Project`. Kommentare und Fortschritt sind
Sidecar-Daten; das Projekt bleibt ein reines `.nova` v4.

### 4.6 Speicherlayout und Präzedenz

```text
<config_dir>/edu/                    %APPDATA%\NOVA Flash Studio Edu\edu
  profile.json                       lokale Rolle + Anzeigename
  recent.json                        zuletzt geöffnete Workspaces
  prefs.json                         lokale UI-/Berichtsoptionen

<workspace>/                         Schul-/USB-/Netzlaufwerk
  school.json                        schulweite Konfiguration (optional)
  classrooms/<class-id>/classroom.json
  assignments/<assignment-id>/assignment.json
  inbox/<class-id>/<assignment-id>/<submission-id>/
  feedback/<class-id>/<assignment-id>/<member-id>.json
  templates/catalog.json + *.nova
  tutorials/*.md
  reports/
```

Präzedenz (niedrig → hoch):

1. eingebaute Defaults,
2. `school.json` (schulweit, gesperrte Felder),
3. Workspace-/Lehrer-Overrides,
4. lokale `prefs.json`.

`school.json` (Beispiel):

```json
{
  "schema": 1,
  "school": "Beispielschule",
  "workspaceRoot": ".",
  "defaultLocale": "de",
  "templateCatalog": "templates/catalog.json",
  "policy": {
    "allowCloudAi": false,
    "allowSwfExport": true,
    "allowCustomStarter": true,
    "retentionDays": null
  },
  "gradeScale": [
    { "id": "ok", "label": "erfüllt" },
    { "id": "partly", "label": "teilweise" },
    { "id": "open", "label": "offen" }
  ]
}
```

Verteilung: Datei kopieren (USB, Netzlaufwerk, GPO/Login-Skript). Kein
Dienst, kein Server, keine Registry außer dem Installer. Unbekannte Felder
werden beim Lesen ignoriert, beim Schreiben erhalten (Vorwärtskompatibilität).

### 4.7 Paketformate

| Endung | Richtung | Inhalt |
|---|---|---|
| `.novaassignment` | Lehrkraft → Lernende | `manifest.json`, `assignment.json`, `starter/*.nova`, `tutorial/*.md`, `checksums.sha256` |
| `.novasubmission` | Lernende → Lehrkraft | `manifest.json`, `submission.json`, `project/*.nova`, optionale Exporte, `checksums.sha256` |
| `.novafeedback` | Lehrkraft → Lernende | `manifest.json`, `feedback/*.json`, optional `report.md` |
| `.novaclassroom` | Backup/Restore | kompletter Klassen-/Aufgabenstand inkl. Inbox und Feedback |

Manifest (einheitlich):

```json
{
  "format": "nova-edu-assignment",
  "schema": 1,
  "id": "0f0c…",
  "createdUnix": 1760000000,
  "createdBy": { "displayName": "Lehrkraft", "role": "teacher" },
  "content": {
    "assignment": "assignment.json",
    "starter": "starter/level-1.nova",
    "tutorial": "tutorial/schritt-1.md"
  },
  "checksums": { "assignment.json": "sha256:…", "starter/level-1.nova": "sha256:…" }
}
```

Härtung beim Lesen (verbindlich):

- nur relative Pfade mit `/`, keine `..`, keine Laufwerks-/UNC-Präfixe,
  keine absoluten Pfade, keine Symlinks;
- Limits: max. 10 000 Einträge, max. 1 GiB entpackt, Kompressionsverhältnis
  ≤ 100:1, keine verschlüsselten Einträge;
- Prüfsummen **vor** dem Schreiben verifizieren;
- in temporäres Verzeichnis entpacken, validieren, dann atomar verschieben;
- Schema-Major prüfen; neueres Major ⇒ klare Fehlermeldung, kein Teilimport;
- `.nova` nach dem Import mit `Project::load` + `validate` prüfen;
- niemals Inhalte aus Paketen ausführen.

### 4.8 Anker und Feedback

- Primär: `object_id` (stabil über `.nova`-Saves).
- Sekundär: `layer_id` + `frame`.
- Tertiär: `scene_id` + `label` (menschenlesbar).
- Beim Öffnen: ID zuerst, dann Frame/Label; nicht auflösbare Anker werden
  sichtbar als „verwaist“ markiert, **nicht** gelöscht.
- Feedback verändert das Projekt nie. „Zum Kommentar springen“ setzt Szene,
  Frame und Auswahl; das ist reine UI-Navigation.
- Kommentare werden sofort atomar persistiert (kein Undo-Eintrag), weil sie
  fachlich keine Projektänderung sind. Das muss in der Doku stehen.

### 4.9 Fortschritt und Bewertung

- Status je Aufgabe/Person: `NotStarted`, `InProgress`, `Submitted`,
  `Reviewed`, `Mastered` (oder schulkonfigurierte Skala).
- Rubrik: Kriterien mit Stufen; Bewertung durch Lehrkraft; optional
  Selbsteinschätzung der Lernenden im Abgabepaket.
- **Keine** automatische Verhaltens-/Zeitmessung, keine Telemetrie.
- Berichte: CSV (RFC-4180-konform escaped), Markdown und statisches HTML
  ohne externe Assets; deterministische Sortierung.

### 4.10 UI-Integration

- `Mode::Classroom` nur sichtbar, wenn `has_active_module("classroom")`.
- Neue Dateien: `edu_state.rs`, `classroom_view.rs`, `edu_dialogs.rs`;
  `app.rs` erhält nur Dispatch-Zweige.
- Navigation: Klassen · Aufgaben · Abgaben · Feedback · Fortschritt ·
  Vorlagen/Tutorials.
- Rollenwahl beim ersten Öffnen des Classroom-Modus (Lehrkraft/Lernende),
  jederzeit wechselbar; keine Passwörter.
- Read-only-Review: Öffnen einer Abgabe in einer Sitzung mit deaktiviertem
  Speichern/Exportieren (Ausnahme: „Kopie speichern unter“), sichtbarer
  Banner, keine stillen Änderungen.
- Kommentar-Overlay auf der Bühne: Marker je Anker, Klick springt zur
  Stelle; im Projektbaum ein Feedback-Panel.
- Alle Edu-Aktionen laufen über `UiAction::Edu*` und sind damit testbar.

### 4.11 CLI (optional, nicht Release-kritisch)

`nova edu status | provision | verify | export-assignment | import-submissions
| report` als headless Ergänzung für Admins/CI. Voraussetzung: `nova.exe`
müsste mit ausgeliefert werden; aktuell packt `package-windows.ps1` nur die
GUI-Exe. Empfehlung: GUI-first; CLI in M20.8 optional in den Edu-Payload.

### 4.12 Sicherheit und Datenschutz

- **Keine Authentifizierung.** Rollen sind eine lokale Auswahl; jede Person
  mit Dateizugriff kann Abgaben lesen/ändern. In UI und Doku explizit nennen.
- **Datenminimierung:** Anzeigename oder Pseudonym, keine Geburtsdaten,
  keine E-Mail-Pflicht, keine Konten.
- **Löschfunktionen:** Klasse/Aufgabe/Abgabe endgültig löschen, inkl.
  zugehöriger Sidecars; Bestätigungsdialog mit Anzahl betroffener Dateien.
- **Retention:** optionales `retentionDays` in `school.json`; Hinweis statt
  automatischem Löschen (kein Datenverlust ohne Bestätigung).
- **Netzwerk:** Classroom-Pfade führen keine HTTP-Aufrufe aus. KI bleibt
  opt-in und kann per Schulpolicy deaktiviert werden (Konfiguration, kein
  DRM).
- **Netzlaufwerke:** keine Live-Sperren; Empfehlung „pro Person ein Ordner,
  Import als Kopie“. Advisory-Lock nur für den Import, nie für offene
  Projektdateien.

### 4.13 Kompatibilitätsgarantien

1. `CURRENT_FORMAT_VERSION` bleibt **4**; keine neuen Projektfelder.
2. `crates/nova-core/tests/editions.rs` bleibt inhaltlich bestehen
   (Projekt-JSON ohne Editionsmarker, byte-identischer Roundtrip).
3. Neuer Test: User-Edition hat kein `classroom`-Modul und keine
   Edu-Menüpunkte.
4. Neuer Test: eine in Edu gespeicherte `.nova` lädt in User/Enterprise
   unverändert (und umgekehrt).
5. Classroom-Pakete sind **kein** Ersatz für `.nova`; die enthaltenen
   Projekte sind normale `.nova`-Dateien.

---

## 5. Feature-Mapping (12 Anforderungen)

| # | Anforderung | Architektur | Milestone |
|---|---|---|---|
| 1 | Classroom Mode | `Mode::Classroom` + Dashboard, Edu-Gating | M20.7 |
| 2 | Lehrer-/Schülerrollen | `EduRole`, `LocalProfile`, Rollenwechsel | M20.1 |
| 3 | Klassen/Gruppen | `Classroom`, `Group`, `Member`, CRUD | M20.2 |
| 4 | Aufgabenverwaltung | `Assignment`, Rubrik, Fälligkeit, Anweisungen | M20.3 |
| 5 | Starterprojekte | `StarterRef`, `.novaassignment` | M20.3 |
| 6 | Projektabgaben | `Submission`, Inbox, Read-only-Review | M20.4 |
| 7 | Feedback/Kommentare | `Comment`, `ProjectAnchor`, `.novafeedback` | M20.5 |
| 8 | Lernfortschritt | `ProgressEntry`, Rubrik, Berichte | M20.6 |
| 9 | Vorlagen/Tutorials | Katalog + Markdown-Viewer, Lehrvorlagen | M20.7 |
| 10 | Offline-Schulbetrieb | dateibasierte Flows, Offline-Abnahme | M20.8 (querschnittlich) |
| 11 | Klassenpaket-Import/-Export | `package.rs`, 4 Pakettypen | M20.3–M20.5, M20.8 |
| 12 | Schulweite Konfiguration | `SchoolConfig`, Präzedenz, Provisioning | M20.1 + M20.8 |

---

## 6. Milestone-Aufteilung

### M20.0 — Baseline, CI und Doku-Ehrlichkeit

**Ziel:** grüne, vertrauenswürdige Basis vor jeder Classroom-Funktion.

- Clippy `-D warnings` reparieren (mind. `nova-swf` 3, `nova-as` 15,
  `nova-core` 22/29 Fehler; danach gesamten Workspace prüfen).
- CI um `cargo test -p nova-edu` erweitern (sobald die Crate existiert).
- Veraltete In-App-Texte korrigieren (`app.rs:3565`, `:3644`): Bitmaps,
  Sounds, verschachtelte Symbol-Timelines, Masken und Shape-Tweens sind
  vorhanden; Grenzen präzise neu formulieren.
- `docs/ROADMAP.md` auf M19/M20 aktualisieren; `README.md:261` und
  `docs/PACKAGING.md`/`RELEASE_CHECKLIST.md` editionsfähig machen.
- `0.15.0-rc1`-Archiv um `enterprise/` ergänzen oder die Lücke dokumentieren.
- `editions.rs`-Test präzisieren (kein Editionsfeld statt Substring-Suche).
- Optional: `Project::save` auf atomaren Replace härten.

**Akzeptanz:** `fmt`, `check`, `clippy -D warnings`, `build --release`,
`test --workspace` grün; Doku beschreibt den tatsächlichen Funktionsstand;
keine Verhaltensänderung an User/Edu/Enterprise-Funktionen.

### M20.1 — `nova-edu`-Fundament, Workspace, Rollen, Schulkonfiguration

**Ziel:** tragfähiges Domänenfundament ohne UI-Ausbau.

- Crate `nova-edu` anlegen (Module aus 4.3), Workspace-Create/Open,
  Schema-Versionierung, Migrationen.
- `SchoolConfig` + Policy-Präzedenz + Validierung.
- `LocalProfile`/`EduRole`; Rollenwahl-Dialog (lokal, ohne Auth).
- `EditionProfile`: `classroom` aktivieren; User-/Enterprise-Tests anpassen.
- Atomare JSON-Stores, Root-Confinement, Advisory-Lock für Importe.

**Akzeptanz:** Unit-Tests für Roundtrip, Migration, Pfad-Escapes,
Präzedenz; App startet in Edu mit Rollenwahl; User-Build unverändert;
`.nova` v4 unverändert.

### M20.2 — Klassen, Gruppen und Teilnehmerlisten

- `Classroom`/`Group`/`Member`-CRUD, Umbenennen, Archivieren/Löschen.
- Mitglieder ohne Konten (Anzeigename/Pseudonym), Gruppenzuordnung.
- Import/Export einer Teilnehmerliste als JSON (keine PII-Pflichtfelder).
- Klassen-Dashboard (leere Zustände, Zähler).

**Akzeptanz:** Persistenz über Neustart; doppelte Namen werden behandelt;
Löschen entfernt nur zugehörige Sidecars; keine Netzwerkaufrufe.

### M20.3 — Aufgaben und Starterprojekte

- `Assignment` inkl. Rubrik, Anweisungen (Markdown), Fälligkeit, Exportpflicht.
- Starter aus `.nova` oder Lehrvorlagen-Katalog; Prüfsumme im Manifest.
- `.novaassignment` schreiben/lesen/verifizieren; Import in Lernenden-Workspace.
- Aufgabenliste je Klasse, Status „offen/verteilt“.

**Akzeptanz:** Paket-Roundtrip; manipulierte Prüfsumme wird abgelehnt;
Zip-Slip/Bomb-Tests; Starter öffnet in allen Editionen als normales `.nova`.

### M20.4 — Abgaben, Inbox und Read-only-Review

- Abgabe aus dem Lernenden-Workspace erzeugen (`.novasubmission`).
- Import einzeln oder als Ordner; Duplikate/Revisionen, späte Abgaben.
- Abgabe-Statusboard je Aufgabe/Klasse.
- Read-only-Sitzung mit „Kopie speichern unter“; kein stilles Überschreiben.

**Akzeptanz:** kompletter Offline-Roundtrip Lehrkraft → Lernende → Lehrkraft
in Temp-Verzeichnissen; Import bricht bei ungültigem Paket ohne Teilkopie ab;
kein Überschreiben ohne Bestätigung.

### M20.5 — Feedback und Kommentare

- `Comment` mit Anker (Szene/Ebene/Objekt/Frame), Kategorie, Status.
- Kommentar-Overlay auf der Bühne + Panel; „Zum Anker springen“.
- Tolerante Ankerauflösung inkl. „verwaist“-Markierung.
- `.novafeedback` exportieren/importieren; Verlauf bleibt lesbar.

**Akzeptanz:** Anker überleben Speichern/Laden und Objekt-Umbenennungen
mindestens über den Label-Fallback; Feedback verändert das Projekt nie;
Roundtrip-Test; Kommentare sind nach Neustart vorhanden.

### M20.6 — Lernfortschritt und Berichte

- Fortschrittsstatus, Rubrikbewertung, Klassensicht, Einzelübersicht.
- CSV/Markdown/HTML-Export, deterministisch, escaped, ohne externe Assets.
- Retention-Hinweis und Löschfunktion.

**Akzeptanz:** Bericht enthält exakt die erwarteten Zeilen/Spalten;
Sonderzeichen/Quotes korrekt; 200 Abgaben in vertretbarer Zeit;
keine Telemetrie, keine Netzwerkzugriffe.

### M20.7 — Classroom Mode, Lehrvorlagen und Tutorials

- `Mode::Classroom` mit Navigation und Dashboards.
- Lehrvorlagen-Katalog (`.nova` + Metadaten) und Offline-Tutorial-Viewer.
- Hilfe-/About-Texte, Tastenkürzel, leere Zustände.
- Sichtbarkeit ausschließlich in Edu.

**Akzeptanz:** User-/Enterprise-Build hat keine Classroom-Einstiege;
Tutorials funktionieren offline; Screens/Dialoge in `docs/EDU_GUIDE.md`
beschrieben; keine Änderung an `ProjectTemplate` nötig.

### M20.8 — Offline-Betrieb, Packaging, Doku und RC

- Vollständiger Offline-Abnahmetest (Netzwerk deaktiviert, zwei Rollen,
  USB-/Ordner-Workspace).
- Edu-Payload enthält Vorlagen/Tutorials/Doku; `BUILD-INFO` mit
  `eduContentVersion`.
- Doku: `EDU_GUIDE.md`, `CLASSROOM_PACKAGE_FORMAT.md`, `EDU_PRIVACY.md`,
  M20-Report, M20-Test-Report; README/Roadmap/Architektur aktualisieren.
- Version `0.16.0-rc1`; `package-editions.ps1 -Editions edu`;
  Artefakte inkl. Enterprise-Hygiene archivieren.

**Akzeptanz:** Release-Gates aus `docs/ROADMAP.md` erfüllt; Installer-Smoke;
Archiv vollständig; ehrliche „nicht verifiziert“-Liste (echte Schule,
Netzlaufwerk, macOS/Linux-Edu).

### Übersicht

| Milestone | Fokus | Hängt ab von |
|---|---|---|
| M20.0 | Baseline/CI/Doku | — |
| M20.1 | Fundament, Rollen, Schulconfig | M20.0 |
| M20.2 | Klassen/Gruppen | M20.1 |
| M20.3 | Aufgaben/Starter/Paket | M20.1, M20.2 |
| M20.4 | Abgaben/Inbox/Review | M20.3 |
| M20.5 | Feedback | M20.4 |
| M20.6 | Fortschritt/Berichte | M20.4 |
| M20.7 | Classroom-UI/Tutorials | M20.2–M20.6 |
| M20.8 | Offline/Packaging/RC | alle |

---

## 7. Test- und Verifikationsstrategie

**Unit (`nova-edu`)**

- Schema-Roundtrip und Migration (`v1` → aktuell), unbekannte Felder.
- Pfad-Confinement: `..`, absolute Pfade, Laufwerke, UNC, Symlinks.
- ZIP-Limits: Einträge, Größe, Ratio, verschlüsselte Einträge.
- Prüfsummen: korrekt, manipuliert, fehlend.
- Ankerauflösung: ID, Layer/Frame, Label, verwaist.
- Fortschrittsaggregation und Berichtssortierung.
- Konfliktfälle: doppelte Abgabe, späte Abgabe, gleicher Anzeigename.

**Integration**

- `teacher_export → student_import → edit → submit → teacher_import →
  feedback_export → student_import` in Temp-Verzeichnissen.
- Klassen-Backup/Restore (`.novaclassroom`).
- Cross-Edition: Edu-`.nova` in User/Enterprise laden und zurückspeichern.
- Edition-Gating: User-Profil ohne `classroom`; Edu-Profil mit.
- CLI-E2E (falls umgesetzt): `nova edu verify`, `nova edu report`.

**Manuell (M20.8)**

- Zwei Windows-Benutzerprofile oder zwei Rechner, Netzwerk deaktiviert.
- Workspace auf USB-Stick und auf einem Netzlaufwerk (nur Import-Kopien).
- Große Klasse (z. B. 30 Lernende × 6 Aufgaben).
- Installer/Edu-Payload, Portable-ZIP, Deinstallation.
- Sichtprüfung: keine Classroom-Einstiege in User/Enterprise.

**Nicht in dieser Umgebung verifizierbar**

- Echte Schule/Lehrkräfte, Datenschutzfreigabe, Schul-IT.
- Adobe-Animate-/Projector-Verhalten (unverändert offen).
- macOS-/Linux-Edu-Pakete.
- Gleichzeitige Schreibzugriffe auf SMB/NFS.

---

## 8. Packaging, Versionierung, Release

- Zielversion: **`0.16.0-rc1`** (Vorschlag; M19 bleibt `0.15.0-rc1`).
- `package-windows.ps1 -Edition edu` kopiert zusätzlich:
  `edu/templates/`, `edu/tutorials/`, `docs/EDU_*.md`,
  `docs/CLASSROOM_PACKAGE_FORMAT.md`.
- `BUILD-INFO.json` erhält `eduContentVersion` (Katalog-/Tutorial-Stand).
- Installer bleibt generisch; Schulkonfiguration wird **nicht** im Installer
  erzwungen. Provisioning per Datei/`nova edu provision` (optional).
- Release-Gates unverändert: Tests, Release-Build, `nova probe`, keine
  stillen Datenverluste, ehrliche Doku, frischer Clone lauffähig.
- Kein DRM, keine Aktivierung, keine Online-Prüfung — auch nicht „optional“.

---

## 9. OpenCode und DeepSeek als Co-Developer

### 9.1 Verifizierte Fakten

| Prüfung | Ergebnis |
|---|---|
| Git-Historie | 21 Commits mit `Co-authored-by: Devin …` (M8–M19) |
| Weitere Co-Author-Trailer | **keine** |
| Git-Config | global `HackintoshWindows <mineventurehd@gmail.com>`; **keine** lokalen `user.*`-Overrides |
| Contributor-Doku | nur `AGENTS.md`; **kein** `CONTRIBUTING.md`, `AUTHORS`, `CODE_OF_CONDUCT` |
| GitHub-CLI/Token | `gh` nicht installiert, kein API-Token verfügbar |
| Repository | öffentlich, 34 Commits, keine Tags/Releases |
| OpenCode-Identität | **nicht verifiziert**; keine offizielle Commit-Bot-Identität auffindbar |
| DeepSeek-Identität | **nicht verifiziert**; nur die Organisation `deepseek-ai`, keine belegte Commit-E-Mail |

`AGENTS.md` bestätigt das bereits: Für DeepSeek existiert **keine** verifizierte
Commit-Identität; eine `deepseek@users.noreply.github.com`-Adresse darf nicht
geraten werden. OpenCode ist in der Tabelle der verifizierten Identitäten
nicht enthalten.

### 9.2 Warum jetzt keine Trailer

GitHub zeigt einen Co-Author nur dann als verknüpftes Konto, wenn die
Trailer-E-Mail nachweislich zum GitHub-Account gehört (typisch
`<id>+<login>@users.noreply.github.com`). Eine erfundene Adresse erzeugt
bestenfalls Text ohne Profil — und verstößt gegen `AGENTS.md`. Deshalb:

- **keine** `Co-authored-by`-Zeile für OpenCode oder DeepSeek,
- **keine** Änderung an `git config`,
- **keine** Änderung an Contributor-Dokumenten,
- Beteiligung wird im Commit-Body als Prosa vermerkt.

### 9.3 Vorschlag für die Contributor-Dokumentation (noch **nicht** angewendet)

1. `CONTRIBUTING.md` anlegen mit Abschnitt „AI co-developers“:
   - Zweck: transparente Nennung tatsächlich beteiligter Agenten.
   - Regel: nur Agenten, die den Diff wirklich bearbeitet haben.
   - Regel: keine erfundenen Namen/E-Mails; bei fehlender Identität Prosa
     im Commit-Body plus Eintrag in der Contributor-Liste.
2. Tabelle „Verified identities“ aus `AGENTS.md` übernehmen und erweitern:
   `Agent | GitHub account | Commit email | Status`.
3. Klarstellen: **OpenCode** ist das Agent-Harness, **DeepSeek V4.1 Flash**
   das Modell. Zwei getrennte Trailer nur bei zwei getrennten, verifizierten
   GitHub-Identitäten; sonst ein kombinierter Prosa-Vermerk, z. B.
   `AI-Assisted-By: OpenCode (DeepSeek V4.1 Flash)`.
4. `README.md`-Abschnitt „Contributors“ mit Verweis auf `CONTRIBUTING.md`.
5. Kein Rewriting alter Commits; die Regel gilt ab Aktivierung.

### 9.4 Benötigte Angaben (von dir)

1. **OpenCode:** exakter GitHub-Login + verifizierte Commit-E-Mail.
2. **DeepSeek:** exakter GitHub-Login + verifizierte Commit-E-Mail.
3. Sollen dafür **dedizierte Bot-Accounts** angelegt werden (nur du kannst
   das tun), oder sollen die Trailer über deinen Account laufen?
4. Ein oder zwei Trailer? (Harness und Modell getrennt oder kombiniert)
5. Freigabe, `CONTRIBUTING.md` und die `AGENTS.md`-Tabelle danach
   anzupassen.

Bis dahin gilt: **OpenCode/DeepSeek werden nicht als Git-Co-Autoren
geführt**, sondern nur als beteiligte Werkzeuge im Commit-Body genannt.

---

## 10. Risiken und offene Entscheidungen

| Risiko / Entscheidung | Auswirkung | Empfehlung |
|---|---|---|
| Keine Authentifizierung | Abgaben/Rollen sind fälschbar | In UI/Doku klar benennen; keine Sicherheitsversprechen |
| Netzlaufwerk ohne Locking | verlorene Änderungen | Pro-Person-Ordner + Import-Kopien; Lock nur beim Import |
| ZIP-Schwachstellen | Pfad-Escape/Bombe | Harte Limits + Tests vor jeder UI-Freigabe |
| Datenschutz/DSGVO | Schulrechtliche Auflagen | Datenminimierung, Löschung, Retention-Hinweis, keine Telemetrie |
| `.nova`-Unveränderlichkeit | Classroom-Metadaten fehlen im Projekt | Sidecars + Anker; Format v4 bleibt |
|Feedback-Anker brechen | Kommentare „verwaisen“ | ID → Frame/Label → sichtbar verwaist |
| UI-Komplexität | `app.rs` wächst weiter | neue Module, nur Dispatch in `app.rs` |
| Nur Deutsch | Schulen mit Englischbedarf | Offen; Lokalisierung ist nicht M20-Kern |
| Windows-only-Edu | macOS/Linux-Schulen | Ehrlich dokumentieren; Edition-Packaging später |
| CI rot | keine belastbaren Merges | M20.0 vor allem anderen |
| Identitäten fehlen | keine Co-Author-Trailer | Angaben aus 9.4 abwarten |

---

## 11. Unmittelbare nächste Schritte

1. **Entscheidung:** M20.0 (Baseline/CI/Doku) jetzt starten?
2. **Identitäten:** Angaben aus 9.4 liefern (OpenCode/DeepSeek).
3. **Scope-Bestätigung:** ein Modul `classroom` vs. mehrere Edu-Module;
   Zielversion `0.16.0-rc1`; Windows-only für M20.
4. **Dann:** `M20.0` umsetzen (Clippy, Doku-Ehrlichkeit, Archivlücke,
   Testpräzisierung) — ohne Classroom-Code.
5. **Danach:** `M20.1` mit `nova-edu`-Skelett, Workspace, Rollen,
   Schulkonfiguration und Edition-Gating.

---

## Anhang A — Nachweise

```text
$ git status
On branch main
Your branch is up to date with 'origin/main'.
nothing to commit, working tree clean

$ git log -1 --format='%H %s'
926e11eb4d3cfeed9d61b0ec51af4bf3191e407f M19: introduce three independent NØVA editions — User, Edu, Enterprise

$ cargo test --workspace
32 Suites: 427 passed, 0 failed, 8 ignored

$ cargo fmt --all -- --check
Exit 0

$ cargo clippy --workspace --all-targets -- -D warnings
error: could not compile `nova-swf` (lib) due to 3 previous errors
error: could not compile `nova-as` (lib) due to 15 previous errors
error: could not compile `nova-core` (lib) due to 22 previous errors
error: could not compile `nova-core` (lib test) due to 29 previous errors
Exit 101

$ git tag --list
(leer)
```

## Anhang B — Zentrale Dateireferenzen

| Thema | Datei |
|---|---|
| Edition-Modell | `crates/nova-core/src/edition.rs` |
| Profil/Registry | `crates/nova-app/src/edition.rs` |
| UI-Zustand/Aktionen | `crates/nova-app/src/state.rs` |
| Menüs/Dialoge/Help | `crates/nova-app/src/app.rs` |
| Projektformat/Persistenz | `crates/nova-core/src/model/project.rs` |
| Vorlagen | `crates/nova-core/src/template.rs` |
| Autosave/atomare Writes | `crates/nova-app/src/autosave.rs` |
| Root-Confinement-Muster | `crates/nova-ai/src/files.rs` |
| Editionstests | `crates/nova-core/tests/editions.rs` |
| Packaging | `scripts/package-windows.ps1`, `scripts/package-installer.ps1`, `scripts/package-editions.ps1` |
| CI | `.github/workflows/rust.yml` |
| Edition-Doku | `docs/EDITIONS.md`, `docs/EDITION_ARCHITECTURE.md` |
| Attributionsregeln | `AGENTS.md` |
