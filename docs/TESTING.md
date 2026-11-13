# Testing & QA Report — v0.3.0-rc2 (Milestones 7, 7.1, 7.2 and 7.3)

What was actually executed for this release candidate, with commands and
results. Statuses: **PASS** / **FAIL** / **PARTIAL** / **NOT VERIFIED** /
**NOT AVAILABLE**. This report keeps the Milestone 6 CLI/installer results, the
Milestone 7 runtime evidence, the Milestone 7.1 format-conversion evidence, the
Milestone 7.2 AI/safe-edit evidence and the Milestone 7.3 real-world validation.

## Automated tests

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | **PASS** (exit 0) |
| `cargo test --workspace` | **PASS** — 254 tests across all suites, 0 failed, 4 opt-in live tests ignored; `nova-ai` has 33, `nova-app` has 74 and `nova-formats` has 20 |
| `cargo test -p nova-ai --test live_ollama -- --ignored` | **PASS** — 3 live Ollama tests (discovery/chat/agent/negative endpoint) |
| `cargo test -p nova-app live_agent_workflow_reject_accept_build_and_undo -- --ignored` | **PASS** — live AI Agent backend E2E |
| `cargo check -p nova-app` | **PASS** — no warnings |
| `cargo build -p nova-app` | **PASS** |
| `dotnet build packaging/windows/installer/NovaFlashStudio.Setup.csproj -c Release` | **PASS** — 0 warnings, 0 errors |
| `powershell scripts/build-release.ps1 -Target windows` | **PASS** — workspace tests, release build, ZIP, Setup.exe, payload/installer smoke tests and checksums |
| `python scripts/verify-packaging.py` | **PASS** — icon containers, `Info.plist`, AppStream, desktop entry, legal files and installer sources |

`cargo test --workspace` was run after the Milestone 7 runtime changes. The
`nova-app` suite includes the new first-launch, migration, persistence, explicit
Ruffle and safe-launch tests listed below. The installer build and packaging
check were rerun after the Milestone 7 documentation/code changes.

### Regenerated Windows artifacts

`scripts/build-release.ps1 -Target windows` regenerated the Windows payload and
installer after the M7 changes. `dist/windows/SHA256SUMS.txt` lists the SHA-256
for the executable, ZIP and Setup.exe; the first line preserves the UTF-8
product name `NØVA Flash Studio.exe`.

`package-installer.ps1` now reads the existing checksum list explicitly as
UTF-8; otherwise Windows PowerShell 5.1 would read the BOM-less file as ANSI and
double-encode the `Ø` in `NØVA` on the next rewrite.

## Milestone 7 automated coverage

| Requirement | Test(s) |
| --- | --- |
| Adobe is the default and first launch is required when unconfigured | `flash::tests::default_settings_select_adobe_and_need_setup` |
| A valid Adobe path suppresses first launch | `flash::tests::first_launch_setup_is_skipped_when_adobe_is_valid` |
| “Later” is persisted and does not repeat every launch | `flash::tests::first_launch_setup_is_not_repeated_after_skipping` |
| Valid native runtime path | `flash::tests::projector_validation_accepts_native_binary` |
| Missing / wrong / deleted runtime is a clear error | `flash::tests::missing_runtime_is_a_clear_error`, `flash::tests::wrong_extension_is_rejected` |
| Settings persist across restart, including spaces/Unicode | `flash::tests::settings_roundtrip_with_unicode_and_spaces` |
| A second save replaces the previous settings file | `flash::tests::saving_twice_replaces_the_previous_settings_file` |
| UTF-8 BOM in a hand-edited settings file | `flash::tests::settings_file_with_utf8_bom_is_accepted` |
| Legacy system-default and projector-path migration | `flash::tests::legacy_system_default_migrates_to_adobe_setup`, `flash::tests::legacy_adobe_projector_migrates_without_losing_the_path`, `flash::tests::legacy_ruffle_projector_migrates_as_explicit_ruffle` |
| Ruffle is never selected implicitly | `flash::tests::adobe_is_default_and_ruffle_is_not_used_implicitly` |
| Explicit Ruffle uses only the Ruffle path | `flash::tests::explicit_ruffle_selection_uses_only_the_ruffle_path` |
| Switching back to Adobe preserves both paths | `flash::tests::switching_back_to_adobe_preserves_both_paths` |
| SWF opening selects the configured runtime and passes one argument | `flash::tests::opening_an_exported_swf_prepares_selected_runtime_and_one_argument` |
| Spaces and Unicode in SWF/runtime paths | `flash::tests::spaces_and_unicode_in_swf_and_runtime_paths_are_safe_arguments` |
| Exported SWF is structurally ready | `flash::tests::exported_swf_is_ready_for_the_player` |
| Missing/invalid SWF is rejected before launch | `flash::tests::missing_swf_is_rejected`, `flash::tests::invalid_swf_magic_is_rejected` |
| A started process is not reported as playback | `flash::tests::launch_report_does_not_claim_playback` |
| `.swf` startup paths are recognized case-insensitively | `app::tests::recognizes_swf_startup_paths_case_insensitively` |

These tests validate the model, command construction and error behavior. They do
not start Adobe or Ruffle; the GUI launch test below supplies actual runtime
evidence.

## Milestone 7.1 format coverage

`nova-formats` contains 20 tests. They exercise real bytes and ZIP/XML fixtures;
none of them mocks an importer or exporter.

| Requirement | Test(s) |
| --- | --- |
| `.nova` dispatch and source metadata survive a round trip | `conversion::tests::nova_roundtrip_preserves_source` |
| Format detection by magic and extension | `conversion::tests::detects_formats_by_magic` |
| FWS and CWS import plus version reporting | `formats::swf::tests::imports_fws_cws_and_reports_the_version` |
| ZWS/LZMA import | `formats::swf::tests::imports_zws_lzma` |
| SWF 9, 10 and 13 imports | `formats::swf::tests::imports_swf_version_nine`, `formats::swf::tests::imports_nova_exported_swf`, `formats::swf::tests::imports_zws_lzma` |
| Sprites, depth layers, frame labels and ABC preservation | `formats::swf::tests::reconstructs_sprites_layers_labels_and_preserves_abc` |
| Unsupported SWF tags are reported without dropping the original | `formats::swf::tests::reports_unknown_tags_without_dropping_the_original` |
| Corrupt SWF is rejected | `formats::swf::tests::rejects_corrupt_swf` |
| Minimal and nested FLA/XFL containers | `formats::fla::tests::imports_minimal_xfl_fixture`, `formats::fla::tests::imports_nested_document_shape_symbol_and_unsupported_element` |
| Non-ZIP FLA and ZIP without `DOMDocument.xml` are rejected | `formats::fla::tests::rejects_non_zip_fla_bytes`, `formats::fla::tests::rejects_zip_without_domdocument` |
| Experimental XFL export can be re-imported | `formats::fla::tests::exports_and_reimports_xfl_package` |
| `.nova` → SWF → import → SWF → import keeps structure and ABC | `conversion::tests::nova_swf_import_export_import_roundtrip_preserves_structure_and_abc` |
| `.fla`/XFL → NØVA → SWF keeps document and timeline | `conversion::tests::fla_to_nova_to_swf_conversion_keeps_document_and_timeline` |
| Analysis distinguishes preserved ABC from source | `analysis::tests::analysis_distinguishes_preserved_abc` |
| Capability extensions are case-insensitive and reports count issues | `capabilities::tests::extensions_are_case_insensitive`, `capabilities::tests::report_counts_and_text` |

### Format conversion results

| Path | Result |
| --- | --- |
| `.nova` → SWF | PASS — structural re-import, labels/layers/dimensions preserved |
| `.nova` → SWF → NØVA → SWF | PASS — ABC bytes identical; output is structurally compatible, not byte-identical |
| `.fla`/XFL → NØVA → SWF | PASS — 550×400 document and timeline frames survive |
| `.nova` → XFL ZIP → NØVA | PASS — document settings, layers and `.as` scripts survive |
| `.swf` (FWS/CWS/ZWS) → NØVA | PASS for supported tags; unsupported tags reported |
| Native `.fla` export | REFUSED by design — no fake file is written |

No generated FLA or XFL file has been opened in Adobe Animate/Flash, and no
reconstructed SWF has been added to the Ruffle playback matrix. These remain
**NOT VERIFIED** and are not claimed as compatible.

## Milestone 7.2 AI and safe-edit coverage

`nova-ai` contains 33 tests. They use `MockTransport` and a memory credential
store; no test makes a live provider request. `nova-app` adds four project-model
apply tests for AI previews.

| Requirement | Test(s) |
| --- | --- |
| Provider settings never contain a key | `provider::tests::provider_json_never_contains_the_api_key` |
| Settings round-trip and default selection | `provider::tests::settings_roundtrip_is_atomic_and_does_not_include_secrets`, `provider::tests::default_provider_and_override_are_explicit` |
| Ollama, Groq and OpenAI-compatible model discovery | `provider::tests::discovers_ollama_models_and_sends_bearer_token_for_cloud`, `provider::tests::discovers_groq_models` |
| Chat parsing and credential handling | `provider::tests::ollama_chat_is_parsed`, `provider::tests::openai_chat_is_parsed_and_requires_key`, `provider::tests::invalid_credentials_are_reported_without_echoing_the_key` |
| Cloud consent is required and per-session override wins | `agent::tests::cloud_requests_require_explicit_consent`, `agent::tests::per_session_provider_override_wins_over_default` |
| Context includes only selected source and marks preserved ABC | `agent::tests::context_contains_structure_and_only_selected_source`, `agent::tests::unselected_source_is_not_included`, `agent::tests::preserved_abc_is_described_as_bytecode_not_source` |
| JSON reply, build action and shell-action rejection | `agent::tests::parses_json_changes_and_build_action`, `agent::tests::plain_text_reply_is_not_treated_as_a_file_change` |
| Root confinement and traversal rejection | `files::tests::rejects_parent_traversal_and_absolute_paths` |
| Create-overwrite and modify-missing protection | `files::tests::preview_refuses_to_overwrite_and_apply_is_reversible` |
| Delete confirmation and undo | `files::tests::delete_requires_confirmation_and_can_be_undone` |
| Build success, diagnostics and preserved-ABC skipping | `build::tests::valid_project_builds`, `build::tests::compiler_errors_are_reported_with_path_and_line`, `build::tests::preserved_abc_is_not_treated_as_source` |
| DPAPI does not store plaintext | `credentials::tests::dpapi_roundtrip_does_not_store_plaintext` |
| AI preview creates/modifies/deletes project scripts | `ai_editor::tests::create_preview_adds_a_script_to_the_project`, `ai_editor::tests::modify_preview_updates_source_without_changing_identity`, `ai_editor::tests::delete_preview_removes_script_and_clears_object_links` |
| Preserved ABC is rejected for modify and delete | `ai_editor::tests::preserved_abc_is_rejected_for_modify_and_delete` |

### AI runtime status

| Check | Result |
| --- | --- |
| Mocked provider/agent/file/build tests | **PASS** |
| Provider settings dialog compile/check | **PASS** |
| Live local Ollama request | **PASS** — 32 models discovered; `qwen2.5-coder:7b` chat and agent requests succeeded |
| Live Groq / Ollama Cloud / OpenAI-compatible cloud request | **NOT AVAILABLE** — no credential or endpoint was configured; no request was attempted |
| AI-generated source compiled by the existing compiler | **PASS** — live agent proposal was accepted and built successfully |
| Adobe playback of AI-modified or reconstructed output | **NOT VERIFIED** |
| DPAPI credential round-trip on this Windows host | **PASS** in the Windows credential-store tests; no real provider key was stored |

## Milestone 7.3 real-world validation

### Repository and Git

| Check | Result |
| --- | --- |
| Git available | **PASS** — `git 2.55.0.windows.5` |
| Baseline repository | **PASS** — `main` initialized because `.git` was absent; baseline commit `165416f3a0ca3f7aebe9672ec38c3224e41cb6af` |
| Ignore policy | **PASS** — `target/`, `dist/`, packaging binaries/objects, logs, secrets, provider/runtime settings, credentials and local projects are excluded |
| Remote | **NOT AVAILABLE** — no remote configured |
| Working tree before rc2 build | live-test and documentation changes only; no production behavior rewrite |

### Live provider connectivity

| Check | Result |
| --- | --- |
| Local Ollama service | **PASS** — Ollama `0.40.16` at `http://localhost:11434`, 32 models discovered |
| Chat request | **PASS** — `qwen2.5-coder:7b` returned a normal chat response |
| Agent request/parsing | **PASS** — parsed reply with 2,009 context characters, `cloud=false` |
| Unavailable endpoint | **PASS** — returned `AiError::Network`, not a fabricated answer |
| Ollama Cloud / Groq / OpenAI-compatible cloud | **NOT AVAILABLE** — no configured credential or endpoint; no live request was made |
| Credentials in environment or provider JSON | **PASS** — no provider API key was found; only `OLLAMA_CONTEXT_LENGTH` was set |

Commands:

```powershell
cargo test -p nova-ai --test live_ollama -- --ignored --nocapture --test-threads=1
```

The live tests are opt-in `#[ignore]` tests and are not part of the default
offline/mocked suite.

### AI Agent backend end-to-end

**PASS** for the app-level `AiEditor` workflow using live local Ollama:

1. The agent explained the selected source.
2. It proposed a new `scripts/Added.as`.
3. Rejecting the proposal wrote no file and left the project unchanged.
4. Accepting wrote the file, updated the project script list and kept the
   original `scripts/Main.as` intact.
5. The existing build pipeline succeeded: `1478 Bytes, 48 Frames, 145 Objekte,
   4 Shapes, 2 Sprites, 2 Skript(e)`.
6. File undo removed the newly created script.

Command:

```powershell
cargo test -p nova-app live_agent_workflow_reject_accept_build_and_undo -- --ignored --nocapture --test-threads=1
```

The interactive GUI workflow (clicking through every AI panel) remains
**PARTIAL**; this evidence covers the app-level backend and the live provider.

### Security audit

| Check | Result |
| --- | --- |
| Windows credential backend | **PASS** — DPAPI `CryptProtectData` with NØVA-specific entropy |
| Plaintext key on disk | **PASS** — `dpapi_roundtrip_does_not_store_plaintext` confirms the secret is not present in the `.cred` blob |
| Provider JSON | **PASS** — `provider_json_never_contains_the_api_key`; settings contain metadata only |
| Error redaction | **PASS** — provider errors containing `sk-...`/`Bearer ...` are redacted; tests confirm the key is not echoed |
| Credential filename traversal | **PASS** — unsafe provider IDs are sanitized |
| Workspace confinement | **PASS** — absolute paths, drive prefixes, `..`, symlink escapes and missing-file modifications are rejected |
| Atomic writes/undo | **PASS** — preview, apply and file undo tests pass |
| Secret scan | **PASS** — only a deliberate `Bearer secret-key` test fixture was found; no real key is tracked |
| GUI key entry/display | **PARTIAL** — write-only credential UI is implemented and unit-tested, but no full interactive credential-entry session was repeated in M7.3 |

### SWF runtime and visual playback

The M7.3 fixture `validation.nova` contains a document class `Main`, a linked
class `Added`, a shape, a sprite, 24 frames, labels `intro`/`finish`, a linear
tween and trace statements. `validation.swf` is 875 bytes with 24 frames, 2
shapes, 2 sprites, 2 ABC scripts and SymbolClass links `(0, Main)` and
`(4, Added)`.

| Runtime | Result | Evidence |
| --- | --- | --- |
| Ruffle Desktop 0.7.1 | **PASS** | PrintWindow captures showed the red document-class marker, cyan linked-sprite rotation, orange tween motion and timeline/label execution. Ruffle emitted recurring `ReferenceError: Error #1056` for named timeline children; it was non-fatal. |
| Adobe debug projector 32.0.0.465 | **PARTIAL / NOT VERIFIED** | The process and an `Adobe Flash Player 32` window started, but the SWF area rendered blank for the validation and sample SWFs; no `flashlog.txt` trace was produced. Adobe load, playback and script execution are **NOT VERIFIED**. |

### SymbolClass execution

| Fixture | Result |
| --- | --- |
| One-frame `symbolclass.nova` / `symbolclass.swf` (687 bytes, 11 tags) | **PASS** — linked `Added` sprite rotated in Ruffle; links `(0, Main)` and `(3, Added)` |
| One-frame SWF import → re-export (`symbolclass-rebuilt.swf`, 698 bytes) | **PASS** — links preserved; linked class still rotated |
| Multi-frame `validation.swf` import → rebuild (`validation-rebuilt.swf`, 1150 bytes, 175 tags) | **PARTIAL** — frames, ABC scripts and links `(0, Main)`, `(49, Added)` preserved; red marker and shapes rendered, but the linked `Added` rotation was not observed after the round-trip |
| Multi-frame rebuilt SWF in Adobe | **NOT VERIFIED** — Adobe rendered blank |

The multi-frame limitation is caused by per-frame reconstruction producing
distinct object identities; the structural SymbolClass links survive but the
linked class does not visibly execute across rebuilt frames.

### FLA/XFL

| Check | Result |
| --- | --- |
| `nova export validation.nova validation-xfl.zip` | **PASS** — 1,953-byte XFL compatibility ZIP; `DOMDocument.xml` 5,076 bytes, `scripts/Main.as`, `scripts/Added.as`, `README.txt` |
| Native `.fla` export | **PASS (refused by design)** — exit 1 with an explicit message; no fake `.fla` was written |
| `nova import validation-xfl.zip imported.nova` | **PASS** — 2 layers, 24 frames, 2 scripts, 1 symbol, 2 shapes; `README.txt` reported as unsupported |
| XFL export → re-import → export round-trip | **PARTIAL** — re-exported 1,941-byte ZIP imported, but frames 1–22 were empty and tween data was not preserved; geometry and library symbols were approximated |
| Nested `DOMDocument.xml`, symbol, shape and unsupported-element fixtures | **PASS** in `nova-formats` unit tests |
| Adobe Animate/Flash authoring compatibility | **NOT AVAILABLE** — Adobe Animate is not installed and no Adobe-authored `.fla` fixture exists; no compatibility claim is made |

### Installer and portable package

The final rc2 artifacts were checked with:

| Check | Result |
| --- | --- |
| `Setup.exe --version` | **PASS** — `NØVA Flash Studio Setup 0.3.0-rc2` |
| `Setup.exe --smoke-test` | **PASS** — payload hash, 45 entries, one EXE, EULA and `smoke-test: OK` |
| Portable ZIP integrity/extraction | **PASS** — 45 entries, expected docs/licenses/binary |
| Portable `NØVA Flash Studio.exe --version` | **PASS** — `NØVA Flash Studio 0.3.0-rc2` |
| Portable `--startup-check` | **PASS** — icon, runtime status, first-launch flag and config path |
| Headless per-user install to a temporary directory | **PASS** — files, manifest, uninstaller and HKCU entry; no desktop/start-menu/association changes |
| Installed `--version` / `--startup-check` | **PASS** |
| Headless uninstall | **PASS** — install directory removed; `%APPDATA%\NOVA Flash Studio\flash-player.json` hash unchanged |
| All-users/UAC install and full association registration | **NOT VERIFIED** in this run; requires a disposable profile/VM |
| Signing | **NOT AVAILABLE** — no legitimate signing certificate; artifacts remain unsigned |

### Performance (Windows, release build)

| Measurement | Result |
| --- | --- |
| `--startup-check`, 5 runs | avg 84.7 ms; min 72.4 ms; max 119.6 ms (first run cold) |
| GUI launch, 3 s after start | 179.3 MB working set; 200.5 MB peak; window created; clean exit |
| Large project load (`nova check`, 2.33 MB JSON, 100 layers, 10,000 frames, 2,000 authored objects) | 28 ms |
| Large project build | 247 ms; 2,034,019-byte SWF; 100 frames, 200,000 placed objects, 2,020 shapes, 20 sprites, 2 scripts |
| Large SWF import | 2,124 ms |
| Small SWF import (`validation.swf`) | 15 ms |
| Peak working set during large build | ~95 MB (polled) |

These are single-host measurements, not a benchmark guarantee.

### Regressions and new issues

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | **PASS** |
| `cargo check --workspace` | **PASS** |
| `cargo test --workspace` | **PASS** — 254 passed, 0 failed, 4 ignored live tests |
| `.nova` format version and CLI commands | **PASS** — unchanged; format version 1 |
| Adobe-default/Ruffle-explicit policy and no silent fallback | **PASS** — unit tests and temp-config startup checks |
| New production-code fixes in M7.3 | **None** — M7.3 added live tests and documentation; the discovered Ruffle/Adobe/XFL/multi-frame issues are documented limitations |
| New non-fatal runtime issue | **PARTIAL** — Ruffle `Error #1056` for named timeline children on the document class; playback continues, no workaround was added |
| New multi-frame issue | **PARTIAL** — visible linked-class execution is lost after multi-frame SWF import/re-export |

### Artifacts and checksums

The final rc2 sizes and SHA-256 values are in
`dist/windows/SHA256SUMS.txt`. The checksum file and packaged Markdown are UTF-8
without BOM; the artifacts are unsigned. The packaged Markdown may predate the
final checksum table because documentation is embedded before the checksum file
is regenerated; `SHA256SUMS.txt` is authoritative.

## CLI verification (Windows, dev machine)

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `nova-flash-studio --version` | `NØVA Flash Studio <ver>` | `NØVA Flash Studio 0.3.0-rc2` | PASS |
| `nova-flash-studio --startup-check` | branding/config/runtime report | version, 256×256 icon, runtime status, first-launch flag, Adobe/Ruffle paths, config path | PASS |
| `nova check samples/as3-example.nova` | project summary | 1 scene, 640×360, 24 fps, 1 layer, 48 frames, 1 script | PASS |
| `nova probe samples/as3-example.swf` | SWF structure | v10, CWS, 640×360, 24 fps, 48 frames, 56 tags | PASS |
| `nova export samples/as3-example.nova out.swf` | SWF written + verified | written; `probe` re-validates | PASS |
| `nova build samples/as3-example.nova out.swf` | diagnostics + SWF | build report, SWF written and structurally valid | PASS |
| `nova ai-status` | provider diagnostics without keys | provider list/credential-present booleans only | PASS (empty and configured test directories) |
| `nova demo out.swf` | demo SWF | 48 frames, 145 placements, 4 shapes, 2 sprites, 346 tags | PASS |
| `nova check <corrupt.nova>` | clear error | `invalid .nova JSON: ...` exit 1 | PASS |
| `nova check <minimal.nova>` | format-tag error | `not a NØVA project (expected format tag ...)` exit 1 | PASS |
| `nova check <missing.nova>` | clear error | file-not-found style message | PASS |
| `nova probe <missing.swf>` | clear error | SWF file not found message | PASS |

### `--startup-check` runtime scenarios

Each scenario used a temporary `NOVA_FLASH_STUDIO_CONFIG_DIR`:

| Config | Actual result | Status |
| --- | --- | --- |
| Missing file | `Adobe Flash Player Projector · Nicht konfiguriert`; `first-launch-setup: true`; `bereit: false` | PASS |
| Valid fake Adobe `MZ` executable | `Adobe Flash Player Projector · Windows-EXE erkannt`; `first-launch-setup: false`; `bereit: true` | PASS |
| Explicit Ruffle | `Ruffle · Windows-EXE erkannt`; Adobe path preserved; `bereit: true` | PASS |
| Missing Adobe path, Ruffle configured | `Die ausgewählte Runtime existiert nicht: …missing.exe`; `bereit: false`; no Ruffle fallback | PASS |
| Legacy `use_system_default: true` | Adobe default, no paths, `first-launch-setup: true` | PASS |
| Local Adobe debug projector configured | `Adobe Flash Player Projector · Windows-EXE erkannt`; `first-launch-setup: false`; `bereit: true`; exact path printed | PASS |

The fake executables were only used for path/status checks; they were never
launched. The real debug projector was launched separately through the app; see
below.

## Runtime execution (Ruffle 0.7.1)

| SWF | Result | Evidence |
| --- | --- | --- |
| `as3-example` export (pre-fix) | FAIL → fixed | `TypeError #1009` in `global$init` |
| `as3-example` export (post-fix) | PASS | `avm_trace: Box gestartet`; enterFrame handlers error-free for 12 s |
| `nova demo` export | PASS | loads + constructs, no AVM2 errors |
| Script-less export | PASS | loads cleanly |
| `.swf` → NØVA → SWF reconstruction | PARTIAL | Ruffle 0.7.1 loaded SWF 10 at 640×360 / 24 fps with no AVM2 error, but did not log the original `avm_trace`; mapped `SymbolClass` links are now re-emitted, but not every reconstructed class was visually verified |
| M7.3 `validation.swf` | PASS | Ruffle visual capture: red document marker, cyan linked-sprite rotation, orange tween motion, timeline labels |
| M7.3 one-frame `symbolclass.swf` | PASS | Linked `Added` sprite rotated; one-frame import/re-export still executed it |
| M7.3 multi-frame `validation-rebuilt.swf` | PARTIAL | Structural links preserved; linked `Added` rotation not observed after round-trip |
| M7.3 Adobe debug projector | PARTIAL / NOT VERIFIED | Process and window started; SWF rendered blank; no trace log |

### Milestone 7 app launch path

A fresh `as3-example` SWF was exported to a temporary project (534 bytes; SWF
v10, 640×360, 24 fps, 48 frames) and opened through NØVA:

- Runtime settings selected **Ruffle** explicitly; Adobe was not configured.
- The toolbar **Open in Flash Player** action started `ruffle.exe` with the SWF
  as one separate argument.
- Ruffle log:

  ```text
  Opening file:///…/as3-example.swf
  Loaded SWF version 10, resolution 640x360 @ 24 FPS
  avm_trace: Box gestartet
  ```

  No AVM2 errors were logged. The app status bar said
  `Ruffle gestartet; Wiedergabe nicht verifiziert`.
- A second run passed the same SWF through `--open <path>` (the path a shell
  association would use). NØVA recognised `.swf`, did not parse it as a
  project, started Ruffle, and produced the same `Loaded SWF version 10` /
  `avm_trace: Box gestartet` evidence.
- The checked-in `samples/as3-example.swf` is an older artifact. Passing it
  through the app path produced the expected pre-fix `TypeError #1009`; this is
  why the regenerated SWF was used for the PASS evidence.

### Milestone 7.2 Adobe process launch

- A local Adobe debug projector was configured through
  `%APPDATA%\NOVA Flash Studio\flash-player.json` (per-user settings, not a
  project or hardcoded source path).
- `nova-flash-studio --startup-check` reported the Adobe projector as
  `Windows-EXE erkannt`, `first-launch-setup: false`, `bereit: true`.
- The current exporter regenerated `samples/as3-example.swf` (538 bytes; SWF v10,
  640×360, 24 fps, 48 frames, 56 tags, zlib).
- The final packaged `NØVA Flash Studio.exe --open samples/as3-example.swf` was
  started. After 7 seconds the app process was still alive, and the OS process
  list contained `flashplayer_32_sa_debug` with a main window titled
  `Adobe Flash Player 32`, plus a second projector process. Both were stopped
  after the check.
- **This is process-launch evidence only.** Visual playback, timeline rendering,
  script execution and Adobe compatibility remain **NOT VERIFIED**.
- The final packaged app was launched without a file, activated and sent
  `Ctrl+Shift+A`. The process stayed alive after the AI workspace shortcut. This
  is a stability smoke test, not a panel-by-panel visual review.

## GUI runtime integration

| Case | Evidence | Status |
| --- | --- | --- |
| First launch, no config | **Flash Player Projector einrichten** dialog shown; Save disabled without a path; Later available | PASS (GUI screenshot) |
| First launch, **Später einrichten** | Dialog closed, app usable, status `Runtime später konfigurieren`; config contained `first_launch_setup_done: true` | PASS (GUI + config inspection) |
| Runtime settings | Adobe selected by default; status `Windows-EXE erkannt`; Adobe/Ruffle paths and validation row visible | PASS (GUI screenshot) |
| Explicit Ruffle selection | Ruffle radio selected and saved; config `default_runtime: "ruffle"`; `last_validation.runtime: "ruffle"` | PASS (GUI + config inspection) |
| Missing Adobe, Ruffle configured | Error dialog naming the missing Adobe runtime; process list showed **no Ruffle process** | PASS (GUI; no silent fallback) |
| `.swf` startup with Ruffle configured | Ruffle process started; status bar `Ruffle gestartet; Wiedergabe nicht verifiziert`; log showed `Loaded SWF version 10` and `avm_trace: Box gestartet` | PASS (GUI + Ruffle log) |
| Valid Adobe path suppresses first launch | Model test + `--startup-check` | PASS (automated/CLI) |

The GUI screenshots were captured during QA but are not checked into the
repository. The config snippets and Ruffle log lines above are the durable
evidence.

## Installer (headless CLI modes)

| Scenario | Result |
| --- | --- |
| `--install --per-user` fresh | PASS — files, HKCU uninstall key, current-user shortcuts |
| `--uninstall` per-user | PASS — no UAC, user data preserved |
| `--install` all-users non-elevated | PASS (as negative test) — exit 2, nothing written, no fallback |
| `--install --all-users` elevated | PASS — Program Files, HKLM, common shortcuts |
| `--uninstall` all-users elevated | PASS |
| Same-scope upgrade | PASS — in-place update |
| Scope-mix install attempt | PASS (refused) — exit 2, no second directory |
| `--migrate` per-user→all-users (elevated) | PASS — source removed, machine target created |
| `--migrate` all-users→per-user (elevated) | PASS |
| `--migrate` non-elevated | PASS (refused) — exit 2 |
| GUI wizard launch/close | PASS (manual smoke) |
| **UAC denial click-through** | **NOT VERIFIED** — interactive "No" not tested |
| `.swf` association registry path | **NOT VERIFIED in M7.3** — the equivalent `--open <swf>` startup path was tested; the installer association was verified in M5/M6. Current HKCR/HKCU inspection shows Winamp/Ruffle/Adobe entries, not NØVA; no association was changed. |
| M7.3 fresh per-user install to temp dir (`--no-desktop --no-start-menu --no-assoc`) | PASS — installed app `--version`/`--startup-check`, manifest and uninstaller present |
| M7.3 headless uninstall | PASS — install directory removed; user config hash unchanged |
| M7.3 all-users/UAC install | NOT VERIFIED — requires a disposable profile/VM |

## GUI application

Manual GUI workflows (Code Mode editing, Animation Mode timeline, undo/redo,
copy/paste, autosave, crash recovery, settings dialogs) were exercised during
development but are **not covered by automated tests** in this report. Treat
general GUI behavior as PARTIALLY VERIFIED. The Milestone 7 runtime dialogs and
launch flows listed above were exercised directly and are the exception. The
Milestone 7.2 AI workspace was compiled and its project-model apply logic is
unit-tested; opening the workspace interactively and exercising every panel was
not part of this run.

## Release artifacts (Milestone 7.3 / rc2)

The rc2 artifacts are:

- `NØVA Flash Studio.exe`
- `NOVA-Flash-Studio-0.3.0-rc2-windows-x64.zip`
- `NOVA-Flash-Studio-0.3.0-rc2-windows-x64-Setup.exe`

Sizes and SHA-256 values are in `dist/windows/SHA256SUMS.txt`, which is the
authoritative list. The ZIP contains 45 entries; its report/docs are packaged
before the checksum file is generated. The checksum file and packaged Markdown
were checked as UTF-8 without BOM. The artifacts are unsigned.

The previous rc1 artifacts were preserved outside `dist/windows` until the rc2
build and verification completed; they are not presented as the current release.

## Known untested areas

- **Adobe Flash Player Projector**: path validation and process launch are tested
  with a local debug projector; the actual visual SWF playback was not observed.
  No Adobe compatibility claim is made.
- **Live cloud AI providers**: local Ollama was exercised live; Groq, Ollama
  Cloud and custom cloud endpoints were not available because no credential or
  endpoint was configured.
- **Interactive AI GUI workflow**: the backend `AiEditor` workflow was validated
  live, but clicking through every AI panel and credential-entry dialog was not
  repeated in M7.3.
- **Adobe visual playback**: the debug projector starts but rendered blank; load,
  rendering and script execution remain **NOT VERIFIED**.
- **Adobe authoring compatibility**: Adobe Animate is not installed and no
  Adobe-authored `.fla` fixture is available; XFL compatibility is **NOT
  AVAILABLE** as a verified claim.
- **Multi-frame SymbolClass execution**: structural links survive SWF
  import/re-export, but visible linked-class execution does not persist across
  rebuilt frames.
- **Interactive UAC denial**: only the non-elevated refusal path was tested.
- **Fresh-install `.swf` association registration**: the startup path was tested
  via `--open`; the registry association itself was not re-exercised in M7.3.
- Long-running stability / memory profiling beyond the single-host measurements.
- Non-Windows platforms (macOS/Linux packaging scripts exist; not built here).
- Full visual/audio correctness in Ruffle beyond the captured M7.3 fixtures.
