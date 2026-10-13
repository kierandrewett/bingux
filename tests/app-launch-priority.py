#!/usr/bin/env python3
"""Probe actual GIO child nice in isolated launcher processes and desktop entries."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "shell/bingux/launch-application.py"
CHILD = r"""
#include <sched.h>
#include <stdio.h>
#include <sys/resource.h>
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    FILE *file = fopen(argv[1], "w");
    if (!file) return 2;
    fprintf(file, "{\"nice\":%d,\"policy\":%d}\n",
            getpriority(PRIO_PROCESS, 0), sched_getscheduler(0));
    return fclose(file) != 0;
}
"""
WRAPPER = r"""
import importlib.util, json, os, sys, time
from pathlib import Path
source, directory, kind, require_boost = sys.argv[1:]
root = Path(directory)
spec = importlib.util.spec_from_file_location("launcher", source)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
# Invoke under a boosted parent to require the negative-nice regression.
# No privileged or host scheduling-policy changes occur in this fixture.
for _ in range(25):
    if os.getpriority(os.PRIO_PROCESS, 0) < 0: break
    if require_boost != "True": break
    time.sleep(.1)
if require_boost == "True" and os.getpriority(os.PRIO_PROCESS, 0) >= 0:
    print("Negative nice unavailable: inheritance regression NOT CHECKED", file=sys.stderr)
    raise SystemExit(77)
if kind == "positive": os.setpriority(os.PRIO_PROCESS, 0, 19)
before = {"nice": os.getpriority(os.PRIO_PROCESS, 0), "policy": os.sched_getscheduler(0)}
flags = ["--dock-feedback"] if kind == "fallback" else ["--host-launch"]
if kind == "action": flags += ["--new-window"]
code = module.main(flags + ["bingux-priority-probe"])
record = root / "child.json"
deadline = time.monotonic() + 2
while not record.exists() and time.monotonic() < deadline: time.sleep(.01)
print(json.dumps({"case":kind,"caller_before":before,
                  "launcher_after":{"nice":os.getpriority(os.PRIO_PROCESS,0),
                                    "policy":os.sched_getscheduler(0)},
                  "child":json.loads(record.read_text()),"code":code}))
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-boost", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="bingux-launch-priority-") as temporary:
        root = args.output.resolve() if args.output else Path(temporary)
        root.mkdir(parents=True, exist_ok=True)
        (root / "child.c").write_text(CHILD)
        subprocess.run(["cc", "-O2", "-Wall", "-Werror", str(root / "child.c"), "-o", str(root / "child")], check=True)
        (root / "wrapper.py").write_text(WRAPPER)
        apps = root / "applications"
        apps.mkdir(exist_ok=True)
        command = f'"{root / "child"}" "{root / "child.json"}"'
        (apps / "bingux-priority-probe.desktop").write_text(
            "[Desktop Entry]\nType=Application\nName=Priority probe\nExec="
            + command
            + "\nActions=new-window;\n[Desktop Action new-window]\nName=New\nExec="
            + command
            + "\n"
        )
        # Bound the manager-failure case without creating any real service.
        binaries = root / "bin"
        binaries.mkdir(exist_ok=True)
        for name in ("systemctl", "systemd-run"):
            path = binaries / name
            path.write_text("#!/bin/sh\nexit " + ("1" if name == "systemd-run" else "0") + "\n")
            path.chmod(0o700)
        env = os.environ | {
            "XDG_DATA_HOME": str(root),
            "XDG_DATA_DIRS": str(root),
            "PATH": str(binaries) + os.pathsep + os.environ["PATH"],
        }
        records = []
        for kind in ("normal", "action", "fallback", "positive"):
            (root / "child.json").unlink(missing_ok=True)
            result = subprocess.run(
                [sys.executable, str(root / "wrapper.py"), str(SOURCE), str(root), kind, str(args.require_boost)],
                env=env,
                capture_output=True,
                text=True,
                timeout=8,
                check=False,
            )
            if result.returncode:
                print(result.stderr, file=sys.stderr)
                raise SystemExit(result.returncode)
            record = json.loads(result.stdout)
            records.append(record)
            print(json.dumps(record), flush=True)
            expected = max(0, record["caller_before"]["nice"])
            assert record["code"] == 0, result.stderr
            assert record["child"]["nice"] == expected, "app inherited launcher boost"
            assert record["child"]["policy"] == record["caller_before"]["policy"]
            assert record["launcher_after"]["nice"] == expected
        (root / "result.json").write_text(
            json.dumps({"source_sha256": hashlib.sha256(SOURCE.read_bytes()).hexdigest(), "cases": records}, indent=2)
            + "\n"
        )
        checked = any(row["caller_before"]["nice"] < 0 for row in records)
        print("PASS: ordinary/action/fallback child scheduling and positive nice preserved")
        if not checked:
            print("NOT CHECKED: boosted-parent reset; rerun at negative nice with --require-boost")


if __name__ == "__main__":
    main()
