# Changelog

All notable changes are documented per release candidate. The `.nova`
project format stays at v4 and edition-neutral across all editions.

## [0.17.0-rc1] — M21 Enterprise Edition + M21 remediation + M22 UI/i18n

### Added

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
  settings, Animate-style left mode rail (Code/Animation/Classroom/
  Enterprise + AI Agent).

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
