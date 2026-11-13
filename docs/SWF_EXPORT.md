# SWF export

`nova_build::build_swf(project)` turns one resolved scene into an SWF byte
stream. This document describes the file structure the exporter actually emits,
including the ActionScript linking path. It also separates structural validation
from runtime playback: Ruffle 0.7.1 playback is verified for the tested sample,
while Adobe projector playback remains unverified.

## Pipeline

```text
Project (validated)
  → compile every enabled script to ABC
  → choose SWF version (≥10 when scripts are embedded)
  → Movie::new_avm2 / new_avm1
  → define library symbols and deduplicated scene shapes
  → wrap/link scripted objects (DefineSprite + SymbolClass)
  → emit DoABC2 blocks
  → bake the timeline frame by frame
  → serialize FWS or CWS
```

The build stops on the first compiler error. It never writes a partially compiled
script or an empty `DoABC2` payload.

The same exporter is used for native `.nova`, imported `.fla`/XFL and imported
`.swf` projects. Imported projects may contain paths reconstructed from SWF
shapes and ABC that must be preserved rather than recompiled.

## Header and compression

The writer emits a standard SWF header:

- `FWS` for uncompressed output;
- `CWS` for zlib output, with the compressed body following the zlib stream;
- file length field;
- stage `RECT` in twips, derived from scene width/height;
- frame rate as 8.8 fixed point;
- frame count;
- AVM2 files carry `FileAttributes` with `ActionScript3 = true` and a
  `SetBackgroundColor` tag.

The checked-in demo currently reports:

```text
SWF geschrieben: samples/nova-demo.swf (1411 Bytes, 48 Frames, 145 Platzierungen, 4 Shapes, 2 Sprites)
Verifikation: SWF v10 · 550×400 px · 24 fps · 48 Frames · 346 Tags · zlib
```

The exact byte count depends on compression and recompilation.

## Character definitions

- **Library symbols** become one-frame `DefineSprite` containers. Each sprite
  places one `DefineShape3` at depth 1, shows a frame and ends.
- **Scene graphics** are deduplicated by `(geometry, fill)` with coordinates
  quantized to 1/100 px. Every unique key becomes one `DefineShape3`.
- An instance's character is either the deduplicated shape or the sprite for its
  library symbol.

Shapes use `DefineShape3`, which supports RGBA fills. Rectangles, ellipses and
rounded rectangles are converted to edge/quadratic-curve paths by `nova-swf`.
Imported `ObjectKind::Path` geometry is emitted directly as `DefineShape3` paths,
including solid fills.

## Baked timeline

Every frame is resolved with the same `nova_core::resolve_scene` used by the
stage and playback. For each frame:

1. `RemoveObject2` is emitted for every depth used in the previous frame.
2. An optional `FrameLabel` is emitted when a layer keyframe defines one.
3. Non-scripted objects are placed at depths starting at 1 with `PlaceObject2`,
   including matrix, character ID, depth and a unique display name.
4. Scripted objects are placed once at a stable depth above the baked timeline.
5. `ShowFrame` ends the frame.

This is deterministic but larger than a keyframe-diff exporter. Geometry and
characters are deduplicated; placements are not.

## Scripted objects

A script can be attached to an object directly (`ObjectInstance.script`) or via
`Script.linked_object`. If the chosen class extends `MovieClip` or `Sprite`, the
exporter:

1. wraps the object's shape/sprite character in a new one-frame `DefineSprite`;
2. adds a `SymbolClass` entry mapping that wrapper to the class name;
3. records the object as scripted.

The wrapper is necessary because a class linked directly to a shape character
cannot be a display object with its own timeline and transform. A one-frame
sprite gives it `MovieClip`/`Sprite` behavior while preserving the original
visual.

### Placement ownership

A scripted object is placed **once**, not once per frame:

- its depth is allocated above all baked timeline depths;
- the first frame in which it appears gets a `PlaceObject2`;
- later frames do not re-place it while it remains visible;
- when it disappears, `RemoveObject2` is emitted;
- if it reappears, it is placed again at the same reserved depth.

This is a deliberate ownership rule. A per-frame `PlaceObject2` would reset
`x`, `y`, `rotation`, `scaleX`, `scaleY` or alpha that ActionScript changed in
`ENTER_FRAME`. The consequence is that authored timeline tweens for that object
are **not** re-applied after placement; the script owns the transform. Objects
without scripts are still baked normally.

The wrapper sprite itself contains one static placement of the visual at depth 1.
The outer object is what the script controls.

## `DoABC2`

Each compiled script produces one tag:

```text
DoABC2 (tag code 82)
  u32 flags       = 1 (lazy initialization)
  name            = script path, NUL-terminated (deduplicated with #N)
  data            = complete ABC block
```

`DoABC2` is the SWF 10+ variant with a name and a flag word; the older `DoABC`
tag (code 72) is not emitted. The name is not an import name; it is an
identifier for the block. When paths collide, the exporter appends `#1`, `#2`,
and so on.

The ABC payload is the direct output of `nova_as::compile`. It is not wrapped,
compressed separately or altered after serialization.

When a project was imported from SWF, `Script.abc` contains preserved AVM2
bytecode. In that case the exporter embeds those bytes unchanged and skips source
compilation and automatic class linking. The preserved bytes are not source and
are never presented as decompiled or original ActionScript.

`SymbolClass` entries are sorted by character ID. Character 0 is used for a
document class. The exporter only links a document class when exactly one
candidate remains; multiple candidates produce a warning and no character-0
link.

### Document classes

A script that is not attached to any object can become the document class when:

- it contains exactly one class, and
- the class name equals the file stem (case-insensitive) or `Main`, and
- the class extends `MovieClip` or `Sprite` (directly or locally).

It is linked to character 0. The movie's main timeline then instantiates that
class. If a script has several classes and none match, the script can still be
embedded but is not linked; the exporter warns when nothing at all was linked.

## Version bump

If scripts are embedded and the project requests a version below 10, the
effective SWF version is raised to 10. A warning reports the change. Without
scripts the requested version is used.

## Warnings and statistics

`BuildOutput` contains:

- `bytes`: the finished file;
- `warnings`: German user-facing messages;
- `stats`:
  - `frames`
  - `placed_objects`
  - `defined_shapes`
  - `defined_sprites`
  - `embedded_scripts`
  - `abc_bytes`
  - `bytes`

Warnings cover ignored scenes, disabled scripts, unavailable assets, AVM1
targets, missing linked objects, ambiguous document classes, version bumps and
scripts that could not be linked. The UI shows them in the export dialog and
console.

## Error behavior

- A project validation error aborts before any SWF is written.
- A compiler error aborts with `BuildError::ActionScript`, including rendered
  file/line/column diagnostics.
- Character-ID and depth exhaustion return `BuildError::Limit`.
- I/O errors are returned as `BuildError::Io`.

There is no fallback that embeds a script despite compiler errors.

## Structural validation

Tests and the CLI use two independent paths:

1. `nova-swf`'s own reader parses the emitted tags and header.
2. Ruffle's external `swf` crate parses the same bytes, including `DoABC2`,
   `SymbolClass`, sprites and shape tags.
3. Ruffle's `swf::avm2::read::Reader` parses the embedded ABC.

Integration tests assert the SWF version, `ActionScript3` attribute, tag names,
linked character IDs, class names, ABC instance counts, document-class links,
scripted placement count and compiler-error propagation.

This proves the file is structurally coherent SWF/ABC and that the ABC survives
embedding. It does **not** prove that an Adobe projector executes the bytecode,
that `DoABC2` lazy initialization behaves as expected at runtime, or that the
movement looks correct. Ruffle Desktop 0.7.1 execution is verified for the
regenerated sample and the app launch path; see
`docs/SWF_RUNTIME_COMPATIBILITY.md`. Adobe Flash Player Projector is not
installed or tested here. The manual Adobe projector test in the README remains
the gate before claiming Adobe runtime compatibility.

## Deliberate limits

- only the active scene is exported;
- imported SWF sprites are flattened; there is no nested editable sprite library;
- imported text, bitmaps, sounds, fonts, filters and video are not embedded;
- assets (bitmaps, sounds, fonts, text) are not embedded;
- library symbols have one-frame timelines;
- scripted objects are code-owned after first placement;
- masks/guides are not emitted;
- preserved ABC is embedded but cannot be edited as source;
- the full Flash API is not declared;
- unsupported AS3 constructs abort export.
