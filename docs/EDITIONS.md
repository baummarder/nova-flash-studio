# NØVA Editions — User, Edu, Enterprise

NØVA Flash Studio ships as **three independent products built from one
shared Rust repository**. There are no forks and no duplicated codebases:
every edition contains the same editor, runtime, formats, AI operations
and project format. Editions differ only in branding, startup copy,
configuration location, packaging metadata and a small feature/module
registry that reserves slots for future edition-specific work.

> This is **not** a licensing system. There are no product keys,
> activation, online authentication, license servers, seat limits, DRM or
> subscription checks. All editions work fully offline.

## Editions at a glance

| | NØVA Flash Studio (User) | NØVA Flash Studio Edu | NØVA Flash Studio Enterprise |
|---|---|---|---|
| Audience | Private users, hobby devs, animators, indie game devs | Schools, teachers, students | Companies, teams, professionals |
| Window title | `NØVA Flash Studio` | `NØVA Flash Studio Edu` | `NØVA Flash Studio Enterprise` |
| Splash tagline | VECTOR · TIMELINE · SWF | LERNEN · ERSTELLEN · VERSTEHEN | PRODUKTION · KOLLABORATION · KONTROLLE |
| Portable ZIP | `NOVA-User-Portable.zip` | `NOVA-Edu-Portable.zip` | `NOVA-Enterprise-Portable.zip` |
| Setup EXE | `NOVA-User-Setup.exe` | `NOVA-Edu-Setup.exe` | `NOVA-Enterprise-Setup.exe` |
| Install dir | `%ProgramFiles%\NØVA Flash Studio` (or per-user) | `…\NØVA Flash Studio Edu` | `…\NØVA Flash Studio Enterprise` |
| Uninstall key | `NOVAFlashStudio` | `NOVAFlashStudioEdu` | `NOVAFlashStudioEnterprise` |
| `.nova` ProgId | `NOVAFlashStudio.Project` | `NOVAFlashStudioEdu.Project` | `NOVAFlashStudioEnterprise.Project` |
| Reserved modules | — | — (classroom aktiv seit M20) | Administration & Zusammenarbeit (M21) |

All three editions contain the **full shared feature set**: vector editor,
timeline/keyframes, skeletal animation + IK + skinning, game development,
AS3 subset, SWF/XFL/FLA workflows, asset management, AI-assisted editing,
compatibility tools and templates.

## Configuration and data directories

Per-edition directories guarantee that installed editions never overwrite
each other's settings, workspaces, recent state, UI preferences, AI
provider configuration, logs, recovery/autosave data or cache.

Windows (`%APPDATA%`):

```
%APPDATA%\NOVA Flash Studio\            ← User (legacy location, preserved)
%APPDATA%\NOVA Flash Studio Edu\
%APPDATA%\NOVA Flash Studio Enterprise\
```

macOS (`~/Library/Application Support/`) uses the same directory names;
Linux uses `$XDG_CONFIG_HOME/nova-flash-studio-<edition>` (or
`~/.config/nova-flash-studio-<edition>`).

Untitled-project autosave recovery lives in the temp directory under
`nova-flash-studio-<edition>` (e.g. `nova-flash-studio-edu`).

Two explicit overrides exist and apply to **all** editions:

- `NOVA_FLASH_STUDIO_CONFIG_DIR` — replaces the whole configuration
  directory (used by tests and portable setups).
- `NOVA_EDITION` — selects the edition at runtime when the binary was not
  compiled with a fixed edition (development builds only; packaged
  binaries are baked at compile time).

Credentials keep using the platform credential mechanism (Windows DPAPI
via `nova-ai`); nothing is stored in plaintext when DPAPI is available.

## Portable mode

The portable ZIPs contain only the product payload (exe, docs, legal
files). Portable mode uses the same per-edition configuration directory
as the installed variant. To keep a portable copy fully self-contained,
set `NOVA_FLASH_STUDIO_CONFIG_DIR` next to the executable or in the
environment before first launch.

## Project compatibility

`.nova` projects are **identical across editions**: the format contains
no edition marker, unknown optional metadata is ignored safely, and
there are no edition-specific restrictions. A project saved in Edu opens
in User and Enterprise unchanged (and vice versa). This is verified by
`crates/nova-core/tests/editions.rs`.

## Building

```powershell
# One edition (tests included unless -SkipTests)
./scripts/package-windows.ps1 -Edition edu
./scripts/package-installer.ps1 -Edition edu -UseExistingPayload

# All three editions: 3 portable ZIPs + 3 installers
./scripts/package-editions.ps1
```

`package-windows.ps1` sets `NOVA_EDITION` for the Rust build; the value
is baked into `nova-app/build.rs` version resources (ProductName,
OriginalFilename, InternalName) and read at runtime by
`nova_core::Edition::current()`.

For a quick local check of a non-default edition:

```powershell
$env:NOVA_EDITION = 'enterprise'
cargo run -p nova-app -- --version   # -> NØVA Flash Studio Enterprise 0.15.0-rc1
```

## Module registry

`crates/nova-app/src/edition.rs` defines `EditionProfile` per edition:
app name, tagline, audience line, bundle id and a module list. Modules
marked `reserved` are documented future work (administration &
collaboration → M21) and are **not** presented as implemented. The Edu
`classroom` module became active in M20 — it is the only gate for
`Mode::Classroom` (`edition::has_active_module("classroom")`).
New edition-specific features should register there instead of adding
scattered edition checks.
