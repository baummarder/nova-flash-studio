# Feasibility — real SWF, a Flash-style editor, and ActionScript 3

This document separates what has been **demonstrated** from what is still a
**plan**. The short version:

- Writing projector-compatible SWF from scratch in Rust is feasible. A real
  binary SWF is produced today and passes an independent parser.
- A Flash-style timeline editor on `egui` is feasible; the hard part is authoring
  UX, not rendering. Milestone 2 added undo/redo, multi-select, tween authoring,
  keyframe workflow and autosave with crash recovery.
- A useful ActionScript 3 subset compiles to real AVM2 ABC and is embedded with
  `DoABC2`/`SymbolClass`. The compiler is conservative: unsupported source is a
  source-located error, not a translation.
- Ruffle Desktop 0.7.1 execution and playback are verified for the regenerated
  sample and the app launch path. A local Adobe debug projector is configured
  and its process launch through the app is verified; visual Adobe playback
  remains **NOT VERIFIED**. Nothing in the UI or docs claims Adobe compatibility
  without that evidence.

---

## 1. What "projector-compatible SWF" actually requires

A SWF that a standalone Flash Player Projector will open must satisfy the
published SWF file format at the byte level:

1. A valid `FWS` (uncompressed) or `CWS` (zlib) header with version, declared
   file length, frame size as a twip-based `RECT`, frame rate as 8.8 fixed point,
   and frame count.
2. A tag stream beginning with the required movie tags and ending with `End`.
3. Correct tag headers: short form (code + 6-bit length) or long form
   (code `0x3F` + 32-bit length).
4. For the features we use: `FileAttributes`, `SetBackgroundColor`,
   `DefineShape3`, `PlaceObject2`, `RemoveObject2`, `ShowFrame`, `DefineSprite`
   and optional `FrameLabel`.
5. Bit-packed records in the exact field order and sign convention of the spec:
   `RECT`, matrices, `LINESTYLE`/`FILLSTYLE` arrays, shape records with variable
   bit widths, and `DefineSprite`'s nested tag list terminated by its own `End`.

For ActionScript 3 content there are additional requirements: an AVM2-capable
SWF version (9+, we emit 10), the AS3 bit in `FileAttributes`, `DoABC2`/`DoABC`
tags containing valid ABC bytecode, and `SymbolClass` or frame-script
associations so the runtime knows which class or code to execute.

## 2. What this repository demonstrates

Implemented and tested:

- `nova-swf`: a bit writer, `RECT`/matrix/color-transform encoders,
  `DefineShape3` with solid fills, rectangles, rounded rectangles and ellipses,
  `PlaceObject2`, `RemoveObject2`, `ShowFrame`, `FrameLabel`, `DefineSprite`,
  `FileAttributes`, `SetBackgroundColor`, `End`, FWS and zlib CWS assembly.
- `nova-build`: a deterministic baking pipeline. It resolves every timeline
  frame, deduplicates shapes by quantized geometry and fill, emits symbol
  sprites, and writes baked placements. Scripted objects are placed once at
  reserved depths and then owned by ActionScript.
- `nova-as`: lexer, parser, AST, symbol/type checking, diagnostics and an AVM2
  ABC emitter for the documented subset. Supported source produces constant
  pools, multinames, methods, traits, initializers and verifier-oriented
  bytecode; any error produces no ABC.
- Script embedding: `DoABC2` with named blocks, `SymbolClass` links for object
  classes and document classes, one-frame sprite wrappers, and automatic SWF
  version bump to ≥10.
- `nova-swf::inspect`: an independent reader that verifies header consistency,
  tag lengths, decompression, declared vs. actual length, and ends with `End`.
- Cross-validation with Ruffle's external `swf = "0.4"` parser as a dev
  dependency. Tests parse emitted movies, `DoABC2` tags, `SymbolClass` tables,
  sprite wrappers and the embedded ABC with Ruffle's AVM2 reader.
- Generated samples:
  - `samples/nova-demo.swf`: **1411 bytes, SWF v10, 550×400, 24 fps, 48 frames,
    145 placements, 4 shapes, 2 sprites, 346 tags, zlib**; the title object is
    scripted.
  - `samples/as3-example.swf`: **538 bytes, SWF v10, 48 frames, one scripted
    box**, compiled from the checked-in `samples/as3-example.nova`.
- 254 workspace tests, including header, bit-packing, tag layout, shape geometry,
  tween resolution, JSON round-trips, history/autosave, compiler diagnostics,
  ABC structure, script linking, format conversion, preserved ABC,
  independent-parser checks, mocked AI provider/agent behavior and safe file
  operations.

This is meaningful evidence: independent implementations accept the bytes and
recover the intended structure, and the compiler's output is structurally valid
ABC. It is **not** proof of projector execution or visual correctness.

## 3. Evidence, confidence and residual risk

### Why projector playback is plausible

- The bytes follow the same tag grammar the projector consumes; there are no
  private extensions or shortcuts.
- The independent parser validates header and tag lengths, which are the most
  common causes of immediate rejection.
- Shapes use `DefineShape3` (RGBA), matrices are encoded with the specified
  `Nbits`/`Ntranslate` layout, object-center translation is converted to twips,
  and sprites terminate their own tag lists.
- The file declares `FileAttributes` with the AS3 flag and a valid SWF 10
  version, so the AVM2-capable runtime path is selected. Scripts are embedded as
  named `DoABC2` blocks and linked through `SymbolClass`.

### What could still fail

- A projector build may be stricter about shape-record edge cases, fill-style
  ordering, or matrix precision than the independent parser.
- Frame-rate rounding, extremely small/large stage sizes, or very high frame
  counts could expose edge cases.
- The AVM2 verifier may reject bytecode that a structural ABC reader accepts.
  Scope depths, stack limits and constructor prologues are modelled on Adobe's
  output, but they have not been executed.
- `DoABC2` uses lazy initialization (`flags = 1`). SymbolClass resolution should
  trigger the class initialization, but that ordering is one of the first things
  a runtime test must confirm.
- The scripted placement policy (place once, do not re-bake) could interact with
  timeline removal/reappearance edge cases in ways only a runtime can reveal.
- The sample is simple. Complex fills, strokes, masks, nested sprites and a
  broad Flash API surface are not covered by runtime evidence.
- Flash Player is end-of-life; "works on one projector build" is not a
  compatibility guarantee for every build or for Ruffle.

### How to close the gap

Run the manual test in the README on at least one Adobe projector, record the
version and output, and add the result to the compatibility matrix. Ruffle Desktop
0.7.1 has already been tested through the app launch path; see
`docs/SWF_RUNTIME_COMPATIBILITY.md`. Until the Adobe test passes, use the phrase
"spec-based SWF, independently parsed" or "Ruffle-verified" rather than
"Adobe-projector-verified".

**Verdict:** SWF export is feasible and already real. Ruffle playback is verified
for the tested sample; Adobe compatibility remains unproven until the manual
Adobe test is performed.

## 4. ActionScript 3: subset compiler done, Ruffle-verified, Adobe unverified

The repository ships a real compiler for a documented subset:

- a hand-written lexer for AS3 tokens, operators, literals and comments;
- a recursive-descent parser producing an AST for packages, classes, fields,
  methods, statements and expressions;
- symbol/scoped type checking, call classification and unsupported-feature
  diagnostics with source spans;
- an AVM2 ABC emitter: constant pools, QName/NamespaceSet multinames, MethodInfo,
  traits, instance/class/script infos, MethodBody, initializers and bytecode;
- `DoABC2` embedding and `SymbolClass` linking, including document classes and
  one-frame sprite wrappers for scripted display objects;
- editor diagnostics plus `nova as-check`, which compiles and reports class/ABC
  counts.

The supported subset includes classes with single inheritance, access modifiers,
`static`, `var`/`const`, constructors, instance/static methods, default
parameters, primitive/class/`*`/`void` types, assignments, arithmetic,
comparisons, booleans, calls, property access, `new`, `if`/`else`, `while`,
`do while`, `for`, `break`, `continue`, `return`, `is`/`as`/`instanceof`, and
dynamic access to imported Flash classes. See
[AS3_COMPILER.md](AS3_COMPILER.md) for the exact boundary.

It is **not** full ActionScript 3. Interfaces, `super(...)`, getters/setters,
`switch`, exceptions, arrays/indexing, object literals, function values/closures,
generics and E4X are rejected. There is no complete `playerglobal` declaration
set; only six global free functions are known. Access modifiers select
namespaces, but compile-time access enforcement is incomplete. Metadata is
serialized as an empty array.

### What remains for runtime confidence

1. **Projector/Ruffle execution.** The decisive missing test. It will reveal
   verifier issues, lazy `DoABC2` initialization ordering, stack/scope mistakes
   and display-list behavior.
2. **Reference ABC comparison.** Compile the same small programs with a reference
   compiler and compare ABC structure/disassembly, not just parser acceptance.
3. **Broader API declarations.** A typed `flash.display`, `flash.events`,
   `flash.utils` and `flash.geom` surface would replace many dynamic calls and
   catch misuse earlier.
4. **Language breadth.** Interfaces, `super`, `switch`, `try/catch`, arrays,
   object literals, function values and metadata annotations are the next
   substantial language steps.
5. **Runtime-focused tests.** Golden ABC fixtures plus playback assertions in at
   least one runtime; Ruffle is useful but not authoritative for Adobe behavior.

### Feasibility judgement

A useful AS3 subset is not merely feasible; it is implemented and structurally
validated end to end. The remaining risk is concentrated in runtime semantics and
API breadth, not in the existence of a compiler or ABC writer.
[AVM2_BYTECODE.md](AVM2_BYTECODE.md) and [SWF_EXPORT.md](SWF_EXPORT.md) describe
the produced format and its validation limits.

The correct product claim today is: **a documented AS3 subset compiles to real
AVM2 ABC and is linked into SWF; Ruffle playback is verified for the tested
sample, while Adobe projector process launch is verified but visual Adobe
playback remains unverified.**

## 4b. Universal format compatibility

Feasible for the documented subsets, with explicit limits:

- `.nova` remains the native, versioned project model.
- `.fla` is handled as a ZIP/XFL container. Import supports document settings,
  timelines, layers, frames, labels, basic shapes, symbol instances and
  `scripts/*.as`. Complex edge strings are approximated from bounds; text,
  bitmaps, sounds, filters and nested symbol timelines are reported as partial
  or preserved-only.
- `.swf` import reconstructs dimensions, frame rate, frames, depth-based layers,
  shapes, flattened sprites, placements, frame labels and preserved ABC. It does
  not decompile bytecode and does not create a nested editable sprite library.
- `.swf` export works from native, imported-FLA and imported-SWF projects.
  Preserved ABC is embedded unchanged. The result is a structural
  reconstruction, not a byte-identical copy.
- Native `.fla` export is refused because no Adobe authoring application was
  available to validate it. An explicitly labelled XFL ZIP is offered instead.

See [FORMAT_COMPATIBILITY.md](FORMAT_COMPATIBILITY.md) for the exact subset,
tested versions and non-claims.

## 5. Flash-style animation editor

Feasible and largely demonstrated:

- `egui` handles the dark panel layout, custom-painted timeline, stage camera,
  hit-testing and direct manipulation well.
- The model resolves keyframes and motion tweens independently of the UI, and the
  exporter uses the same resolution function, so what is seen is what is baked.
- Undo/redo, multi-select, tween creation, keyframe workflow and autosave with
  recovery are implemented. Remaining authoring work: layer reordering and folder
  semantics, project-tree editing, find/replace and other editor depth, symbol
  edit mode, nested timelines, masks, text, bitmap and sound import. None of these
  challenges the core architecture; they are engineering and UX milestones.

Performance note: the current exporter bakes every frame and re-places every
object. That is fine for the demo and ordinary short clips, but large projects
need keyframe-diff placement and shape reuse policies. The 65,535 character-ID
limit is checked and reported as an error.

## 6. QuickJS: tools, not ActionScript

QuickJS is embedded for editor tools and extensions. It is a good fit for
scriptable project inspection, batch operations and future plug-ins. It is **not**
an AS3 runtime:

- AS3 semantics, AVM2 classes and the Flash display list are not JavaScript.
- Emitting JavaScript and calling it "AS3 export" would be false and is
  explicitly avoided.
- The console labels itself JavaScript/QuickJS; `nova.project` is a read-only
  JSON snapshot.

## 7. HTML5 as a secondary target

Not implemented. A later HTML5 exporter could share the resolved-timeline model
and emit Canvas/DOM or WebGL animation. It is secondary because the project's
primary promise is real SWF, and because a faithful HTML5 runtime would need its
own display-list, tween and asset pipeline. The `.nova` model is renderer-agnostic
enough to support this later.

## 8. Risks and mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Adobe projector configured but playback not observed | Adobe compatibility claims unproven | Ruffle runtime evidence + independent parser + process-launch check + manual Adobe playback checklist; keep wording honest |
| Flash EOL / unsupported projector | Distribution and security concerns | Offline/legacy use, explicit warning, no bundled runtime |
| AS3/ABC scope explodes | Product promise delayed | Subset-first compiler, declaration stubs, milestone gates |
| Baked export size/performance | Large SWFs for long timelines | Keyframe-diff placement, shape dedup already in place |
| Shape edge cases rejected by projector | Broken playback | Expand shape tests, compare against reference SWFs, runtime matrix |
| Legal/trademark confusion | Brand risk | Clear "not affiliated with Adobe" notice; own assets |

## 9. Go / no-go criteria

Proceed with the SWF-first strategy if:

- the demo SWF opens and animates in at least one runtime (Ruffle 0.7.1 is
  verified; an Adobe projector test is still required for Adobe claims);
- the scripted AS3 example executes and its `ENTER_FRAME` movement is visible;
- the independent parser continues to accept every emitted SWF and ABC file;
- the AS3 subset can be tested against a reference runtime;
- users understand the current limits (subset compiler, Adobe-unverified
  runtime, no HTML5 yet).

If projector playback fails, fix the byte-level issue before adding features.
If AS3/ABC work cannot be staffed, keep the product positioned as a **graphics
animation tool with an AS3 source editor**, not as an AS3 runtime.

## 10. Conclusion

The ambitious combination is feasible in stages:

1. **Real SWF graphics and animation** — demonstrated, pending projector
   confirmation.
2. **A credible Flash-style editor** — demonstrated at prototype depth, now with
   undo/redo, multi-select, tween/keyframe workflow and autosave.
3. **A real AS3 subset compiler and ABC/SWF linker** — implemented and
   structurally validated; runtime execution is the decisive remaining test.

The project is honest about which stage it is in, and the build pipeline fails
loudly (warnings and errors) instead of pretending missing features exist.
