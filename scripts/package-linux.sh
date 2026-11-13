#!/usr/bin/env bash
# Build a Linux x86_64 AppImage. Run this on Linux (or in a Linux CI job).
#
# Usage:
#   scripts/package-linux.sh [--no-build]
#
# Required at packaging time:
#   appimagetool in PATH, or APPIMAGETOOL=/path/to/appimagetool
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --no-build) SKIP_BUILD=1 ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done

VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
if [[ -z "$VERSION" ]]; then
    echo "Could not read workspace version from Cargo.toml" >&2
    exit 1
fi

ARCH="${ARCH:-x86_64}"
BIN="$ROOT/target/release/nova-flash-studio"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release -p nova-app
fi
if [[ ! -f "$BIN" ]]; then
    echo "Missing executable: $BIN" >&2
    exit 1
fi

APPDIR="$ROOT/dist/linux/AppDir"
rm -rf "$APPDIR"
mkdir -p \
    "$APPDIR/usr/bin" \
    "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/metainfo" \
    "$APPDIR/usr/share/icons/hicolor"

install -m 755 "$BIN" "$APPDIR/usr/bin/nova-flash-studio"
install -m 755 packaging/linux/AppRun "$APPDIR/AppRun"
install -m 644 packaging/linux/nova-flash-studio.desktop \
    "$APPDIR/usr/share/applications/nova-flash-studio.desktop"
install -m 644 packaging/linux/nova-flash-studio.desktop \
    "$APPDIR/nova-flash-studio.desktop"

for size in 16 24 32 48 64 128 256 512 1024; do
    src="$ROOT/assets/icons/hicolor/${size}x${size}/apps/nova-flash-studio.png"
    dest="$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dest"
    install -m 644 "$src" "$dest/nova-flash-studio.png"
done
install -m 644 "$ROOT/assets/icons/hicolor/256x256/apps/nova-flash-studio.png" \
    "$APPDIR/nova-flash-studio.png"

sed -e "s/@VERSION@/$VERSION/g" \
    -e "s/@DATE@/$(date +%F)/g" \
    packaging/linux/nova-flash-studio.appdata.xml \
    > "$APPDIR/usr/share/metainfo/studio.nova.flash.appdata.xml"

# Structural checks before invoking appimagetool.
test -x "$APPDIR/AppRun"
test -x "$APPDIR/usr/bin/nova-flash-studio"
test -s "$APPDIR/nova-flash-studio.png"
test -s "$APPDIR/nova-flash-studio.desktop"
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$APPDIR/nova-flash-studio.desktop"
fi
if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli validate "$APPDIR/usr/share/metainfo/studio.nova.flash.appdata.xml" || true
fi

if command -v ldd >/dev/null 2>&1; then
    if ldd "$APPDIR/usr/bin/nova-flash-studio" | grep -q 'not found'; then
        echo "Warning: unresolved shared libraries; install the runtime dependencies documented in docs/PACKAGING.md." >&2
        ldd "$APPDIR/usr/bin/nova-flash-studio" | grep 'not found' >&2 || true
    fi
fi

TOOL="${APPIMAGETOOL:-}"
if [[ -z "$TOOL" ]]; then
    TOOL="$(command -v appimagetool || true)"
fi
if [[ -z "$TOOL" ]]; then
    echo "appimagetool not found. AppDir is staged at: $APPDIR" >&2
    echo "Install appimagetool or set APPIMAGETOOL=/path/to/appimagetool." >&2
    exit 3
fi

OUT="$ROOT/dist/linux/NØVA-Flash-Studio.AppImage"
rm -f "$OUT"
# Many CI images lack FUSE; extract-and-run makes the tool usable there.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" "$TOOL" "$APPDIR" "$OUT"
chmod +x "$OUT"

echo "Linux AppImage created: $OUT"
echo "Version: $VERSION · Arch: $ARCH"
