# Branding

NØVA Flash Studio uses one visual identity across the executable, window,
splash screen, installers and package metadata.

## Identity

| Item | Value |
| --- | --- |
| Product name | **NØVA Flash Studio** |
| Slug / executable | `nova-flash-studio` |
| Bundle identifier | `studio.nova.flash` |
| Company | NØVA Flash Studio contributors |
| License | MIT OR Apache-2.0 |

The logo is a bold **N** monogram crossed by a thin cyan-to-violet orbital ring.
It is designed to keep a recognizable silhouette at 16 px while retaining a
premium, technical look at 1024 px. The rounded-square field uses a dark navy to
deep-violet gradient with restrained cyan and violet glows. There are no
platform-specific variants: every platform receives the same master artwork.

### Palette

| Role | Hex |
| --- | --- |
| Field (top-left) | `#071426` |
| Field (middle) | `#0A0D20` |
| Field (bottom-right) | `#10071F` |
| Cyan highlight | `#6EF7FF` |
| Cyan core | `#35C9FF` |
| Periwinkle | `#7C8CFF` |
| Violet | `#B06CFF` |
| Accent node | `#A78BFA` |
| Splash background | `#04070F` |
| Splash primary text | `#EEF7FF` |

## Source and generated assets

| Path | Purpose |
| --- | --- |
| `assets/branding/nova-logo.svg` | Editable vector master (1024 × 1024) |
| `assets/branding/nova-logo-1024.png` | Rendered high-resolution original |
| `assets/branding/nova-logo-{16,24,32,48,64,128,256,512}.png` | PNG sizes for documentation and packaging |
| `assets/icons/nova-flash-studio.ico` | Windows icon with 16, 24, 32, 48, 64, 128 and 256 px entries |
| `assets/icons/nova-flash-studio.icns` | macOS icon with standard PNG-backed ICNS entries |
| `assets/icons/hicolor/<size>x<size>/apps/nova-flash-studio.png` | Linux hicolor theme icons, 16–1024 px |
| `assets/icons/nova-flash-studio-256.png` | Embedded window icon |
| `assets/icons/nova-flash-studio-512.png` | Copy of the ICNS master for packaging tools |

Generated assets are checked in, so a normal Rust build does not need Inkscape,
Python or Pillow. Regenerating them is a maintainer task.

### Regenerating icons

Requirements:

- Inkscape (only to re-render the SVG master)
- Python 3 with Pillow (resampling and ICO/ICNS containers)

```powershell
# 1. Render the vector master (Windows path shown; use the normal command on macOS/Linux)
& 'C:\Program Files\Inkscape\bin\inkscape.exe' `
  --export-type=png `
  --export-filename='assets/branding/nova-logo-1024.png' `
  --export-width=1024 --export-height=1024 `
  'assets/branding/nova-logo.svg'

# 2. Generate all PNG sizes, the ICO and the ICNS
python scripts/generate-icons.py

# 3. Validate the containers and packaging templates
python scripts/verify-packaging.py
```

The generator uses LANCZOS resampling and writes a multi-image ICO via Pillow.
The ICNS is assembled deterministically from PNG-backed standard chunks (`icp4`
through `ic14`). The verification script checks dimensions and container
structure.

## Application embedding

- **Windows:** `crates/nova-app/build.rs` finds the Windows SDK `rc.exe` and
  compiles a generated resource script containing the ICO and `VERSIONINFO`.
  The resource is linked into the release executable. The build fails softly
  with a warning if `rc.exe` is unavailable; the packaging script refuses to
  publish an executable whose `ProductName` is missing.
- **macOS:** `scripts/package-macos.sh` copies `nova-flash-studio.icns` into
  `NØVA Flash Studio.app/Contents/Resources/` and references it from
  `CFBundleIconFile` in `Info.plist`.
- **Linux:** the AppImage recipe installs the hicolor PNG set and uses
  `Icon=nova-flash-studio` in the desktop entry. The 256 px image is also placed
  at the AppDir root for AppImage tooling.
- **Runtime:** `crates/nova-app/src/branding.rs` embeds the 512 px splash logo
  and the 256 px window icon with `include_bytes!`. The same files are used by
  the native window and the splash overlay.

## Splash screen

The splash is an in-application overlay, not a separate process or window. It is
shown after core initialization has completed and then fades into the main UI.

| Parameter | Value |
| --- | --- |
| Minimum visible time | 650 ms |
| Total lifetime | 930 ms |
| Fade-out | 280 ms |
| Skip allowed after | 350 ms |
| Skip input | Click, `Space`, `Escape` |
| Background | Dark animated panel with slow cyan/violet glows and diagonal light lines |
| Content | 512 px logo, product name, version from `CARGO_PKG_VERSION`, animated bar, status text |
| Copyright line | Company and license |

The splash never starts a background job or adds a fixed sleep to hide work. If
logo decoding fails, the screen falls back to a drawn monogram and continues. If
core initialization fails, `StartupFailure` shows a readable error with a copy
button and a clean exit action instead of closing silently.

Initialization warnings (for example a malformed player settings file) are
logged and shown in a subdued warning line on the splash. The app still starts
with safe defaults.

## Accessibility and small sizes

- The monogram is thick enough to read at 16 px; the orbit and grid dissolve
  naturally at small sizes.
- The splash can be dismissed early with the keyboard or pointer.
- Text uses the system/egui font stack; no custom font file is required.
- The window icon uses a 256 px source and is scaled by the window manager.

## QA checklist

- [x] ICO opens and reports sizes 16, 24, 32, 48, 64, 128 and 256.
- [x] ICNS has a valid header and standard `ic07`–`ic10` chunks.
- [x] All PNG sizes have the expected square dimensions.
- [x] The same 1024 px master feeds every platform asset.
- [x] The window icon and splash texture decode from embedded bytes.
- [x] Splash lifetime is bounded below 1.2 s and has a fade-out.
- [x] Windows packaging verifies `ProductName`, `FileVersion` and icon extraction.
- [x] Windows splash and main window visually inspected at 1456 × 939; the
      window icon matches the 32 px branding asset.
- [ ] Visual inspection on a physical Retina/HiDPI macOS display (requires macOS).
- [ ] Linux desktop icon cache rendering in GNOME/KDE (requires Linux).

The unchecked items are platform-specific visual checks, not missing code.
