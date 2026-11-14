# M19 Test Report — The Three Editions

**Version:** `0.15.0-rc1`

## Automatisierte Tests

`cargo test --workspace` — **alle Suites grün, 0 Fehler** (32 Test-Binaries,
inkl. aller M12–M18-Regressionen: nova-ai m16/m17/m18, nova-as compiler,
nova-formats SWF/XFL/FLA, nova-build, nova-runtime, nova-app).

## M19-spezifische Tests

### `crates/nova-core/src/edition.rs` (Unit)

- `all_editions_have_unique_identity` — id/product/config/recovery-Namen
  paarweise eindeutig über alle 3 Editionen.
- `parse_accepts_known_ids_only` — `user|edu|enterprise` (case-insensitive),
  alles andere → None.
- `user_keeps_legacy_config_dir` — User = `NOVA Flash Studio`, Edu und
  Enterprise weichen ab.
- `config_dir_honours_override` — `NOVA_FLASH_STUDIO_CONFIG_DIR` gewinnt.

### `crates/nova-core/tests/editions.rs` (Integration)

- `project_format_has_no_edition_field` — serialisiertes Projekt enthält
  kein `"edition"`-Feld.
- `project_roundtrips_identically_for_all_editions` — Pretty-JSON
  Round-Trip byte-identisch.
- `edition_identity_is_complete_and_unique` — alle 3 Editionen:
  eindeutige ids/Namen/Dirs, Parse-Roundtrip.
- `edition_dirs_are_isolated` — 3 unterschiedliche Config-Dir-Namen,
  User = Legacy-Pfad.

### `crates/nova-app/src/edition.rs` (Unit)

- `every_edition_profile_is_complete` — jeder Profile hat App-Name mit
  „NØVA", Tagline, Audience, Bundle-ID, ≥9 Core-Module.
- `reserved_modules_are_marked_not_implemented` — Edu/Enterprise je
  genau 1 reserviertes Modul; Core hat keins.

## Packaging-/Installer-Verifikation

- `dotnet build NovaFlashStudio.Setup.csproj`: **0 Fehler, 0 Warnungen**.
- `EditionInfo.cs` wird pro Edition regeneriert und danach auf
  User-Defaults restauriert (Git-Tree bleibt sauber).
- Smoke-Tests pro Installer: `--version` → `„<ProductName> Setup <version>"`,
  `--smoke-test` → Payload-SHA256 + Inhalt OK.
- `package-windows.ps1` prüft pro Edition: eingebetteter ProductName ==
  erwartet, FileVersion nicht leer, `--version`-Output ==
  `„<ProductName> <version>"`, `--startup-check` enthält window-icon.
- `archive-releases.ps1` archiviert Editions-Payloads nach
  `dist/archive/<version>/<edition>/` mit Prüfsummen-Verifikation.

## Getestete Edition-Matrix

| Prüfung | Ergebnis |
|---|---|
| 3 Editionen kompilieren aus demselben Workspace | ✅ (`NOVA_EDITION` env) |
| Korrekte Produktnamen in exe-Version-Resource | ✅ (build.rs, Script-Assert) |
| Korrekte `--version`-Ausgabe pro Edition | ✅ (Smoke-Test im Script) |
| Separate Config-Verzeichnisse | ✅ (Unit + Integration) |
| Keine Cross-Edition-Config-Leaks | ✅ (eigene Dir-Namen + Override-Test) |
| `.nova` Cross-Edition-Kompatibilität | ✅ (byte-identischer Round-Trip) |
| Installer pro Edition (eigenes Setup-EXE) | ✅ (dotnet publish + Smoke) |
| Koexistenz (unterschiedliche Keys/ProgIds/Dirs) | ✅ (EditionInfo-Tabelle) |
| AI-Provider-Settings pro Edition | ✅ (gleicher Config-Dir-Mechanismus) |
| M12–M18 Regressionen | ✅ (voller Workspace-Lauf) |

## Nicht ausgeführt / ehrlich vermerkt

- **Keine** echte Installation der drei Setup-EXEs auf einem
  Windows-System getestet (nur `--version`/`--smoke-test` der Installer
  und Payload-Struktur). Koexistenz folgt aus distinkten
  Registry-Keys/ProgIds/Installationsordnern — strukturell belegt,
  nicht installationsgetestet.
- Ruffle-Läufe unverändert (keine Runtime-Änderung in M19).
- Portabler Modus: Config-Isolation via `NOVA_FLASH_STUDIO_CONFIG_DIR`
  dokumentiert; dedizierter Portable-Config-Ordner im ZIP ist nicht
  implementiert.
