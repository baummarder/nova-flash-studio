# Windows `.nova` File Association — Technical Specification (M20.0)

Status: **specification only** — no registry writes are performed in
M20.0. Implementation is planned for M20.2 (edition-aware installer +
portable opt-in). This document is the reviewable contract for that work.

## 1. Goal

Double-clicking a `.nova` file opens it in the corresponding NØVA
edition. Right-click offers **„Mit NØVA Flash Studio öffnen“** (and the
Edu/Enterprise variants). Each edition owns its own ProgId family so
User, Edu and Enterprise can coexist without fighting over the
association.

**Current state (M20.0):** the installer already registers `.nova` via
the existing `AssociateNova` option (`InstallEngine.RegisterAssociation`,
default on in the wizard) using the per-edition ProgIds from
`EditionInfo`. This specification therefore documents the *current*
machinery, identifies the remaining gaps (`Capabilities` /
`RegisteredApplications` / `OpenWithProgids` registration and the
UserChoice story), and defines the acceptance criteria for the M20.2
follow-up. No registry writes happen in M20.0 itself.

## 2. Extension and ProgIds

| Edition | ProgId (`.nova`) | ProgId (`.swf`, optional) |
|---|---|---|
| User | `NOVAFlashStudio.Project` | `NOVAFlashStudio.SWF` |
| Edu | `NOVAFlashStudioEdu.Project` | `NOVAFlashStudioEdu.SWF` |
| Enterprise | `NOVAFlashStudioEnterprise.Project` | `NOVAFlashStudioEnterprise.SWF` |

The ProgIds already exist as constants (`packaging/editions.json`,
`EditionInfo.cs`, `InstallEngine.RegisterAssociation`) — M20.0 adds no
new identity, only documents how `.nova` joins the existing `.swf`
machinery.

## 3. Registry layout (per-user default)

Installer scope `per-user` (default) writes only under `HKCU`; the
`all-users` scope writes the equivalent `HKLM` keys (requires elevation,
already handled by the existing install engine).

```
HKCU\Software\Classes\NOVAFlashStudio<Project|SWF>.Project
    (Default)      = "NØVA Flash Studio Project"   ; friendly name per edition
    DefaultIcon    = <installDir>\<AppExe>,0
    shell\open\command
        (Default)  = "<installDir>\<AppExe>" "%1"

HKCU\Software\Classes\.nova
    (Default)      = NOVAFlashStudio<Project>.Project   ; ProgId forward
    OpenWithProgids\NOVAFlashStudio<Project>.Project    = ""
    OpenWithList\NOVAFlashStudio<Project>.Project       = "NØVA Flash Studio"
```

Per-edition example (Edu): `NOVAFlashStudioEdu.Project`,
`DefaultIcon`/`open\command` pointing at `…\NØVA Flash Studio Edu.exe`,
friendly name „NØVA Flash Studio Edu Projekt“.

## 4. Application registration (needed for „Öffnen mit“ lists)

```
HKCU\Software\<VendorKey>\Capabilities\FileAssociations
    .nova = NOVAFlashStudio<Project>.Project
HKCU\Software\RegisteredApplications
    <installDir>\<AppExe> = ""
```

`<VendorKey>` follows the existing uninstall-key convention
(`NOVAFlashStudio`, `NOVAFlashStudioEdu`, `NOVAFlashStudioEnterprise`).
`RegisteredApplications` makes Windows list the app in **„Öffnen mit“**
and Settings → Default apps without claiming the default. These two
hives are **not written today** and are the main M20.2 gap — without
them the ProgId works for double-click but the app may not appear in
the modern „Öffnen mit“ picker on Windows 10/11.

## 5. Default-application behaviour (UserChoice)

Since Windows 10 1809, the *default* handler lives in
`HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.nova\UserChoice`
with a hash-protected `ProgId` + `Hash` value. Rules:

- The installer **never writes `UserChoice`** — it is hash-protected and
  writing it is unsupported/fragile.
- The installer **does** write the `OpenWithProgids` and
  `OpenWithList` entries, so the edition appears in „Öffnen mit“.
- Making an edition the *default* application requires either the
  Settings UI or the documented `UserChoice` hash computation
  (reverse-engineered, not recommended). M20.2 documents this honestly:
  **„standardmäßig öffnen“ stays a user action in Windows Settings**.
- If `UserChoice` already points to another app, uninstall must not
  touch it (restoration is a Windows concern, not ours).

## 6. Icon

Each edition embeds the same NØVA icon resource today. When
edition-specific `.nova` icons exist, `DefaultIcon` already points at
`<AppExe>,0` — the icon index inside the exe's `.rsrc`, so a dedicated
`.nova` file icon can be added as resource index 1 later without
registry changes.

## 7. Uninstall / restoration

`InstallEngine` already snapshots the previous association state into
`install-manifest.json` (`RegisterAssociation` records prior `HKCR`
values). On uninstall:

- delete only the ProgId keys **this installation created**;
- restore the recorded previous `HKCR\.nova` default **only if it still
  points to our ProgId** (never overwrite a newer user choice);
- remove `OpenWithProgids`/`OpenWithList` entries;
- leave `UserChoice` untouched — Windows restores defaults itself when
  the registered app disappears.

## 8. Portable mode

Portable ZIPs do **not** self-register. An optional future
`--register-associations` CLI flag may offer HKCU registration from the
portable exe; it must:

- be strictly opt-in (never silent);
- write only `HKCU` (no admin needed);
- record `portable: true` in the manifest so uninstall knows the exe
  may live on removable media — cleanup must not assume a stable path.

## 9. Coexistence matrix

| Scenario | Expected |
|---|---|
| User + Edu installed | separate ProgIds, separate `OpenWithList` entries, each exe opens its own brand |
| `.nova` default = User, right-click „Öffnen mit“ | Edu/Enterprise listed and launch their own exe |
| Uninstall Edu | only `NOVAFlashStudioEdu.*` removed; User default untouched |
| Uninstall all | `.nova` has no NØVA handler; `UserChoice` may linger until Windows cleans it (harmless) |

## 10. Implementation checklist (M20.2 — implemented)

- [x] `.nova` -> per-edition ProgId registration exists via
      `AssociateNova` -> `RegisterAssociation(".nova", EditionInfo.ProgIdNova, ...)`
      with snapshot/restore in `install-manifest.json`.
- [x] `Capabilities\FileAssociations` + `RegisteredApplications` writes
      in `HKCU`/`HKLM` scope — `RegisterAppCapabilities`, recorded via
      `InstallManifest.AppCapabilities` and removed on uninstall only
      while values still name this edition's ProgIds.
- [x] `OpenWithProgids`/`OpenWithList` entries for `.nova` under the
      extension key, removed per-ProgId on uninstall.
- [x] Context-menu verb `shell\openWithNOVA` ("Mit <Product> öffnen")
      inside each ProgId — removed with the ProgId tree.
- [x] Wizard default stays as-is: `.nova` association **on**,
      `.swf` association **off** (documented existing behaviour).
- [x] Uninstall: `RemoveAssociation` restores the previous extension
      default only when it still points to our ProgId; `UserChoice`
      untouched; `RemoveAppCapabilities` guards per-value.
- [x] Tests: `test-edition-packaging.ps1` asserts distinct ProgIds per
      edition; manual HKCU verification on a disposable profile remains
      a release-checklist item (no registry writes during agent runs).
- [x] Doku: `docs/PACKAGING.md` association section updated.
- [ ] Portable `--register-associations` opt-in flag (spec §8) —
      documented future work, not implemented in M20.
