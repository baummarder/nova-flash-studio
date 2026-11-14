# SWF Runtime Compatibility Report

Status of generated SWF files as of `v0.5.0-rc1`. The visual runtime evidence is
from Milestone 7.3; M8 export additions are tested structurally and are labelled
as such below. This report distinguishes four different claims that are easy to
confuse:

| Layer | What it proves | What it does not prove |
| --- | --- | --- |
| **Structural validation** | The SWF path exists, starts with `FWS`/`CWS`/`ZWS`, and the tag/ABC reader parses it | That a runtime executes it |
| **Runtime path validation** | The selected file is a native executable/bundle of the expected form | That it is Adobe/Ruffle, or that it starts |
| **Process launch validation** | The OS created the runtime process; NØVA has a PID | That the SWF loaded, executed, or rendered |
| **Playback validation** | The runtime log or captured frame shows the SWF loaded and script/timeline activity without fatal errors | Full visual, timing, audio, or Adobe compatibility |

The **Prüfen** button in Runtime settings performs runtime path validation only.
**Open in Flash Player** performs process launch validation. Actual playback is
verified separately by reading the runtime log or capturing the runtime window;
NØVA deliberately reports “gestartet; Wiedergabe nicht verifiziert” on a
successful `spawn()`.

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
  (`flashplayer_32_sa_debug.exe`, 16,811,576 bytes, version 32.0.0.465) is
  configured through per-user Runtime settings in this development environment.
  Process and window launch are observed; visual playback is **NOT VERIFIED**
  because the tested SWF area rendered blank and no trace log was produced. No
  Adobe binary was downloaded or bundled by NØVA. No Adobe compatibility claim
  is made.
- Visual evidence: M7.3 used a PrintWindow capture script so the runtime window
  could be inspected without relying on the contaminated desktop capture. The
  capture showed the red document-class marker, cyan linked-sprite rotation and
  orange tween motion.

## Results

| Artifact | Producer | Structural | Ruffle direct | Ruffle via NØVA |
| --- | --- | --- | --- | --- |
| `samples/as3-example.swf` (checked in) | older compiler | parses | **FAIL** — pre-fix `TypeError #1009` | **FAIL** — same stale artifact, `TypeError #1009`; do not use as baseline |
| Regenerated `as3-example` export | `nova-cli` | `probe`: SWF v10, CWS, 640×360, 24 fps, 48 frames, DoABC2+SymbolClass | **PASS** — `avm_trace: Box gestartet`; `enterFrame` handlers ran for the full 12 s session without errors | **PASS** — Ruffle process started with the exact SWF; log showed `Opening file:///…/as3-example.swf`, `Loaded SWF version 10, resolution 640x360 @ 24 FPS`, `avm_trace: Box gestartet`, no AVM2 errors |
| `nova demo` output | `nova-cli` | `probe`: SWF v10, CWS, 550×400, 24 fps, 48 frames, 346 tags | **PASS** — loads, constructs document class, no AVM2 errors | **NOT VERIFIED** — direct Ruffle evidence only |
| Export with `includeScripts=false` | `nova-cli` | parses | **PASS** — loads cleanly | **NOT VERIFIED** |
| Corrupt/truncated SWF | — | rejected | n/a (rejected before player) | n/a (rejected before process launch) |

## Milestone 7.3 visual-playback evidence

The M7.3 fixture was authored as `validation.nova` with document class `Main`,
linked class `Added`, a shape, a sprite, 24 frames, labels `intro`/`finish`, a
linear tween and trace statements. The final `validation.swf` is 875 bytes, has
24 frames, 2 shapes, 2 sprites, 2 ABC scripts and SymbolClass links `(0, Main)`
and `(4, Added)`. An independent parse found 84 tags; both `DoABC2` entries
contain the class names and trace strings.

| Artifact | Structural | Ruffle 0.7.1 visual | Adobe 32.0.0.465 |
| --- | --- | --- | --- |
| `validation.swf` (875 bytes) | **PASS** — 24 frames, 2 shapes, 2 sprites, 2 ABC, links `(0, Main)`/`(4, Added)` | **PASS** — red document-class marker, cyan linked-sprite rotation, orange tween motion, timeline/labels execute | **PARTIAL / NOT VERIFIED** — process and window start; SWF area blank; no `flashlog.txt` |
| `symbolclass.swf` (687 bytes, one frame, 11 tags) | **PASS** — links `(0, Main)`/`(3, Added)` | **PASS** — linked `Added` sprite rotates | **NOT VERIFIED** — blank window |
| `symbolclass-rebuilt.swf` (698 bytes, one-frame import/re-export) | **PASS** — links preserved | **PASS** — linked class still rotates | **NOT VERIFIED** |
| `validation-rebuilt.swf` (1,150 bytes, 175 tags, multi-frame import/re-export) | **PASS** — frames, ABC and links `(0, Main)`/`(49, Added)` preserved; shape count grows 2→48 because per-frame geometry is baked | **PARTIAL** — red marker and shapes render; linked `Added` rotation not observed across rebuilt frames | **NOT VERIFIED** |

Ruffle emitted recurring `ReferenceError: Error #1056: Cannot create property
… on Main` for named timeline children. The error was non-fatal; playback and
the document-class marker continued. The compiler rejects `dynamic` on a class,
so a `public dynamic class Main` workaround was not available and was not added.

The multi-frame round-trip limitation comes from per-frame reconstruction
producing distinct object identities. The structural `SymbolClass` links survive,
but the linked display class does not visibly execute across the rebuilt frames.
The one-frame round-trip is the verified working case.

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

Milestone 7.1 added `.fla`/XFL and `.swf` import plus SWF re-export. Milestone
7.3 added runtime evidence for the one-frame and multi-frame reconstructed cases:

- FWS, CWS and ZWS/LZMA imports are parsed in tests.
- SWF 9, 10 and 13 headers are reported in tests.
- A `.nova` → SWF → import → SWF export → import round trip preserves ABC bytes.
- A `.fla`/XFL → NØVA → SWF round trip preserves stage size and timeline frames.
- One-frame `symbolclass.swf` → import → re-export → Ruffle: **PASS** — linked
  class still rotates.
- Multi-frame `validation.swf` → import → re-export → Ruffle: **PARTIAL** —
  structural links survive, visible linked-class execution does not.
- No generated `.fla` or XFL package was opened in Adobe Animate or Flash.

Adobe remains **NOT VERIFIED**. Ruffle evidence in this document applies to the
regenerated `as3-example`, the M7.3 validation fixtures and the one-frame
reconstruction; it is not a general compatibility claim for arbitrary imported
SWFs.

## Defects found by runtime testing

Runtime execution exposed four ABC-emitter bugs that structural parsing could
not see. All four were fixed and are covered by tests or documented in
`docs/AVM2_BYTECODE.md`:

1. `global$init` `TypeError #1009` — `newclass` operand/scope sequence.
2. `ReferenceError #1069` / `TypeError #1006` — global calls used the wrong
   receiver object.
3. `this.Event` shadowing — imported class names inside `MovieClip`
   subclasses resolved as dynamic instance members.
4. `VerifyError #1032` — `pushdouble` emitted inline bytes instead of a
   double-pool index.

M7.3 also found a non-fatal Ruffle `ReferenceError: Error #1056` for named
timeline children on the document class. It was not fixed or worked around; the
compiler rejects `dynamic`, and the issue is documented as a limitation.

## What "PASS" means — and does not mean

- PASS means: Ruffle parsed the SWF, executed the ABC, ran the document-class
  constructor and `enterFrame` handlers without fatal errors, and either `trace`
  output appeared or a captured frame showed the expected visual change.
- PASS for the app launch path additionally means: NØVA selected the configured
  Ruffle executable, passed the exact SWF as one argument, and the OS created
  the process. The playback evidence comes from Ruffle's log or a captured
  runtime window, not from the `spawn()` return value.
- PASS does **not** prove: full visual correctness of every shape/timeline,
  frame-accurate animation, text rendering, audio/video, or Adobe Flash Player
  compatibility.
- Ruffle-specific gaps (e.g. its own incomplete API coverage) can mask or mimic
  player bugs; results are evidence, not proof.
- OpenH264 video download fails in this environment (network-restricted); no
  SWF in the test set uses video, so this did not affect results.

## Unsupported / unverified

- Adobe Flash Player / projector: **NOT VERIFIED**. A local debug projector is
  configured and starts a window, but the tested SWFs rendered blank and no
  trace log appeared. Adobe binaries are not redistributed by this project.
  Adobe load, playback and script execution remain unconfirmed.
- AVM1 content: NØVA does not emit AVM1; irrelevant.
- Runtime behavior of the full AS3 subset: see
  `docs/AS3_COMPATIBILITY.md` — only the sample paths above are
  runtime-verified.
- The `nova demo` and script-less exports were not launched through NØVA's
  Open in Flash Player action; their runtime evidence is direct Ruffle only.
- Multi-frame reconstructed SWF: structural links survive, but visible linked
  display-class execution is **PARTIAL** and not a general compatibility claim.
- Ruffle `Error #1056` for named timeline children is non-fatal and unfixed.
- Audio/video, text rendering and frame-accurate timing were not verified.

## Mega Milestone 8 additions

M8 adds export features but does **not** replace the M7.3 visual evidence.

| M8 export feature | Verification | Status |
| --- | --- | --- |
| Text objects baked to `DefineShape3` | `nova-build` test asserts shapes and no `DefineText`; independent SWF parser preserves tags | **PASS (structural)**; visual text rendering **NOT VERIFIED** |
| Layer folders and hidden-folder descendants | Export test checks hidden content is absent | **PASS** |
| Multi-scene concatenation and boundary labels | SWF frame-count/label test | **PASS (structural)**; playback **NOT VERIFIED** |
| Four templates build real SWFs | CLI/template and `nova-build` tests | **PASS (structural)** |
| AS3 array/object/index ABC | ABC structural parse test | **PASS (structural)**; runtime **NOT VERIFIED** |

Text is emitted as vector shapes using the built-in 5×7 font. Because no
`DefineText`/font tags are emitted, there is no selectable, editable or
system-font text in the player. The M8 scene/text fixtures were not launched in
Ruffle or Adobe. Multi-frame `SymbolClass` execution and the Ruffle `Error
#1056` limitation remain as documented above.

Adobe visual playback remains **NOT VERIFIED**, and no M8 result should be read
as Adobe compatibility evidence.
