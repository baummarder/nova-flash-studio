#!/usr/bin/env python3
"""Structural checks for branding and cross-platform packaging templates.

These checks are intentionally platform-neutral: they verify icon containers,
Info.plist values, AppStream metadata and desktop-entry keys. They do not claim
that a macOS or Linux binary was built on the current machine.
"""

from __future__ import annotations

import plistlib
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent

ICNS_TYPES = {b"icp4", b"icp5", b"icp6", b"ic07", b"ic08", b"ic09", b"ic10", b"ic11", b"ic12", b"ic13", b"ic14"}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"FAIL: {message}")


def check_pngs() -> None:
    for size in (16, 24, 32, 48, 64, 128, 256, 512, 1024):
        path = ROOT / "assets" / "branding" / f"nova-logo-{size}.png"
        require(path.is_file(), f"missing {path.relative_to(ROOT)}")
        with Image.open(path) as image:
            require(image.size == (size, size), f"{path.name} has size {image.size}")
        hicolor = ROOT / "assets" / "icons" / "hicolor" / f"{size}x{size}" / "apps" / "nova-flash-studio.png"
        require(hicolor.is_file(), f"missing {hicolor.relative_to(ROOT)}")
    print("PNG sizes: ok")


def check_ico() -> None:
    path = ROOT / "assets" / "icons" / "nova-flash-studio.ico"
    require(path.is_file(), "missing ICO")
    with Image.open(path) as image:
        sizes = sorted(image.info.get("sizes", []))
    expected = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    require(sizes == expected, f"ICO sizes are {sizes}")
    print("Windows ICO: ok", sizes)


def check_icns() -> None:
    path = ROOT / "assets" / "icons" / "nova-flash-studio.icns"
    require(path.is_file(), "missing ICNS")
    data = path.read_bytes()
    require(len(data) > 16 and data[:4] == b"icns", "invalid ICNS magic")
    total = struct.unpack(">I", data[4:8])[0]
    require(total == len(data), "ICNS length mismatch")
    offset = 8
    found = set()
    while offset < len(data):
        require(offset + 8 <= len(data), "truncated ICNS chunk")
        kind = data[offset : offset + 4]
        length = struct.unpack(">I", data[offset + 4 : offset + 8])[0]
        require(length >= 8 and offset + length <= len(data), f"bad ICNS chunk {kind!r}")
        require(kind in ICNS_TYPES, f"unexpected ICNS chunk {kind!r}")
        found.add(kind)
        offset += length
    require({b"ic07", b"ic08", b"ic09", b"ic10"} <= found, "ICNS is missing standard sizes")
    print("macOS ICNS: ok", len(found), "chunks")


def check_info_plist() -> None:
    path = ROOT / "packaging" / "macos" / "Info.plist"
    with path.open("rb") as handle:
        plist = plistlib.load(handle)
    require(plist.get("CFBundleIdentifier") == "studio.nova.flash", "wrong bundle id")
    require(plist.get("CFBundleIconFile") == "nova-flash-studio.icns", "wrong icon file")
    require(plist.get("CFBundleExecutable") == "nova-flash-studio", "wrong executable")
    require(plist.get("CFBundleShortVersionString") == "@VERSION@", "missing version placeholder")
    print("macOS Info.plist: ok")


def check_linux_metadata() -> None:
    desktop = (ROOT / "packaging" / "linux" / "nova-flash-studio.desktop").read_text(encoding="utf-8")
    for key in ("Type=Application", "Name=NØVA Flash Studio", "Exec=nova-flash-studio", "Icon=nova-flash-studio"):
        require(key in desktop, f"desktop file missing {key}")
    require("MimeType=" not in desktop, "desktop file must not claim a silent SWF association")
    appdata = ET.parse(ROOT / "packaging" / "linux" / "nova-flash-studio.appdata.xml").getroot()
    require(appdata.tag == "component", "invalid AppStream root")
    require(appdata.findtext("id") == "studio.nova.flash", "wrong AppStream id")
    require(appdata.findtext("metadata_license") is not None, "missing metadata license")
    require(appdata.find("releases/release") is not None, "missing release entry")
    app_run = (ROOT / "packaging" / "linux" / "AppRun").read_text(encoding="utf-8")
    require("exec " in app_run and "nova-flash-studio" in app_run, "AppRun does not execute the app")
    print("Linux desktop/AppStream/AppRun: ok")


def check_scripts() -> None:
    for name in ("package-macos.sh", "package-linux.sh"):
        text = (ROOT / "scripts" / name).read_text(encoding="utf-8")
        require(text.startswith("#!/usr/bin/env bash"), f"{name} missing bash shebang")
        require("set -euo pipefail" in text, f"{name} missing strict mode")
    windows = (ROOT / "scripts" / "package-windows.ps1").read_text(encoding="utf-8")
    require("NØVA Flash Studio.exe" in windows, "Windows script does not produce the branded exe")
    require("ProductName" in windows, "Windows script does not verify version metadata")
    require("legal" in windows, "Windows script does not bundle legal files")
    print("Packaging scripts: ok")


def check_legal() -> None:
    legal = ROOT / "legal"
    for name in ("LICENSE-MIT.txt", "LICENSE-APACHE-2.0.txt",
                 "THIRD-PARTY-NOTICES.md", "EULA-DRAFT.md"):
        path = legal / name
        require(path.is_file() and path.stat().st_size > 0, f"missing legal/{name}")
    eula = (legal / "EULA-DRAFT.md").read_text(encoding="utf-8")
    require("DRAFT" in eula or "ENTWURF" in eula, "EULA lost its draft marker")
    require("MIT" in eula and "Apache" in eula, "EULA does not reference the dual license")
    require("Produktschlüssel" in eula or "Aktivierung" in eula,
            "EULA does not state key/activation-free use")
    licensing = (ROOT / "docs" / "LICENSING.md")
    require(licensing.is_file(), "missing docs/LICENSING.md")
    notices = (legal / "THIRD-PARTY-NOTICES.md").read_text(encoding="utf-8")
    require("cargo metadata" in notices, "third-party notices are not generated")
    require(notices.count("| `") > 50, "third-party notices look truncated")
    print("Legal files: ok")


def check_installer() -> None:
    base = ROOT / "packaging" / "windows" / "installer"
    csproj = (base / "NovaFlashStudio.Setup.csproj").read_text(encoding="utf-8")
    require("payload.zip" in csproj, "installer does not embed payload.zip")
    require("EULA-DRAFT.md" in csproj, "installer does not embed the EULA")
    require("net8.0-windows" in csproj and "UseWindowsForms" in csproj,
            "installer project targets the wrong framework")
    engine = (base / "InstallEngine.cs").read_text(encoding="utf-8")
    require("LocalApplicationData" in engine, "installer lacks per-user default dir")
    require("Uninstall" in engine and "NOVAFlashStudio" in engine,
            "installer lacks uninstall registry handling")
    require("PreviousDefault" in engine, "installer does not restore prior file associations")
    program = (base / "Program.cs").read_text(encoding="utf-8")
    for flag in ("--install", "--uninstall", "--smoke-test"):
        require(flag in program, f"installer CLI missing {flag}")
    wizard = (base / "WizardForm.cs").read_text(encoding="utf-8")
    require("RelaunchElevated" in wizard, "wizard does not request UAC elevation")
    require("InstallScope.AllUsers" in wizard, "wizard lacks all-users scope")
    engine_text = engine  # already loaded above
    require('Verb = "runas"' in engine_text, "installer lacks UAC runas elevation")
    require("LocalApplicationData" in engine_text and "ProgramFiles" in engine_text,
            "installer lacks per-user/all-users default dirs")
    script = (ROOT / "scripts" / "package-installer.ps1").read_text(encoding="utf-8")
    require("dotnet publish" in script, "installer script does not publish with dotnet")
    require("--smoke-test" in script, "installer script does not smoke-test the setup exe")
    release = (ROOT / "scripts" / "build-release.ps1").read_text(encoding="utf-8")
    require("package-installer.ps1" in release, "release dispatcher skips the installer")
    workflow = (ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    require("package-installer.ps1" in workflow, "release workflow skips the installer")
    print("Windows installer sources: ok")


def main() -> None:
    check_pngs()
    check_ico()
    check_icns()
    check_info_plist()
    check_linux_metadata()
    check_scripts()
    check_legal()
    check_installer()
    print("All packaging structure checks passed.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, ET.ParseError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
