# Code Signing (M29)

NØVA Windows artifacts support **Authenticode** signing with SHA-256
digests and RFC-3161 timestamps via the Windows SDK `signtool.exe`.

> **Status:** the infrastructure is complete and tested with local
> self-signed certificates. No publicly trusted publisher certificate
> is configured — all current artifacts are `SIGNING_NOT_CONFIGURED` /
> `UNSIGNED`. Self-signed certificates are for technical tests only and
> are **never** presented as trusted publisher signatures.

## Signing statuses

| Status | Meaning |
|---|---|
| `SIGNED` | Authenticode signature created **and** `signtool verify /pa` passed (trusted chain + timestamp). |
| `UNSIGNED` | File was processed but carries no signature. |
| `SIGNING_NOT_CONFIGURED` | No certificate configured — unsigned dev/RC build. |
| `SIGNATURE_INVALID` | Signature exists but verification failed (tampered file, untrusted chain). |
| `CERTIFICATE_EXPIRED` | Certificate outside its validity window — hard error. |
| `SIGNING_FAILED` | Signing/verification tooling or certificate unusable — hard error. |

## Configuration

Precedence: environment variables > `packaging/signing.json`
(gitignored; template: `packaging/signing.example.json`).

Store mode (recommended):

```json
{
  "thumbprint": "AA:BB:...",
  "store": "My",
  "timestampUrl": "http://timestamp.digicert.com",
  "requireSigned": false
}
```

Environment equivalents (CI secrets): `NOVA_SIGN_CERT_THUMBPRINT`,
`NOVA_SIGN_CERT_STORE`, `NOVA_SIGN_CERT_SUBJECT`, `NOVA_SIGN_PFX_PATH`,
`NOVA_SIGN_PFX_PASSWORD` (env only — never on a command line, never
logged), `NOVA_SIGN_TIMESTAMP_URL`, `NOVA_SIGN_REQUIRE=1`.

PFX mode imports the certificate into `Cert:\CurrentUser\My`
temporarily, signs, then **removes it again** — private key material
never persists in the store or repository.

## Certificate requirements

- Validity window must cover the signing date (`CERTIFICATE_EXPIRED`
  otherwise).
- Must carry the **Code Signing EKU** `1.3.6.1.5.5.7.3.3`.
- Chain must build; self-signed certs are flagged `selfSigned` and can
  never produce `SIGNED` (verify requires trust).
- Selection by **thumbprint** (preferred, unambiguous) or subject
  substring.

## Pipeline order (M29.3)

1. `package-windows.ps1` — release build → version/icon checks →
   **sign app EXE** → re-hash → ZIP → `SHA256SUMS.txt` →
   `SIGNING-STATUS.json` → `BUILD-INFO.json`.
2. `package-installer.ps1` — `dotnet publish` → copy setup EXE →
   **sign setup EXE** → smoke tests → checksum → `SIGNING-STATUS.json`.
3. `build-release-package.ps1` — stage `release/<version>/windows/`
   → `release-manifest.json` + `RELEASE.md` + `SHA256SUMS.txt`.
4. `verify-release.ps1` — read-only: hashes, manifest, signatures,
   unexpected/missing files.

Signatures are created **before** archiving so the ZIP contains signed
executables; archive hashes are computed afterwards.

## Publisher metadata (M29.5)

- App EXE: `NOVA_COMPANY_NAME` / `NOVA_COPYRIGHT` env vars feed
  `build.rs` (defaults are placeholders — see below).
- Installer: `-p:NovaCompany` / `-p:NovaCopyright` MSBuild properties
  (same env passthrough in `package-installer.ps1`).
- **Open placeholders:** the legal publisher identity is NOT decided.
  `NOVA Flash Studio contributors` is a placeholder, not a company.
  The rights holder must set real values before public release.

## SmartScreen / Defender honesty (M29.7)

- Authenticode proves *origin integrity*, not reputation.
- SmartScreen reputation accrues per signer+binary over time; a new
  signature does **not** eliminate SmartScreen prompts.
- Defender may still flag unknown publishers; signing reduces but does
  not remove that risk.
- No Defender exclusions, SmartScreen bypasses or UAC weakening exist
  or may be added.

## Real release checklist (later)

1. Obtain a code-signing certificate (OV/EV) for the actual rights
   holder; complete identity validation.
2. Configure thumbprint via env/`signing.json`; run
   `package-windows.ps1` + `package-installer.ps1` per edition.
3. `build-release-package.ps1 -RequireSigned`.
4. `verify-release.ps1` — all green.
5. Fill EULA placeholders (`docs/EULA.md`) and publisher metadata.
6. Only then publish.
