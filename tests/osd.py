#!/usr/bin/env python3
"""Exercise the production OSD state and surfaces in the private compositor."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile

repo = Path(__file__).resolve().parent.parent
if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin's private test session")
with tempfile.TemporaryDirectory(prefix="bingux-osd-") as directory:
    fixture = Path(directory)
    for name in ("Theme", "SymbolicIcon", "AnimatedCount", "OsdState", "OsdSurface", "PanelOutline"):
        shutil.copy2(repo / f"shell/bingux/{name}.qml", fixture)
    (fixture / "qmldir").write_text(
        "singleton Theme 1.0 Theme.qml\n"
        + "".join(
            f"{name} 1.0 {name}.qml\n"
            for name in ("SymbolicIcon", "AnimatedCount", "OsdState", "OsdSurface", "PanelOutline")
        )
    )
    surface = fixture / "OsdSurface.qml"
    surface.write_text(
        surface.read_text()
        .replace("property var osdWindows", "property var testWindows: []\n    property var osdWindows")
        .replace(
            "id: osdWindow",
            """id: osdWindow
            property alias testCard: osdCard
            property alias testMask: clickThroughTarget
            Component.onCompleted: root.testWindows = root.testWindows.concat([osdWindow])""",
        )
    )
    shutil.copy2(repo / "tests/osd.qml", fixture / "shell.qml")
    result = subprocess.run(
        [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture), "--no-color"],
        env=os.environ | {"QT_QPA_PLATFORM": "wayland"},
        capture_output=True,
        text=True,
        timeout=15,
    )
    output = result.stdout + result.stderr
    print(output)
    if result.returncode or "OSD_TEST_PASSED" not in output:
        raise SystemExit(1)
