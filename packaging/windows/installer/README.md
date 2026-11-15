# Windows Installer

Custom .NET 8 WinForms setup wizard + uninstaller
(`installer/*.cs`), built per edition by
`scripts/package-installer.ps1`. No WiX/Inno/NSIS dependency.

## Architecture

- `Program.cs` — entry point; GUI wizard by default, CLI otherwise
  (`--version`, `--help`, `--smoke-test`, `--install DIR`,
  `--uninstall`, `--all-users`, `--per-user`, `--desktop`,
  `--start-menu`, `--assoc-nova`, `--assoc-swf`, `--migrate`,
  `--quiet`, `--notify`).
- `WizardForm.cs` — six pages (Welcome, License, Scope/Directory,
  Options, Progress, Finish). Classic 2009 desktop styling via
  `WizardTheme` (charcoal palette matching the app, gradient header,
  beveled buttons).
- `InstallEngine.cs` — install/uninstall engine. Writes
  `install-manifest.json` (files, shortcuts, association records,
  capabilities flag) so upgrades remove only recorded artifacts and
  uninstalls never touch foreign entries.
- `EditionInfo.cs` — regenerated per edition by the packaging script
  (product name, exe name, uninstall key, ProgIds). Never edit by
  hand; change `packaging/editions.json`.

## Behavior

- **Per-user (default):** `%LOCALAPPDATA%\Programs\<Product>`, HKCU,
  no UAC.
- **All-users:** `Program Files`, HKLM, standard UAC prompt via
  `runas` relaunch; denying UAC aborts without side effects.
- **Upgrade:** an existing install at the same scope+directory is
  updated in place; files/shortcuts/associations recorded in the old
  manifest are replaced. Conflicting installs (other scope/dir)
  require explicit migration confirmation — never silent.
- **Associations:** registers ProgId open command + OpenWith entries
  + Capabilities/RegisteredApplications. Records previous defaults;
  uninstall restores them and never touches `UserChoice`.
- **Uninstall:** removes only manifest-recorded files, shortcuts,
  associations and the uninstall registry key. User data under
  `%APPDATA%` is never deleted.
- **Edition isolation:** distinct product names, install dirs,
  uninstall keys and ProgIds per edition — installs coexist.

## Wizard pages & navigation (M24)

Flow: **Sprache** (nur beim ersten Start ohne gespeicherte Sprache)
→ Willkommen → Lizenz → Installationsort → Optionen → **Zusammenfassung**
→ Fortschritt → Fertig.

- Footer is a `TableLayoutPanel`+`FlowLayoutPanel` right-aligned strip
  docked bottom — Zurück/Weiter|Installieren/Abbrechen can never be
  clipped by content or window size.
- Buttons adapt per page: Back hidden on Progress/Finish, Install
  label only on the summary page, license gate on the license page,
  Cancel becomes "Schließen" on Finish and launches the app.
- `_content` padding is symmetric (8 px); the header/footer heights
  are handled purely by docking.

## Localization (M24.2)

- Strings live in `i18n/<lang>.json` embedded as `i18n.<lang>.json`
  resources (de-DE, en-GB, da-DK, fr-FR, pl-PL; en-GB fallback).
- `InstallerText` resolves `wiz.*` keys; all pages implement
  `RefreshText()` so the header language combo re-localizes the whole
  wizard live.
- Choice persists per edition in
  `HKCU\Software\<UninstallKey>\Installer\Language`.
- CLI: `--set-language CODE`, `--get-language` (headless tests).
- Engine/headless progress messages remain German (limitation).

## Tests

`scripts/test-installer.ps1` runs the headless install → verify →
uninstall cycle for all three editions (39 checks): payload,
manifest, registry entry, docs folder, uninstaller copy, removal,
and user-data preservation.

Manual wizard test: run `NOVA-<Edition>-Setup.exe` from
`dist/windows/<edition>/` — Welcome → License → Installationsort →
Optionen → Fortschritt → Fertig (launch + docs buttons).
