# NØVA Flash Studio vs Adobe Animate — honest technical comparison

Scope: drawing, timeline, symbols, AS3, SWF/XFL, audio, game development,
extension. Based on the shipped 0.9.0-rc1 feature set, not aspirations.

## Drawing tools

| | NØVA | Animate |
|---|---|---|
| Primitive shapes | rect/ellipse/path import, corner radius | full vector drawing (pen, pencil, brush, primitives) |
| Boolean ops / shape editing | none | unite/subtract/intersect/punch, smooth/subselect |
| Gradients | basic fills | linear/radial gradients, bitmap fills |
| Brushes | none | art/dyaamic brushes, spray, calligraphy |
| Perspective/skew tools | none | yes |

**Verdict:** NØVA is a compositor, not a drawing application. Animate wins
decisively for original vector artwork.

## Timeline & animation

| | NØVA | Animate |
|---|---|---|
| Keyframes | per-property, per-frame | same + motion presets, ease editor |
| Shape tweens | no | yes (shape hints) |
| Motion tweens | position/scale/rotation/alpha/color | full transform + filters + 3D |
| Inverse kinematics | no | bone/tool IK |
| Onion skin | basic | full |
| Nested timelines | symbols with own timelines | same, deeper nesting norms |

**Verdict:** adequate for UI/game animation; far behind for character
animation.

## MovieClips & symbols

Comparable conceptually: library symbols, instances, nested clips, export
for ActionScript linkage. NØVA's symbol model is simpler (no button states,
limited graphic-vs-movieclip typing).

## ActionScript 3

| | NØVA | Animate |
|---|---|---|
| Language | subset: classes, packages, typed arithmetic, events | full AS3 |
| Standard library | playerglobal APIs work at SWF v13 (JSON, ByteArray, Timer, TextField, BitmapData, Graphics, SimpleButton, geom.*, String/Array); no Vector/try/for-in/package-functions | complete playerglobal |
| Debugging | trace + Ruffle logs | debugger, breakpoints, profiler |
| Target | AVM2/SWF13 via own compiler | same runtime, official compiler |

**Verdict:** M14 closed most of the *runtime* gap — common playerglobal
APIs verified working in Ruffle. Remaining compiler limits (`Vector.<T>`,
`try/catch`, `for-in`, package functions, closures) plus missing authoring
systems (bones, shape tweens, font/button tags) still make porting real
Flash code impractical; new game code against the documented subset works.

## SWF export

NØVA emits valid SWF10/AVM2 that Ruffle plays — verified. Animate's export
is the reference implementation: more tag types (masks, buttons, JPEG,
video, fonts), better compatibility with the broader Flash ecosystem
(Adobe AIR, older players). NØVA covers the common game-content subset.

## XFL/FLA

Animate reads/writes XFL natively and FLA binary. NØVA parses XFL packages
and detects FLA, exports XFL with honest capability reporting — useful for
inspection/migration, not for interchange with live Animate projects.

## Audio

Both embed/stream sound; NØVA covers SoundCue-style timeline audio +
preview playback. Animate additionally has sound sync modes, MP3 encoding
options, microphone/ webcam components — NØVA is narrower but sufficient
for game SFX/music cues.

## Game development

This is NØVA's differentiator: scenes, tilemaps, particles, physics,
camera, input, typed events, AI-assisted authoring, game templates,
deterministic runtime, Ruffle-validated SWF output. Animate has *no*
built-in game runtime — game dev there means writing everything in AS3
or using CreateJS/Starling/Heaps. For a 2D-game-focused workflow NØVA is
arguably ahead; for general animation/motion-graphics Animate is ahead.

## Extensibility & AI

Animate: JSFL scripting (ExtendScript), CEP panels. NØVA: gamekit AS3
scripts, transactional AI operation set (~40 ops), provider-agnostic LLM
integration with privacy classification — a different model: the editor
itself is agent-operable end-to-end.

## Bottom line

NØVA Flash Studio is **not** a drop-in Flash/Animate replacement. It is a
narrower, game-focused authoring environment that legitimately produces
working SWFs for its supported subset. Choose it for 2D game projects
where the built-in runtime/AI/tilemap/particle pipeline matters; choose
Animate for general animation, vector illustration, broadcast content, or
any project requiring the full AS3 ecosystem.
