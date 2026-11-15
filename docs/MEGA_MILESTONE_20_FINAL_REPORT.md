# MEGA MILESTONE 20 — Final Report: Education Edition

**Target:** `0.16.0-rc1` · **Base:** `a0eb6f5` (M20.0) on `d4b9e5a`/`091f6bf`
· **Status:** complete — release candidate prepared, **not pushed, not released**.

## 1. Scope delivered

| Sub-milestone | Content | Status |
|---|---|---|
| M20.0 | CI/Clippy repair, stale help/docs, editions manifest, packaging/archive tests, `.nova` association spec | preserved (commits `091f6bf`, `a0eb6f5`) |
| M20.1 | `nova-edu` crate: workspace, school config/policy, roles, profile, persistence | done |
| M20.2 | Classroom/group/member CRUD + Windows `.nova` association implementation | done |
| M20.3 | Assignments, starter projects, rubrics, `.novaassignment` | done |
| M20.4 | Submissions, inbox, revisions, checksums, read-only review | done |
| M20.5 | Feedback, anchored comments, orphan handling, `.novafeedback` | done |
| M20.6 | Progress derivation, CSV/Markdown/HTML reports | done |
| M20.7 | Classroom mode UI (Edu-gated) | done |
| M20.8 | `.novaclassroom`, offline flow, Edu packaging, docs, artifacts | done |

## 2. Architecture

- **`crates/nova-edu`** (new workspace member): headless, UI- and
  network-independent domain layer. Modules: `error`, `store`,
  `workspace`, `school`, `profile`, `classroom`, `assignment`,
  `submission`, `feedback`, `progress`, `report`, `package`, `template`.
- **`nova-app`**: `Mode::Classroom` + `edition::has_active_module`
  gate; `edu_state.rs` (`EduUi`), `classroom_view.rs` (teacher/student
  workflows). Classroom mode owns the full central panel; project
  panels are hidden and edu data never enters `Project`, undo history
  or autosave.
- **`.nova` stays v4**, edition-neutral. All Edu data lives in
  workspace sidecars (`school.json`, `classrooms/`, `assignments/`,
  `inbox/`, `feedback/`, `reports/`, `templates/`) or the Edu config
  dir (`edu-profile.json`, `edu-prefs.json`) — never in `Project`.

## 3. Key workflows

- **Teacher:** workspace → classes/groups/members (+JSON roster) →
  assignments with starter `.nova` → distribute as `.novaassignment` →
  review submissions read-only → feedback comments (categories,
  anchors) → `.novafeedback` → progress grid + CSV/MD/HTML reports.
- **Student:** link roster entry → open starter → work in normal
  editor → "Als Abgabe exportieren" (revisioned inbox entry +
  `.novasubmission` package) → read/acknowledge feedback.
- **Offline exchange:** four hardened ZIP package kinds with
  `manifest.json` (schema/format/id/author/content/checksums),
  SHA-256 verification **before** any state change, traversal/
  symlink/encryption/size-ratio rejection, temp-dir extraction,
  transactional commit, `.nova` payloads validated via `Project::load`
  on open. Nothing in a package is ever executed.

## 4. Roles honesty

Teacher/Assistant/Student are local workflow labels stored in
`edu-profile.json`. They are **not** authentication, encryption or a
security boundary; the UI header states this verbatim. No accounts,
servers, telemetry, DRM, license keys or seat limits anywhere.

## 5. Windows `.nova` association (M20.2)

Implemented per `docs/WINDOWS_FILE_ASSOCIATION.md` in
`packaging/windows/installer/InstallEngine.cs`:

- per-edition ProgIds (`NOVAFlashStudio{,Edu,Enterprise}.Project`),
  `DefaultIcon`, `shell\open\command`, snapshot/restore of the previous
  default (only while it still points to us);
- **new:** `Capabilities\FileAssociations` + `RegisteredApplications`
  ("Öffnen mit"/Settings lists), `OpenWithProgids`/`OpenWithList`,
  context-menu verb `shell\openWithNOVA`;
- uninstall removes only this edition's values; `UserChoice` is never
  written; editions coexist (User default untouched when Edu
  uninstalls, etc.). Portable `--register-associations` opt-in remains
  documented future work (spec §8).

## 6. Verification (executed)

```
cargo test --workspace                          → 462 passed, 0 failed (34 suites)
cargo clippy --workspace --all-targets -- -D warnings → clean
cargo fmt --all -- --check                      → clean
dotnet build NovaFlashStudio.Setup.csproj -c Release → 0 warnings/errors
scripts/package-editions.ps1 -SkipTests         → six artifacts + checksums
scripts/test-edition-packaging.ps1              → all PASS
scripts/test-archive-releases.ps1               → all PASS
```

`nova-edu` unit tests cover: workspace layout/meta, school config
validation, profile roles, classroom/group/member CRUD and validation,
assignment lifecycle + starter path safety, submission revisions +
checksums + never-overwrite + late detection, feedback comments +
resolve/acknowledge + orphan detection + bundle listing, progress
status derivation + per-member rows + class overview, CSV escaping +
HTML escaping + civil-date conversion, package roundtrip + missing
manifest + truncation + traversal rejection + kind extensions,
template catalog + sample workspace population.

## 7. Artifacts (`0.16.0-rc1`, SHA-256)

`dist/windows/<edition>/` + `dist/archive/0.16.0-rc1/<edition>/`
(user/, edu/, enterprise/ — each with app dir, portable ZIP, Setup EXE,
`BUILD-INFO.json`, `SHA256SUMS.txt`; archive contents verified identical
to current artifacts by `test-edition-packaging.ps1`).

| Artifact | SHA-256 |
|---|---|
| user `NOVA-User-Portable.zip` | `6D2D61260A72558C0952E5433718597E02A7DBCA66F51EA5500C4DB0381F83FB` |
| user `NOVA-User-Setup.exe` | `D3D21DA99EDDAAF8131B883903390F31AF46301B0FC75011285426BFB00EEB27` |
| edu `NOVA-Edu-Portable.zip` | `84DBF0BA0132C3706FD67B18973F49BE72295DA82C37D23C9C8B859132E20DD9` |
| edu `NOVA-Edu-Setup.exe` | `00DC936A123A3CB7BEBFF51538BA5E38A943EF33A5A42729886E74219FF6D751` |
| enterprise `NOVA-Enterprise-Portable.zip` | `B1C713511D43B8F28EEE2442BAFB292D87459667992CC1006A44DE0988E7FC61` |
| enterprise `NOVA-Enterprise-Setup.exe` | `D0F99E123427799941FC27A0DD3D6F56AA43A186AFB7E41F4A1AD56120F355C8` |

App EXE hashes are in each edition's `SHA256SUMS.txt`.

## 8. Limitations (honest)

- Anchored comments: anchor *creation* UI inside the stage is minimal —
  comments can carry anchors via the data model; the classroom UI
  currently adds general (unanchored) comments. Orphan detection and
  resolution work against any loaded project.
- `.novasubmission` import records a new revision server-side; it does
  not merge into an existing student's chain beyond revision numbering.
- Rubric marks are stored per bundle; there is no weighted gradebook
  calculation beyond the stored marks.
- Portable file-association opt-in flag not implemented (spec §8).
- Manual HKCU registry verification on a disposable Windows profile is
  a release-checklist item; agent runs perform no registry writes.
- Enterprise administration module remains reserved for M21.

## 9. Commits (this milestone)

- `ebc49f4` M20.1: add nova-edu domain crate
- `2e63bef` M20.2–M20.7: classroom UI, workflows, file association
- `HEAD`   M20.8: version 0.16.0-rc1, docs, packaging, this report
  (self-referential hash intentionally omitted)

Every commit carries the required Devin co-author trailer (verified via
`git log -1 --format=%B`).

## 10. Non-goals confirmed

- No push, no release/tag, no force-push, no history rewrite.
- `.nova` format unchanged (v4); User Edition functionality intact;
  edition isolation preserved; fully offline.
