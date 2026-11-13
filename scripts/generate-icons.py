#!/usr/bin/env python3
"""Generate platform icon assets from the high-resolution NØVA master.

Requires Pillow. The SVG is the editable source; nova-logo-1024.png is the
rendered master. Re-render the master with Inkscape if the SVG changes:

    inkscape --export-type=png --export-filename=assets/branding/nova-logo-1024.png \
        --export-width=1024 --export-height=1024 assets/branding/nova-logo.svg

This script only performs deterministic resampling and container packaging, so
it can run on Windows, macOS and Linux.
"""

from __future__ import annotations

import struct
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
BRANDING = ROOT / "assets" / "branding"
ICONS = ROOT / "assets" / "icons"
LINUX_ICONS = ICONS / "hicolor"
MASTER = BRANDING / "nova-logo-1024.png"

PNG_SIZES = (16, 24, 32, 48, 64, 128, 256, 512, 1024)
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
# ICNS type, pixel size. These are the standard PNG-backed icon types.
ICNS_TYPES = (
    ("icp4", 16),
    ("icp5", 32),
    ("icp6", 64),
    ("ic07", 128),
    ("ic08", 256),
    ("ic09", 512),
    ("ic10", 1024),
    ("ic11", 32),
    ("ic12", 64),
    ("ic13", 256),
    ("ic14", 512),
)


def load_master() -> Image.Image:
    if not MASTER.exists():
        raise SystemExit(f"missing master: {MASTER}")
    image = Image.open(MASTER).convert("RGBA")
    if image.size != (1024, 1024):
        raise SystemExit(f"master must be 1024x1024, got {image.size}")
    return image


def resize(master: Image.Image, size: int) -> Image.Image:
    # LANCZOS keeps the neon edges crisp without ringing at tiny sizes.
    return master.resize((size, size), Image.Resampling.LANCZOS)


def write_pngs(master: Image.Image) -> None:
    BRANDING.mkdir(parents=True, exist_ok=True)
    for size in PNG_SIZES:
        resize(master, size).save(BRANDING / f"nova-logo-{size}.png", optimize=True)

    for size in PNG_SIZES:
        directory = LINUX_ICONS / f"{size}x{size}" / "apps"
        directory.mkdir(parents=True, exist_ok=True)
        resize(master, size).save(directory / "nova-flash-studio.png", optimize=True)


def write_ico(master: Image.Image) -> None:
    ICO = ICONS / "nova-flash-studio.ico"
    ICO.parent.mkdir(parents=True, exist_ok=True)
    # Pillow writes a multi-image ICO when `sizes` is supplied.
    master.save(ICO, format="ICO", sizes=[(s, s) for s in ICO_SIZES])
    # Keep a 256px PNG next to the ICO for Windows tooling and docs.
    resize(master, 256).save(ICONS / "nova-flash-studio-256.png", optimize=True)


def write_icns(master: Image.Image) -> None:
    target = ICONS / "nova-flash-studio.icns"
    chunks: list[bytes] = []
    for kind, size in ICNS_TYPES:
        import io

        buffer = io.BytesIO()
        resize(master, size).save(buffer, format="PNG", optimize=True)
        payload = buffer.getvalue()
        chunks.append(kind.encode("ascii") + struct.pack(">I", len(payload) + 8) + payload)

    body = b"".join(chunks)
    target.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)
    resize(master, 512).save(ICONS / "nova-flash-studio-512.png", optimize=True)


def main() -> None:
    master = load_master()
    write_pngs(master)
    write_ico(master)
    write_icns(master)
    print(f"Generated icons in {BRANDING} and {ICONS}")


if __name__ == "__main__":
    main()
