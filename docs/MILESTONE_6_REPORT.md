# Milestone 6 Report — Release Candidate v0.3.0-rc1

Date: 2026-11-12 · Platform: Windows 11 x64 · Status: **RC, not stable**

This milestone audited the whole repository, hardened the installer scope
model, ran application QA, audited the AS3 compiler against real runtime
evidence, validated SWF output structurally **and** by execution in Ruffle,
and produced the `v0.3.0-rc1` artifacts.

## 1. Repository audit findings

- Workspace is a Cargo workspace (`nova-core`, `nova-swf`, `nova-build`,
  `nova-as`, `nova-script`, `nova-app`, `nova-cli`) plus a .NET 8 WinForms
  installer under `packaging/windows/installer/`.
- No `.git` repository exists in the workspace; no commits were created.
- Docs were largely accurate for Milestone 5 but predated runtime evidence;
  `docs/AVM2_BYTECODE.md` documented the *buggy* script-init sequence and was
  corrected.
- Version was single-sourced from `[workspace.package] version`; packaging
  scripts propagate it to filenames, the exe resource and installer metadata.

## 2. Implemented changes

### AS3 compiler / ABC emitter (`crates/nova-as`)

Four runtime defects were found by executing generated SWFs in Ruffle and
fixed (all verified at runtime afterwards):

1. **Script initializer / `newclass`** — the emitted sequence pushed the
   class's own uninitialized multiname as scope. Per the AVM2 spec,
   `newclass` pops a *basetype* operand and needs base-class scopes on the
   scope stack. Rewrote `emit_script` to
   `getscopeobject 0 / getlex base / dup / pushscope / newclass / popscope /
   initproperty`. Fixes `TypeError #1009` in `global$init`.
2. **Global function calls** — `trace()` and other builtins were called with
   `getscopeobject 0`/`getglobalscope` as the `callproperty` receiver, which
   is not the object owning the builtin → `#1069`/`#1006`. Now emits
   `findpropstrict <name>` (scope-chain resolution to the playerglobal owner)
   before args and `callproperty`.
3. **Name resolution shadowing** — inside `MovieClip` subclasses, imported
   class names (`Event`) and package functions were swallowed by the
   "inherited Flash member" dynamic fallback, producing `this.Event`. Class
   and function resolution now precede that fallback in `infer_ident`.
4. **`pushdouble` encoding** — emitted 8 inline bytes; AVM2 expects a u30
   index into the double constant pool → `VerifyError #1032` on any method
   using a float literal. Values are now interned via `add_double`.

Supporting changes: new `GETGLOBALSCOPE`/`FINDPROPSTRICT` opcode constants
(`GETGLOBALSCOPE` later removed as unused), `find_prop_strict` builder op,
`class_base_multiname` helper, `emit_push_double` helper, updated unit test
(`compiles_movieclip_subclass_and_reports_metadata`) and new debug example
`crates/nova-as/examples/dump_script.rs`.

### CLI (`crates/nova-cli`)

- `probe` reports a clear "SWF file not found" error instead of a raw zlib
  failure on missing input.
- `export` writes to a temporary file and renames into place — a failed
  export no longer truncates an existing output SWF.

### Installer (`packaging/windows/installer/`)

- Explicit `--migrate` workflow for moving installs between per-user and
  all-users scopes (elevation required for the machine side); verified both
  directions. Scope/directory guards prevent silent scope mixing (exit 2).
- Per-user remains the wizard default; all-users requires explicit selection
  with UAC explanation (Milestone 5 work, re-verified).

## 3. Bug fixes summary

| # | Defect | Severity | Status |
| --- | --- | --- | --- |
| 1 | `newclass` script-init sequence → runtime #1009 | Critical (no AS3 ran) | FIXED, runtime-verified |
| 2 | Global call receiver → #1069/#1006 | Critical | FIXED, runtime-verified |
| 3 | Class-name shadowing in MovieClip subclasses | High | FIXED, runtime-verified |
| 4 | `pushdouble` inline bytes → VerifyError #1032 | High | FIXED, runtime-verified |
| 5 | CLI misleading missing-file errors | Low | FIXED |
| 6 | CLI export could truncate existing output | Medium | FIXED (temp+rename) |

## 4. Installer improvements

See §2. Scope separation (HKCU vs HKLM, per-user vs common shortcuts),
existing-install detection per scope, upgrade-in-place, and explicit
migration are implemented and were exercised via the headless CLI modes.
The interactive UAC **denial** click was not tested; only the
non-elevated refusal path (exit 2, nothing written) was verified.

## 5. AS3 compatibility matrix

See `docs/AS3_COMPATIBILITY.md`. Summary: conservative subset — packages,
imports, single inheritance, fields/methods/statics, core statements and
operators, `is/as/instanceof`, external Flash names as dynamic multinames.
Unsupported constructs produce source-located diagnostics and abort
compilation; no partial ABC is ever embedded. Roughly half the AS3 surface
is unsupported (interfaces, namespaces, exceptions, closures, getters/setters,
for-in/each, switch, XML, …).

## 6. SWF export validation

`nova probe` + the `swf` crate parser validate header version, compression,
dimensions, frame rate, frame count, tag stream (FileAttributes,
SetBackgroundColor, DefineShape3, DefineSprite, DoABC2, SymbolClass,
PlaceObject2, ShowFrame, End). Sample and demo exports pass. See
`docs/SWF_RUNTIME_COMPATIBILITY.md`.

## 7. Actual runtime execution results

Ruffle Desktop 0.7.1 (Vulkan, RTX 5070) was downloaded for this milestone:

- `nova export samples/as3-example.nova` → **PASS**: SWF loads, ABC executes,
  `avm_trace: Box gestartet` printed, `enterFrame` handlers ran ~12 s
  error-free.
- `nova demo` output → **PASS**: loads and constructs, no AVM2 errors.
- Script-less export → **PASS**.
- Pre-fix exports failed with the four errors listed above — this is how the
  bugs were found.

## 8. Flash Player integration

`crates/nova-app/src/flash.rs` audited: settings stored atomically under
`%APPDATA%\NOVA Flash Studio\flash-player.json`, SWF signature validation,
projector binary validation (MZ/ELF/Mach-O + exec bit), `ShellExecuteW` /
`Command::arg` launch without shell strings, honest status text. Unit tests
cover settings roundtrip, path safety, validation. **Adobe Flash Player was
not installed or tested** — integration is launch/config only, never bundled.

## 9. Performance & stability

- No crashes observed in CLI paths; error paths return exit 1/2 with clear
  messages.
- Export now uses temp-file+rename (no partial outputs).
- No performance regressions introduced; release build ~1 m 04 s.
- GUI long-run stability not profiled — PARTIALLY VERIFIED.

## 10. Documentation changes

- NEW `docs/AS3_COMPATIBILITY.md` — feature matrix with verification levels.
- NEW `docs/SWF_RUNTIME_COMPATIBILITY.md` — structural vs runtime report.
- NEW `docs/TESTING.md` — commands, expected/actual, pass/fail.
- NEW `docs/RELEASE_CHECKLIST.md` — RC checklist with honest gaps.
- NEW `docs/MILESTONE_6_REPORT.md` — this file.
- UPDATED `docs/AVM2_BYTECODE.md` — corrected script-init sequence,
  `findpropstrict` global-call pattern, `pushdouble` pool indexing.
- UPDATED `docs/PACKAGING.md` — version `0.3.0-rc1`, RC verification section.
- `README.md`, `docs/LICENSING.md`, `legal/EULA-DRAFT.md` — unchanged
  (already accurate; EULA still DRAFT).

## 11. Exact commands executed (key ones)

```
cargo fmt --all -- --check
cargo test --workspace
dotnet build packaging/windows/installer/NovaFlashStudio.Setup.csproj -c Release
dotnet publish ... -r win-x64
cargo run -p nova-cli -- check samples/as3-example.nova
cargo run -p nova-cli -- probe samples/as3-example.swf
cargo run -p nova-cli -- export samples/as3-example.nova out.swf
cargo run -p nova-cli -- demo out.swf
cargo run -p nova-as --example dump_script -- box.as
ruffle.exe <generated>.swf            # + %LOCALAPPDATA%\Ruffle\log\ruffle.log
powershell -File scripts/build-release.ps1 -Target windows
NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe --version / --smoke-test
```

## 12. Test results

- `cargo test --workspace`: all suites PASS (167+ tests; exact count varies
  with doctest aggregation — every suite reported `0 failed`).
- Installer headless scenarios: all PASS except interactive UAC denial
  (NOT RUN).
- Ruffle runtime: PASS for all post-fix artifacts (see §7).

## 13. Generated artifacts

- `dist\windows\NOVA-Flash-Studio-0.3.0-rc1-windows-x64.zip` (~7.5 MB)
- `dist\windows\NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe` (~75.7 MB)
- `dist\windows\NØVA Flash Studio\NØVA Flash Studio.exe` (staged binary)
- `dist\windows\SHA256SUMS.txt`

## 14. SHA-256 checksums

Authoritative checksums live outside the payload in
`dist\windows\SHA256SUMS.txt` (the report itself is inside the ZIP, so
self-referential hashes cannot be embedded here). At packaging time they
covered the staged `NØVA Flash Studio.exe`, the `-windows-x64.zip` and the
`-windows-x64-Setup.exe`; all three matched `Get-FileHash` output.

## 15. Known limitations

- AS3 subset only; unsupported constructs fail at compile time (documented).
- External Flash API unchecked until runtime.
- GUI workflows manually verified only; no GUI automation.
- UAC interactive denial not exercised.
- Adobe Flash Player runtime untested; Ruffle is the only runtime evidence.
- Multi-class scripts need base-before-derived declaration order.
- Binaries unsigned; SmartScreen behavior untested.
- EULA is a draft pending legal review.

## 16. Remaining work before stable

- Interactive UAC denial test; broader GUI QA pass.
- More AS3 surface (or explicit descope) per roadmap.
- Adobe Flash Player smoke test on a machine that has it (if legally
  available to the tester).
- Legal review of EULA; decide license finalization.
- Code signing decision.
- CI matrix for macOS/Linux packaging claims.
