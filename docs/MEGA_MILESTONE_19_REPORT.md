# MEGA MILESTONE 19 — The Three Editions

**Version:** `0.15.0-rc1` · **Base:** `0.14.0-rc1` (M18, commit `d99960f`)

M19 wandelt NØVA von einer Anwendung in **drei eigenständig distributierbare
Produkte** um — gebaut aus einer gemeinsamen Rust-Codebasis, ohne Forks
und ohne Code-Duplikation. guiding principle: gemeinsame Technical
Foundation, edition-spezifische Hülle.

## 1. Architektur

Siehe `docs/EDITION_ARCHITECTURE.md` für das Schichtenmodell.
Kurzfassung:

- **`nova-core::edition`** (neu): `Edition { User, Edu, Enterprise }`,
  Auflösung via `NOVA_EDITION` (Compile-time `option_env!` → Runtime-Env
  → User-Default), `product_name()`, `config_dir_name()`,
  `recovery_dir_name()`, `config_dir()` (plattformübergreifend,
  `NOVA_FLASH_STUDIO_CONFIG_DIR`-Override).
- **`nova-app::edition`** (neu): `EditionProfile` — App-Name, Tagline,
  Audience-Zeile, Bundle-ID und **Module-Registry** (`EditionModule`
  mit `reserved`-Flag für dokumentierte Future-Slots).
- **`nova-app::branding`**: `app_name()`, `bundle_id()`, `tagline()`,
  `audience()` — edition-aware; Konstanten `APP_NAME`/`BUNDLE_ID`
  bleiben als Basis-Defaults.
- **`nova-app::build.rs`**: Version-Resource (ProductName,
  FileDescription, InternalName, OriginalFilename) wird aus
  `NOVA_EDITION` generiert.
- **Integration**: `main.rs` (Viewport-Titel, App-ID, `--version`),
  `splash.rs` (Edition-Name + Tagline + Audience), About-Dialog
  (Edition-Name + Audience), Neuanlage-Dialog (Edition-Hinweis auf
  reservierte Module), `flash.rs` + `autosave.rs` (Edition-Pfade),
  `nova-ai::provider::default_config_dir` (delegiert an
  `nova_core::Edition::config_dir` → AI-Provider-Settings sind
  automatisch pro Edition getrennt), `nova-cli` profitiert ebenfalls.

## 2. Die drei Editionen

| | User | Edu | Enterprise |
|---|---|---|---|
| Produktname | NØVA Flash Studio | NØVA Flash Studio Edu | NØVA Flash Studio Enterprise |
| Tagline | VECTOR · TIMELINE · SWF | LERNEN · ERSTELLEN · VERSTEHEN | PRODUKTION · KOLLABORATION · KONTROLLE |
| Config-Dir | `NOVA Flash Studio` (Legacy, unverändert) | `NOVA Flash Studio Edu` | `NOVA Flash Studio Enterprise` |
| Bundle-ID | `studio.nova.flash.user` | `studio.nova.flash.edu` | `studio.nova.flash.enterprise` |
| Setup | `NOVA-User-Setup.exe` | `NOVA-Edu-Setup.exe` | `NOVA-Enterprise-Setup.exe` |
| Portable | `NOVA-User-Portable.zip` | `NOVA-Edu-Portable.zip` | `NOVA-Enterprise-Portable.zip` |
| Uninstall-Key | `NOVAFlashStudio` | `NOVAFlashStudioEdu` | `NOVAFlashStudioEnterprise` |
| ProgIds | `NOVAFlashStudio.*` | `NOVAFlashStudioEdu.*` | `NOVAFlashStudioEnterprise.*` |
| Reservierte Module | — | Klassenzimmer-Werkzeuge (M20) | Administration & Zusammenarbeit (M21) |

Alle Editionen enthalten den **vollständigen gemeinsamen Funktionsumfang**
(M12–M18 unverändert). Reservierte Module sind im Registry als `reserved`
markiert und werden als „geplant" gekennzeichnet — nicht als implementiert
ausgegeben.

## 3. Konfiguration & Daten

- `%APPDATA%\NOVA Flash Studio{, Edu, Enterprise}` unter Windows;
  macOS `~/Library/Application Support/` analog; Linux
  `$XDG_CONFIG_HOME/nova-flash-studio-<edition>`.
- User behält das Legacy-Verzeichnis → bestehende Installationen
  verlieren keine Settings/Credentials.
- Autosave-Recovery: `%TEMP%\nova-flash-studio-<edition>`.
- Credentials: DPAPI via `nova-ai` (kein Klartext).
- `NOVA_FLASH_STUDIO_CONFIG_DIR` überschreibt alles (Portable/Tests).
- Kein Lizenzsystem: keine Keys, Activation, Online-Auth, Seat-Limits,
  DRM oder Abo-Prüfungen. Alles offline-fähig.

## 4. Projekt-Kompatibilität

`.nova` enthält **kein** Editionsfeld; Serialisierung byte-identisch;
Cross-Edition-Tests in `crates/nova-core/tests/editions.rs` beweisen:
kein Editions-Marker im JSON, Round-Trip identisch, eindeutige
Config-Dir-Namen, User-Legacy-Pfad. Unbekannte optionale Metadaten
bleiben safe (serde-Defaults, bestehendes Verhalten).

## 5. Packaging

- `scripts/package-windows.ps1 -Edition <id>`: setzt `NOVA_EDITION`,
  baut Release, staged nach `dist/windows/<id>/`, prüft
  ProductName/FileVersion/`--version`/`--startup-check`, schreibt
  ZIP/SHA256SUMS/BUILD-INFO (mit `edition`-Feld), archiviert vorherige
  Artefakte dieser Edition.
- `scripts/package-installer.ps1 -Edition <id>`: regeneriert
  `EditionInfo.cs` (ProductName, AppExeName, UninstallFileName,
  UninstallKey, ProgIdNova/Swf, SetupExeName), `dotnet publish` mit
  `NovaProductName`/`NovaSetupAssemblyName`, Smoke-Tests
  (`--version`, `--smoke-test`), SHA256-Sums; stellt `EditionInfo.cs`
  auf User-Defaults zurück (sauberer Git-Tree).
- `scripts/archive-releases.ps1`: archiviert jetzt auch
  `dist/windows/<edition>/` → `dist/archive/<version>/<edition>/`
  (Version aus BUILD-INFO.json, Prüfsummen-verifiziert, kein Löschen).
- `scripts/package-editions.ps1`: Orchestrierung aller drei Editionen
  (6 Artefakte).
- Installer-C# (`InstallEngine`, `WizardForm`, `Program`, `Helpers`):
  alle sichtbaren Produktnamen/Exe-Namen/Registry-Keys/ProgIds über
  `EditionInfo` parametrisiert; AssemblyName/Produkt-Metadaten via
  MSBuild-Properties.

## 6. Tests

- `nova-core::edition` Unit-Tests: eindeutige Identität, Parse,
  Legacy-User-Pfad, Config-Override.
- `nova-core/tests/editions.rs`: kein Editionsfeld im Projekt,
  byte-identischer Round-Trip, isolierte Config-Dirs.
- `nova-app::edition` Tests: Profile-Vollständigkeit, Core-Module in
  allen Editionen, genau ein reserviertes Modul bei Edu/Enterprise.
- `cargo test --workspace`: alle Suites grün (inkl. M12–M18-Regressionen).
- Installer: `dotnet build` 0 Fehler/0 Warnungen; Smoke-Tests pro
  Edition (`--version` = „<ProductName> Setup <version>",
  `--smoke-test` = Payload-Hash OK).

## 7. Bekannte Einschränkungen

- Edu/Enterprise-„Willkommens-Erlebnis" = Splash-Tagline/Audience +
  Hinweis im Neuanlage-Dialog; dedizierte Start-Views kommen mit den
  Modulen (M20/M21).
- Installer-Icons sind_shared (gleiches NOVA-Icon); eigene
  Editions-Logos sind Branding-Erweiterung, nicht Pflicht.
- macOS/Linux-Packaging bleibt uneditioniert (zukünftige Arbeit);
  Edition-Logik ist plattformneutral.
- `.github/workflows/release.yml` unverändert (bau User-Edition mit
  Legacy-Kompatibilität via Defaults).
