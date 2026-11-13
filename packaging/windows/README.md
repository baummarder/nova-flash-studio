# Windows packaging resources

The Windows icon and `VERSIONINFO` block are generated at build time by
`crates/nova-app/build.rs` from:

- `assets/icons/nova-flash-studio.ico`
- the workspace version in `Cargo.toml`

The build script locates the Windows SDK `rc.exe`, writes a temporary `.rc` file
under Cargo's `OUT_DIR`, compiles it to `.res`, and passes the result to the
MSVC linker. There is no hand-maintained binary resource in this folder.

`scripts/package-windows.ps1` performs the release build and refuses to publish
an executable unless `ProductName` and `FileVersion` are embedded. See
`docs/PACKAGING.md`.
