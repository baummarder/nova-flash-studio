# Changelog

All notable changes are documented per release candidate. The `.nova`
project format stays at v4 and edition-neutral across all editions.

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
