# Testing & QA Report — v0.3.0-rc1 (Milestones 7, 7.1 and 7.2)

What was actually executed for this release candidate, with commands and
results. Statuses: **PASS** / **FAIL** / **NOT RUN** / **MANUAL**. This report
keeps the Milestone 6 CLI/installer results, the Milestone 7 runtime evidence,
the Milestone 7.1 format-conversion evidence and the Milestone 7.2 AI/safe-edit
evidence.

## Automated tests

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | **PASS** (exit 0) |
| `cargo test --workspace` | **PASS** — 254 tests across all suites, 0 failed; `nova-ai` has 33, `nova-app` has 74 and `nova-formats` has 20 |
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
| Live Ollama/Groq/OpenAI-compatible request | **NOT RUN** — no live provider was configured for the test run |
| AI-generated source compiled by the existing compiler | **PASS** in mocked build tests; no live model output was used |
| Adobe playback of AI-modified or reconstructed output | **NOT VERIFIED** |
| DPAPI credential round-trip on this Windows host | **PASS** in the Windows credential-store tests; no real provider key was stored |

## CLI verification (Windows, dev machine)

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `nova-flash-studio --version` | `NØVA Flash Studio <ver>` | `NØVA Flash Studio 0.3.0-rc1` | PASS |
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
| **UAC denial click-through** | **NOT RUN** — interactive "No" not tested |
| `.swf` association registry path | **NOT RUN in M7** — the equivalent `--open <swf>` startup path was tested; the installer association was verified in M5/M6 |

## GUI application

Manual GUI workflows (Code Mode editing, Animation Mode timeline, undo/redo,
copy/paste, autosave, crash recovery, settings dialogs) were exercised during
development but are **not covered by automated tests** in this report. Treat
general GUI behavior as PARTIALLY VERIFIED. The Milestone 7 runtime dialogs and
launch flows listed above were exercised directly and are the exception. The
Milestone 7.2 AI workspace was compiled and its project-model apply logic is
unit-tested; opening the workspace interactively and exercising every panel was
not part of this run.

## Release artifacts (Milestone 7.2)

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `NØVA Flash Studio.exe` | 23,522,816 | `5AE00C26DA352E51EB81159DF6D05436BF893E06E803626D34321CE13B30AC0B` |
| `NOVA-Flash-Studio-0.3.0-rc1-windows-x64.zip` | 9,882,733 | `DD3A5BDEAA0C9EA0305FB7A602C0ABDE5BA2D49CC0D7FF20C4A1351C68184763` |
| `NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe` | 81,795,347 | `87BAB81BE244F1404A42C862F777D1DB73E6D1B1385A1AC1649A5538C67ECE98` |

`dist/windows/SHA256SUMS.txt` is the authoritative list. The ZIP contains 44
entries; its report/docs are packaged before the checksum file is generated.
The checksum file and packaged Markdown were checked as UTF-8 without BOM. The
artifacts are unsigned.

## Known untested areas

- **Adobe Flash Player Projector**: path validation and process launch are tested
  with a local debug projector; the actual visual SWF playback was not observed.
  No Adobe compatibility claim is made.
- **Live AI providers**: all provider/agent tests use mocked HTTP. No real
  Ollama/Groq/OpenAI-compatible endpoint or real API key was exercised.
- **AI-generated code**: mocked responses are compiled by the build tests, but no
  live model output was reviewed or executed.
- **Interactive UAC denial**: only the non-elevated refusal path was tested.
- **Fresh-install `.swf` association registration**: the startup path was tested
  via `--open`; the registry association itself was not re-exercised in M7.
- Long-running stability / memory profiling.
- Non-Windows platforms (macOS/Linux packaging scripts exist; not built here).
- Full visual/audio correctness in Ruffle beyond the logged sample execution.
