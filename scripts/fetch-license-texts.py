#!/usr/bin/env python3
"""Download the standard SPDX license texts used by the release notices.

This helper is only used when refreshing the checked-in texts under LICENSES/.
It deliberately downloads the official SPDX text files without modification.
"""

from __future__ import annotations

import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "LICENSES"
BASE = "https://raw.githubusercontent.com/spdx/license-list-data/main/text/"

IDS = [
    "0BSD",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "GPL-2.0-only",
    "ISC",
    "LGPL-2.1-or-later",
    "MIT",
    "OFL-1.1",
    "Ubuntu-font-1.0",
    "Unicode-3.0",
    "Unlicense",
    "Zlib",
    "LLVM-exception",
]


def main() -> None:
    OUT.mkdir(exist_ok=True)
    for identifier in IDS:
        url = BASE + identifier + ".txt"
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read()
        (OUT / f"{identifier}.txt").write_bytes(data)
        print(f"{identifier}: {len(data)} bytes")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:  # pragma: no cover - network helper
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
