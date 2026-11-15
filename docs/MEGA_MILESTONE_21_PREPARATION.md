# MEGA MILESTONE 21 — Preparation & Architecture Plan

**Base:** `8bbe2b8` (M20 complete, `0.16.0-rc1`) · **Target:** `0.17.0-rc1`
· **Status:** audit complete, implementation starts after this document.

## 1. Ist-Zustand (verified)

- Workspace `cargo test --workspace`: **462 passed / 0 failed** (34 suites);
  `clippy -D warnings` and `fmt --check` clean; installer builds with 0
  warnings; all six edition artifacts + archive verified (M20 report).
- Three editions share one codebase; `packaging/editions.json` is the
  packaging source of truth; `EditionInfo.cs` is generated per edition.
- `nova_core::Edition` + `nova-app::edition` module registry gate
  edition-specific UI. Edu `classroom` module is active; Enterprise has
  exactly one reserved module `administration`.
- `nova-edu` (M20): workspace/school/profile/classroom/assignment/
  submission/feedback/progress/report/package/template — headless,
  offline, file-based, hardened ZIP packages. Pattern to mirror.
- `nova-build`: `build_swf`, `build_scene`, `bake_tilemap`, warning
  descriptions — the real SWF pipeline to reuse for build profiles.
- `nova-formats`: XFL/FLA/SWF import/export; `nova-ai`: provider config +
  safe project ops; `Project`/`Project::load`/`validate`/`to_json` are the
  `.nova` v4 contract — **unchanged by M21**.
- Config dirs: `%APPDATA%\NOVA Flash Studio{, Edu, Enterprise}`;
  recovery dirs `nova-flash-studio{,-edu,-enterprise}`.

## 2. Architecture decisions

- **New crate `nova-ent`** (mirrors `nova-edu`): headless domain layer,
  zero UI/network deps beyond `nova-core`/`serde`/`uuid`/`sha2`/`zip`.
- **Enterprise workspace** = directory with `org.json` marker
  (`nova-enterprise-workspace` schema). Projects are **registered by
  path** (`projects/<uuid>.json` records → absolute `.nova` path +
  sha256 at registration time). Project files are never moved, copied
  into, or rewritten by workspace operations — snapshots *copy* bytes
  into `versions/`, restores write back only via an explicit, backed-up
  action.
- **Roles are policy labels, not security.** `EnterpriseRole` +
  per-project overrides produce a *permission matrix* used by UI/API
  gating. Docs state clearly: real enforcement = OS filesystem
  permissions; no pseudo-auth is implemented.
- **Asset library** is a sidecar store (`assets/` index JSON + blob
  files with sha256). It does not replace `Project.assets`; library
  entries can be *linked into* a project via existing asset import.
- **Versioning** stores full `.nova` byte snapshots (schema-versioned
  metadata: author label, message, parent, sha256, diff summary
  computed by comparing `Project` JSON — scene/asset/layer/object
  counts and names). Restore = explicit copy-back after writing a
  safety snapshot of the current state first.
- **Reviews** reuse the anchor pattern from `nova-edu` (own types in
  `nova-ent`, same resolution semantics, orphan marking).
- **Build profiles** = named, versioned JSON configs referencing
  `nova-build`/`nova-formats` export kinds + optional SWF options;
  build history records output path + sha256 + warning counts.
- **AI/integrations**: config-only endpoints (Ollama-style local HTTP),
  disabled by default; secrets stored as *references* (env var names),
  never plaintext. No data leaves the machine without explicit action.

## 3. Module plan (nova-ent)

| Module | M21 | Content |
|---|---|---|
| `error`/`store`/`workspace` | .1 | EntError/EntResult, atomic JSON IO, ws layout, org.json |
| `org`/`profile`/`team` | .1/.2 | Organization, EntProfile, members, roles, per-project grants, permission matrix |
| `project_reg` | .1 | project registry (path + id + checksum), recent list |
| `assets` | .3 | library folders/categories/tags/versions, import/export, sha256 |
| `versions` | .4 | snapshots, history, restore, compare/diff summary |
| `review` | .5 | requests, statuses, comments+anchors, decisions, history |
| `admin`/`audit` | .6 | overview stats, audit log (append-only JSONL) |
| `builds` | .7 | profiles, history, validation, reports |
| `integrations` | .8 | local endpoint config, AI policy, secret references |
| `package` | .8/.10 | `.novaent` workspace export/import (hardened ZIP, same rules as nova-edu) |

## 4. Edition gating

- `edition.rs`: Enterprise gains active modules `administration`,
  `collaboration`(reserved→active only where implemented — keep single
  gate `administration` for `Mode::Enterprise`).
- `Mode::Enterprise` in `nova-app`, gated by
  `edition::has_active_module("administration")`; toolbar/menu entries
  hidden for User/Edu. Edu `Mode::Classroom` untouched.
- `ent_state.rs` + `enterprise_view.rs` mirror the edu pattern
  (workspace gate → tabs: Übersicht, Projekte, Team, Assets,
  Versionen, Reviews, Builds, Administration, Integrationen).

## 5. Security concept

- Path confinement via `safe_join` equivalent; package reader hardened
  exactly like `nova-edu::package` (limits, checksums pre-write,
  temp extraction, no execution).
- Snapshots/restores never overwrite without a prior safety snapshot;
  all mutations recorded in the audit log.
- No secrets in plaintext; integration endpoints default-disabled.
- Roles documented as workflow policy; OS ACLs remain the boundary.

## 6. Test strategy

- Unit tests per module (temp dirs, like nova-edu): CRUD, validation,
  permission matrix, asset integrity, snapshot/restore roundtrip,
  review state machine, build profile validation, package
  roundtrip/traversal/checksum rejection, audit log append.
- Edition gating tests: `has_active_module` per edition; Mode gating.
- Regression: full `cargo test --workspace` + packaging tests +
  archive tests; Edu/User paths untouched.

## 7. Submilestone plan

- **M21.0** this document.
- **M21.1** `nova-ent` foundation: error/store/workspace/org/profile/
  project registry.
- **M21.2** team + roles + permission matrix.
- **M21.3** asset library.
- **M21.4** versioning/snapshots/compare/restore.
- **M21.5** review workflows.
- **M21.6** admin center + audit log.
- **M21.7** build profiles + history (nova-build reuse).
- **M21.8** integrations config + `.novaent` packages.
- **M21.9** Enterprise UI (`Mode::Enterprise`, tabs, gating).
- **M21.10** version `0.17.0-rc1`, docs (`ENTERPRISE.md`, final
  report), packaging all editions, archive `dist/archive/0.17.0-rc1/`,
  full QA.
