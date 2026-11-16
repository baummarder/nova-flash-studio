# NØVA Flash Studio

**A native desktop studio for vector animation, character animation and
interactive 2D content — written in Rust.**

NØVA Flash Studio is an actively developed authoring environment inspired by
classic Flash-era desktop workflows: a stage, a multi-track timeline, vector
drawing tools, nested symbols, an ActionScript 3 subset and real SWF export.
It is a standalone application — **not** Adobe Animate and not an official
Flash replacement — built on its own codebase, project format and toolchain.

The project is MIT/Apache-2.0 licensed, works fully offline, requires no
account, and ships as three independently distributable editions built from
one shared codebase.

---

## Table of contents

- [What it is](#what-it-is)
- [Feature overview](#feature-overview)
- [Editions](#editions)
- [File formats](#file-formats)
- [AI-assisted development](#ai-assisted-development)
- [Installation](#installation)
- [Build from source](#build-from-source)
- [Repository architecture](#repository-architecture)
- [Development status](#development-status)
- [Contributing](#contributing)
- [Security and privacy](#security-and-privacy)
- [License](#license)
- [Documentation](#documentation)

---

## What it is

NØVA Flash Studio combines, in one native application:

- a **vector stage editor** with drawing tools, Bézier paths, gradients and
  Boolean operations;
- a **timeline** with scenes, layers, keyframes, held frames and motion
  tweens;
- a **skeletal animation system** — bone hierarchies, FK/IK, mesh skinning;
- a **2D game-development layer** — tilemaps, collision, physics, cameras,
  particles, scenes and an AS3 game kit;
- a **code editor** with a documented ActionScript 3 subset compiled to real
  AVM2 bytecode;
- an **SWF writer** producing genuine binary SWF files that run in
  Ruffle (verified) and are structurally valid for other players;
- an **AI-assisted development workflow** with strict permission, review and
  sandbox boundaries.

The UI is localized in five languages (`de-DE`, `en-GB`, `da-DK`, `fr-FR`,
`pl-PL`), uses a classic desktop creative-tool layout, and is written in Rust
with `egui`/`eframe` (wgpu). The current release line is `0.20.0-rc1` — a
release candidate under active development, not a stable 1.0.

---

## Feature overview

Status legend: **implemented** = shipped and tested · **partial** = works
with documented limits · **preview** = editor simulation only, not exported.

### Animation & vector authoring

| Feature | Status | Notes |
|---|---|---|
| Scenes, layers, layer folders, masks | implemented | Multi-scene projects; export is per-scene or opt-in concatenation |
| Keyframes, held frames, labels, playhead | implemented | Full timeline with ruler scrubbing and per-cell menus |
| Motion tweens with easing | implemented | Same easing model in editor, playback and exporter |
| Vector drawing (pen, pencil, shapes) | implemented | Interactive pen tool, node editing, path operations |
| Cubic Bézier segments | implemented | Smooth/independent handle modes |
| Gradients (linear/radial, multi-stop) | implemented | Exported as native SWF gradient fill styles |
| Boolean path ops (union/intersect/subtract/XOR) | implemented | Transform-aware, holes supported |
| Static text objects | implemented | Built-in 5×7 vector font, baked to shapes; no font embedding |
| Bitmap and audio assets | implemented | Embedded, editable metadata, timeline sound cues |

### Character & skeletal animation

| Feature | Status | Notes |
|---|---|---|
| Bone hierarchies, FK rotation clips | implemented | `.nova`-serializable, deterministic per-frame baking |
| Two-bone analytic IK + pole targets | implemented | FABRIK multi-bone chains, draggable joints |
| Mesh skinning / weight painting | implemented | LBS vertex deformation, normalized weights, paint tool |
| Nested MovieClips & symbol timelines | implemented | Per-symbol timelines, library, enter/exit editing |
| Morph shapes / animation constraints | not implemented | Documented as open roadmap items |

### Code & runtime

| Feature | Status | Notes |
|---|---|---|
| ActionScript 3 subset | implemented | Lexer/parser/type checker → real AVM2 ABC; documented subset, unsupported constructs are rejected with diagnostics |
| SWF export (FWS/CWS, v10/v13) | implemented | Shapes, sprites, placements, labels, `DoABC2`, `SymbolClass`, zlib compression |
| SWF runtime preview | partial | "Open in Flash Player": Adobe projector (default) or Ruffle (explicit opt-in); playback itself is not verified by NØVA |
| Ruffle verification | implemented | Exported demos and templates verified running in Ruffle 0.7.1 |
| Game kit (`game.NovaGame`) | implemented | Scene lifecycle, event bus, collision, camera, particles — exported into SWF via AS3 bridge |
| QuickJS tool console | implemented | Editor tools/extensions only — **not** an AS3 runtime |

### Game development

| Feature | Status | Notes |
|---|---|---|
| Tilemaps & tilesets | implemented | Tile painting editor, region ops, slicing; bake to bitmap on SWF export |
| Collision (rect/circle/polygon/tile) | implemented | Layer/mask filtering, SAT for polygons, enter/exit events |
| Physics, cameras, input | implemented | Preview simulation + exported AS3 runtime parity |
| Particle emitters | preview | Nine presets simulated in preview; **not** exported to SWF/XFL |
| Game templates | implemented | Blank, pong, breakout, platformer, topdown, racing, menu + tilemap demos |

### AI-assisted development

| Feature | Status | Notes |
|---|---|---|
| Local + cloud providers | implemented | Ollama (local/cloud), Groq, OpenAI-compatible; credentials in OS credential store |
| Typed tool calling | implemented | Provider-native tool calls where supported + validated text fallback; never arbitrary code execution |
| Project access grants | implemented | Per-project read-only/read-write file grants, sensitive-file denylist |
| Review & staging | implemented | All AI changes staged; per-file diffs; explicit accept per change; rollback |
| Isolated build/test sandbox | implemented | Per-task project copy, allowlist commands, staleness detection — **not** OS-level isolation |
| Task history & limits | implemented | Bounded iterations, tool-call and command limits, per-edition history |

See [docs/AI_AGENT_EDITOR.md](docs/AI_AGENT_EDITOR.md) for the full security
model.

### Authoring workflow

Undo/redo with transaction grouping, autosave with crash recovery,
syntax-highlighted code editor with diagnostics, draggable Stage/Code
split layouts (three persisted workspace layouts), five-language UI,
classic desktop theme, project templates, asset audit and compatibility
reports.

---

## Editions

Three products, **one codebase**. Editions differ in branding, window
title, configuration directory, installer identity and a module registry —
not in core capability.

| | NØVA Flash Studio | NØVA Flash Studio Edu | NØVA Flash Studio Enterprise |
|---|---|---|---|
| Audience | Hobbyists, animators, indie devs | Schools, teachers, students | Teams, studios, professionals |
| Setup EXE | `NOVA-User-Setup.exe` | `NOVA-Edu-Setup.exe` | `NOVA-Enterprise-Setup.exe` |
| Portable ZIP | `NOVA-User-Portable.zip` | `NOVA-Edu-Portable.zip` | `NOVA-Enterprise-Portable.zip` |
| Config dir | `NOVA Flash Studio` | `NOVA Flash Studio Edu` | `NOVA Flash Studio Enterprise` |
| Edition modules | — | Classroom mode (assignments, submissions, feedback, progress) | Administration workspace (workspaces, roles-as-policy, asset library, snapshots, review, audit log, `.novaent` packages) |

Important honesty notes:

- Edu roles and Enterprise team roles are **workflow labels, not
  authentication or security boundaries** — OS file permissions are the
  real boundary.
- `.nova` projects are **edition-neutral** (format v4) and open in every
  edition; edition-specific data lives in sidecars, never inside
  `Project`.
- Enterprise-only features never restrict User or Edu capabilities.

Details: [docs/EDITIONS.md](docs/EDITIONS.md),
[docs/CLASSROOM.md](docs/CLASSROOM.md), [docs/ENTERPRISE.md](docs/ENTERPRISE.md).

---

## File formats

| Format | Import | Export | Honest limits |
|---|---|---|---|
| `.nova` | Full native project (v4, JSON) | Full native project | NØVA's own format; not an Adobe exchange format |
| `.fla` / XFL | CS5+ ZIP/XFL subset; uncompressed XFL folders; binary OLE2 `.fla` detected and **rejected with guidance** | **Native `.fla` export refused by design**; experimental unverified XFL ZIP package | Complex geometry, nested timelines, text, bitmaps, sounds are partial or preserved-only; XFL ZIP not tested in Adobe Animate |
| `.swf` | Header, dimensions, FPS, frames, layers, shapes, sprites, placements, labels, ABC, metadata | Real binary SWF from `.nova`/imported sources | Reconstruction is lossy (sprites flattened, preserved ABC is bytecode not source); not byte-identical |

Every import/export produces a **compatibility report** naming unsupported
data — nothing is silently discarded. See
[docs/FORMAT_COMPATIBILITY.md](docs/FORMAT_COMPATIBILITY.md).

---

## AI-assisted development

The integrated **AI Agent Editor** (`Ctrl+Shift+A`) is a bounded
development agent, not a chat toy:

- **Providers:** Ollama (local and Ollama Cloud), Groq, OpenAI-compatible
  endpoints. API keys live in the OS credential store (Windows DPAPI),
  never in project files or logs. Cloud providers require explicit
  per-session consent — nothing is sent without it.
- **Permissions:** the agent sees only files covered by per-project
  M25 grants; a sensitive-path denylist always applies. Read-only tools
  cannot modify anything.
- **Typed tools:** the agent calls a versioned, validated tool registry
  (project inspection, staged file create/update/patch/delete/rename,
  `.nova` editor operations) — never arbitrary code execution.
- **Review before apply:** every change lands in an isolated
  `ReviewSession` with before/after diffs; destructive operations need
  explicit per-file acceptance; rollback is supported.
- **Sandbox:** builds and tests run in a per-task copy of the project
  with allowlisted commands and staleness detection. This is a
  project-copy sandbox — **not** OS-level isolation.
- **Limits:** bounded task iterations, tool-call counts and command
  timeouts; full history visible in the task detail.

Providers without native tool-calling use a validated structured-text
fallback — model output is never executed as code. See
[docs/AI_AGENT_EDITOR.md](docs/AI_AGENT_EDITOR.md) and
[docs/AI_PROVIDER_GUIDE.md](docs/AI_PROVIDER_GUIDE.md).

---

## Installation

Prebuilt Windows artifacts are produced by the packaging pipeline under
`dist/windows/<edition>/` (and assembled into `release/<version>/windows/`).
**No public download URLs exist yet** — artifacts are built locally or via
CI; there is no signed release published.

### Windows (current focus)

Each edition ships two artifacts:

- `NOVA-<Edition>-Portable.zip` — unpack and run; fully portable.
- `NOVA-<Edition>-Setup.exe` — self-contained installer wizard
  (.NET 8, no prerequisites). Per-user install by default (no admin);
  optional all-users install, custom directory, `.nova`/`.swf` file
  associations, scope-aware uninstaller, upgrade handling.

**Signing status:** artifacts are currently **unsigned**
(`SIGNING_NOT_CONFIGURED`) — Windows SmartScreen/Defender may warn;
this is expected and documented in
[docs/CODE_SIGNING.md](docs/CODE_SIGNING.md). The signing pipeline is
implemented and ready for a real certificate.

### Other platforms

macOS (`.app` bundle) and Linux (AppImage) packaging scripts exist under
`scripts/package-macos.sh` / `package-linux.sh` but were **not built or
tested on their native hosts** — treat them as prepared, unverified.

---

## Build from source

Requirements:

- **Rust 1.88+** (edition 2024; developed/tested on current stable)
- Windows: MSVC toolchain, .NET 8 SDK (installer only), Windows SDK
  (optional, for embedded version resources and code signing)
- A GPU-capable desktop session for the GUI

```powershell
git clone <repository-url>
cd Flash-Editor

# Run the app (loads the built-in demo project)
cargo run -p nova-app

# Full test suite
cargo test --workspace

# Lint / format gates used by CI
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Headless CLI companion
cargo run -p nova-cli -- --help
cargo run -p nova-cli -- demo samples/nova-demo.swf
cargo run -p nova-cli -- export samples/as3-example.nova samples/as3-example.swf
```

### Edition-specific builds

Packaged binaries bake their edition at compile time via `NOVA_EDITION`:

```powershell
$env:NOVA_EDITION = 'edu'        # or 'enterprise' / 'user'
cargo build --release -p nova-app
```

The full packaging scripts (`package-windows.ps1`,
`package-installer.ps1`, `package-editions.ps1`,
`build-release-package.ps1`, `verify-release.ps1`) automate tests,
branding checks, signing status, checksums and the release manifest —
see [docs/PACKAGING.md](docs/PACKAGING.md) and
[docs/RELEASE.md](docs/RELEASE.md).

---

## Repository architecture

```text
crates/
  nova-core/     project model (.nova v4), scenes/layers/timeline,
                 skeletal animation, game runtime, edition identity
  nova-formats/  .nova persistence, FLA/XFL and SWF import/export
  nova-swf/      spec-based SWF bit writer + independent tag reader
  nova-build/    project -> SWF baking, script linking (DoABC2/SymbolClass)
  nova-as/       AS3 lexer/parser/AST/type checker/AVM2 ABC emitter
  nova-script/   QuickJS host for editor tools (not AS3)
  nova-ai/       providers, credentials, access grants, review session,
                 sandbox, tasks, typed tool registry
  nova-edu/      Education edition: classroom, assignments, submissions
  nova-ent/      Enterprise edition: workspaces, roles-policy, library,
                 snapshots, reviews, audit, .novaent packages
  nova-app/      eframe desktop UI, branding, splash, i18n, AI workspace
  nova-cli/      headless companion (import/export/analyze/build/probe)
packaging/       editions.json (identity source of truth), Windows
                 installer project, macOS/Linux templates, signing config
scripts/         packaging, installer tests, signing, release assembly,
                 icon/legal generation
legal/           license texts, third-party notices, EULA (draft status)
docs/            architecture, compatibility, milestone reports,
                 AI/security/packaging documentation
samples/         demo projects and Ruffle verification captures
```

More detail: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

## Screenshots

> **TODO:** Application screenshots are not yet committed to the
> repository. Planned captures: main authoring layout (Stage + Code
> split), timeline with tweens, vector/gradient editing, skeletal
> weight painting, tilemap editor, AI Agent review diff, installer
> wizard (all five languages). Genuine captures only — no mockups.

The application icon and branding assets live in `assets/`:

<p align="left">
  <img src="assets/branding/nova-logo-256.png" alt="NØVA logo" width="96">
</p>

---

## Development status

Current line: **`0.20.0-rc1`** (release candidate). The project is under
active development through numbered mega-milestones; each milestone ships
a documented final report under `docs/MEGA_MILESTONE_*_FINAL_REPORT.md`.

Honest high-level state:

- Core authoring, formats, AS3 subset, game layer, editions, installer,
  i18n and the AI workflow are implemented and regression-tested.
- Windows is the only fully built/tested platform.
- Adobe Flash Player playback is **not verified**; Ruffle verification
  covers the documented fixtures.
- No public release, tag or download exists yet; signing infrastructure
  is ready but unsigned pending a certificate.
- EULA texts are translations of a draft pending legal review;
  publisher identity placeholders are unresolved.

See [docs/ROADMAP.md](docs/ROADMAP.md) and
[docs/FLASH_COMPARISON.md](docs/FLASH_COMPARISON.md) for scope and
priorities.

---

## Contributing

- **Bugs:** open an issue describing steps, expected/actual behavior and
  platform; include `nova probe`/compatibility-report output for format
  problems.
- **Features:** propose via issue first; check
  [docs/ROADMAP.md](docs/ROADMAP.md) and milestone reports for planned
  scope — several Flash features (morph shapes, motion guides, native
  FLA write, HTML5 export) are deliberately out of scope or partial.
- **Code:** Rust 2024, workspace crates, `cargo test --workspace` must
  stay green, `cargo fmt` + `cargo clippy -D warnings` are enforced.
  Match existing conventions; keep changes edition-neutral where the
  `.nova` format is concerned.
- **Docs:** update the relevant `docs/` page and, for milestones, add a
  final report. Keep claims honest — the project documents limitations
  explicitly rather than implying compatibility.
- **AI attribution:** if an AI agent contributes, it must be credited
  via `Co-authored-by:` trailers per [AGENTS.md](AGENTS.md). No pushes,
  releases or tags without maintainer approval.

---

## Security and privacy

- **Offline-first:** no mandatory account, cloud, telemetry, DRM or
  licensing check.
- **AI data flow:** project content leaves the machine only to a
  user-configured provider after explicit consent; local Ollama keeps
  everything on-device.
- **AI sandbox limits:** the agent sandbox is a project copy with
  allowlisted commands — it is **not** OS-level isolation; do not run
  untrusted projects through agent builds expecting containment.
- **File associations/registry:** the installer changes only its own
  ProgIds/keys and preserves user data on uninstall.
- **Vulnerability reports:** please report privately to the maintainers
  rather than opening a public issue for security-sensitive findings.

Details: [docs/SECURITY.md](docs/SECURITY.md),
[docs/AI_AGENT_EDITOR.md](docs/AI_AGENT_EDITOR.md).

---

## License

**MIT OR Apache-2.0** at your option (see `Cargo.toml` `license` field;
full texts in `legal/` and shipped in every package).

- `legal/THIRD-PARTY-NOTICES.md` + `LICENSES/` — dependency inventory
  with SPDX texts.
- `legal/LICENSE-EULA*` — end-user license texts in five languages,
  translated from a **draft** pending legal review; placeholders
  (`[RECHTEINHABER]`, addresses, contact) are intentionally unresolved —
  see [docs/EULA.md](docs/EULA.md) and [docs/LICENSING.md](docs/LICENSING.md).
- Flash and Adobe are trademarks of Adobe Inc.; NØVA is not affiliated
  with or endorsed by Adobe.

---

## Documentation

| Document | Content |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crate boundaries, data flow, formats, UI design |
| [docs/EDITIONS.md](docs/EDITIONS.md) | User/Edu/Enterprise comparison and configuration isolation |
| [docs/FORMAT_COMPATIBILITY.md](docs/FORMAT_COMPATIBILITY.md) | `.nova`/FLA/XFL/SWF subsets and honest limits |
| [docs/AS3_COMPILER.md](docs/AS3_COMPILER.md) | Supported AS3 subset and diagnostics |
| [docs/AI_AGENT_EDITOR.md](docs/AI_AGENT_EDITOR.md) | Agent workflow, tools, permissions, sandbox |
| [docs/AI_PROVIDER_GUIDE.md](docs/AI_PROVIDER_GUIDE.md) | Provider setup and credentials |
| [docs/PACKAGING.md](docs/PACKAGING.md) | Build/package/sign pipeline per platform |
| [docs/RELEASE.md](docs/RELEASE.md) | Release assembly and verification steps |
| [docs/CODE_SIGNING.md](docs/CODE_SIGNING.md) | Authenticode setup, statuses, certificate requirements |
| [docs/SECURITY.md](docs/SECURITY.md) | Security boundaries and honest limits |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestones and planned work |
| [docs/TESTING.md](docs/TESTING.md) | What is actually tested and how |
| [docs/FLASH_COMPARISON.md](docs/FLASH_COMPARISON.md) | Honest Animate comparison |
| `docs/MEGA_MILESTONE_*_FINAL_REPORT.md` | Per-milestone delivery reports (M9–M29) |
