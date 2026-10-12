#!/usr/bin/env python3
"""Exercise retained sidebar content in a running native Wayland session."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path(__file__).resolve().parents[1] / "shell/bingux")
    parser.add_argument(
        "--selection", choices=("terminal", "notes", "monitor", "calendar", "media", "tasks"), default="terminal"
    )
    parser.add_argument("--restore", action="store_true")
    parser.add_argument("--detached", action="store_true")
    parser.add_argument("--placement", choices=("top-right", "control-centre"), default="")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="bingux-sidebar-lazy-") as temporary:
        root = Path(temporary)
        shell = root / "shell"
        shutil.copytree(args.source, shell)
        config = root / "config/bingux"
        config.mkdir(parents=True)
        for file in shell.glob("*.qml"):
            file.write_text(
                file.read_text().replace(
                    '"file://" + Quickshell.env("HOME") + "/.config/bingux/', '"' + config.as_uri() + "/"
                )
            )
        (config / "sidebar.ini").write_text(
            "[General]\n"
            f"contentType={args.selection}\n"
            f"wasOpen={str(args.restore).lower()}\n"
            f"detached={str(args.detached).lower()}\n"
        )
        shutil.copyfile(Path(__file__).with_suffix(".qml"), shell / "shell.qml")
        report = root / "results.txt"
        environment = os.environ | {
            "XDG_CONFIG_HOME": str(root / "config"),
            "BINGUX_NOTES_TEST_RESULTS": str(report),
            "BINGUX_SIDEBAR_SELECTION": args.selection,
            "BINGUX_SIDEBAR_RESTORE": "1" if args.restore else "0",
            "BINGUX_SIDEBAR_PLACEMENT": args.placement,
        }
        result = subprocess.run(
            [environment.get("QUICKSHELL_BIN", "quickshell"), "-p", str(shell)],
            env=environment,
            capture_output=True,
            text=True,
            timeout=35,
            check=False,
        )
        log = result.stdout + result.stderr
        checks = report.read_text() if report.exists() else "No report\n"
        if args.output:
            args.output.mkdir(parents=True, exist_ok=True)
            (args.output / "runtime.log").write_text(log)
            (args.output / "results.txt").write_text(checks)
        print(checks)
        passed = result.returncode == 0 and "FAILURES 0" in checks and "PASSED true" in checks
        passed = passed and not any(
            error in log for error in ("TypeError:", "ReferenceError:", "Failed to load configuration")
        )
        if not passed:
            print(log)
        return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
