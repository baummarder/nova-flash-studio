# Mega Milestone 29 — Code Signing & Release Preparation

**Version:** `0.20.0-rc1` · **Status:** technically complete —
**`SIGNING_NOT_CONFIGURED`** (no trusted certificate; honest unsigned
status, nothing claimed as publicly signed).

## 1. Architecture audit

Existing pipeline (M19–M27): `package-windows.ps1` (edition-branded
release build → staged payload → archive → swap), `package-installer.ps1`
(`dotnet publish` single-file setup, generated `EditionInfo.cs`),
`archive-releases.ps1` (checksum-verified archive),
`test-edition-packaging.ps1` / `test-installer.ps1` /
`test-archive-releases.ps1`, `packaging/editions.json` as identity
source of truth, version resource via `crates/nova-app/build.rs`,
GitHub `release.yml` (unsigned builds). No signing, manifest or
release-package stage existed — all added in M29.

## 2. New components

| File | Purpose |
|---|---|
| `scripts/lib/Signing.ps1` | signtool discovery, config resolution, cert validation, sign+verify, status writer |
| `scripts/sign-windows.ps1` | standalone sign/verify CLI for arbitrary paths |
| `scripts/verify-release.ps1` | read-only release verification (hashes, manifest, signatures, unexpected files) |
| `scripts/build-release-package.ps1` | assembles `release/<version>/windows/` + `release-manifest.json` + `RELEASE.md` |
| `scripts/test-signing.ps1` | 12 technical tests (self-signed certs) |
| `packaging/signing.example.json` | config template (`signing.json` gitignored) |
| `docs/CODE_SIGNING.md`, `docs/RELEASE.md`, `docs/SECURITY.md` | new docs |

## 3. Signing architecture

- **signtool.exe** located via Windows SDK kits (x64) or PATH.
- **Config:** `packaging/signing.json` or `NOVA_SIGN_*` env vars
  (thumbprint preferred; subject fallback; PFX via env password only).
- **Validation:** existence → validity window (`CERTIFICATE_EXPIRED`)
  → Code Signing EKU `1.3.6.1.5.5.7.3.3` → chain build (self-signed
  flagged, never `SIGNED`).
- **Sign:** `/fd SHA256 /tr <TSA> /td SHA256 /sha1 <thumb>` then
  `verify /pa`; timestamp via RFC-3161 (default `timestamp.digicert.com`).
- **Pipeline order:** app EXE signed **before** ZIP (signed exe inside
  archive); setup EXE signed before checksums; all hashes recomputed on
  final bytes.
- **PFX mode:** temporary import into `Cert:\CurrentUser\My`, removed
  after signing — no key material persists.
- **Statuses:** `SIGNED`, `UNSIGNED`, `SIGNING_NOT_CONFIGURED`,
  `SIGNATURE_INVALID`, `CERTIFICATE_EXPIRED`, `SIGNING_FAILED`.
  Missing cert → unsigned build continues; invalid cert → abort;
  `-RequireSigned` makes any non-SIGNED a hard error.

## 4. Publisher metadata

`NOVA_COMPANY_NAME` / `NOVA_COPYRIGHT` env vars feed `build.rs`
(version resource) and `-p:NovaCompany/NovaCopyright` (installer
csproj). Defaults `NØVA Flash Studio contributors` are documented
**placeholders** — no company was invented; rights holder must set
real values before release.

## 5. Release package

`release/0.20.0-rc1/windows/` (gitignored, generated):

```
user/        NOVA-User-Portable.zip, NOVA-User-Setup.exe,
             NØVA Flash Studio/NØVA Flash Studio.exe, SIGNING-STATUS.json, ...
edu/         ...
enterprise/  ...
SHA256SUMS.txt  release-manifest.json  RELEASE.md  CHANGELOG.md  EULA.md  CODE_SIGNING.md
```

`release-manifest.json` per artifact: path, edition, product, version,
platform, arch, size, sha256, signingStatus, publisher, cert
thumbprint, signatureValid, timestamped.

## 6. Verification results

- `test-signing.ps1`: **12/12 PASS** — missing cert, expired →
  `CERTIFICATE_EXPIRED`, wrong EKU → `SIGNING_FAILED`, valid
  self-signed → `READY` (selfSigned flagged), sign produces signature,
  untrusted chain → `SIGNATURE_INVALID` (honest), tampered file fails,
  unsigned fails, release verify pass/fail on hash mismatch.
- `verify-release.ps1` on `release/0.20.0-rc1/windows`: **21 checks,
  0 problems, 0 warnings**.
- `test-installer.ps1` (all editions, headless install/verify/
  uninstall, user-data preservation): **PASS**.
- `test-edition-packaging.ps1`: **PASS** (after one manual
  `archive-releases.ps1` rerun — see §8).
- `test-archive-releases.ps1`: **PASS**.
- `cargo test --workspace`: PASS · `cargo fmt --check`: clean ·
  `cargo clippy --workspace --all-targets -D warnings`: clean.

## 7. Artifact hashes (unsigned, 0.20.0-rc1)

| Artifact | SHA-256 |
|---|---|
| user portable | `86CC1D84F98EFD6BE27DFF9F21E07FB7801880FF43C6FED7910D2847FA51D43C` |
| user setup | `ED533F331A71282AC5EF04FD87F15458D42F8BCCF2466DA94AD46684543E3FC6` |
| user exe | `1FC2B57B2E5A818C4831DF95DDC042DBA0F3DD48D05DE72D24DCBD7A8168009C` |
| edu portable | `CB3EEE013C8FD4F021B26863ED0BCE1FADAC8E6234C264bb1af673f038ceec86` |
| edu setup | `776E5BBD4BD10D82BBFF2EA23D5E270C0296E3709E49BCC6F22C0F7A72EF16B0` |
| edu exe | `DF2F85C7FE3B32B7884FB70C9BAA9C3895391E5A7C010F5546262FC1EA19E88A` |
| enterprise portable | `5AD15307F1DD70CC99BE130E825B0104BAEBB53B2D8A1A0044379B71ABD2A1C8` |
| enterprise setup | `FADAE2D60DDB86D3AE0E4E74D4A88B29EDE3CB4594E87EF6356C3D23EAAC2394` |
| enterprise exe | `BB20C5FFA807622CA6387B9F58C67AB4D2DA8C2767C312C9C33C538F3A33686F` |

## 8. Known limitations / open items

- **No trusted certificate** — all artifacts `SIGNING_NOT_CONFIGURED`.
  Real signing requires rights-holder certificate + config.
- **Archive ordering quirk (pre-existing):** `archive-releases.ps1`
  runs inside `package-windows.ps1` before the last edition's swap, so
  the final edition's archive lags one run; rerun
  `archive-releases.ps1` after the last edition (documented in
  `docs/RELEASE.md` order).
- **PS 5.1 constraint:** all new `.ps1` files are ASCII-only (UTF-8
  smart quotes/em-dashes inside strings break parsing) — noted in
  `AGENTS.md`.
- **Self-signed ≠ trusted:** pipeline correctly refuses `SIGNED` for
  untrusted chains; only CA-trusted certs yield `SIGNED`.
- SmartScreen/Defender: signing does not guarantee reputation —
  documented honestly in `docs/CODE_SIGNING.md`.
- EULA placeholders (`[RECHTEINHABER]` etc.) and publisher metadata
  remain open release prerequisites.

## 9. Commits

- `5c6ac59` — M29.1–29.6 signing pipeline + manifest/verify
- `134fb24` — M29 docs + version `0.20.0-rc1`
- (this report) — M29 final report

No push, no release, no tag.

## 10. Manual tests still required

- Real-certificate signing run once a cert exists.
- GUI installer walk-through per edition/language (headless tests pass).
- SmartScreen/Defender behavior on a clean Windows machine.
