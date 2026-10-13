#!/usr/bin/env python3
"""Run tray menu lazy-load QML regression checks in an isolated shell copy."""

import os
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[1]


def main():
    binary = os.environ.get("BINGUX_QUICKSHELL", "quickshell")
    if not shutil.which(binary):
        raise SystemExit(f"Quickshell executable not found: {binary}")
    with tempfile.TemporaryDirectory(prefix="bingux-tray-menu-lazy-") as directory:
        work = Path(directory)
        for source in (REPO / "shell/bingux").glob("*.qml"):
            shutil.copy2(source, work / source.name)
        for source in (REPO / "shell/bingux").glob("*.js"):
            shutil.copy2(source, work / source.name)
        shutil.copy2(REPO / "shell/bingux/qmldir", work / "qmldir")
        shutil.copy2(REPO / "tests/tray-menu-lazy.qml", work / "shell.qml")

        # Adapt only handle identity and substitute lightweight popup/tooltip
        # components so the fixture needs no DBusMenu service or PanelWindow.
        (work / "TrayMenu.qml").write_text("""import QtQuick
Item {
    property var menu: null
    visible: false
    property var actions: null
    property var screen: null
    property var anchorWindow: null
    property Item anchorItem: null
    property bool anchorsBoundAtCompletion: false
    Component.onCompleted: anchorsBoundAtCompletion = !!anchorItem && !!anchorWindow && !!screen
    readonly property var menuEntries: actions || []
    readonly property bool hasMenuItems: menuEntries.some(entry => entry && !entry.isSeparator)
    onHasMenuItemsChanged: if (!hasMenuItems) visible = false
}""")
        (work / "BarTooltip.qml").write_text("""import QtQuick
Item {
    property Item anchorItem
    property var barWindow
    property bool requested: false
    property string text: ""
}""")
        tray_source = (work / "Tray.qml").read_text()
        for old, new in (
            (
                "readonly property var trayMenuHandle: modelData.menu",
                "readonly property var trayMenuHandle: modelData.testHandle",
            ),
            (
                "menu: trayButton.trayMenuHandle",
                "menu: null\n                                actions: trayButton.modelData.testActions",
            ),
        ):
            if tray_source.count(old) != 1:
                raise SystemExit(f"Expected one provider binding to adapt: {old}")
            tray_source = tray_source.replace(old, new)
        (work / "Tray.qml").write_text(tray_source)

        result_file = work / "results.txt"
        result_file.write_text("")
        environment = os.environ | {
            "QT_QPA_PLATFORM": "offscreen",
            "BINGUX_CONTROL_TEST_RESULTS": str(result_file),
        }
        environment.pop("WAYLAND_DISPLAY", None)
        environment.pop("DISPLAY", None)
        run = subprocess.run(
            ["dbus-run-session", "--", binary, "-p", str(work), "--no-color"],
            env=environment,
            text=True,
            capture_output=True,
            timeout=30,
        )
        print(run.stdout, end="")
        print(run.stderr, end="")
        if run.returncode:
            raise SystemExit(run.returncode)
        if not result_file.exists():
            raise SystemExit("QML fixture exited without a result file")
        try:
            result = json.loads(result_file.read_text())
        except (ValueError, OSError) as error:
            raise SystemExit(f"QML fixture did not write a complete JSON report: {error}")
        expected = sorted(re.findall(r"function (test_\w+)\(", (work / "shell.qml").read_text()))
        if (
            result.get("finished") is not True
            or result.get("failures") != 0
            or result.get("skipped") != 0
            or sorted(result.get("passedCases", [])) != expected
        ):
            raise SystemExit(f"QML fixture failed or omitted cases: {result}")
        print(f"PASS: {len(expected)} tray menu QML cases; 0 failures; 0 skipped")


if __name__ == "__main__":
    main()
