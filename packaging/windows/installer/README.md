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

## Tests

`scripts/test-installer.ps1` runs the headless install → verify →
uninstall cycle for all three editions (39 checks): payload,
manifest, registry entry, docs folder, uninstaller copy, removal,
and user-data preservation.

Manual wizard test: run `NOVA-<Edition>-Setup.exe` from
`dist/windows/<edition>/` — Welcome → License → Installationsort →
Optionen → Fortschritt → Fertig (launch + docs buttons).
