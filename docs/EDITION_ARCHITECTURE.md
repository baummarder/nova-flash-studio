# Edition Architecture (M19)

How the three NØVA editions share one codebase without forking.

## Layering

```
┌─────────────────────────────────────────────────────────┐
│ nova-app (eframe UI)                                    │
│   edition.rs  → EditionProfile: name, tagline, bundle,  │
│                 module registry (feature slots)         │
│   branding.rs → edition-aware app_name()/bundle_id()    │
│   main.rs     → viewport title/app_id from profile      │
│   flash.rs / autosave.rs / ai_editor.rs                 │
│               → edition-aware config & recovery paths   │
├─────────────────────────────────────────────────────────┤
│ nova-core                                                │
│   edition.rs → Edition { User, Edu, Enterprise }        │
│               → id/product_name/config_dir_name/        │
│                 recovery_dir_name, current(),           │
│                 config_dir() (platform paths +          │
│                 NOVA_FLASH_STUDIO_CONFIG_DIR override)  │
│   .nova project model — SHARED, no edition field         │
├─────────────────────────────────────────────────────────┤
│ nova-ai / nova-formats / nova-as / nova-build /          │
│ nova-runtime / nova-swf / nova-script                    │
│   Shared Core — identical for all editions.              │
│   nova-ai::default_config_dir delegates to               │
│   nova_core::Edition::config_dir → per-edition AI        │
│   provider settings for free.                            │
└─────────────────────────────────────────────────────────┘
```

## Edition resolution

`Edition::current()` precedence:

1. **Compile time** — `option_env!("NOVA_EDITION")`, set by
   `package-windows.ps1 -Edition <id>` during `cargo build --release`.
   Packaged binaries are permanently bound to their edition.
2. **Runtime** — `NOVA_EDITION` environment variable (dev/test override;
   ignored values fall through to User).
3. **Default** — `User`.

`cargo:rerun-if-env-changed=NOVA_EDITION` in `nova-app/build.rs` makes
switching editions rebuild the version resource correctly.

## What is edition-specific

| Concern | Mechanism |
|---|---|
| Product name / window title / splash / About | `edition::profile()` via `branding::*` |
| Viewport app id (coexistence) | `profile().bundle_id` — `studio.nova.flash.user|.edu|.enterprise` |
| exe version resource | `build.rs` reads `NOVA_EDITION` |
| Config dir (settings, AI providers, credentials, logs) | `Edition::config_dir()` |
| Untitled autosave recovery | `Edition::recovery_dir_name()` |
| Installer identity | generated `EditionInfo.cs` per publish |
| Feature slots | `EditionProfile.modules` (`reserved` = future work) |

## What is deliberately shared

- `.nova` format v4 — no edition field, no restrictions.
- All editor features, runtime, SWF/XFL/FLA import/export, AS3 compiler.
- AI operation set and provider infrastructure (settings are per-edition
  only because they live in the per-edition config dir).
- Templates, autosave format, undo history.

## Packaging flow

```
package-editions.ps1
 └─ per edition: package-windows.ps1 -Edition <id>
      ├─ cargo build --release with NOVA_EDITION=<id>
      ├─ stage dist/windows.staging-<id>/ → dist/windows/<id>/
      │    <Product>/<Product>.exe + docs + legal
      │    NOVA-<Slug>-Portable.zip, SHA256SUMS.txt, BUILD-INFO.json
      └─ archive-releases.ps1 (archives previous edition payload)
    package-installer.ps1 -Edition <id> -UseExistingPayload
      ├─ regenerate EditionInfo.cs (product/exe/uninstall key/ProgIds)
      ├─ dotnet publish → NOVA-<Slug>-Setup.exe
      └─ restore EditionInfo.cs to User defaults
```

Installer internals: payload ZIP is embedded as `payload.zip`; the
single top-level folder inside the ZIP is auto-detected as payload root,
so per-edition folder names work unchanged. Uninstall registry key,
ProgIds and shortcuts all derive from `EditionInfo` — editions can be
installed, launched and uninstalled independently.

## Isolation guarantees

- **Settings**: `%APPDATA%\NOVA Flash Studio{, Edu, Enterprise}` — User
  keeps the legacy directory so existing installs migrate seamlessly.
- **AI providers**: `provider settings` JSON lives inside the edition
  config dir → separate provider configs per edition.
- **Recovery**: `%TEMP%\nova-flash-studio-<edition>\`.
- **Installer**: distinct uninstall keys, ProgIds, install dirs, setup
  exe names → coexistence verified by distinct registry/file-association
  identities.
- **Projects**: identical format; cross-edition open/save tested.

## Non-goals (M19)

- No licensing/DRM/activation/seat management.
- No classroom management (M20) or team administration/collaboration
  (M21) — only reserved module slots and distinct start copy.
- No separate forks, repositories or duplicated feature code.
