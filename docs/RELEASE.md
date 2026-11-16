# Release Process (M29)

How a NØVA Flash Studio Windows release candidate is built, signed,
verified and packaged. Nothing here publishes anything — pushes,
tags and GitHub releases remain manual maintainer actions.

## Pipeline

```powershell
# 1. Per edition: tests + release build + payload + installer
powershell -ExecutionPolicy Bypass -File scripts/package-installer.ps1 -Edition user
powershell -ExecutionPolicy Bypass -File scripts/package-installer.ps1 -Edition edu
powershell -ExecutionPolicy Bypass -File scripts/package-installer.ps1 -Edition enterprise

# 2. Assemble release/<version>/windows/ + manifest + summary
powershell -ExecutionPolicy Bypass -File scripts/build-release-package.ps1
#    add -RequireSigned once a trusted certificate is configured

# 3. Verify (read-only)
powershell -ExecutionPolicy Bypass -File scripts/verify-release.ps1 `
    -ReleaseDir release/<version>/windows

# 4. Regression suites
powershell -ExecutionPolicy Bypass -File scripts/test-installer.ps1
powershell -ExecutionPolicy Bypass -File scripts/test-edition-packaging.ps1
powershell -ExecutionPolicy Bypass -File scripts/test-signing.ps1
```

## What each step guarantees

- `package-windows.ps1` — workspace tests (unless `-SkipTests`),
  edition-branded release EXE with embedded icon/version resource,
  `--version`/`--startup-check` smoke tests, optional Authenticode
  signing **before** zipping, SHA-256 of final bytes, portable ZIP,
  `BUILD-INFO.json`, `SIGNING-STATUS.json`, archive of previous
  payload via `archive-releases.ps1`.
- `package-installer.ps1` — `dotnet publish` single-file self-contained
  setup, edition identity via generated `EditionInfo.cs` (restored
  afterwards), optional signing of the setup EXE, `--version` /
  `--smoke-test` gates, checksum append.
- `build-release-package.ps1` — copies final artifacts into
  `release/<version>/windows/<edition>/`, recomputes SHA-256 on the
  copied files, writes `release-manifest.json` (per-artifact: name,
  edition, version, platform, arch, size, sha256, signing status,
  publisher, cert thumbprint, signature validity, timestamp flag) and
  a human-readable `RELEASE.md`.
- `verify-release.ps1` — read-only: re-hashes every file, cross-checks
  `SHA256SUMS.txt`, `release-manifest.json` and `SIGNING-STATUS.json`,
  runs `signtool verify /pa` on SIGNED entries, reports missing or
  unlisted executables. **Never modifies files.**

## Unsigned development builds

Without certificate configuration every artifact is honestly marked
`SIGNING_NOT_CONFIGURED`/`UNSIGNED`. RC builds may ship unsigned; a
release declared "signed" must contain only verified `SIGNED` entries
(`-RequireSigned` enforces this).

## Before any public release

- [ ] Real code-signing certificate for the actual rights holder.
- [ ] Publisher metadata (`NOVA_COMPANY_NAME`, `NOVA_COPYRIGHT`).
- [ ] EULA placeholders filled + legal review (`docs/EULA.md`).
- [ ] `verify-release.ps1` all green; installer tests all editions.
- [ ] SmartScreen/Defender expectations documented to users
      (signing ≠ reputation, see `docs/CODE_SIGNING.md`).
