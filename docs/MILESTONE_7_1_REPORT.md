# Milestone 7.1 Report — Universal Flash Project Compatibility

Date: 2026-11-13 · Platform: Windows 11 x64 · Status: **RC, not stable**

> Historical report for Milestone 7.1. It predates the Milestone 7.2 AI Agent
> Editor and `SymbolClass` reconstruction; see
> [`MILESTONE_7_2_REPORT.md`](MILESTONE_7_2_REPORT.md) for the current state.

Milestone 7.1 extends the existing NØVA Flash Studio with real `.nova`, `.fla`
and `.swf` import/export paths. It preserves the Milestone 7 runtime policy
(Adobe-default, Ruffle-explicit-only), the `.nova` project format and the
existing AS3 compiler/SWF writer.

The implementation is deliberately narrower than “supports FLA” or “supports
SWF”. This report separates implemented behavior, tested behavior and unverified
claims.

## 1. Non-claims

- NØVA does **not** claim universal Adobe FLA support.
- NØVA does **not** claim that its experimental XFL ZIP is a valid Adobe `.fla`
  authoring file. It has not been opened in Adobe Animate or Flash.
- NØVA does **not** claim Adobe Flash Player playback. No Adobe projector is
  installed on the build machine.
- NØVA does **not** decompile AVM2 bytecode. Preserved ABC is compiled bytecode,
  not recovered ActionScript source.
- A reconstructed SWF is structurally compatible, not byte-for-byte identical to
  the imported SWF.
- No AI Agent/provider implementation exists in this repository, so there is no
  agent UI or model call to extend. The analysis surface intended for an agent is
  documented below.

## 2. Architecture

The shared project model remains `nova_core::Project`. Format-specific code lives
in the new `nova-formats` crate:

| Module | Responsibility |
| --- | --- |
| `formats/nova.rs` | Native `.nova` load/save and import report |
| `formats/fla.rs` | ZIP/XFL `.fla` import and experimental XFL export |
| `formats/swf.rs` | SWF header/tag import and reconstruction |
| `conversion.rs` | Format detection, dispatch, `ImportedProject`, `ExportResult` |
| `capabilities.rs` | Capability tables, support levels and compatibility reports |
| `analysis.rs` | Serializable project/script analysis for CLI and agents |

The model was extended without breaking existing `.nova` files:

- `ObjectKind::Path`, `PathCommand` and `PathShape` for reconstructed vector
  geometry.
- `CodeProvenance` (`OriginalSource`, `Decompiled`, `Reconstructed`, `Generated`,
  `CompiledBytecode`).
- `SourceInfo` / `SourceFormat` on `Project`.
- `Script.provenance`, `Script.abc` and `Script::imported_bytecode()`.

All new fields are optional or defaulted, so older projects still deserialize.

## 3. SWF import

The importer uses the MIT/Apache-licensed `swf` crate. It is a reconstruction
pipeline, not a decompiler.

Implemented:

- FWS, CWS and ZWS/LZMA headers.
- Header version, compression, stage dimensions, background colour, frame rate
  and declared frame count.
- `DefineShape`/`DefineShape2`/`DefineShape3` as editable path objects with solid
  fills; bounds are used for selection.
- `DefineSprite` timelines flattened into the main timeline with composed
  matrices.
- Depth-based layers, one keyframe per reconstructed frame.
- `PlaceObject`/`PlaceObject2`, `RemoveObject`/`RemoveObject2`, visibility and
  alpha.
- Frame labels and `DefineSceneAndFrameLabelData`.
- `DoABC`/`DoABC2` bytecode preserved byte-for-byte.
- `SymbolClass`, `ExportAssets` and XMP metadata reported as information.
- Unknown/unsupported tags reported; original SWF bytes retained in
  `ImportedProject::source_bytes`.
- Corrupt or invalid SWF bytes rejected before a project is produced.

Tested header versions: **SWF 9, 10 and 13**. Other versions are read by the
header parser but have not been exhaustively exercised.

Important limits:

- Sprites are flattened; there is no editable nested sprite library.
- Strokes, gradients, bitmap fills, filters, text, sound and video are not fully
  editable.
- `SymbolClass` character links are reported but not reconstructed as SWF
  character links on re-export. Preserved ABC is embedded, but the reconstructed
  movie may not instantiate the original display class.
- ABC is not decompiled. The script text shown in the editor is an explicit
  marker, not original source.

## 4. FLA/XFL import

Modern `.fla` files are ZIP/XFL containers. NØVA reads a documented subset:

- A ZIP containing `DOMDocument.xml` at the root or in a nested directory.
- Document width, height, frame rate and background colour.
- `DOMTimeline`, `DOMLayer`, `DOMFrame`, keyframes and frame labels.
- `DOMRectangleObject` and `DOMEllipseObject` as native shapes.
- `DOMShape` as an editable path approximated from its bounds. XFL edge strings
  are not fully decoded; this is reported as a geometry warning.
- `DOMSymbolInstance` as a library symbol; nested symbol timelines are not fully
  reconstructed.
- `DOMGroup` recursively.
- `DOMBitmapInstance` and `DOMTextInstance` as clearly reported placeholders.
- `scripts/*.as` as `OriginalSource`, keeping the relative path where possible.
- Other entries listed as unsupported; the original container is not modified.

Rejected with a clear error: non-ZIP FLA bytes, a ZIP without `DOMDocument.xml`
and malformed XML. No partial project is written for those cases.

## 5. Export behavior

### `.nova`

Native save remains the normal project format. It is deterministic JSON and keeps
imported source metadata. Imported FLA/SWF projects intentionally have no save
path, so `Ctrl+S` cannot overwrite the original file with NØVA JSON.

### `.swf`

SWF export works from:

- native `.nova` projects;
- projects reconstructed from imported `.fla`/XFL;
- projects reconstructed from imported `.swf`.

The exporter emits real SWF bytes, preserves imported ABC unchanged and reports
warnings for data the editor could not represent. It never silently discards
unsupported elements.

### `.fla`

Native `.fla` export is **refused** by the Save and Export dialogs and by the CLI.
NØVA does not write JSON or an unverified ZIP under a `.fla` name.

### Experimental XFL ZIP

The export dialog and `nova export … .zip` can write an XFL compatibility
package containing `DOMDocument.xml`, `scripts/` and a README stating its
limitations. It is explicitly labelled experimental and is not a `.fla`
replacement.

## 6. UI and CLI

- Open Project filters: NØVA Project (`*.nova`), Adobe Flash Authoring (`*.fla`),
  Shockwave Flash Movie (`*.swf`), all supported formats.
- Imported FLA/SWF projects show a compatibility report dialog.
- Save As refuses `.fla` with an explicit error; `.nova` remains native.
- Export supports `.swf`, an XFL `.zip` package, and refuses `.fla`.
- The toolbar/menu label is **Film exportieren**.
- `ObjectKind::Path` is editable in the properties panel and resolves correctly
  for stage hit-testing/size.

CLI additions:

```text
nova import <datei> [ausgabe.nova]
nova analyze <datei>
nova export <datei> [out]      # .swf, .zip (XFL) or .nova
nova check <datei>             # .nova, .fla or .swf
```

`nova export input.swf` defaults to `input.reconstructed.swf`, so an input file
is not overwritten.

## 7. AI Agent integration surface

This repository has no AI Agent Editor, model provider or agent runtime. No
placeholder agent UI was added.

The format layer exposes what an agent would need:

- `nova_formats::analyze_path()` returns a serializable `AnalysisResult` with
  project structure, compatibility issues, capability data and per-script
  provenance.
- `ScriptAnalysis` includes `provenance`, `provenanceLabel`,
  `hasPreservedAbc`, `abcBytes`, `sourceBytes` and `isOriginalSource`.
- `nova analyze <file>` prints the same data as JSON plus a human-readable report.

Original source, reconstructed source, generated text and compiled bytecode are
distinct model states. Preserved ABC is never presented as original source.

## 8. Tests and conversion results

Final test run: **217 workspace tests, 0 failed**.

| Suite | Tests |
| --- | --- |
| `nova-app` | 70 |
| `nova-as` unit + integration | 34 + 29 |
| `nova-build` unit + integration | 4 + 5 |
| `nova-core` | 17 |
| `nova-formats` | 20 |
| `nova-script` | 13 |
| `nova-swf` unit + independent parser | 21 + 3 |
| Doc tests | 1 |

`nova-formats` uses real bytes and ZIP/XML fixtures; no importer or exporter is
mocked.

Conversion results:

| Path | Result |
| --- | --- |
| `.nova` → SWF → NØVA | PASS — dimensions, frame rate, layers and labels survive |
| `.nova` → SWF → NØVA → SWF → NØVA | PASS — preserved ABC bytes identical |
| `.fla`/XFL → NØVA → SWF → NØVA | PASS — 550×400 document and timeline frames survive |
| `.nova` → XFL ZIP → NØVA | PASS — document settings, layers and `.as` scripts survive |
| `.swf` (FWS/CWS/ZWS) → NØVA | PASS for supported tags; unsupported tags reported |
| Corrupt SWF / non-ZIP FLA / ZIP without DOMDocument | PASS — rejected with clear errors |
| Native `.fla` export | REFUSED by design — no fake file written |

These are structural tests. They do not prove visual equivalence or Adobe
authoring compatibility.

## 9. Runtime results

- Ruffle Desktop 0.7.1 previously verified playback of the regenerated
  `as3-example` and demo SWFs; that evidence is unchanged.
- A reconstructed SWF from `.swf` → NØVA → SWF was launched in Ruffle 0.7.1:
  Ruffle logged `Opening file:///…/as3-rebuilt.swf` and `Loaded SWF version 10,
  resolution 640x360 @ 24 FPS` with no AVM2 error. It did **not** log the
  original `avm_trace: Box gestartet`, because `SymbolClass` character links are
  not reconstructed and the preserved ABC is not instantiated. This is a known
  limitation, not a playback PASS for scripted content.
- Adobe Flash Player Projector is not installed. Adobe path validation, launch
  and playback are **NOT VERIFIED**.
- No generated FLA or XFL package was opened in Adobe Animate or Flash.

## 10. Build and packaging

Commands run after the final test additions:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS — 217 tests, 0 failed |
| `powershell scripts/build-release.ps1 -Target windows` | PASS — tests, release build, ZIP, Setup.exe, smoke tests and checksums |
| `python scripts/verify-packaging.py` | PASS — icons, plists, AppStream, desktop entry, legal files and installer sources |

Artifacts:

- `dist/windows/NØVA Flash Studio/NØVA Flash Studio.exe`
- `dist/windows/NOVA-Flash-Studio-0.3.0-rc1-windows-x64.zip`
- `dist/windows/NOVA-Flash-Studio-0.3.0-rc1-windows-x64-Setup.exe`
- `dist/windows/SHA256SUMS.txt`

`SHA256SUMS.txt` contains the hashes for the executable, ZIP and Setup.exe; the
Setup build log also reports the payload hash and a successful installer smoke
test. The artifacts are unsigned; no Authenticode certificate is available.
Windows packaging performs the workspace tests, release build, ZIP/Setup smoke
tests and checksum generation. macOS and Linux packages remain scripted but were
not built on their native hosts here.

## 11. Remaining work

- Validate native `.fla` import/export in Adobe Animate or Flash on a machine
  where a compatible authoring application is available.
- Validate Adobe Flash Player Projector path, launch and playback.
- Reconstruct `SymbolClass` character links so preserved ABC can be instantiated
  in a reconstructed SWF.
- Decode complex XFL edge strings and nested symbol timelines.
- Add editable text, bitmap, sound, filter and video import.
- Interactive UAC-denial and fresh-install `.swf` association checks.
- Code-signing decision; binaries remain unsigned.
- Legal review of the EULA draft; it remains a draft.
