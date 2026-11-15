# Packaging

NØVA Flash Studio is a native Rust/eframe application. Release packages contain
the compiled executable or bundle plus the branding assets and documentation;
target machines do **not** need a Rust toolchain.

The source version is the single release version. It lives in
`[workspace.package] version` in `Cargo.toml` (currently `0.15.0-rc1`). The Windows
resource, splash screen, macOS `Info.plist` and package scripts read that value.

## Editions (M19+)

NØVA ships as **three editions from one codebase** — User, Edu and
Enterprise — selected at build time via `NOVA_EDITION`
(`packaging/editions.json` is the packaging single source of truth; see
`docs/EDITIONS.md` and `docs/EDITION_ARCHITECTURE.md`).

```text
dist/
  windows/
    user/
      NØVA Flash Studio/NØVA Flash Studio.exe
      NOVA-User-Portable.zip
      NOVA-User-Setup.exe
      SHA256SUMS.txt
      BUILD-INFO.json
    edu/
      NØVA Flash Studio Edu/…
      NOVA-Edu-Portable.zip
      NOVA-Edu-Setup.exe
      SHA256SUMS.txt
      BUILD-INFO.json
    enterprise/
      NØVA Flash Studio Enterprise/…
      NOVA-Enterprise-Portable.zip
      NOVA-Enterprise-Setup.exe
      SHA256SUMS.txt
      BUILD-INFO.json
```

Each edition has its own product folder, portable ZIP, installer,
uninstall key, ProgId family, bundle id and configuration directory.
`scripts/package-editions.ps1` builds all three; the per-edition scripts
are `package-windows.ps1 -Edition <id>` and
`package-installer.ps1 -Edition <id>`. `scripts/test-edition-packaging.ps1`
verifies manifest ↔ artifacts ↔ archive consistency (checksums included).

## Artifact layout (legacy single-edition view)

```text
dist/
  windows/
    NØVA Flash Studio/
      NØVA Flash Studio.exe
      README.md
      docs/*.md
      EULA-DRAFT.md
      LICENSE-MIT.txt
      LICENSE-APACHE-2.0.txt
      THIRD-PARTY-NOTICES.md
      licenses/*.txt
    NOVA-Flash-Studio-<version>-windows-x64.zip      (portable, pre-M19 naming)
    NOVA-Flash-Studio-<version>-windows-x64-Setup.exe (installer, pre-M19 naming)
    SHA256SUMS.txt
  macos/
    NØVA Flash Studio.app/
      Contents/
        Info.plist
        PkgInfo
        MacOS/nova-flash-studio
        Resources/nova-flash-studio.icns
    NOVA-Flash-Studio-<version>-macos.zip
  linux/
    AppDir/
      AppRun
      nova-flash-studio.desktop
      nova-flash-studio.png
      usr/bin/nova-flash-studio
      usr/share/applications/nova-flash-studio.desktop
      usr/share/icons/hicolor/...
      usr/share/metainfo/studio.nova.flash.appdata.xml
    NØVA-Flash-Studio.AppImage
```

`dist/` is ignored by Git. Checked-in templates live under `packaging/`.

Release packages contain the executable/bundle, docs and legal files only. They
do **not** contain per-user AI provider settings, DPAPI credential blobs or
runtime settings. Those live outside the install directory under
`%APPDATA%\NOVA Flash Studio{, Edu, Enterprise}` per edition
(`ai-providers.json`, `credentials/`,
`flash-player.json`). They are not part of the portable ZIP or the installer
payload and are never removed by uninstall.

## Requirements

### All platforms

- Rust 1.88 or newer, edition 2024
- A desktop session and a working graphics backend for runtime testing
  (DX12/Vulkan on Windows, Metal on macOS, Vulkan/OpenGL plus X11 or Wayland on
  Linux)
- Inkscape and Pillow only if you regenerate icons

### Windows

- Visual Studio Build Tools / MSVC linker (the standard Rust MSVC toolchain)
- Windows SDK `rc.exe` for the executable icon and version resource. The build
  script searches `%ProgramFiles(x86)%\Windows Kits\10\bin\<version>\<arch>`.
  If it is missing, the build still succeeds without metadata, but
  `scripts/package-windows.ps1` fails rather than shipping an unbranded EXE.
- PowerShell 5.1 or newer for packaging
- .NET 8 SDK (`dotnet`) only when building the installer
  (`scripts/package-installer.ps1`). The installer is a self-contained
  single-file WinForms executable; no WiX, Inno Setup or NSIS is required.

### macOS

- macOS with Xcode command line tools (`codesign`, `plutil`, `ditto`)
- A matching Rust target; for Apple Silicon use `aarch64-apple-darwin`, for
  Intel `x86_64-apple-darwin`
- Optional: a Developer ID Application identity and a `notarytool` keychain
  profile for public distribution

### Linux

Build dependencies (Ubuntu/Debian example):

```bash
sudo apt-get install -y \
  build-essential pkg-config \
  libxkbcommon-dev libwayland-dev libx11-dev libxi-dev \
  libxcursor-dev libxrandr-dev libxinerama-dev libgl1-mesa-dev \
  libasound2-dev libudev-dev
```

`appimagetool` must be available in `PATH` or via `APPIMAGETOOL`. The AppImage
recipe stages the complete AppDir before invoking it.

## Windows

```powershell
# Runs the workspace tests, release build, metadata checks and ZIP creation
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1

# Faster packaging when tests already passed
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -SkipTests

# Skip the optional ZIP
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -SkipTests -NoZip

# Build the installer on top of the portable ZIP (rebuilds payload by default)
powershell -ExecutionPolicy Bypass -File scripts/package-installer.ps1

# Reuse an existing ZIP and only compile the installer
powershell -ExecutionPolicy Bypass -File scripts/package-installer.ps1 -UseExistingPayload

# One-shot: portable ZIP + installer via the release dispatcher
powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1 -Target windows -SkipTests
```

The script:

1. builds `nova-app` in release mode;
2. copies the binary to `NØVA Flash Studio.exe`;
3. bundles `legal/` (license texts, `THIRD-PARTY-NOTICES.md`, `EULA-DRAFT.md`)
   and every SPDX text under `LICENSES/` into `licenses/`;
4. verifies `ProductName`, `FileVersion` and the embedded icon;
5. runs `NØVA Flash Studio.exe --version` and `--startup-check` as smoke tests;
6. writes `SHA256SUMS.txt`;
7. optionally creates the ZIP.

The resulting folder is self-contained from a Rust perspective. The Windows
system still supplies the graphics runtime and standard system DLLs. The ZIP
is the **portable distribution**: extract it anywhere and run
`NØVA Flash Studio.exe`; no installation or administrator rights are needed.

### Installer

`scripts/package-installer.ps1` produces
`NOVA-Flash-Studio-<version>-windows-x64-Setup.exe`: a self-contained,
single-file .NET 8 WinForms wizard (about 78 MB because it carries the .NET
runtime plus the embedded payload ZIP). It requires no WiX/Inno/NSIS toolchain.
Sources live in `packaging/windows/installer/`.

The wizard offers:

- welcome, license (embedded **German EULA draft**, acceptance required) and
  finish pages with the NØVA logo;
- **per-user install is the default**
  (`%LOCALAPPDATA%\Programs\NØVA Flash Studio`, HKCU metadata, current-user
  Start Menu/desktop shortcuts, **no elevation, no UAC prompt**);
- **all-users install is an explicit choice**
  (`C:\Program Files\NØVA Flash Studio` + HKLM + all-users Start Menu).
  The scope page explains that administrator rights are required; when the
  setup is not elevated, clicking *Installieren* relaunches the same exe
  through the standard Windows UAC prompt (`ShellExecute` verb `runas`) with
  the chosen options. A denied prompt shows an error and stays on the page —
  there is **no silent fallback to per-user**;
- a custom install directory with browse dialog (honored for both scopes;
  switching scope updates the default path but preserves a custom one);
- optional all-users Start Menu shortcut (default on) and optional desktop
  shortcut (per-user desktop for per-user installs, public desktop for
  all-users installs);
- opt-in file associations for `.nova` (default on) and `.swf` (default off)
  under `HKCU\Software\Classes` (per-user) or `HKLM\Software\Classes`
  (all-users). The previous default value is recorded in the manifest and
  restored on uninstall **only while the association still points to this
  installation**. Windows 10/11 decides the actual default application in
  system settings — the installer registers the open command but does not
  and cannot forcibly override a user's default app;
- **scope-separated upgrade handling:** existing per-user and machine-wide
  installs are detected independently (HKCU and HKLM uninstall keys). An
  install may only be upgraded in place — same scope **and** same directory.
  A different scope or a different directory is rejected with an explanation
  (wizard validation and engine check); the user must uninstall the existing
  installation first. Stale files from the previous manifest are removed and
  recorded shortcuts/associations are refreshed;
- a generated `NOVA Flash Studio Uninstall.exe` inside the install
  directory, registered under `...\Uninstall\NOVAFlashStudio`
  (`UninstallString` / `QuietUninstallString`). Per-user uninstalls run
  unelevated; machine-wide uninstalls re-request elevation through UAC.
  The setup exe itself implements `--uninstall`.

There are **no product keys, license keys, activation codes, online
activation, license-server communication, account registration or periodic
license checks** anywhere in the installer or the application; the software
is fully usable offline.

Headless modes (used by CI and tests; `--per-user` is the default scope,
`--all-users` opts in and requires elevation, `--notify` shows a result
dialog for elevated runs):

```powershell
Setup.exe --version
Setup.exe --smoke-test          # verifies embedded payload hash/content
Setup.exe --install <dir> [--per-user|--all-users] [--no-desktop]
                               [--no-start-menu] [--assoc-nova] [--assoc-swf]
                               [--no-assoc] [--quiet] [--notify]
Setup.exe --uninstall [--quiet] [--notify]
```

**Uninstall never touches user data.** Projects and settings live under
`%APPDATA%\NOVA Flash Studio` (see `crates/nova-app/src/flash.rs`), outside the
install directory; the uninstaller only removes files listed in
`install-manifest.json`, recorded shortcuts, registry entries it created, and
the install directory itself when empty. File associations are only removed
when they still point at this installation; a changed association is left
alone. Portable ZIP installs are completely independent: they have no
registry entries, shortcuts or manifest and are never affected by
install/uninstall cycles.

Limitations:

- The installer and the app are **unsigned**; no Authenticode certificate is
  available, so SmartScreen may warn.
- The UAC relaunch passes options on the command line; a denied prompt is
  reported but there is no retry loop inside the wizard (the user restarts
  the install).
- The EULA is a **draft** (`legal/EULA-DRAFT.md`) and the license choice
  (`MIT OR Apache-2.0`, mirroring `Cargo.toml`) still needs owner/legal sign-off.
- The wizard UI is German to match the application; CLI help is bilingual-
  neutral English/German mix (German primary).
- File associations register `shell\open` only; no `Open With` capability
  entries or icon overlays are written.

### Milestone 7.2 release verification

The Windows artifacts were regenerated after the AI Agent Editor, provider and
`SymbolClass` changes. The packaging run performs:

- `cargo test --workspace` (254 tests);
- `cargo build --release -p nova-app`;
- `ProductName`, `FileVersion` and icon extraction checks;
- `NØVA Flash Studio.exe --version` and `--startup-check`;
- `Setup.exe --version` and `--smoke-test` (embedded payload);
- `SHA256SUMS.txt` generation.

The final artifact hashes and sizes are recorded in
[`MILESTONE_7_2_REPORT.md`](MILESTONE_7_2_REPORT.md) and in
`dist/windows/SHA256SUMS.txt`:

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `NØVA Flash Studio.exe` | 23,522,816 | `5AE00C26DA352E51EB81159DF6D05436BF893E06E803626D34321CE13B30AC0B` |
| `NOVA-Flash-Studio-0.3.0-rc1-windows-x64.zip` | 9,882,733 | `DD3A5BDEAA0C9EA0305FB7A602C0ABDE5BA2D49CC0D7FF20C4A1351C68184763` |
| `NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe` | 81,795,347 | `87BAB81BE244F1404A42C862F777D1DB73E6D1B1385A1AC1649A5538C67ECE98` |

The artifacts remain unsigned. No live AI provider request and no visual Adobe
playback are part of this packaging verification. Because this document is
packaged before `SHA256SUMS.txt` is written, the checksum file is authoritative
if a later documentation-only rebuild changes the ZIP or Setup hash.

### Milestone 7.3 / rc2 verification

The rc2 build is produced by the same packaging pipeline after the M7.3 live
tests and documentation. The package run executes the workspace tests, the
release build and the branded-artifact checks, then writes:

- `NØVA Flash Studio.exe`
- `NOVA-Flash-Studio-0.3.0-rc2-windows-x64.zip`
- `NOVA-Flash-Studio-0.3.0-rc2-windows-x64-Setup.exe`
- `SHA256SUMS.txt`

Verification performed on the rc2 artifacts:

| Check | Result |
| --- | --- |
| `Setup.exe --version` | PASS — `NØVA Flash Studio Setup 0.3.0-rc2` |
| `Setup.exe --smoke-test` | PASS — payload hash, 45 entries, one EXE, EULA and `smoke-test: OK` |
| Portable ZIP extraction | PASS — 45 entries with binary, docs and legal files |
| Portable `--version` / `--startup-check` | PASS |
| Headless per-user install to a temporary directory (`--no-desktop --no-start-menu --no-assoc`) | PASS — files, manifest, uninstaller and HKCU entry; no shortcuts or associations |
| Installed app `--version` / `--startup-check` | PASS |
| Headless uninstall | PASS — install directory removed; `%APPDATA%\NOVA Flash Studio\flash-player.json` hash unchanged |
| All-users/UAC install | NOT VERIFIED — requires a disposable profile/VM |
| Signing | NOT AVAILABLE — no legitimate certificate; artifacts remain unsigned |

Final sizes and SHA-256 values are in `dist/windows/SHA256SUMS.txt`. The
packaged Markdown predates the final checksum table; the checksum file is
authoritative. The previous rc1 artifacts were backed up outside `dist/windows`
until the rc2 build and verification passed.

## macOS

```bash
bash scripts/package-macos.sh

# Package an already-built release executable
bash scripts/package-macos.sh --no-build

# Explicit architecture
MACOS_TARGET=aarch64-apple-darwin bash scripts/package-macos.sh
```

The script creates a standard bundle:

- `Contents/Info.plist` rendered from `packaging/macos/Info.plist`
- `Contents/PkgInfo`
- `Contents/MacOS/nova-flash-studio`
- `Contents/Resources/nova-flash-studio.icns`

The bundle identifier is `studio.nova.flash`, the executable is named
`nova-flash-studio`, and Finder displays **NØVA Flash Studio**. The app does not
declare itself as a `.swf` handler, so installing it does not silently change
the user's file associations.

### Signing and notarization

For local development, an unsigned/ad-hoc bundle may run after the usual macOS
Gatekeeper override. For distribution:

```bash
export CODESIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export NOTARY_PROFILE="nova-notary"       # created with notarytool store-credentials
bash scripts/package-macos.sh
```

The script signs with hardened runtime and a secure timestamp, verifies the
signature, submits the ZIP with `notarytool --wait`, staples the ticket, and
rebuilds the archive. Notes:

- Sign nested binaries first if you add helpers or dynamic libraries later.
- Notarization requires a paid Apple Developer account.
- `--deep` is convenient for this single-binary bundle but is not a substitute
  for signing every nested component in a larger app.
- Verify with `spctl --assess --type execute --verbose=4 "NØVA Flash Studio.app"`.

Universal binaries are not produced automatically. Build both targets and
combine them with `lipo`, or add a dedicated universal CI job.

## Linux

```bash
bash scripts/package-linux.sh

# Stage AppDir without rebuilding
bash scripts/package-linux.sh --no-build

# Explicit appimagetool
APPIMAGETOOL=/path/to/appimagetool bash scripts/package-linux.sh
```

The script installs:

- `usr/bin/nova-flash-studio`
- `AppRun` (resolves the AppDir and executes the binary with explicit arguments)
- `nova-flash-studio.desktop` at the AppDir root and in `usr/share/applications`
- hicolor icons from 16 to 1024 px
- AppStream metadata in `usr/share/metainfo/studio.nova.flash.appdata.xml`

The output is `dist/linux/NØVA-Flash-Studio.AppImage`. It runs without a Rust
toolchain. The host must still provide the graphics stack (Vulkan/OpenGL, X11 or
Wayland) expected by `eframe`/`wgpu`. The script warns if `ldd` reports missing
libraries; those libraries are not bundled automatically. For broad
distribution, test on both an X11 and a Wayland session and consider a
`linuxdeploy`-based bundle for additional libraries.

The desktop entry intentionally omits `MimeType=application/x-shockwave-flash`,
so installing the AppImage does not claim `.swf` files.

## Build-system notes

- **Cargo workspace:** the existing workspace is reused; only `nova-app` is
  packaged as a GUI.
- **cargo-bundle:** a reasonable future option for macOS `.app` generation, but
  it is not required here. The checked-in `Info.plist` and script make the
  bundle layout explicit and auditable. No cargo-bundle dependency is added.
- **AppImage tooling:** `appimagetool` performs the final image assembly.
  `linuxdeploy` is optional for bundling extra shared libraries; the current
  recipe stages only the application and metadata.
- **Windows resources:** `crates/nova-app/build.rs` generates a `.rc` file,
  invokes `rc.exe`, and passes the `.res` file to the linker. There is no
  build-time dependency on a third-party resource crate.
- **Release profile:** thin LTO and one codegen unit are configured in the
  workspace `[profile.release]`.

## GitHub Actions

`.github/workflows/release.yml` defines:

| Job | Runner | Output |
| --- | --- | --- |
| `windows` | `windows-latest` | ZIP, unpacked directory and Setup.exe installer |
| `macos` | `macos-latest` | unsigned `.app` ZIP |
| `linux` | `ubuntu-24.04` | AppImage |
| `structure` | `ubuntu-24.04` | icon/container/metadata validation |

It is prepared for tags matching `v*` and manual dispatch. It has **not been
executed in this repository session**. The macOS job produces an unsigned
artifact; signing/notarization secrets must be added before public distribution.

## Verification status

| Check | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Source compiles | yes, native | NOT AVAILABLE (no macOS host) | NOT AVAILABLE (no Linux host) |
| Unit/integration tests | yes, native | NOT AVAILABLE (no macOS host) | NOT AVAILABLE (no Linux host) |
| Package script executed | yes, native | no macOS host | no Linux host |
| Artifact structure checked | yes | template checks only | template checks only |
| Executable `--version` smoke | yes | not applicable here | not applicable here |
| Embedded icon/version resource | yes | n/a | n/a |
| Portable ZIP | yes, built + integrity-checked | n/a | n/a |
| Installer `--smoke-test` | yes | n/a | n/a |
| Headless install → uninstall cycle | yes (per-user and all-users, incl. `.nova` association) | n/a | n/a |
| Wizard GUI launch | yes (window opened, closed cleanly) | n/a | n/a |
| UAC elevation (all-users install + uninstall) | yes, real prompts approved on this machine | n/a | n/a |
| Runtime/playback test | no projector available | no projector | no projector |

### Recorded verification (Windows, 2026-11-11)

The following results were recorded on the Windows development machine for this
milestone. They are not a claim about macOS or Linux.

- `cargo fmt --all -- --check`: clean.
- `cargo test --workspace`: 167 tests passed, 0 failed.
- `python scripts/verify-packaging.py`: all icon/container/metadata checks passed.
- `scripts/package-windows.ps1 -SkipTests`: release build and packaging succeeded.
- `NØVA Flash Studio.exe` version resource: `ProductName=NØVA Flash Studio`,
  `FileVersion=0.2.0`, `ProductVersion=0.2.0`.
- Extracted 32 px EXE icon is pixel-identical to
  `assets/branding/nova-logo-32.png` (RGBA difference has no bounding box).
- `NØVA Flash Studio.exe --version` printed `NØVA Flash Studio 0.2.0`;
  `--startup-check` reported a valid window icon, player status and config path.
- GUI launch smoke test: the process remained alive with a visible
  `NØVA Flash Studio` window and closed on request.
- The splash and main window were captured at 1456 × 939. The splash showed the
  logo, version `0.2.0`, progress bar and fade; the toolbar and **Flash Player**
  menu both displayed the exact label **Open in Flash Player**.
- Clicking **Open in Flash Player** with no export showed the clear
  “Keine SWF-Datei vorhanden” dialog.
- The **Flash Player Integration** dialog recognized
  `C:/Windows/notepad.exe` as a `Windows-EXE` and showed the ready status. This
  was a validation check only; no settings were saved and the test projector was
  not launched.
- ZIP integrity: `zipfile.testzip()` returned `None`; the archive contains the
  branded folder, EXE, README and all docs.
- Checksums are written to `dist/windows/SHA256SUMS.txt`.

### Recorded verification — Milestone 5 installer (Windows, 2026-11-11)

- `scripts/package-windows.ps1 -SkipTests`: portable ZIP rebuilt with the legal
  files (`EULA-DRAFT.md`, `LICENSE-*.txt`, `THIRD-PARTY-NOTICES.md`,
  `licenses/*.txt` — 15 SPDX texts).
- `scripts/package-installer.ps1 -UseExistingPayload`: `dotnet publish`
  single-file self-contained build succeeded; setup exe is ~75.7 MB.
- `NOVA-Flash-Studio-0.2.0-windows-x64-Setup.exe --version` prints
  `NØVA Flash Studio Setup 0.2.0`; `--smoke-test` reports payload SHA-256,
  33 archive entries (after the revision; 31 before), 1 embedded app EXE and
  the EULA text.
- Headless per-user install to a temp directory succeeded (exit 0): files,
  `install-manifest.json`, HKCU uninstall entry and the `.nova` association
  were all registered.
- Headless uninstall (`--uninstall --quiet`) removed the install directory,
  the uninstall registry key, the ProgId and the `.nova` default value;
  `%APPDATA%` user data was not touched (verified by code path and directory
  listing).
- Reinstall over the same directory (upgrade path) succeeded and removed a
  previously registered association when the option was unchecked.
- The wizard GUI was launched, showed the `NØVA Flash Studio – Setup` window
  and closed cleanly on request. Individual wizard pages were not clicked
  through manually.

### Recorded verification — installer revision (Windows, 2026-11-11)

Revision scope: all-users default, UAC elevation, German EULA, no
key/activation concepts.

- Non-elevated `--install <dir> --all-users` fails with exit code 2 and a
  clear admin/UAC message; **no directory or registry entry is created**
  (no silent per-user fallback — verified).
- Elevated all-users install (`Start-Process -Verb RunAs`, UAC approved)
  succeeded: files installed, `HKLM\...\Uninstall\NOVAFlashStudio` written,
  `.nova` association registered under `HKLM\Software\Classes`, manifest
  scope `HKLM`.
- Elevated all-users install with default options created the Start Menu
  shortcut under `%ProgramData%\...\Programs\NØVA Flash Studio` and the
  desktop shortcut under `%PUBLIC%\Desktop`; both were removed by uninstall.
- Machine-wide uninstall launched **non-elevated** re-requested elevation
  through UAC; the elevated child removed the install directory, the HKLM
  uninstall key, the ProgId and the `.nova` default value.
- Per-user install/uninstall (`--per-user`) still works without elevation
  and cleans HKCU keys, associations and the install directory.
- `Setup.exe --smoke-test` reports the new payload hash and the German EULA
  text (~4.9 KB); `verify-packaging.py` passes all checks.

Code signing and SmartScreen behavior remain untested.

### Recorded verification — scope correction (Windows, 2026-11-11)

The default scope was corrected back to **per-user**; all-users remains an
explicit, UAC-elevated choice.

- Default `--install <dir>` (no scope flag) installs per-user: HKCU uninstall
  key, current-user Start Menu + Desktop shortcuts, manifest scope `HKCU`,
  **no elevation** (exit 0).
- Per-user uninstall ran unelevated and removed the directory, HKCU key and
  both user shortcuts.
- Elevated `--install <dir> --all-users` (UAC approved): manifest scope
  `HKLM`, HKLM uninstall key, HKLM `.nova` association, shortcuts under
  `%ProgramData%\...\Start Menu` and `%PUBLIC%\Desktop`.
- Elevated reinstall into the **same** directory/scope succeeded (upgrade).
- A per-user install attempt into a **different** directory while the
  machine-wide install existed was rejected with exit 2 and no files
  (engine-level scope/directory guard).
- Non-elevated machine-wide uninstall re-requested UAC; elevated child
  removed the directory, machine shortcuts, HKLM uninstall key and the
  `HKLM\Software\Classes` `.nova`/ProgId entries.
- Wizard GUI launched and closed cleanly.

Not exercised: an actual UAC **denial** (requires interactive "No" click);
the denial path throws a clear `InstallException` and is covered by code
review only.

### Recorded verification — v0.3.0-rc1 (Windows, 2026-11-12)

- Workspace version `0.3.0-rc1`; installer `AssemblyVersion`/`FileVersion`
  `0.3.0.0`. `scripts/build-release.ps1 -Target windows` rebuilt the release
  binary, portable ZIP and installer end to end.
- `NOVA-Flash-Studio-0.3.0-rc1-windows-x64.zip` and
  `NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe` produced;
  `SHA256SUMS.txt` (outside the payload) records all artifact hashes — see
  that file for the authoritative values, since this document ships inside
  the ZIP.
- `NØVA Flash Studio.exe --version` → `NØVA Flash Studio 0.3.0-rc1`;
  `Setup.exe --version` → `NØVA Flash Studio Setup 0.3.0-rc1`;
  `Setup.exe --smoke-test` → payload hash + EULA check OK.
- Installer payload now carries 37 entries (legal/docs included).
- Milestone 6 added an explicit `--migrate` scope-migration workflow
  (per-user ↔ all-users, elevation required, verified both directions).
- Playback was verified with **Ruffle 0.7.1** for the first time: generated
  SWFs execute (see `docs/SWF_RUNTIME_COMPATIBILITY.md`). Adobe Flash Player
  remains untested.

### Recorded verification — Milestone 7.2 (Windows, 2026-11-13)

- `cargo fmt --all -- --check`: clean.
- `cargo test --workspace`: 254 tests passed, 0 failed.
- `cargo build --release --workspace`: succeeded.
- `scripts/package-windows.ps1 -SkipTests`: release build, branded EXE, icon and
  version metadata checks, `--version`/`--startup-check` smoke tests and portable
  ZIP succeeded.
- `scripts/package-installer.ps1 -UseExistingPayload`: `dotnet publish`
  single-file setup succeeded; `--version` and `--smoke-test` passed. The
  payload reports 44 ZIP entries and 1 embedded app EXE.
- `python scripts/verify-packaging.py`: all icon, container, metadata, legal and
  installer-source checks passed.
- ZIP integrity: 44 entries, including `README.md`, all 24 `docs/*.md` files
  (including this report) and the legal files. `SHA256SUMS.txt` is outside the
  ZIP and contains the EXE, ZIP and Setup hashes.
- UTF-8 checks: `SHA256SUMS.txt` and all packaged Markdown files decode as UTF-8
  without BOM. The Unicode product/executable names survive in the ZIP.
- Adobe process launch through `--open` was observed with a local debug
  projector; visual playback is **NOT VERIFIED**.
- No live AI provider request was made. AI tests use mocked HTTP.

The repository is deliberately explicit: a Windows artifact was built and
smoke-tested on the Windows development machine; macOS and Linux artifacts are
scripted and structurally validated but were **not built on those platforms in
this session**. Do not describe them as tested native builds until the
corresponding CI or host run succeeds.

### Milestone 8 / 0.4.0-rc1 verification (Windows)

The validated rc2 artifacts were copied to `dist/archive/0.3.0-rc2/` before the
packager deleted `dist/windows`. The M8 build then ran:

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | **PASS** |
| `cargo check --workspace` | **PASS** (no warnings) |
| `cargo test --workspace` during packaging | **PASS** — 281 passed, 0 failed, 4 ignored |
| `powershell scripts/build-release.ps1 -Target windows` | **PASS** |
| Release executable version/resource check | **PASS** — `0.4.0-rc1` |
| `NØVA Flash Studio.exe --version` | **PASS** — `NØVA Flash Studio 0.4.0-rc1` |
| `--startup-check` | **PASS** (packager gate) |
| ZIP creation | **PASS** |
| `Setup.exe --version` | **PASS** — `NØVA Flash Studio Setup 0.4.0-rc1` |
| `Setup.exe --smoke-test` | **PASS** — 46 payload entries, payload hash matches ZIP |
| Headless per-user install to a temp directory | **PASS** |
| Installed exe version | **PASS** — `0.4.0-rc1` |
| HKCU uninstall entry | **PASS** — `DisplayVersion 0.4.0-rc1` |
| Headless quiet uninstall | **PASS** — install directory and HKCU entry removed |
| `python scripts/verify-packaging.py` | **PASS** |

Artifacts:

- `NØVA Flash Studio.exe` (portable payload)
- `NOVA-Flash-Studio-0.4.0-rc1-windows-x64.zip`
- `NOVA-Flash-Studio-0.4.0-rc1-windows-x64-Setup.exe`

`dist/windows/SHA256SUMS.txt` is authoritative for the final sizes and
SHA-256 values; it is generated after the documentation is packaged so this
page does not go stale. The binaries are unsigned. macOS/Linux artifacts were
not built on their native hosts.

## Release checklist

1. Update `Cargo.toml` workspace version.
2. Run `cargo fmt --all -- --check`.
3. Run `cargo test --workspace`.
4. Regenerate icons if branding changed; run `python scripts/verify-packaging.py`.
5. Build and smoke-test the Windows package.
6. Build the Windows installer (`scripts/package-installer.ps1`); run a
   headless install/uninstall cycle on a test machine and record the result.
7. Review `legal/EULA-DRAFT.md` and confirm the `MIT OR Apache-2.0` license
   choice before any public release; regenerate `THIRD-PARTY-NOTICES.md` after
   dependency changes (`python scripts/gen-third-party-notices.py`).
8. Build on macOS, sign and notarize if distributing, then verify with `spctl`.
9. Build and test the AppImage on X11 and Wayland.
10. Run the manual projector test in `README.md`; record the exact player build.
11. Upload artifacts and checksums; keep the untested-platform caveat in release
    notes.
