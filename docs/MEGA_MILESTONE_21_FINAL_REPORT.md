# MEGA MILESTONE 21 — Final Report: Enterprise Edition

**Target:** `0.17.0-rc1` · **Base:** `8bbe2b8` (M20 complete) ·
**Status:** complete — release candidate prepared, **not pushed, not released**.

## 1. Scope delivered

| Sub-milestone | Content | Status |
|---|---|---|
| M21.0 | Audit + `docs/MEGA_MILESTONE_21_PREPARATION.md` | done |
| M21.1 | Enterprise workspace foundation (`nova-ent`) | done |
| M21.2 | Team & roles, permission matrix | done |
| M21.3 | Shared asset library | done |
| M21.4 | Project versioning / snapshots / restore | done |
| M21.5 | Review & approval workflows | done |
| M21.6 | Administration center + audit log | done |
| M21.7 | Build profiles, history, validation | done |
| M21.8 | Optional local integrations + `.novaent` packages | done |
| M21.9 | `Mode::Enterprise` UI + edition gating | done |
| M21.10 | QA, packaging, archive, docs | done |

## 2. Architecture

- **New crate `crates/nova-ent`** (workspace member): headless domain
  layer depending on `nova-core`, `nova-build`, `nova-formats`,
  `serde`, `uuid`, `zip`, `sha2`. Modules: `error`, `store`,
  `workspace`, `profile`, `team`, `assets`, `versions`, `review`,
  `admin`, `builds`, `integrations`, `package`.
- **`nova-app`**: `Mode::Enterprise` + `ent_state.rs` (`EntUi`) +
  `enterprise_view.rs` (9 tabs). Gated solely by
  `edition::has_active_module("administration")` — the module is active
  only in `ENTERPRISE_MODULES`. Toolbar, `SetMode` guard and
  central-panel dispatch all check the registry; User/Edu cannot
  reach the mode.
- **`.nova` stays v4**, edition-neutral. Enterprise data lives in
  workspace sidecars (`org.json`, `team.json`, `projects/`, `assets/`,
  `versions/`, `reviews/`, `builds/`, `audit/`, `integrations/`) and the
  Enterprise config dir (`ent-profile.json`) — never in `Project`.

## 3. Key features

- **Workspace**: `org.json` marker, layout, prefs; projects registered
  by absolute path + sha256 baseline — files never moved/copied by
  workspace operations.
- **Team/roles**: 7 roles (Owner…Viewer), org role + per-project
  overrides, `default_permissions`/`member_can` policy matrix.
  Explicitly documented: policy, **not** authentication; OS ACLs are
  the boundary. No pseudo-login implemented.
- **Asset library**: content-addressed blobs (`assets/blobs/<sha>.bin`),
  folders/categories/tags, versions, search, export, integrity verify;
  SVG/PNG/JPEG/GIF/audio/video/`.nova`/symbol kinds.
- **Versioning**: byte snapshots with parent chain, scene/asset/layer/
  object diff summaries, safety snapshot before every restore,
  sha256 verification, tamper rejection.
- **Reviews**: Draft→Pending→InReview→ChangesRequested/Approved/Rejected
  with mandatory reason on reject/changes, history events, comments
  with `ReviewAnchor` + orphan detection.
- **Admin**: `admin::overview` stats + append-only `audit.jsonl`,
  CSV export, `.novaent` workspace export/import.
- **Builds**: named profiles → `nova_build::build_swf` /
  `nova_formats::export_xfl`, `exports/` artifacts, JSONL history with
  sha256 + `verify_build_record`.
- **Integrations**: Ollama/OpenAI-compatible local endpoints,
  disabled by default, `api_key_env` stores only the env-var *name*,
  URL validation rejects embedded credentials; project-scoped policy.
- **`.novaent` packages**: hardened ZIP (same rules as nova-edu:
  manifest, per-file sha256 verified before write, traversal/
  symlink/encryption/size/ratio limits), whitelisted commit that never
  overwrites and never imports `org.json`.

## 4. Verification (executed)

```
cargo test --workspace                          → ~484 passed, 0 failed (all suites)
cargo clippy --workspace --all-targets -- -D warnings → clean
cargo fmt --all -- --check                      → clean
scripts/package-editions.ps1 -SkipTests         → six artifacts + checksums
scripts/test-edition-packaging.ps1              → all PASS
scripts/test-archive-releases.ps1               → all PASS
```

`nova-ent` unit tests (22) cover: workspace create/open/validate,
roles+matrix, member CRUD + per-project overrides, project
registration (+non-.nova rejection), asset import/search/verify/
tamper/delete, snapshot history/diff/restore/safety/tamper,
compare_current, review lifecycle + anchored comments + orphans,
build profiles + SWF build + history verification, integrations
defaults/validation, package roundtrip/import + traversal/checksum
rejection, audit log + overview.

Edition-gating tests assert: `administration` active only for
Enterprise, `classroom` only for Edu, neither leaks into User.

## 5. Artifacts (`0.17.0-rc1`, SHA-256 — **remediated rebuild**)

> **Remediation note (commit `140aa94`):** the first `0.17.0-rc1`
> artifact set contained UTF-8→CP1252 mojibake in Enterprise UI strings
> (found by `docs/MEGA_MILESTONE_21_ACCEPTANCE_AUDIT.md`, finding F1).
> All three editions were rebuilt after the encoding repair; the hashes
> below are the **current, verified** values. The earlier hashes are
> superseded and must not be used.

`dist/windows/<edition>/` + `dist/archive/0.17.0-rc1/<edition>/`
(user/, edu/, enterprise/ — verified identical by
`test-edition-packaging.ps1`).

| Artifact | SHA-256 |
|---|---|
| user `NOVA-User-Portable.zip` | `1C1A1A2D7CBA244B190A00627B74E5FB3C0BCC28555161FBC8920FB741004FBC` |
| user `NOVA-User-Setup.exe` | `01C7DA45A848FBD65477B4C74A687A5A9AF5F805E8DF0AB2E4DA827262DD6E67` |
| edu `NOVA-Edu-Portable.zip` | `9D21C858799E2CE44FC077FA5ED5FFB955AD837766123F9D0F5D177C92507A11` |
| edu `NOVA-Edu-Setup.exe` | `D462AB2F44E5F5D75E1688EA2AE2BBB07925D43A657E3990357F59CB7CC0493F` |
| enterprise `NOVA-Enterprise-Portable.zip` | `2D54C6037F9FBC626A6E97F5BB4BD57614ACEB508ED39A9B4D1404FCA1EB409A` |
| enterprise `NOVA-Enterprise-Setup.exe` | `F503A48655CEFBB69B1B5B76140744A1C856C9340283FE939FAB0DC92A06DF3E` |

App EXE hashes in each edition's `SHA256SUMS.txt`.

## 6. Commits (this milestone)

- `7230b75` M21.0 audit + preparation doc
- `855b937` M21.1–M21.8 `nova-ent` domain crate
- `9d94791` M21.9 Enterprise UI + edition gating
- `a3fda58` M21.10 version `0.17.0-rc1`, docs, packaging, this report
- `140aa94` M21 remediation: mojibake repair (F1), transactional
  `.novaent` import (F4), packaging archive order (F3), encoding
  regression tests, rebuilt+re-archived artifacts

Every commit carries the required Devin co-author trailer (verified via
`git log -1 --format=%B`).

## 7. Limitations (honest)

- Roles/permissions are **workflow policy, not authentication** —
  documented in UI, docs and code. No passwords, sessions or SSO.
- Review anchors can be *resolved* against a loaded project; creating
  anchors by clicking objects in the stage is not yet wired (comments
  currently use general/unanchored entries via the UI).
- `.novaent` packages are metadata backups: registered project paths
  remain machine-specific absolute paths; project files are not bundled.
- Integrations are config-only — actual LLM calls still go through the
  existing `nova-ai` provider layer; no new network code was added.
- Build profiles cover SWF/XFL targets only (the existing pipelines);
  no signing, notarization or CI hooks.
- Enterprise↔Edu data is not shared (separate crates/workspaces by
  design); `.nova` projects remain fully interchangeable.

## 8. Non-goals confirmed

- No push, no release/tag, no force-push, no history rewrite.
- `.nova` format unchanged (v4); User & Edu functionality intact;
  edition isolation preserved; fully offline; no telemetry/DRM/cloud.
