#!/usr/bin/env python3
"""Compile an .as file into a minimal .nova project, export SWF, run Ruffle.

Usage: python tools/as3-ruffle-test.py <file.as> [--keep]
Prints Ruffle ERROR lines; exit 0 always (inspection tool).
"""
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUFFLE = Path(r"C:\Program Files\Ruffle\bin\ruffle.exe")


def main() -> int:
    src = Path(sys.argv[1]).resolve()
    source = src.read_text(encoding="utf-8")

    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        # Clone the blank-game project (has game kit + document-class link).
        project = json.loads((ROOT / "samples/m10-blank-game.nova").read_text(encoding="utf-8"))
        for script in project["project"]["scripts"]:
            if script["path"] == "scripts/Main.as":
                script["source"] = source
        nova = tmp / "test.nova"
        nova.write_text(json.dumps(project), encoding="utf-8")

        swf = tmp / "test.swf"
        r = subprocess.run(
            ["cargo", "run", "-q", "-p", "nova-cli", "--", "export", str(nova), str(swf)],
            cwd=ROOT, capture_output=True, text=True, timeout=300,
        )
        if r.returncode != 0:
            print("EXPORT FAILED")
            print(r.stdout[-3000:])
            print(r.stderr[-3000:])
            return 1
        print(r.stdout.strip().splitlines()[-1] if r.stdout.strip() else "exported")

        out = tmp / "ruffle.log"
        import os
        env = dict(os.environ)
        env["RUST_LOG"] = "warn"
        proc = subprocess.Popen(
            [str(RUFFLE), str(swf)],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
            env=env,
        )
        time.sleep(4)
        proc.kill()
        log = proc.stdout.read() if proc.stdout else ""
        errors = [l for l in log.splitlines() if "ERROR" in l or "WARN" in l and "avm" in l]
        seen = set()
        for line in errors:
            # strip ansi + timestamp
            clean = line.split("ERROR", 1)[-1].split("WARN", 1)[-1] if ("ERROR" in line or "WARN" in line) else line
            clean = clean.strip()[:220]
            if clean not in seen:
                seen.add(clean)
                print(clean)
        if not errors:
            print("RUFFLE CLEAN")
        return 0


if __name__ == "__main__":
    sys.exit(main())
