# Milestone 7.3 Report — Real-World Validation and RC2

Status: **release candidate** `v0.3.0-rc2`. This report records what was actually
executed on the development host. It distinguishes **PASS**, **FAIL**,
**PARTIAL**, **NOT VERIFIED** and **NOT AVAILABLE**. Mocked/offline tests and
live tests are labelled separately. No API key or credential value is included.

## Scope

Milestone 7.3 validated the existing NØVA Flash Studio systems in a real
environment and produced the rc2 candidate:

- local and cloud AI provider connectivity;
- the AI Agent Editor backend end-to-end workflow;
- credential and workspace security;
- generated SWF playback in Ruffle and Adobe Flash Player;
- `SymbolClass` execution before and after SWF import/re-export;
- XFL export/import and native `.fla` refusal;
- Windows installer, portable ZIP and headless install/uninstall;
- regressions, performance and known limitations.

No major feature was added, no working subsystem was rewritten, no test was
weakened or deleted, and no unverified capability is claimed. The `.nova` format
version remains 1 and CLI behavior is unchanged.

## Environment

| Item | Value |
| --- | --- |
| Host | Windows, `C:\Users\admin\Documents\Flash-Editor` |
| Git | 2.55.0.windows.5 |
| Rust | Workspace edition 2024, MSRV 1.88 |
| Ollama | 0.40.16 at `http://localhost:11434`, 32 installed models |
| Live model | `qwen2.5-coder:7b` |
| Ruffle | 0.7.1, `ruffle.exe`, 26,999,808 bytes |
| Adobe projector | debug build 32.0.0.465, `flashplayer_32_sa_debug.exe`, 16,811,576 bytes |
| Adobe Animate | not installed; **NOT AVAILABLE** |
| Signing certificate | none; **NOT AVAILABLE** |

## Repository and Git status

| Check | Result |
| --- | --- |
| Existing repository | **NOT AVAILABLE** — `.git` was absent at the start of M7.3 |
| Baseline repository | **PASS** — `main` initialized and baseline committed as `165416f3a0ca3f7aebe9672ec38c3224e41cb6af` |
| Ignore policy | **PASS** — excludes `target/`, `dist/`, packaging binaries/objects, logs, secrets, provider/runtime settings, credentials and local projects |
| Baseline secret scan | **PASS** — no high-risk credential pattern in tracked content |
| Remote | **NOT AVAILABLE** — no remote configured |
| M7.3 source changes | live tests plus documentation; no production-code behavior changes |

The repository is local-only. There is no CI run, remote branch or pull request
associated with rc2.

## Provider connectivity

### Live local Ollama — PASS

| Check | Result |
| --- | --- |
| Service discovery | **PASS** — 32 models returned |
| Chat request with `qwen2.5-coder:7b` | **PASS** — normal chat response |
| Agent request/parsing | **PASS** — reply parsed with 2,009 context characters, `cloud=false` |
| Unavailable endpoint | **PASS** — `AiError::Network`; no fabricated answer |

Command:

```powershell
cargo test -p nova-ai --test live_ollama -- --ignored --nocapture --test-threads=1
```

The live tests are opt-in `#[ignore]` tests. The default workspace suite remains
offline and uses mocked HTTP and `MemoryCredentialStore`.

### Cloud providers — NOT AVAILABLE

No `GROQ_API_KEY`, `OPENAI_API_KEY`, `OLLAMA_API_KEY` or `ANTHROPIC_API_KEY` was
set, and no cloud provider entry existed in the user configuration. Ollama Cloud,
Groq and custom OpenAI-compatible cloud endpoints were therefore not contacted.
No cloud connectivity, cloud consent flow or cloud error handling is claimed as
live-verified.

## AI Agent Editor end-to-end

The live backend workflow in `AiEditor` was exercised with local Ollama:

| Step | Result |
| --- | --- |
| Explain selected `scripts/Main.as` | **PASS** |
| Propose new `scripts/Added.as` | **PASS** |
| Reject proposal | **PASS** — no file written, project unchanged |
| Accept proposal | **PASS** — file created and project script list updated; original `scripts/Main.as` intact |
| Build after accept | **PASS** — `1478 Bytes, 48 Frames, 145 Objekte, 4 Shapes, 2 Sprites, 2 Skript(e)` |
| File undo | **PASS** — new script removed |
| Unavailable endpoint | **PASS** — network error, no fabricated reply |

Command:

```powershell
cargo test -p nova-app live_agent_workflow_reject_accept_build_and_undo -- --ignored --nocapture --test-threads=1
```

This is app-level backend evidence. The full interactive GUI click-through
(provider settings, key entry, every AI panel state) is **PARTIAL** and was not
repeated as a single manual session.

## Security audit

| Check | Result |
| --- | --- |
| Windows credential backend | **PASS** — DPAPI `CryptProtectData` with NØVA-specific entropy |
| Encrypted blob contains plaintext | **PASS** — `dpapi_roundtrip_does_not_store_plaintext` confirms the secret is absent |
| Provider JSON contains key | **PASS** — no key or key field is serialized |
| Error redaction | **PASS** — key-like material in provider errors is redacted |
| Credential filename traversal | **PASS** — unsafe provider IDs are sanitized |
| Workspace root confinement | **PASS** — absolute paths, drive prefixes, `..`, symlink escapes and missing-file modifications are rejected |
| Atomic apply/undo | **PASS** — preview, apply and file undo tests pass |
| Secret scan | **PASS** — only a deliberate `Bearer secret-key` test fixture is tracked; no real key was found |
| Environment scan | **PASS** — only `OLLAMA_CONTEXT_LENGTH` was present; no provider key |
| User config scan | **PASS** — only `flash-player.json` existed; no `ai-providers.json` or credential blob |
| Full interactive key-entry GUI | **PARTIAL** — write-only UI and tests exist; no full click-through was repeated |

Security tests are automated. No real cloud credential was stored, printed or
transmitted during M7.3.

## SWF runtime

### Fixture

`validation.nova` was created in a disposable temporary directory with:

- document class `Main`;
- linked class `Added`;
- a shape and a sprite;
- 24 frames, labels `intro`/`finish`;
- a linear tween;
- trace statements;
- a red root marker.

The final `validation.swf` is 875 bytes with 24 frames, 2 shapes, 2 sprites, 2
ABC scripts and SymbolClass links `(0, Main)` and `(4, Added)`. An independent
parse found 84 tags; both `DoABC2` entries contain the class names and trace
strings.

### Ruffle 0.7.1 — PASS

PrintWindow captures showed:

- the red document-class marker;
- the cyan linked sprite rotating;
- the orange tween moving;
- timeline and label execution.

Ruffle emitted recurring `ReferenceError: Error #1056: Cannot create property …
on Main` for named timeline children. It was non-fatal; playback continued. The
NØVA compiler rejects `dynamic`, so a `public dynamic class Main` workaround was
not available and was not added.

### Adobe debug projector 32.0.0.465 — PARTIAL / NOT VERIFIED

The projector process started and an `Adobe Flash Player 32` window opened. The
SWF area rendered blank for the validation and sample SWFs, and no
`flashlog.txt` trace was produced. Adobe load, visual playback, timeline
rendering and script execution are **NOT VERIFIED**. No Adobe compatibility
claim is made. The projector is not bundled by NØVA.

## SymbolClass execution

| Fixture | Result |
| --- | --- |
| One-frame `symbolclass.nova` / `symbolclass.swf` (687 bytes, 11 tags) | **PASS** — linked `Added` sprite rotated; links `(0, Main)` and `(3, Added)` |
| One-frame SWF import → re-export (`symbolclass-rebuilt.swf`, 698 bytes) | **PASS** — links preserved; linked class still rotated |
| Multi-frame `validation.swf` import → rebuild (`validation-rebuilt.swf`, 1,150 bytes, 175 tags) | **PARTIAL** — frames, ABC scripts and links `(0, Main)`/`(49, Added)` preserved; red marker and shapes rendered, but the linked `Added` rotation was not observed after the round-trip |
| Multi-frame rebuilt SWF in Adobe | **NOT VERIFIED** — Adobe rendered blank |

The multi-frame limitation is caused by per-frame reconstruction producing
distinct object identities. Structural `SymbolClass` links survive, but the
linked display class does not visibly execute across rebuilt frames. The
one-frame round-trip is the verified working case.

## FLA / XFL

| Check | Result |
| --- | --- |
| CLI XFL export | **PASS** — `validation-xfl.zip` 1,953 bytes; `DOMDocument.xml` 5,076 bytes; `scripts/Main.as`; `scripts/Added.as`; `README.txt` |
| Native `.fla` export | **PASS (refused by design)** — exit 1 with an explicit message; no fake `.fla` written |
| CLI XFL import | **PASS** — 2 layers, 24 frames, 2 scripts, 1 symbol, 2 shapes; `README.txt` reported as unsupported |
| Nested `DOMDocument.xml`, symbol, shape and unsupported-element fixtures | **PASS** in `nova-formats` unit tests |
| XFL export → re-import → export round-trip | **PARTIAL** — re-exported 1,941-byte ZIP imported, but frames 1–22 were empty and tween data was not preserved; geometry and library symbols were approximated |
| Adobe Animate/Flash authoring compatibility | **NOT AVAILABLE** — Adobe Animate is not installed and no Adobe-authored `.fla` fixture exists |
| Native FLA export | **NOT AVAILABLE** by design |

The XFL package is an explicitly unverified compatibility package. It is not a
replacement for a `.fla` file tested in Adobe Animate.

## Installer and portable package

The rc2 Windows artifacts were verified with:

| Check | Result |
| --- | --- |
| `Setup.exe --version` | **PASS** — `NØVA Flash Studio Setup 0.3.0-rc2` |
| `Setup.exe --smoke-test` | **PASS** — payload hash, 45 entries, one EXE, EULA and `smoke-test: OK` |
| Portable ZIP extraction | **PASS** — 45 entries with binary, docs and legal files |
| Portable `NØVA Flash Studio.exe --version` | **PASS** — `NØVA Flash Studio 0.3.0-rc2` |
| Portable `--startup-check` | **PASS** — icon, runtime status, first-launch flag and config path |
| Headless per-user install to a temporary directory | **PASS** — `--per-user --no-desktop --no-start-menu --no-assoc`; files, manifest, uninstaller and HKCU entry created |
| Installed `--version` / `--startup-check` | **PASS** |
| Headless uninstall | **PASS** — install directory removed; `%APPDATA%\NOVA Flash Studio\flash-player.json` hash unchanged |
| All-users/UAC install | **NOT VERIFIED** in M7.3; requires a disposable profile/VM |
| Full `.swf` association registration | **NOT VERIFIED** in M7.3; current state was inspected only |
| Code signing | **NOT AVAILABLE** — no legitimate certificate; artifacts remain unsigned |

The headless install used the actual uninstaller filename
`NØVA Flash Studio Uninstall.exe`. The installer did not delete user
configuration.

## Regressions

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | **PASS** |
| `cargo check --workspace` | **PASS** |
| `cargo test --workspace` | **PASS** — 254 passed, 0 failed, 4 ignored live tests |
| `.nova` format version | **PASS** — still version 1 |
| CLI commands and output behavior | **PASS** — unchanged |
| Adobe-default / Ruffle-explicit policy | **PASS** — unit tests and temp-config startup checks |
| No silent runtime/provider fallback | **PASS** — missing/invalid runtime and invalid JSON produce visible errors |
| Existing mocked AI/provider/file tests | **PASS** |
| Existing format, build, compiler and runtime tests | **PASS** |

No regression was found in the default offline suite. The issues listed under
"New bugs and fixes" are documented limitations, not newly introduced
regressions.

## Performance

Single-host Windows release-build measurements:

| Measurement | Result |
| --- | --- |
| `--startup-check`, 5 runs | avg 84.7 ms; min 72.4 ms; max 119.6 ms (first run cold) |
| GUI launch, 3 s after start | 179.3 MB working set; 200.5 MB peak; window created; clean exit |
| Large project load (`nova check`, 2.33 MB JSON, 100 layers, 10,000 frames, 2,000 authored objects) | 28 ms |
| Large project build | 247 ms; 2,034,019-byte SWF; 100 frames, 200,000 placed objects, 2,020 shapes, 20 sprites, 2 scripts |
| Large SWF import | 2,124 ms |
| Small SWF import (`validation.swf`) | 15 ms |
| Peak working set during large build | ~95 MB (polled) |

These are evidence from one machine, not a benchmark guarantee. The large build
intentionally exercises held-frame placement, so the SWF contains 200,000 placed
objects even though the authored project has 2,000 objects.

## New bugs and fixes

No application production-code fix was made in M7.3. The milestone added live
tests, documentation and one packaging-metadata fix. Issues discovered during
validation:

| Issue | Status |
| --- | --- |
| Rc2 installer `--version` appended `+<git-sha>` because .NET source-revision metadata was enabled | **FIXED** — `IncludeSourceRevisionInInformationalVersion=false` in the installer project; rebuilt `Setup.exe --version` now prints exactly `NØVA Flash Studio Setup 0.3.0-rc2` |
| Ruffle `Error #1056` for named timeline children on the document class | **PARTIAL** — non-fatal, unfixed; no `dynamic` workaround |
| Multi-frame SWF import/re-export loses visible linked-class execution | **PARTIAL** — structural links preserved, visible execution not |
| Adobe projector renders tested SWFs blank | **NOT VERIFIED** — process/window starts; likely environment/tool issue; not worked around |
| XFL round-trip leaves frames 1–22 empty and loses tween data | **PARTIAL** — documented format limitation; not fixed in M7.3 |
| Full interactive AI GUI workflow not repeated | **PARTIAL** — backend live E2E passed |

No issue was hidden or presented as fixed without evidence.

## Artifacts and checksums

Current artifacts:

- `NØVA Flash Studio.exe`
- `NOVA-Flash-Studio-0.3.0-rc2-windows-x64.zip`
- `NOVA-Flash-Studio-0.3.0-rc2-windows-x64-Setup.exe`
- `dist/windows/SHA256SUMS.txt`

`SHA256SUMS.txt` is authoritative. The previous rc1 artifacts were backed up
outside `dist/windows` before the rc2 build and are not presented as the current
release. The packaged Markdown may predate the final checksum table because
documentation is embedded before the checksum file is regenerated.

Final values at publication are recorded in `dist/windows/SHA256SUMS.txt`:

The authoritative size and SHA-256 values are recorded in
`dist/windows/SHA256SUMS.txt`. The verified values at publication were:

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `NØVA Flash Studio.exe` | 23,522,816 | `0DC17188E7130258A59DA3F5E8FA9E53FEA515E9CA81BBACF17D0E322E68A734` |
| `NOVA-Flash-Studio-0.3.0-rc2-windows-x64.zip` | 9,899,639 | `A4E64885DD119980650169AD64D6F40E8BD6B531615C1497BA19BC6837E591C2` |
| `NOVA-Flash-Studio-0.3.0-rc2-windows-x64-Setup.exe` | 81,812,243 | `0C38BA5A655B6027C4B3D1788013FBF9A29275218D4265671CE9576122949EF6` |

The payload hash reported by `Setup.exe --smoke-test` was
`a4e64885dd119980650169ad64d6f40e8bd6b531615c1497ba19bc6837e591c2`, matching the
portable ZIP. The packaged copy of this document may contain a slightly earlier
snapshot because documentation is embedded before the checksum file is
regenerated; `dist/windows/SHA256SUMS.txt` is always authoritative.

The artifacts are unsigned. No signing claim is made.

## Remaining limitations

- Adobe visual playback, Adobe load and Adobe script execution are **NOT
  VERIFIED**. The debug projector starts a blank window in this environment.
- Adobe Animate/Flash authoring compatibility is **NOT AVAILABLE**; no
  Adobe-authored `.fla` fixture exists.
- XFL round-trip is **PARTIAL**: frames 1–22 empty and tween data lost.
- Multi-frame `SymbolClass` execution after SWF import/re-export is **PARTIAL**.
- Ruffle `Error #1056` for named timeline children is non-fatal and unfixed.
- Ollama Cloud, Groq and custom cloud AI endpoints are **NOT AVAILABLE** for live
  testing because no credential or endpoint was configured.
- The full interactive AI Agent GUI workflow is **PARTIAL**.
- All-users/UAC installation and full `.swf` association registration were not
  re-run in M7.3; they require a disposable profile/VM.
- Full GUI editing walkthrough and long-running stability/memory profiling are
  not automated.
- Native macOS/Linux builds were not produced on this host.
- Artifacts are unsigned; no legitimate signing certificate is available.

## Result summary

| Area | Result |
| --- | --- |
| Repository/baseline | **PASS** (local repository; remote **NOT AVAILABLE**) |
| Live local Ollama provider | **PASS** |
| Live cloud providers | **NOT AVAILABLE** |
| AI Agent backend E2E | **PASS** |
| AI Agent interactive GUI | **PARTIAL** |
| Security audit | **PASS** (GUI key-entry session **PARTIAL**) |
| Ruffle visual playback | **PASS** for the M7.3 fixtures |
| Adobe visual playback | **NOT VERIFIED** |
| One-frame SymbolClass round-trip | **PASS** |
| Multi-frame SymbolClass round-trip | **PARTIAL** |
| XFL export/import | **PASS** |
| XFL round-trip fidelity | **PARTIAL** |
| Native FLA export | **NOT AVAILABLE** by design |
| Adobe authoring compatibility | **NOT AVAILABLE** |
| Installer/portable rc2 | **PASS** |
| All-users/UAC and association registration | **NOT VERIFIED** |
| Default regression suite | **PASS** |
| Performance | **PASS** (single-host evidence) |
| Signing | **NOT AVAILABLE** |
