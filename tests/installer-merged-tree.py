#!/usr/bin/env python3
"""The installer overlay runs as one QML tree with one Theme singleton.

Packs shell/bingux plus distro/installer/shell into a merged tree with
scripts/pack-shell.py --tree-out, checks the tree with qmldir-complete.py, then
runs a probe under bingux-shell -p (offscreen, no compositor). The probe is a
scratch copy of the tree whose Theme logs once per instance and whose root
creates a base component (KioskCard) and an installer component (InstallerField).
The check is exactly one Theme instance, reached from both.

usage: installer-merged-tree.py [--shell BINARY]   (default build/bingux-shell/quickshell/src/bingux-shell)
"""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROBE_MAIN = """import QtQuick
import Quickshell

ShellRoot {
    KioskCard {
        Component.onCompleted: console.warn("PROBE-BASE", Theme.fontFamily)
    }
    InstallerField {
        Component.onCompleted: console.warn("PROBE-INSTALLER", Theme.fontFamily)
    }
}
"""
THEME_HOOK = 'QtObject {\n    id: root\n    Component.onCompleted: console.warn("PROBE-THEME-INSTANCE", root)\n'


def run(*command):
    return subprocess.run(command, capture_output=True, text=True, check=False)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--shell", default=str(ROOT / "build/bingux-shell/quickshell/src/bingux-shell"))
    arguments = parser.parse_args()
    scratch = Path(tempfile.mkdtemp(prefix="installer-merged-tree-"))
    try:
        tree = scratch / "tree"
        packed = run(
            sys.executable,
            str(ROOT / "scripts/pack-shell.py"),
            "--overlay",
            str(ROOT / "distro/installer/shell"),
            "--entries",
            '{"installer": "InstallerShell.qml"}',
            "--tree-out",
            str(tree),
            str(scratch / "installer.sxa"),
        )
        if packed.returncode:
            raise SystemExit(f"pack-shell failed: {packed.stderr}")
        checked = run(
            sys.executable,
            str(ROOT / "tests/qmldir-complete.py"),
            "--entry",
            "InstallerShell.qml",
            str(tree),
        )
        if checked.returncode:
            raise SystemExit(f"qmldir-complete failed: {checked.stderr}")

        probe = scratch / "probe"
        shutil.copytree(tree, probe)
        theme = probe / "Theme.qml"
        source = theme.read_text()
        if source.count("QtObject {\n    id: root\n") != 1:
            raise SystemExit("Theme.qml does not have the expected root; update the probe hook")
        theme.write_text(source.replace("QtObject {\n    id: root\n", THEME_HOOK, 1))
        (probe / "ProbeMain.qml").write_text(PROBE_MAIN)

        env = {k: v for k, v in os.environ.items() if k != "WAYLAND_DISPLAY"}
        env["QT_QPA_PLATFORM"] = "offscreen"
        try:
            result = subprocess.run(
                [arguments.shell, "-p", str(probe / "ProbeMain.qml")],
                capture_output=True,
                text=True,
                env=env,
                timeout=15,
                check=False,
            )
            output = result.stdout + result.stderr
        except subprocess.TimeoutExpired as expired:
            # The shell runs until killed; the probe has logged by then.
            output = "".join(part.decode() if isinstance(part, bytes) else part or "" for part in (expired.stdout, expired.stderr))
        instances = output.count("PROBE-THEME-INSTANCE")
        base = output.count("PROBE-BASE")
        installer = output.count("PROBE-INSTALLER")
        if "ERROR" in output and "Failed to load configuration" in output:
            raise SystemExit("probe failed to load:\n" + "\n".join(l for l in output.splitlines() if "ERROR" in l))
        if instances != 1:
            raise SystemExit(f"expected one Theme instance, saw {instances}")
        if base != 1 or installer != 1:
            raise SystemExit(f"base component ran {base} time(s), installer component {installer} time(s); expected 1 each")
        print("installer-merged-tree: one Theme instance reached from base and installer code")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
