# MEGA MILESTONE 27 — Final Report

**Isolated Agent Build & Test Sandbox + Proper EULA** — branch `main`,
basis `af9b754` (post-M26). Version bumped `0.17.0-rc1 → 0.18.0-rc1`
(0.19 would have skipped the version history).

## 1. Implemented functions

### M27.1/27.2 — Sandbox engine (`crates/nova-ai/src/sandbox.rs`)

- `Sandbox::create(task_id, project_root, review, base_dir, max_bytes)` —
  private copy per task under `<config>/ai-sandboxes/<task-id>/project`.
- Copy excludes `.git`, `target`, `node_modules`, `dist`, `build`,
  `.idea`, `.vs`, `.nova-ai`, **all symlinks/junctions**, every path
  matching the M25 `is_sensitive` denylist, and aborts on a size cap
  (`DEFAULT_MAX_SANDBOX_BYTES` = 2 GiB, configurable via
  `AgentLimits.max_sandbox_bytes`).
- Staged `ReviewSession` state is materialized exactly: creates,
  modifies (byte-compared) and deletes. `sync()` re-materializes after
  each agent iteration — content-compared so only changed files are
  rewritten and sandbox build outputs (`target/`) survive.
- Paths no longer staged are restored to the original project state.
- `resolve()` rejects `..`, absolute paths and drive letters.
- Cleanup is idempotent (`cleanup()` + `Drop`); per-task dirs cannot
  collide (sanitized task id).
- **Honest security position:** project-copy sandbox with allowlist
  argv commands, no shell, timeouts, size caps, no admin rights.
  **Not implemented:** OS-level network isolation, Windows job objects,
  memory quotas, filesystem ACLs. Cargo may reach crates.io. This is
  documented in code, UI and docs — no isolation is claimed that does
  not exist.

### M27.3/27.4 — Build/test pipeline + self-correction

- All allowlisted commands now execute with `sandbox.root()` as
  workdir — never the original project.
- `CommandRecord` gained `sandbox_id` + `touched_files`; `Task` gained
  `sandbox_id` + `sandbox_fingerprint`.
- If the agent stages changes but requests no checks on a Rust project,
  `cargo check` runs automatically — integrity checks cannot be
  silently skipped.
- Command output feeds back into the next iteration (M26 mechanism,
  unchanged); `AgentLimits` (iterations, seconds, files, commands,
  workspace bytes, sandbox bytes) still bound the loop; pause keeps
  review + sandbox state.
- `AgentLoop::sandbox_check()` re-runs the sandbox check without a
  provider call (powers the "Build/Tests erneut" action).

### M27.5/27.6 — UI

- Task detail shows `BUILD/TESTS (Sandbox): X ok · Y fehlgeschlagen`,
  sandbox test status, and a staleness marker: `TESTSTAND VERALTET`
  when the live `ReviewSession::fingerprint()` differs from the
  fingerprint the last test ran against.
- Actions: **Sandbox-Details** (path, size, copied bytes, elapsed,
  skipped files, honest-limits note), **Build/Tests erneut**.
- `review_task_id` tracks which task owns the current review session;
  project-root changes drop sandbox + ownership (sandbox dir deleted).
- Review/apply path unchanged: `ReviewSession` → per-file accept →
  grant re-check → apply → rollback; task marked Completed on apply.

### M27.8 — EULA

- Five complete 18-section EULAs: `legal/LICENSE-EULA.md` (de-DE
  master), `.en-GB`, `.da-DK`, `.fr-FR`, `.pl-PL` — content-equivalent
  translations, **version 1.0**, placeholders `[RECHTEINHABER]`,
  `[JURISTISCHE ANSCHRIFT]`, `[KONTAKTADRESSE]`,
  `[DATENSCHUTZHINWEIS/URL]` intentionally open. Explicitly states:
  no licence keys, no activation, no DRM, no mandatory online
  connection, no device/seat limits, user ownership of content,
  editions differ only by actual functionality, third-party licences
  untouched, external AI terms apply, local AI stays possible.
- `docs/EULA.md` documents version, placeholders, legal-review status.
- Installer: csproj embeds `eula.<lang>.txt`; `ReadEulaText(lang)` →
  selected language → en-GB → de-DE → legacy `EULA.txt`.
  LicensePage reloads per language, shows `EULA v1.0`, and
  **consent is no longer pre-checked** — Weiter/Installieren stays
  disabled until explicit acceptance.
- `InstallManifest.EulaVersion` + registry `EulaVersion` record the
  accepted version; every install re-requires acceptance, so a new
  EULA version is never silently treated as accepted.
- App: **Hilfe → Lizenzvereinbarung anzeigen (EULA)**
  (`menu.help.eula`, all 5 locales) opens the bundled EULA matching
  the UI language with fallback.

## 2. Changed/new files

- `crates/nova-ai/src/sandbox.rs` (new, ~440 lines + 6 tests)
- `crates/nova-ai/src/review.rs` — `Clone`, `fingerprint()` + test
- `crates/nova-ai/src/tasks.rs` — CommandRecord/Task/AgentLimits fields
- `crates/nova-ai/src/agent_loop.rs` — sandbox lifecycle, sandbox
  workdir, auto-check, `sandbox_check()`
- `crates/nova-ai/src/runner.rs`, `lib.rs` — record fields, exports
- `crates/nova-app/src/ai_editor.rs` — sandbox UI, retest worker,
  staleness marker, details window, ownership reset
- `crates/nova-app/src/app.rs`, `state.rs`, `i18n/*.json` — EULA menu
- `legal/LICENSE-EULA*.md` (5 new), `docs/EULA.md`
- `packaging/windows/installer/{csproj,InstallEngine,InstallerText,WizardForm}.cs`
- `Cargo.toml` 0.18.0-rc1, `README.md`, `AGENTS.md`, `CHANGELOG.md`,
  `docs/AI_AGENT_EDITOR.md`

## 3. Test results

- `cargo test --workspace`: **all PASS, 0 failed** (82 nova-ai incl.
  6 sandbox + 1 fingerprint; nova-app 93)
- `cargo fmt --all -- --check`: clean · `cargo clippy --workspace
  --all-targets -- -D warnings`: clean
- `dotnet build` installer: 0 warnings/0 errors
- `test-installer.ps1` all editions: **PASS** (incl. headless
  install/verify/uninstall, user-data preservation)
- `test-edition-packaging.ps1` + archive: **PASS**

## 4. Builds (0.18.0-rc1, SHA-256)

| Artifact | SHA-256 |
|---|---|
| user portable | `615281635AB68919EAE353D0265A70ADB77039DFEB7CA25D2DD6E6F63EFA5C4F` |
| user setup | `C5E2D2FB1E3FE13F85B9CCAAD21153AD3FC6B5C54C7E609EFA39F0A855EDD3FB` |
| user exe | `318FD3F64ECDA3DED401E5E0D3389E481ABA1906F146607D67AB4A083037FE3C` |
| edu portable | `6FC3AF08133F1BB817EF934416F67A6EF07BEF1FD86C55B46648F93048BC0661` |
| edu setup | `FB22BDAEB0E0BDA409E31CEAABCD90805183CAFC7EDF04378EFE8E8026B7DDB7` |
| edu exe | `F70679FC9B918B278058AB94D8E7CC6B03467765B8444190011F0CC6D5065212` |
| enterprise portable | `D3BE955DE77799733686805E002F5FCE8015206DAD5FEBA0DB45DA4E4C2BD2E3` |
| enterprise setup | `BE63DAA480D1654E7D8402AFC685804B46F127BD931E1C9160B53D674F77BC9E` |
| enterprise exe | `6A6026A3802DB92FE1D51C7C9A4CBD49CF0D3350805FA822DCEDD7A5967DCB73` |

## 5. Commits

- `acaedcd`→ sandbox commit (see `git log`; message: M27.1-27.6)
- EULA/installer commit (M27.8 + version)
- `dd58293` — docs
- (hashes verified via `git log -1 --format=%B`, trailer present)

## 6. Known limitations / open items

- Sandbox = project copy + allowlist commands; **no OS-level network,
  process-tree or memory isolation** (documented honestly everywhere).
- Commands run against the sandboxed *staged* state — correct — but
  `.nova`-project builds still use existing pipelines only.
- EULA is an **unreviewed draft**; placeholders must be filled and the
  text legally reviewed before public release (see `docs/EULA.md`).
- GUI flows (sandbox details, retest button, license page in all 5
  languages) verified by compile + tests; interactive click-through is
  a manual check.

## 7. Manual GUI tests still required

1. AI Agent → task with Rust changes → watch sandbox commands in
   task detail; change staging afterwards → confirm VERALTET marker;
   Build/Tests erneut; Sandbox-Details.
2. Installer: license page in each of 5 languages, consent required,
   EULA v1.0 shown, registry/manifest `EulaVersion` present;
   Help → EULA menu in the app.
