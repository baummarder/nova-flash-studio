# Changelog

All notable changes are documented per release candidate. The `.nova`
project format stays at v4 and edition-neutral across all editions.

## [0.19.0-rc1] — M28 native AI tool calling

### Added

- **M28 — Typed tool-calling architecture** (`nova-ai/src/tools.rs`):
  provider-independent `ToolRegistry` with versioned schemas, risk
  classes (read-only/staging/destructive), unique call ids, structured
  error codes, argument/output bounds and per-task call limits.
- **Project tools:** info/tree/metadata/search/read/diff plus scene,
  asset and file-diff inspection — all behind M25 grants and the
  sensitive-file denylist.
- **File staging tools:** create/update/delete/rename/apply_patch and
  `.as` script writes — everything lands in the M25 `ReviewSession`;
  deletes default to unaccepted (explicit per-file confirmation).
- **Editor tools:** scene/layer/object/keyframe/frame-label/tween/
  symbol/place-symbol operations mapped onto the validated
  `ProjectOperation` pipeline; the `.nova` file is edited
  transactionally and staged as a file change.
- **Provider integration:** `chat_tools` uses native `tools`/
  `tool_calls` for Ollama and OpenAI-compatible APIs with correct
  per-provider message shapes; graceful fallback to the M26 JSON
  protocol plus a validated text-envelope parser — never arbitrary
  code execution.
- **Agent loop:** multi-turn tool rounds inside each iteration
  (bounded), sandbox re-sync after staged changes, per-task tool
  history (`TOOLS (n)`) in the AI Agent panel.

## [0.18.0-rc1] — M26 AI agent workflow + M27 sandbox & EULA

### Added

- **M26 — AI agent development tasks:** natural-language tasks with
  unique IDs, 10-status lifecycle, persisted per-project history
  (`ai-tasks.json`), structured plan with user approval gate,
  iterative implementation in the M25 staging workspace, allowlist-only
  build/test commands (`runner.rs`), iteration/time/file/command limits,
  cooperative cancel and rerun. Tasks panel in the AI Agent Editor.
- **M27 — Isolated agent sandbox:** per-task sandbox = project copy +
  staged changes (`sandbox.rs`), sensitive-file/symlink/size-cap
  exclusions, incremental content-compared sync, build/test commands
  executed inside the sandbox only, `cargo check` auto-run when changes
  are staged without checks, review-fingerprint staleness detection,
  `Build/Tests erneut` action and sandbox details window. Honest limit:
  project-copy sandbox, no OS-level network/process isolation.
- **M27 — Full EULA:** five complete language versions
  (`legal/LICENSE-EULA.*`) covering all 18 required sections with
  explicit no-key/no-activation/no-DRM/no-seat-limit terms; installer
  license page now requires explicit consent (nothing pre-checked),
  shows `EULA v1.0`, reloads per language with en-GB fallback, records
  `EulaVersion` in manifest + registry; app menu opens the EULA in the
  UI language.

## [0.17.0-rc1] — M21 Enterprise Edition + M21 remediation + M22 UI/i18n + M23 workspace/installer + M24 installer/i18n + M25 AI access

### Added

- **M24 — Installer completion:** five-language setup wizard
  (embedded `i18n/*.json`, en-GB fallback, per-edition registry
  persistence, first-run language page, header language selector,
  new summary page), footer rebuilt so Zurück/Weiter/Installieren/
  Abbrechen are always visible, `--set-language`/`--get-language`
  CLI.
- **M25 — AI agent project access:** per-project grant store
  (`ai-access.json`) with file/folder or whole-tree grants,
  read-only vs read+write levels, project-wide read-only cap,
  sensitive-file denylist (.env, keys, credentials, .git, .ssh),
  isolated `ReviewSession` staging with snapshots, per-file diff
  accept/reject, conflict detection, grant re-check and rollback on
  apply; permission prompt + grants manager in the AI Agent Editor.

- **M23 — Integrated code workspace:** three editor layouts —
  Stage Only, Stage + Code (draggable divider, both panes fully
  interactive) and Code Only — selectable via a compact toolbar
  segment and the view menu, localized in all five languages.
  Workspace arrangement persisted per edition in
  `app-settings.json` (`layout` + `split_ratio`).
- **M23 — Installer redesign:** the setup wizard now uses the
  classic 2009 desktop look (shared `WizardTheme` palette,
  gradient header, beveled buttons) and the finish page offers
  "Dokumentation öffnen". Engine unchanged: per-user/all-users
  scopes, UAC relaunch, manifest-based upgrade/uninstall,
  association restore, user-data preservation.
- **M23 — Installer tests:** `scripts/test-installer.ps1` runs a
  headless install → verify → uninstall cycle per edition
  (39 checks: payload, manifest, registry, docs, uninstaller,
  removal, user-data survival).

- **M21 — Enterprise Edition (`nova-ent`):** workspaces (`org.json`),
  project registry with SHA-256 baselines, 7-role team model with
  per-project overrides and a documented policy permission matrix
  (not authentication), asset library with integrity verification,
  snapshot versioning with compare/restore and safety snapshots,
  review workflows with anchored comments, build profiles + history,
  append-only audit log + CSV export, opt-in local integrations
  (disabled by default), `.novaent` workspace packages and a
  nine-tab administration UI gated to the Enterprise edition.
- **M22 — UI redesign & i18n:** five UI languages
  (`de-DE`, `en-GB`, `da-DK`, `fr-FR`, `pl-PL`, en-GB fallback),
  central `i18n` module with `{placeholder}` support, embedded
  UTF-8-no-BOM locale files, per-edition `app-settings.json`
  persistence, first-run language wizard, live language switcher in
  settings, classic 2009 theme (charcoal/bevel) with left icon
  toolbox and right Properties+Library dock.

### Fixed

- **M21 remediation:** UTF-8→CP1252 mojibake in five M21 source files
  repaired; `.novaent` import is now transactional (plan → stage →
  verify → commit → rollback, `org.json` never overwritten);
  packaging archives all editions after the final swap.
- **M22:** the Windows GUI no longer opens a console window —
  `#![windows_subsystem = "windows"]` plus parent-console attach for
  CLI flags and a `nova-crash.log` panic log.

### Notes

- Roles (Edu/Enterprise) are workflow-policy labels, not
  authentication; OS file permissions remain the security boundary.
- `.novaent` packages contain workspace metadata/backups, not full
  project files.
- i18n coverage: chrome/dialogs/tab labels in all five languages;
  deep feature-panel literals and domain error strings remain
  German pending a dedicated translation pass (`docs/I18N.md`).
