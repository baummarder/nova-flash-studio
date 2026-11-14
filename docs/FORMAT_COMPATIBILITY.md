# Format compatibility report

This report describes the real format support in NØVA Flash Studio
`0.4.0-rc1`. It is intentionally narrower than "supports FLA" or "supports
SWF": every claim below is tied to implemented code, fixtures and tests. The
M8 additions are documented in their own section below; M7.3 format results are
retained as historical evidence.

## Non-claims

- NØVA does **not** claim universal Adobe FLA support.
- NØVA does **not** claim that an exported XFL ZIP is a valid Adobe `.fla`
  authoring file. It has not been opened in Adobe Animate or Flash.
- NØVA does **not** claim Adobe Flash Player playback. A local debug projector
  was configured; its process and window started, but the tested SWF area
  rendered blank and no trace log appeared. Adobe load, visual playback and
  script execution remain **NOT VERIFIED**.
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

## Runtime results (Milestone 7.3)

- Ruffle Desktop 0.7.1 visually played the M7.3 `validation.swf` (24 frames,
  2 ABC scripts, SymbolClass `(0, Main)` and `(4, Added)`): the red
  document-class marker, cyan linked-sprite rotation, orange tween motion and
  timeline labels were captured. Ruffle logged recurring non-fatal
  `ReferenceError: Error #1056` for named timeline children.
- A one-frame `SymbolClass` fixture executed its linked `Added` class in Ruffle
  before and after a one-frame SWF import/re-export.
- A multi-frame SWF import/re-export preserved frames, ABC scripts and structural
  links `(0, Main)`/`(49, Added)`, but the linked `Added` rotation was not observed
  after the round-trip. This is **PARTIAL**.
- The Adobe Flash Player debug projector 32.0.0.465 started a process and window,
  but rendered the tested SWFs blank and produced no `flashlog.txt`. Adobe
  playback is **NOT VERIFIED**.
- See `docs/SWF_RUNTIME_COMPATIBILITY.md` for the full evidence table.

## AI Agent Editor status

The repository now contains the Milestone 7.2/7.3 AI Agent Editor, provider
layer and credential store. The format layer exposes the integration surface the
agent uses:

- `nova_formats::analyze_path()` returns a serializable `AnalysisResult` with
  `ProjectAnalysis`, `ScriptAnalysis`, compatibility issues and capability data.
- `ScriptAnalysis` includes `provenance`, `provenanceLabel`, `hasPreservedAbc`,
  `abcBytes`, `sourceBytes` and `isOriginalSource`.
- `nova analyze <file>` prints the same JSON plus the human-readable report.

No AI-generated code is labelled original. The agent writes only after a
confirmed diff; preserved ABC remains bytecode and is never presented as
reconstructed source. Milestone 7.3 validated the backend workflow live against
local Ollama `qwen2.5-coder:7b`; cloud providers and the full interactive GUI
workflow remain unverified.

## Mega Milestone 8 format changes

### `.nova`

- Format version remains `1`. New fields (`Text` objects, layer folder parent/
  expanded, `export_all_scenes`) use serde defaults and load old projects.
- Multiple scenes are persisted and editable. `export_all_scenes` defaults to
  `false`, so existing active-scene-only behavior is preserved.
- Text objects store text, size, alignment and colour with a built-in font.
  There is no external font reference to lose.

### SWF export/import

- Text is exported as `DefineShape3` geometry. There is no `DefineText`,
  `DefineFont`, system-font or editable runtime text. Imported `DefineText` is
  still not reconstructed as editable text.
- Multi-scene export concatenates scene timelines and inserts scene boundary
  labels. This is an NØVA export convention, not an Adobe scene marker.
- Layer folders are editor-only; hidden folder descendants are excluded from the
  exported timeline.
- Import still flattens sprites and does not reconstruct nested MovieClip
  timelines, bitmaps, sounds or fonts.

### XFL export/import

- The XFL exporter writes text as baked shape glyphs, not editable text fields.
- The M7.3 XFL round-trip limitation is unchanged: frames 1–22 were empty and
  the authored tween was lost. M8 did not claim or achieve tween-fidelity.
- Native `.fla` export remains explicitly unsupported. The ZIP/XFL package is
  experimental and **NOT VERIFIED** in Adobe Animate.

### M8 status

| Area | Import | Export | Fidelity |
| --- | --- | --- | --- |
| Multi-scene `.nova` | **PASS** | **PASS** (opt-in) | Native |
| Text | Preserved/unsupported as editable | Shapes only | Static |
| Layer folders | **PASS** | Flattened by visibility | Editor-only |
| XFL | **PASS** subset | **PASS** experimental | **PARTIAL** |
| Native `.fla` | **PASS** subset | **NOT AVAILABLE** | — |

## Current limitations

- Native `.fla` export is not implemented.
- The XFL package is experimental and untested in Adobe Animate.
- The M7.3 XFL export/re-import round-trip preserved document settings and
  scripts, but frames 1–22 were empty and the authored tween was lost. Geometry
  and library symbols are approximated.
- Complex XFL edge geometry, filters, text, bitmaps, sounds and nested symbol
  timelines are incomplete.
- SWF sprites are flattened; there is no editable nested symbol library.
- SWF strokes, gradients, bitmap fills, filters, text, sound and video are not
  fully editable.
- `SymbolClass` character links are reconstructed when the referenced character
  maps to a visible reconstructed object. Unmappable links are reported, not
  invented. Preserved ABC is embedded, but a reconstructed movie may still differ
  from the original display-class behavior.
- A one-frame SymbolClass round-trip executed its linked class in Ruffle, but a
  multi-frame rebuilt SWF preserved only the structural links; visible linked-class
  execution across rebuilt frames is **PARTIAL**.
- Preserved ABC cannot be recompiled or edited as ActionScript.
- Adobe visual playback, Adobe authoring application compatibility and XFL
  authoring compatibility remain **NOT VERIFIED**.
