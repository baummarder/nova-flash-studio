# Mega Milestone 10.1 — Game Runtime Completion & Release Stabilization

**Release:** `0.6.0-rc2` (Windows x64) · **Base:** `0.6.0-rc1` (M10, preserved)
**Scope:** close the three documented M10 gaps — scene transitions, game event
bus, audio in game preview — plus lifecycle unification, AI coverage, tests
and Ruffle regression.

---

## 1. Audit outcome (Phase 0)

What already existed and was **reused, not duplicated**:

- `nova_core::game` — deterministic preview simulation (`collect_bodies`,
  `step`, `step_camera`, `InputState`, `BodyState`, `CameraState`).
- `nova-app` `GamePreview` — a thin renderer-side mirror of the simulation
  state, replaced by the new runtime (no second simulation was added).
- `nova_core::gamekit` + `games` — installable AS3 helpers and seven
  templates; untouched except encoding repair.
- `Asset`/`SoundCue` model + SWF `DefineSound`/`StartSound` export (M9).
- `play_sound_bytes` (PlaySoundW) — a manual asset-preview helper; it cannot
  do mixing, volume, pause or multiple instances, so it was **not** reused
  for the game preview. A real backend was added instead (see §4).
- `windows-sys` `Win32_Media_Audio` — already present for PlaySoundW.

## 2. Scene transitions (Phase 1)

New model field (format stays **version 3**, serde-defaulted — v2/v3 files
load unchanged):

```rust
pub struct SceneTransition { pub key: String, pub target: String }
// GameSceneConfig.transitions: Vec<SceneTransition>
```

`key` uses the input-binding naming space (`"Space"`, `"Enter"`, `"A"`,
`"ArrowLeft"`, …) and fires on the **rising edge** while the scene is
active; `target` is a scene **name** (id strings also resolve).

`nova_core::game::runtime::GameRuntime::goto_scene(project, target)`:

- resolves by name or UUID string;
- returns `RuntimeError::SceneNotFound` / `NoScenes` **without mutating
  runtime state** (verified by test);
- is a no-op when the target is the active scene;
- persists each scene's body states (`scene_states` map) so leaving and
  returning restores positions — the unload/load lifecycle;
- dispatches `SceneUnload(old)` → `SceneTransition(from→to)` →
  `SceneLoad(new)`;
- recollects bodies via `collect_bodies` and reseeds/restores states,
  camera and config.

Triggers:

- **Configured key transitions** evaluated inside `GameRuntime::step`
  (at most one per step; a failed target dispatches a `SceneTransition`
  event carrying the error and the simulation keeps running).
- **External scene selection** while playing: the app routes scene-tree
  changes through `goto_scene`, so the same lifecycle/events apply.
- **Programmatic**: `goto_scene` is public API for tests and future UI.

AS3 cannot call into the host process, so exported SWFs do **not** get
scene switching from `.nova` scene configs — documented limitation (§8).

## 3. Game event bus (Phase 2)

`nova_core::game::runtime::EventBus` + `GameEvent`/`GameEventKind`:

- kinds: `GameStart`, `GamePause`, `GameResume`, `GameStop`, `GameRestart`,
  `SceneLoad`, `SceneUnload`, `SceneTransition`, `CollisionEnter`,
  `CollisionExit`, `InputPressed`;
- `register(kind, Fn(&GameEvent)) -> ListenerId`, `register_all`,
  `remove(id) -> bool`, `listener_count`;
- multiple listeners per kind; dispatch iterates a snapshot taken by
  removing the listener Vec during the call, so listeners may safely
  register/remove during dispatch (order preserved);
- recursion: re-entrant `dispatch` is allowed up to
  `MAX_DISPATCH_DEPTH = 8`; deeper calls increment `dropped_events` and
  return without invoking listeners — no unbounded recursion;
- collision events are edge-detected from **pre-integration** positions
  each step (enter/exit pairs, sorted UUID pairs);
- input events fire on rising edges of the six logical actions;
- scene/lifecycle events fire from the runtime methods above.

Listeners are Rust callbacks — there is deliberately no AS3-side listener
surface (the AS3 subset has no host-call mechanism).

## 4. Audio in game preview (Phase 3)

New `crates/nova-app/src/audio.rs` (`AudioEngine`) on **rodio 0.23**
(symphonia decoders: WAV/MP3/FLAC/OGG/AAC…):

- `DeviceSinkBuilder::open_default_sink()` → shared `Mixer`; every `play`
  decodes the embedded asset bytes into its own `rodio::Player`, so
  multiple instances overlap;
- `set_volume` (master gain, persisted in `AppState::preview_volume` with a
  timeline-transport slider), `pause_all`, `resume_all`, `stop_all`,
  `stop_asset`, `prune` (finished players are dropped every frame);
- construction never panics: without an audio device the engine stays
  inert and `last_error` explains why; `play` returns `Err`;
- `cues_on_frame(scene, frame, assets)` collects `SoundCue`s from sound
  layers; `NovaApp::play_sound_cues` fires them when the playhead enters
  a frame (including on play/restart), logs warnings for missing assets
  and decode failures.

Playback lifecycle follows transport: pause → `pause_all`, play →
`resume_all`, stop/restart → `stop_all`.

**Verified:** WAV bytes decode through `rodio::Decoder` (unit test),
invalid bytes are rejected, cue collection filters missing assets,
headless/no-device path returns errors. **Not verified:** actual audible
output (no audio assertion possible headless) and Ruffle audio playback —
see §7.

## 5. Game lifecycle (Phase 4)

`RuntimeStatus::{Running, Paused, Stopped}` on `GameRuntime`:

- `step` is a no-op unless `Running` and `dt > 0`; `dt` clamped to 100 ms
  (prevents tunneling/time jumps after tab switches); `time` accumulates
  clamped dt only;
- pause freezes all body/camera/contact state (tested: positions and
  `time` unchanged across paused steps);
- `stop` resets to authored positions, zeroes time, clears contacts and
  keeps per-scene memory for `goto_scene` returns;
- `restart` resets the current scene and keeps running;
- UI: play/pause toggle maps to `start`/`pause`/`resume`, new "Neu"
  transport button calls `restart`, "Stop" calls `stop`; stage overlay
  shows `Szene · t=…s · läuft/pausiert/gestoppt`.

## 6. AI agent integration (Phase 5)

New typed operation `ProjectOperation::SetSceneTransition { key, target,
scene?, remove? }`:

- validates non-empty/bounded key, existing target scene name, update-in-
  place per key (no duplicates), meaningful errors for missing transition;
- transactional like all ops (preview → confirm → apply, undo via history);
- tested in `nova-ai` (add, update, invalid target rejection preserving
  project equality, remove).

Editor coverage: scene inspector gained a "Szenenwechsel" section
(key field, target-scene combo, remove, add). Audio cues were already
editable (M9 `PlaceSoundCue`, `ImportAudioAsset`); lifecycle settings are
runtime state, not project data, so there is intentionally no lifecycle
op — documented rather than faked.

## 7. Tests (Phase 6)

`nova-core` `game::runtime` tests (12):

- lifecycle event order, pause freeze/resume, restart reset, dt clamp;
- `goto_scene` success + event kinds, unknown-target rejection preserving
  scene/bodies/status, per-scene state restore on return;
- event bus: multiple listeners, removal, count, depth-limit drop;
- collision enter/exit edges; key-triggered transition fires once per
  press; failed transition keeps runtime running.

`nova-app` `audio` tests (4): WAV decodes via rodio, invalid bytes
rejected, `cues_on_frame` filters missing assets, no-device engine errors
cleanly.

`nova-ai` (1 new): `SetSceneTransition` validation/update/remove.

Full `cargo test --workspace`: **all green** (~330 tests).

## 8. Runtime validation (Phase 7, Ruffle 0.7.1)

SWF export is unchanged by M10.1 (transitions/audio are preview-side), so
validation is a **regression re-run**: all seven `samples/m10-*.nova`
exported and played in Ruffle 0.7.1 for ~6 s each.

| Template | Result |
|---|---|
| blank-game | PASS (no AVM2 errors) |
| pong | PASS |
| breakout | PASS |
| platformer | PASS |
| topdown | PASS |
| racing | PASS |
| menu | PASS |

Status key: **PASS** = ran without AVM2/verifier errors in stderr;
**NOT VERIFIED** = not exercised (see below).

- Scene transitions in SWF: **NOT IMPLEMENTED** (preview runtime only).
- Game events in SWF: **NOT IMPLEMENTED** (Rust-side listeners only).
- Audio in SWF export: unchanged M9 behavior (`StartSound` event-like);
  Ruffle audio output **NOT VERIFIED** (no audio assets in templates).
- Audio in editor preview: decode path **PASS** (unit-tested); audible
  output **NOT VERIFIED** headless.

## 9. Release (Phase 8)

- `cargo fmt --check`, `cargo check --workspace`, `cargo test --workspace`
- version `0.6.0-rc2`, Windows ZIP + installer rebuilt, smoke-tested,
  `dist/windows/SHA256SUMS.txt` updated
- `0.6.0-rc1` artifacts preserved under `dist/` (see §10).

## 10. Known limitations (honest list)

- Scene transitions: editor preview only; no AS3 `gotoAndStop`/scene API.
- Event bus: Rust listeners only; no AS3 event surface.
- Preview audio: plays each cue once per frame entry; `loop_count`,
  `offset`, `duration` honoured by SWF export only.
- Collision events use pre-integration positions; dynamic-vs-dynamic pairs
  are not resolved (M10 scope, unchanged).
- macOS/Linux packages scripted but not built on native hosts.
- Adobe Flash playback unverified (Ruffle is the validation target).
- An encoding incident during M10 (`Set-Content -Encoding utf8`
  double-encoding) was found and repaired in commit `44a12f7`; a repo-wide
  scan confirms no remaining mojibake in tracked text files.
