#!/usr/bin/env python3
"""NØVA M12 runtime validation harness.

Exports a .nova project (or accepts a .swf directly), runs it under Ruffle,
captures `avm_trace` output and ERROR/WARN log lines, then evaluates
runtime assertions — not just startup.

This is a *runtime behaviour* probe: tests pass only when the expected trace
markers were actually emitted while the SWF was running, and no ERROR lines
appeared. It is not a frame-accurate or pixel-accurate validator.

Usage:
    python tools/ruffle-runtime-test.py PROJECT.nova [--swf OUT.swf]
        [--seconds 8] [--expect "marker1" --expect "marker2"]
        [--forbid "bad marker"] [--json report.json] [--keep]

    python tools/ruffle-runtime-test.py FILE.swf --expect ...

Ruffle location: $RUFFLE or default install paths below.
Exit code 0 = all assertions passed, 1 = failure, 2 = infrastructure error.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
NOVA_CLI = REPO / "target" / "debug" / "nova.exe"
RUFFLE_CANDIDATES = [
    os.environ.get("RUFFLE", ""),
    r"C:\Program Files\Ruffle\bin\ruffle.exe",
    r"C:\Users\admin\Downloads\ruffle-0.7.1-windows-x86_64\ruffle.exe",
]

TRACE_RE = re.compile(r"avm_trace.*?:\s?(.*)$")
ERROR_RE = re.compile(r"\bERROR\b")
WARN_RE = re.compile(r"\bWARN\b")


def find_ruffle() -> str:
    for cand in RUFFLE_CANDIDATES:
        if cand and Path(cand).exists():
            return cand
    found = shutil.which("ruffle")
    if found:
        return found
    sys.exit("ruffle executable not found; set $RUFFLE")


def export_swf(project: Path, out: Path) -> list[str]:
    if not NOVA_CLI.exists():
        sys.exit(f"nova CLI not built: {NOVA_CLI} (run cargo build -p nova-cli)")
    proc = subprocess.run(
        [str(NOVA_CLI), "export", str(project), str(out)],
        capture_output=True, text=True, timeout=120,
    )
    lines = (proc.stdout + proc.stderr).splitlines()
    if proc.returncode != 0:
        sys.exit("nova export failed:\n" + "\n".join(lines))
    return lines


def run_ruffle(swf: Path, seconds: float) -> dict:
    log = swf.with_suffix(swf.suffix + ".log")
    err = swf.with_suffix(swf.suffix + ".err")
    env = dict(os.environ, RUST_LOG="info")
    with open(log, "w", encoding="utf-8", errors="replace") as out_f, open(
        err, "w", encoding="utf-8", errors="replace"
    ) as err_f:
        proc = subprocess.Popen(
            [find_ruffle(), str(swf)],
            stdout=out_f, stderr=err_f, env=env,
        )
        try:
            proc.wait(timeout=seconds)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=10)
    text = log.read_text(encoding="utf-8", errors="replace")
    errtext = err.read_text(encoding="utf-8", errors="replace") if err.exists() else ""
    traces = []
    errors = []
    warnings = []
    for line in (text + "\n" + errtext).splitlines():
        m = TRACE_RE.search(line)
        if m:
            traces.append(m.group(1).strip())
        elif ERROR_RE.search(line):
            errors.append(line.strip()[:400])
        elif WARN_RE.search(line) and "avm_trace" not in line:
            warnings.append(line.strip()[:300])
    return {"traces": traces, "errors": errors, "warnings": warnings}


def evaluate(result: dict, expects: list[str], forbids: list[str]) -> dict:
    checks = []
    ok = True
    for marker in expects:
        found = any(marker in t for t in result["traces"])
        checks.append({"kind": "expect", "marker": marker, "found": found, "passed": found})
        ok = ok and found
    for marker in forbids:
        found = any(marker in t for t in result["traces"])
        checks.append({"kind": "forbid", "marker": marker, "violated": found, "passed": not found})
        ok = ok and not found
    checks.append({"kind": "no_errors", "count": len(result["errors"]), "passed": not result["errors"]})
    ok = ok and not result["errors"]
    return {"passed": ok, "checks": checks}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("target", help=".nova project or .swf file")
    ap.add_argument("--seconds", type=float, default=8.0)
    ap.add_argument("--expect", action="append", default=[], help="trace substring that must appear")
    ap.add_argument("--forbid", action="append", default=[], help="trace substring that must NOT appear")
    ap.add_argument("--json", help="write machine-readable report here")
    ap.add_argument("--keep", action="store_true", help="keep exported SWF/logs")
    args = ap.parse_args()

    target = Path(args.target)
    workdir = Path(tempfile.mkdtemp(prefix="nova-ruffle-"))
    try:
        export_lines = []
        if target.suffix.lower() == ".nova":
            swf = workdir / (target.stem + ".swf")
            export_lines = export_swf(target, swf)
        else:
            swf = target
        result = run_ruffle(swf, args.seconds)
        verdict = evaluate(result, args.expect, args.forbid)
        report = {
            "target": str(target),
            "swf": str(swf),
            "seconds": args.seconds,
            "export": export_lines,
            "trace_count": len(result["traces"]),
            "traces": result["traces"],
            "errors": result["errors"],
            "warnings": result["warnings"],
            "verdict": verdict,
        }
        if args.json:
            Path(args.json).write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(f"traces: {len(result['traces'])}  errors: {len(result['errors'])}")
        for chk in verdict["checks"]:
            mark = "PASS" if chk["passed"] else "FAIL"
            print(f"  [{mark}] {chk}")
        for e in result["errors"][:10]:
            print("  ERROR:", e)
        if not verdict["passed"]:
            return 1
        return 0
    finally:
        if not args.keep:
            shutil.rmtree(workdir, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
