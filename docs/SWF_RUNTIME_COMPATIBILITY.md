# SWF Runtime Compatibility Report

Status of generated SWF files as of `v0.3.0-rc1` and Milestone 7. This report
distinguishes four different claims that are easy to confuse:

| Layer | What it proves | What it does not prove |
| --- | --- | --- |
| **Structural validation** | The SWF path exists, starts with `FWS`/`CWS`/`ZWS`, and the tag/ABC reader parses it | That a runtime executes it |
| **Runtime path validation** | The selected file is a native executable/bundle of the expected form | That it is Adobe/Ruffle, or that it starts |
| **Process launch validation** | The OS created the runtime process; NØVA has a PID | That the SWF loaded, executed, or rendered |
| **Playback validation** | The runtime log shows the SWF loaded and script/timeline activity without errors | Full visual, timing, audio, or Adobe compatibility |

The **Prüfen** button in Runtime settings performs runtime path validation only.
**Open in Flash Player** performs process launch validation. Actual playback is
verified separately by reading the runtime log; NØVA deliberately reports
“gestartet; Wiedergabe nicht verifiziert” on a successful `spawn()`.

## Test setup

- Runtime: **Ruffle Desktop 0.7.1**
  (`ruffle-0.7.1-windows-x86_64.zip`, Vulkan backend, NVIDIA GeForce RTX 5070).
  Direct runs used `ruffle.exe <file.swf>`; app-launch runs used NØVA's
  **Open in Flash Player** action and a `.swf` startup argument, with Ruffle
  selected explicitly in Runtime settings. Results were read from
  `%LOCALAPPDATA%\Ruffle\log\ruffle.log`.
- Parser: the `swf` crate's AVM2 reader (already used by repo tests) and the
  `nova probe` CLI inspector.
- Adobe Flash Player Projector: a local **debug projector**
  (`flashplayer_32_sa_debug.exe`, 16,811,576 bytes, dated 2026-11-09) is
  configured through per-user Runtime settings in this development environment.
  Process launch is tested; visual playback is **NOT VERIFIED**. No Adobe binary
  was downloaded or bundled by NØVA. No Adobe compatibility claim is made.

## Results

| Artifact | Producer | Structural | Ruffle direct | Ruffle via NØVA |
| --- | --- | --- | --- | --- |
| `samples/as3-example.swf` (checked in) | older compiler | parses | **FAIL** — pre-fix `TypeError #1009` | **FAIL** — same stale artifact, `TypeError #1009`; do not use as baseline |
| Regenerated `as3-example` export | `nova-cli` | `probe`: SWF v10, CWS, 640×360, 24 fps, 48 frames, DoABC2+SymbolClass | **PASS** — `avm_trace: Box gestartet`; `enterFrame` handlers ran for the full 12 s session without errors | **PASS** — Ruffle process started with the exact SWF; log showed `Opening file:///…/as3-example.swf`, `Loaded SWF version 10, resolution 640x360 @ 24 FPS`, `avm_trace: Box gestartet`, no AVM2 errors |
| `nova demo` output | `nova-cli` | `probe`: SWF v10, CWS, 550×400, 24 fps, 48 frames, 346 tags | **PASS** — loads, constructs document class, no AVM2 errors | NOT RUN — direct Ruffle evidence only |
| Export with `includeScripts=false` | `nova-cli` | parses | **PASS** — loads cleanly | NOT RUN |
| Corrupt/truncated SWF | — | rejected | n/a (rejected before player) | n/a (rejected before process launch) |

## Milestone 7 launch-path evidence

The app launch path was exercised with a freshly exported SWF in a temporary
project, not with the stale checked-in artifact:

1. `nova export <temp>/as3-example.nova <temp>/as3-example.swf` wrote a 534-byte
   SWF and `probe` reported SWF v10, 640×360, 24 fps, 48 frames, 56 tags, zlib.
2. Runtime settings were configured with Ruffle Desktop 0.7.1 selected
   explicitly. NØVA started `ruffle.exe` with the SWF as one separate argument.
3. Ruffle logged:

   ```text
   Opening file:///…/as3-example.swf
   Loaded SWF version 10, resolution 640x360 @ 24 FPS
   avm_trace: Box gestartet
   ```

   No AVM2 errors were logged. The app status bar said
   `Ruffle gestartet; Wiedergabe nicht verifiziert`.
4. A second run passed the SWF via `--open <path>` (the same path used by a
   shell association). NØVA recognised the `.swf` extension, did not parse it as
   a project, started Ruffle, and again produced `Loaded SWF version 10` and
   `avm_trace: Box gestartet`.
5. The checked-in `samples/as3-example.swf` was also passed through the app path;
   Ruffle logged the expected pre-fix `TypeError #1009`. This is evidence that
   the old checked-in artifact must be regenerated before runtime testing.

The app never constructs a shell command from a path. On Windows and Linux it
uses `Command::new(executable).arg(swf)`; on macOS a bundle uses
`open -a <app> <swf>` with separate arguments. Spaces, quotes, Unicode and other
special characters are therefore passed as one argument. Automated tests cover
spaces and Unicode in both the SWF and runtime paths.

## Milestone 7.2 Adobe process-launch evidence

The local debug projector was configured through the normal per-user Runtime
settings file, not a project or a hardcoded application path:

1. `nova-flash-studio --startup-check` reported
   `runtime: Adobe Flash Player Projector · Windows-EXE erkannt. (bereit: true)`
   and the configured projector path from `%APPDATA%\NOVA Flash Studio\flash-player.json`.
2. `samples/as3-example.swf` was regenerated with the current exporter:
   538 bytes, SWF v10, 640×360, 24 fps, 48 frames, 56 tags, zlib.
3. `nova-flash-studio.exe --open samples/as3-example.swf` was started. After
   8 seconds the OS process list contained `flashplayer_32_sa_debug` with a
   main window titled `Adobe Flash Player 32`, plus a second projector process.
   The NØVA process was still running. Both were stopped after the check.
4. This proves **process launch** through the app path. No visual playback,
   timeline rendering, script execution or Adobe compatibility was observed;
   those remain **NOT VERIFIED**.

The projector executable is not bundled, not referenced by a source path and
not required for the application to start.

## Format-import runtime status

Milestone 7.1 added `.fla`/XFL and `.swf` import plus SWF re-export. These paths
are structurally tested but are not part of the runtime playback matrix above:

- FWS, CWS and ZWS/LZMA imports are parsed in tests.
- SWF 9, 10 and 13 headers are reported in tests.
- A `.nova` → SWF → import → SWF export → import round trip preserves ABC bytes.
- A `.fla`/XFL → NØVA → SWF round trip preserves stage size and timeline frames.
- Reconstructed SWFs have not been launched in Ruffle or Adobe Flash Player.
- No generated `.fla` or XFL package was opened in Adobe Animate or Flash.

Adobe remains **NOT VERIFIED**. Ruffle evidence in this document still applies to
the regenerated `as3-example` and demo artifacts, not to arbitrary imported SWFs.

## Defects found by runtime testing (all fixed)

Runtime execution exposed four ABC-emitter bugs that structural parsing could
not see. All are fixed and covered by tests or documented in
`docs/AVM2_BYTECODE.md`:

1. `global$init` `TypeError #1009` — `newclass` operand/scope sequence.
2. `ReferenceError #1069` / `TypeError #1006` — global calls used the wrong
   receiver object.
3. `this.Event` shadowing — imported class names inside `MovieClip`
   subclasses resolved as dynamic instance members.
4. `VerifyError #1032` — `pushdouble` emitted inline bytes instead of a
   double-pool index.

## What "PASS" means — and does not mean

- PASS means: Ruffle parsed the SWF, executed the ABC, ran the document-class
  constructor and `enterFrame` handlers without logged errors, and `trace`
  output appeared.
- PASS for the app launch path additionally means: NØVA selected the configured
  Ruffle executable, passed the exact SWF as one argument, and the OS created
  the process. The playback evidence comes from Ruffle's log, not from the
  `spawn()` return value.
- PASS does **not** prove: visual correctness of shapes/timeline, frame-accurate
  animation, text rendering, audio/video, or Adobe Flash Player compatibility.
- Ruffle-specific gaps (e.g. its own incomplete API coverage) can mask or mimic
  player bugs; results are evidence, not proof.
- OpenH264 video download fails in this environment (network-restricted); no
  SWF in the test set uses video, so this did not affect results.

## Unsupported / unverified

- Adobe Flash Player / projector: **NOT VERIFIED** (not installed; Adobe
  binaries are not redistributed by this project). Path validation, process
  launch and playback are all unconfirmed for Adobe.
- AVM1 content: NØVA does not emit AVM1; irrelevant.
- Runtime behavior of the full AS3 subset: see
  `docs/AS3_COMPATIBILITY.md` — only the sample paths above are
  runtime-verified.
- The `nova demo` and script-less exports were not launched through NØVA's
  Open in Flash Player action; their runtime evidence is direct Ruffle only.
