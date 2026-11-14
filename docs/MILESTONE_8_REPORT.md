# Mega Milestone 8 Report — `0.4.0-rc1`

M8 moves NØVA Flash Studio from the validated `0.3.0-rc2` editor baseline to a
broader Flash authoring platform. This report states what was actually
implemented and verified. It does not claim completeness, Adobe compatibility or
runtime proof that was not obtained.

Result categories: **PASS**, **FAIL**, **PARTIAL**, **NOT VERIFIED**,
**NOT AVAILABLE**.

## Baseline and release decision

- Baseline: `0.3.0-rc2`, commit `fd178f376d00bf94278f73d048c19bd02dafbf31`.
- M8 version: `0.4.0-rc1`.
- `.nova` format version: `1` (new fields are serde-defaulted).
- The validated rc2 artifacts are preserved in `dist/archive/0.3.0-rc2/`.
- Binaries are unsigned. No signing certificate was available or used.

## Implemented

### Document model and architecture

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Static text objects (`TextObject`, alignment, size, colour) | **PASS** | Model, metrics, validation, stage rendering and SWF/XFL shape export |
| Built-in 5×7 vector font (ASCII + common German) | **PASS** | Glyph table and path tests; limited character coverage |
| Text export as shapes, no fake `DefineText` | **PASS** | `m8_features` test asserts shapes and absence of text tags |
| Layer folders, recursive visibility/lock | **PASS** | Hierarchy/cycle tests and export exclusion test |
| Folder-link repair and layer removal | **PASS** | `nova-core` tests |
| Multiple scenes | **PASS** | Native model + save/load; scene validation |
| Multi-scene SWF export | **PASS (opt-in)** | Concatenated timelines and boundary labels; playback not visually checked |
| Format-version compatibility | **PASS** | Serde defaults; existing format version 1 |
| Templates: empty, animation, platformer, UI kit | **PASS** | Four real templates create/build/validate; shared GUI/CLI |

### Animation editor and UX

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Object copy/paste/duplicate | **PASS** | App actions/tests |
| Align and distribute | **PASS** | Multiselection actions |
| Grid snapping while dragging | **PASS** | Stage drag path |
| Code find/replace with case toggle | **PASS** | Code-view tests |
| Project-wide text search | **PASS** | Code-view results, no automatic rewrite |
| New-project template wizard | **PASS** | GUI dialog plus CLI |
| Guides, rulers, rotation-centre handles | **NOT AVAILABLE** | Not implemented |
| Line/brush/pencil/eraser/fill tools | **NOT AVAILABLE** | Only rect, ellipse and text insertion exist |
| Masks, guide layers, shape tweens, multi-frame onion editing | **NOT AVAILABLE** | Timeline supports motion tweens only |
| Nested symbol timelines / symbol editing mode | **NOT AVAILABLE** | Symbols remain one-frame sprites |
| Scale/rotate handles on stage | **PARTIAL** | Numeric/shared transforms and drag move exist; no full transform gizmo |

### AS3 compiler and editor

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Array literals | **PASS (structural)** | Checker + `newarray` ABC + parser test |
| Object literals | **PASS (structural)** | `newobject`, string keys |
| Indexed reads/writes and compound/index updates | **PASS (structural)** | `MultinameL`, dynamic `getproperty`/`setproperty` |
| Runtime semantics of arrays/objects/indexing | **NOT VERIFIED** | No Ruffle/Adobe execution fixture |
| Interfaces, `super.method()`, `switch`, try/catch/finally | **NOT AVAILABLE** | Still compiler diagnostics |
| Getters/setters, closures, `for-in`, namespaces, `Vector` | **NOT AVAILABLE** | Still compiler diagnostics |
| Code completion, go-to-definition, references, rename, folding | **NOT AVAILABLE** | Not implemented in this slice |
| Find/replace and project search | **PASS** | Editor only; no symbol-aware navigation |
| Full `flash.*` API compatibility layer | **NOT AVAILABLE** | Dynamic multinames only; no complete declarations |

### AI Agent Editor

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Structured project operations | **PASS** | 12 operation types, validation and preview |
| Transactional apply/rollback | **PASS** | Model untouched on any error; app test |
| Explicit confirmation | **PASS** | Preview tab and “Alle bestätigen” gate |
| Operation limits | **PASS** | 32 operations per batch |
| Project/stage/scene/layer/object/keyframe/label/tween/symbol/script operations | **PASS** | Unit and app tests |
| Undo grouped as one model snapshot | **PASS** | App accept path |
| File diffs remain separate and confirmed | **PASS** | Existing M7.3 path preserved |
| Live provider validation in M8 | **NOT VERIFIED** | No new live run; M7.3 local Ollama result remains historical |
| Cloud providers | **NOT AVAILABLE** | No credentials/endpoints configured |
| Autonomous execution / provider tools | **NOT AVAILABLE** | Deliberate security boundary |

### SWF, XFL and runtime

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Text as SWF shapes | **PASS (structural)** | Shape tags and independent parser |
| Multi-scene frame counts/labels | **PASS (structural)** | `nova-build` tests |
| Hidden folder exclusion | **PASS (structural)** | `nova-build` test |
| XFL text shape export | **PASS (structural)** | XFL writer arm |
| XFL tween/keyframe round-trip fidelity | **PARTIAL** | M7.3 limitation unchanged: tween lost, empty frames 1–22 |
| Multi-frame `SymbolClass` visible execution | **PARTIAL** | Structural links only |
| Ruffle M7.3 visuals | **PASS (historical)** | Marker, linked sprite, tween, labels |
| M8 SWF visual playback | **NOT VERIFIED** | No M8 fixture launched in Ruffle/Adobe |
| Adobe visual playback | **NOT VERIFIED** | Projector window blank in M7.3; no M8 run |
| Adobe Animate/XFL authoring | **NOT AVAILABLE** | No Adobe Animate or authored `.fla` fixture |
| Bitmap/audio/font import and export | **NOT AVAILABLE** | Preserved-only or absent |
| Native `.fla` export | **NOT AVAILABLE** | Explicitly refused |
| Ruffle `Error #1056` | **PARTIAL** | Non-fatal, unfixed |

### Game workflow and project management

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Platformer template with keyboard movement, platforms, coins, HUD | **PASS** | Real editable project, build test |
| Animation template with scenes, text, symbols, tweens | **PASS** | Real editable project |
| UI kit template with panels, cards, button script | **PASS** | Real editable project |
| Project validation and export config | **PASS** | CLI/GUI shared model |
| Autosave/recovery | **PASS** | Existing M7.3 implementation preserved |
| Dedicated example gallery (six requested examples) | **PARTIAL** | Four templates exist; no separate six-project gallery |
| Physics/collision helper library | **NOT AVAILABLE** | Example contains simple script logic only |
| Audio integration example | **NOT AVAILABLE** | No audio pipeline |

### Extensibility

| Area | Result | Evidence / limit |
| --- | --- | --- |
| Safe template extensibility | **PARTIAL** | Templates are compiled-in Rust, not a downloadable plugin API |
| Plugin permission model / signed extensions | **NOT AVAILABLE** | Not implemented |
| Theme and layout persistence | **PARTIAL** | Existing dark theme and panels; no full extension API |

## Release artifacts

The rc2 artifacts were preserved in `dist/archive/0.3.0-rc2/`. The M8 Windows
build was produced by `scripts/build-release.ps1 -Target windows`:

- `NØVA Flash Studio.exe` (portable payload)
- `NOVA-Flash-Studio-0.4.0-rc1-windows-x64.zip`
- `NOVA-Flash-Studio-0.4.0-rc1-windows-x64-Setup.exe`

The final byte sizes and SHA-256 values are recorded in
`dist/windows/SHA256SUMS.txt`, generated after this documentation is packaged.

Verification:

- `Setup.exe --version` reports `NØVA Flash Studio Setup 0.4.0-rc1`.
- `Setup.exe --smoke-test` reports 46 payload entries, one executable, a valid
  EULA and a payload hash matching the ZIP.
- Headless per-user install to a temporary directory succeeded; the installed
  executable reported `NØVA Flash Studio 0.4.0-rc1`, and the HKCU uninstall
  entry reported `DisplayVersion 0.4.0-rc1`.
- Headless quiet uninstall removed the install directory and the uninstall
  registry entry.
- `python scripts/verify-packaging.py` passed all structure checks.
- The binaries are unsigned. No code-signing claim is made.
- macOS and Linux packages were not built on native hosts.

`dist/windows/SHA256SUMS.txt` is authoritative for the generated artifacts.

## Test results

All commands were run on the Windows development host from the repository root.

| Command | Result |
| --- | --- |
| `cargo fmt --all` | **PASS** |
| `cargo check -p nova-app` | **PASS** |
| `cargo test --workspace` | **PASS** — 281 passed, 0 failed, 4 ignored live tests |
| `cargo test -p nova-ai` | **PASS** — 41 tests |
| `cargo test -p nova-app --bin nova-flash-studio` | **PASS** — 78 tests, 1 ignored |
| `cargo test -p nova-as` | **PASS** — 34 library, 30 compiler integration |
| `cargo test -p nova-build --test m8_features` | **PASS** — 4 tests |
| `cargo test -p nova-core` | **PASS** — 27 tests |
| `cargo test -p nova-formats` | **PASS** — 20 tests |
| `powershell scripts/build-release.ps1 -Target windows` | **PASS** — tests, release build, ZIP, Setup.exe, checksums |
| `Setup.exe --smoke-test` | **PASS** — 46 payload entries, hash match |
| Headless install/uninstall | **PASS** — version and registry verified, clean removal |
| `python scripts/verify-packaging.py` | **PASS** |

The ignored tests are the three live Ollama tests and one live AI Agent app
test. They were not rerun in M8. The default suite is offline and uses mocked
HTTP and in-memory credentials.

## Honest limitations

- M8 is a scoped implementation, not the full feature list from the milestone
  brief. Large areas remain unavailable: bitmap/audio authoring, nested symbol
  timelines, masks, shape tweens, line/brush tools, advanced AS3 control flow,
  a full Flash API, a plugin system and a complete example gallery.
- Text is static geometry. There is no editable text field, system font or font
  embedding.
- Multi-scene export is a concatenation convention, not Adobe scene metadata.
- AS3 collections/indexing are structurally checked but not executed.
- XFL fidelity is still partial and Adobe Animate compatibility is unverified.
- Adobe playback is unverified. Ruffle evidence is limited to the M7.3 fixture.
- The interactive GUI and local AI workflow were not re-exercised in M8 beyond
  the automated app tests.
- No performance benchmark for the new text/multi-scene paths was recorded.

## Recommended next steps

1. Execute AS3 array/object/index fixtures in Ruffle and capture AVM2 errors.
2. Add a real visual M8 fixture for text, folders, multi-scene export and
   playback; record frames and errors.
3. Implement nested symbol timelines and symbol editing mode on top of the
   existing stable symbol IDs.
4. Add `switch`, try/catch/finally, interfaces and `super.method()` with ABC
   tests and runtime fixtures.
5. Add bitmap/audio assets and an honest import/export pipeline, or continue to
   label them unavailable.
6. Improve XFL keyframe/tween reconstruction and test in Adobe Animate if a
   licensed authoring environment becomes available.
7. Add a plugin/extension permission model before accepting downloaded
   extensions.
8. Re-run live local Ollama and, with explicit consent, a cloud provider.
9. Add a documented example gallery only when each example is an editable,
   tested project.
