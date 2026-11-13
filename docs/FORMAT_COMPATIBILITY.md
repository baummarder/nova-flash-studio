# Format compatibility report

This report describes the real format support added for NØVA Flash Studio
`0.3.0-rc1`. It is intentionally narrower than "supports FLA" or "supports
SWF": every claim below is tied to implemented code, fixtures and tests.

## Non-claims

- NØVA does **not** claim universal Adobe FLA support.
- NØVA does **not** claim that an exported XFL ZIP is a valid Adobe `.fla`
  authoring file. It has not been opened in Adobe Animate or Flash.
- NØVA does **not** claim Adobe Flash Player playback. A local debug projector
  was configured and its process launch was observed, but no visual playback or
  script execution was verified.
- NØVA does **not** decompile AVM2 bytecode. Preserved ABC is bytecode, not
  recovered ActionScript source.
- A reconstructed SWF is not byte-for-byte identical to the imported SWF.

## Format matrix

| Format | Import | Export | Evidence |
| --- | --- | --- | --- |
| `.nova` | Full native JSON model | Full native JSON model | Existing model round-trip tests plus new format dispatch tests |
| `.fla` (ZIP/XFL) | Documented partial subset | Native `.fla` export refused | XFL fixtures, nested-container fixture and unsupported-entry tests |
| `.swf` | Reconstructs editable subset; preserves ABC | Real SWF from NØVA, imported FLA or imported SWF | FWS/CWS/ZWS, SWF 9/10/13, sprite, label, ABC, unknown-tag and corrupt-file tests |
| `.zip` XFL package | Re-imported by the same XFL parser | Explicitly experimental compatibility package | XFL export/re-import tests |

## SWF import

The importer uses the MIT/Apache-licensed `swf` crate. It is a reconstruction
pipeline, not a decompiler.

### Header and document settings

- FWS, CWS and ZWS/LZMA are imported. FWS and CWS are tested from the NØVA
  writer; ZWS is tested with a minimal LZMA fixture.
- Stage width, height, background colour, frame rate and header frame count are
  imported.
- SWF 9, 10 and 13 fixtures are tested. Header versions are reported verbatim;
  other versions have not been exhaustively exercised.
- Invalid or corrupt SWF bytes are rejected before a project is produced.

### Shapes

- `DefineShape`/`DefineShape2`/`DefineShape3` shapes are converted into editable
  path objects with solid fills.
- Shape bounds become the object selection bounds; path commands are kept in
  the NØVA model.
- Strokes, gradients and bitmap fills are not fully editable. They are reported
  as compatibility warnings rather than silently discarded.

### Sprites and timeline

- `DefineSprite` timelines are flattened into the main timeline. Nested sprite
  content is placed with composed matrices.
- Each SWF depth becomes one NØVA layer; every reconstructed frame is a keyframe.
- `PlaceObject`/`PlaceObject2`, `RemoveObject`/`RemoveObject2`, visibility,
  alpha and frame labels are handled.
- `DefineSceneAndFrameLabelData` labels are imported where present.
- Deep sprite nesting is bounded; content beyond the bound is reported.

### ActionScript and ABC

- `DoABC` and `DoABC2` payloads are preserved byte-for-byte in
  `Script.abc`.
- The visible script text is a marker that explicitly says original source was
  not recovered. It is never presented as decompiled or original code.
- `SymbolClass` links are reported and reconstructed when the referenced
  character maps to a visible reconstructed object; `ExportAssets` and metadata
  are reported as information.
- On re-export, preserved ABC is embedded directly instead of recompiling the
  marker text.

### Unsupported data

Unknown or unsupported tags (text, buttons, fonts, sounds, video, filters,
bitmaps and unknown tag codes) are listed in the compatibility report. The
original SWF bytes remain available in `ImportedProject::source_bytes`; saving a
`.nova` project does not overwrite or delete the original file.

## FLA/XFL import

Modern `.fla` files are ZIP/XFL containers. NØVA reads a documented subset.

### Container and document

- A ZIP containing `DOMDocument.xml` at the root or in a nested directory is
  accepted.
- A ZIP without `DOMDocument.xml`, a non-ZIP file and malformed XML are rejected.
- `width`, `height`, `frameRate` and `backgroundColor` are imported.

### Timelines and elements

- `DOMTimeline`, `DOMLayer`, `DOMFrame`, keyframes and frame labels are mapped to
  NØVA scenes, layers and frames.
- `DOMRectangleObject` and `DOMEllipseObject` become native shapes.
- `DOMShape` becomes an editable path approximated from its bounds. XFL edge
  strings are not fully decoded, and this is reported as a geometry warning.
- `DOMSymbolInstance` creates a library symbol; nested symbol timelines are not
  fully reconstructed.
- `DOMGroup` is traversed recursively.
- `DOMBitmapInstance` and `DOMTextInstance` become clearly reported placeholders.
- Other elements are listed as unsupported; the original container is not
  modified.

### ActionScript

- Files ending in `.as` are imported as **original source** with
  `CodeProvenance::OriginalSource`.
- Scripts keep their relative path where possible.
- The importer does not invent source for missing or compiled-only content.

## Export behaviour

### `.nova`

Native save remains the normal project format. It is deterministic JSON and
keeps imported source metadata.

### `.swf`

SWF export works for:

- native `.nova` projects;
- projects reconstructed from imported `.fla`;
- projects reconstructed from imported `.swf`.

The exporter emits real SWF bytes and reports warnings for data the editor could
not represent. Imported ABC is preserved. A reconstructed SWF is explicitly
labelled as a reconstruction, not a byte-identical copy.

### `.fla`

Native `.fla` export is refused. The Save and Export dialogs show an explicit
error instead of writing JSON or an unverified ZIP under a `.fla` name.

### Experimental XFL ZIP

The export dialog can write a `.zip` XFL compatibility package containing
`DOMDocument.xml`, `scripts/` and a README stating its limitations. This is
**not** a `.fla` replacement and has not been tested in Adobe Animate.

## Code provenance

The model distinguishes five states:

| Provenance | Meaning |
| --- | --- |
| `OriginalSource` | Authored in NØVA or read verbatim from an authoring file |
| `Decompiled` | Recovered by a decompiler; never presented as original |
| `Reconstructed` | Rebuilt from bytecode or an incomplete container |
| `Generated` | Written by an importer/exporter or template |
| `CompiledBytecode` | Only AVM2 ABC bytes exist; no source was recovered |

SWF imports use `CompiledBytecode` and keep the ABC bytes. XFL `.as` entries use
`OriginalSource`. Marker text is never labelled as original.

## Conversion test results

The `nova-formats` test suite includes:

- NØVA → SWF → import: stage size, frame rate, layers and labels survive.
- NØVA → SWF → import → SWF export → import: preserved ABC bytes are identical.
- NØVA → XFL ZIP → import: document settings, layers and scripts survive.
- FLA/XFL → NØVA → SWF → import: stage size and timeline frames survive.
- FWS, CWS and ZWS/LZMA variants all import.
- SWF 9, 10 and 13 headers are reported correctly.
- A synthetic unknown tag `Unknown(999)` is reported.
- Corrupt SWF, non-ZIP FLA and ZIP-without-DOMDocument inputs are rejected.
- Nested `project/DOMDocument.xml`, `DOMShape`, `DOMSymbolInstance` and an
  unsupported `DOMVideoInstance` are exercised.

These are structural conversion tests. They do not prove visual equivalence or
Adobe authoring compatibility.

## Runtime results

- Ruffle Desktop 0.7.1 playback of the previously tested NØVA sample remains the
  only runtime evidence in this repository. See
  `docs/SWF_RUNTIME_COMPATIBILITY.md`.
- Adobe Flash Player Projector is not installed. Adobe path validation, launch
  and playback are **NOT VERIFIED**.
- Reconstructed SWFs from imported FLA/SWF are structurally validated but have
  not been added to the Ruffle playback matrix.

## AI Agent Editor status

This repository does not contain an AI Agent Editor, a model provider, or an
agent runtime. There is therefore no agent UI to extend and no provider call to
modify.

The format work exposes the integration surface an agent would need:

- `nova_formats::analyze_path()` returns a serializable `AnalysisResult` with
  `ProjectAnalysis`, `ScriptAnalysis`, compatibility issues and capability data.
- `ScriptAnalysis` includes `provenance`, `provenanceLabel`, `hasPreservedAbc`,
  `abcBytes`, `sourceBytes` and `isOriginalSource`.
- `nova analyze <file>` prints the same JSON plus the human-readable report.

No AI-generated code is labelled original. The Milestone 7.2 agent writes only
after a confirmed diff; preserved ABC remains bytecode and is never presented as
reconstructed source.

## Current limitations

- Native `.fla` export is not implemented.
- The XFL package is experimental and untested in Adobe Animate.
- Complex XFL edge geometry, filters, text, bitmaps, sounds and nested symbol
  timelines are incomplete.
- SWF sprites are flattened; there is no editable nested symbol library.
- SWF strokes, gradients, bitmap fills, filters, text, sound and video are not
  fully editable.
- `SymbolClass` character links are reconstructed when the referenced character
  maps to a visible reconstructed object. Unmappable links are reported, not
  invented. Preserved ABC is embedded, but a reconstructed movie may still differ
  from the original display-class behavior.
- Preserved ABC cannot be recompiled or edited as ActionScript.
- Adobe runtime and Adobe authoring application compatibility remain unverified.
