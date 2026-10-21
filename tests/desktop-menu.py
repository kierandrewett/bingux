#!/usr/bin/env python3
"""Run DesktopMenu QML checks: placement, action list, dispatch and visible failure.

Offscreen Quickshell has no layer-shell backend, so ShellPopup cannot load.
TrayMenu is replaced by a stub that keeps the properties DesktopMenu sets.
Escape, outside-click dismissal and real monitor placement need a live Wayland
session and are not covered here.
"""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

REPO = Path(__file__).resolve().parents[1]
SHELL = REPO / "shell/bingux"
MARKERS = {
    "dispatch": "DESKTOP_MENU_DISPATCH_PASSED",
    "missing": "DESKTOP_MENU_MISSING_PASSED",
}
TRAY_MENU_STUB = """import QtQuick
Item {
    property var actions: []
    property var screen: null
    property real preferredX: 0
    property real preferredY: 0
    property bool compactRows: false
}
"""


def prepare(work):
    for pattern in ("*.qml", "*.js"):
        for source in SHELL.glob(pattern):
            shutil.copy2(source, work / source.name)
    shutil.copy2(SHELL / "qmldir", work / "qmldir")
    shutil.copy2(REPO / "tests/desktop-menu.qml", work / "shell.qml")
    (work / "TrayMenu.qml").write_text(TRAY_MENU_STUB)


def run(binary, scenario, settings_bin):
    with tempfile.TemporaryDirectory(prefix="bingux-desktop-menu-") as directory:
        work = Path(directory)
        prepare(work)
        environment = os.environ | {
            "QT_QPA_PLATFORM": "offscreen",
            "BINGUX_DESKTOP_MENU_SCENARIO": scenario,
            "BINGUX_SETTINGS_BIN": str(settings_bin),
            "BINGUX_HELPER_DIR": str(SHELL),
        }
        environment.pop("WAYLAND_DISPLAY", None)
        environment.pop("DISPLAY", None)
        result = subprocess.run(
            ["dbus-run-session", "--", binary, "-p", str(work), "--no-color"],
            env=environment,
            text=True,
            capture_output=True,
            timeout=90,
        )
        output = result.stdout + result.stderr
        print(output, end="")
        if result.returncode:
            raise SystemExit(result.returncode)
        if MARKERS[scenario] not in output:
            raise SystemExit(f"{scenario}: QML fixture did not report {MARKERS[scenario]}")


def main():
    binary = os.environ.get("BINGUX_QUICKSHELL", "quickshell")
    if not shutil.which(binary):
        raise SystemExit(f"Quickshell executable not found: {binary}")
    with tempfile.TemporaryDirectory(prefix="bingux-desktop-menu-bin-") as directory:
        directory = Path(directory)
        marker = directory / "dispatched.txt"
        settings = directory / "bingux-settings"
        settings.write_text(f"#!/bin/sh\nprintf '%s' \"$1\" > '{marker}'\n")
        settings.chmod(0o755)
        run(binary, "dispatch", settings)
        # The settings launch is detached; allow a bounded window for the page write.
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline and not marker.exists():
            time.sleep(0.1)
        if not marker.exists() or marker.read_text().strip() != "display":
            raise SystemExit("Display Settings did not dispatch the display settings page")
        run(binary, "missing", directory / "missing-bingux-settings")
    print("PASS: desktop menu placement, dispatch and visible failure")


if __name__ == "__main__":
    main()
