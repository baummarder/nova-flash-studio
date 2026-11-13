#!/usr/bin/env bash
# Build a macOS .app bundle. Run this on macOS with Xcode command line tools.
#
# Usage:
#   scripts/package-macos.sh [--no-build]
#
# Optional environment:
#   MACOS_TARGET=aarch64-apple-darwin|x86_64-apple-darwin
#   CODESIGN_IDENTITY="Developer ID Application: ..."
#   NOTARY_PROFILE="notarytool-keychain-profile"
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

TARGET_ARG=()
BIN_DIR="$ROOT/target/release"
if [[ -n "${MACOS_TARGET:-}" ]]; then
    TARGET_ARG=(--target "$MACOS_TARGET")
    BIN_DIR="$ROOT/target/$MACOS_TARGET/release"
fi

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    cargo build --release -p nova-app "${TARGET_ARG[@]}"
fi

if [[ ! -f "$BIN_DIR/nova-flash-studio" ]]; then
    echo "Missing executable: $BIN_DIR/nova-flash-studio" >&2
    exit 1
fi

APP="NØVA Flash Studio.app"
OUT="$ROOT/dist/macos"
BUNDLE="$OUT/$APP"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"

install -m 755 "$BIN_DIR/nova-flash-studio" "$BUNDLE/Contents/MacOS/nova-flash-studio"
install -m 644 "$ROOT/assets/icons/nova-flash-studio.icns" "$BUNDLE/Contents/Resources/nova-flash-studio.icns"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$BUNDLE/Contents/Info.plist"
install -m 644 packaging/macos/PkgInfo "$BUNDLE/Contents/PkgInfo"

# Structural validation. plutil is available on macOS and catches malformed XML.
if command -v plutil >/dev/null 2>&1; then
    plutil -lint "$BUNDLE/Contents/Info.plist"
fi
test -x "$BUNDLE/Contents/MacOS/nova-flash-studio"
test -s "$BUNDLE/Contents/Resources/nova-flash-studio.icns"
grep -q 'studio.nova.flash' "$BUNDLE/Contents/Info.plist"

if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
    # Hardened runtime is required for notarization. Sign nested code first if it
    # is added later; the current bundle contains only the main executable.
    codesign --force --deep --options runtime --timestamp \
        --sign "$CODESIGN_IDENTITY" "$BUNDLE"
    codesign --verify --deep --strict --verbose=2 "$BUNDLE"
else
    echo "No CODESIGN_IDENTITY set; bundle is unsigned and not notarized." >&2
fi

ZIP="$OUT/NOVA-Flash-Studio-$VERSION-macos.zip"
rm -f "$ZIP"
if command -v ditto >/dev/null 2>&1; then
    (cd "$OUT" && ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP")
else
    (cd "$OUT" && zip -qry "$ZIP" "$APP")
fi

if [[ -n "${CODESIGN_IDENTITY:-}" && -n "${NOTARY_PROFILE:-}" ]]; then
    echo "Submitting $ZIP for notarization..."
    xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$BUNDLE"
    rm -f "$ZIP"
    (cd "$OUT" && ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP")
fi

echo "macOS bundle created: $BUNDLE"
echo "macOS archive created: $ZIP"
echo "Version: $VERSION"
if [[ -z "${CODESIGN_IDENTITY:-}" ]]; then
    echo "Reminder: codesign and notarytool are required before public distribution." >&2
fi
