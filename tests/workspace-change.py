#!/usr/bin/env python3
"""Exercise the production workspace-change state and surface in the private compositor."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile

repo = Path(__file__).resolve().parent.parent
if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin's private test session")

MODES = (
    ("normal motion", {}),
    ("reduced motion", {"BINGUX_REDUCED_MOTION": "1"}),
)

for label, extra_env in MODES:
    with tempfile.TemporaryDirectory(prefix="bingux-workspace-change-") as directory:
        fixture = Path(directory)
        for name in ("Theme", "PanelOutline", "WorkspaceState", "WorkspaceChangeState", "WorkspaceChangeSurface"):
            shutil.copy2(repo / f"shell/bingux/{name}.qml", fixture)
        # The test drives WorkspaceState.ingest() directly. Point it at a socket
        # that does not exist so it never talks to the live compositor.
        state_file = fixture / "WorkspaceState.qml"
        state_file.write_text(state_file.read_text().replace("pragma Singleton\n", "", 1))
        (fixture / "qmldir").write_text(
            "singleton Theme 1.0 Theme.qml\n"
            + "PanelOutline 1.0 PanelOutline.qml\n"
            + "WorkspaceState 1.0 WorkspaceState.qml\n"
            + "WorkspaceChangeState 1.0 WorkspaceChangeState.qml\n"
            + "WorkspaceChangeSurface 1.0 WorkspaceChangeSurface.qml\n"
        )
        surface = fixture / "WorkspaceChangeSurface.qml"
        surface.write_text(
            surface.read_text()
            .replace("property var workspaceWindows", "property var testWindows: []\n    property var workspaceWindows")
            .replace(
                "id: changeWindow",
                """id: changeWindow
            property alias testCard: changeCard
            property alias testMask: clickThroughTarget
            Component.onCompleted: root.testWindows = root.testWindows.concat([changeWindow])""",
            )
        )
        shutil.copy2(repo / "tests/workspace-change.qml", fixture / "shell.qml")
        env = os.environ | {
            "QT_QPA_PLATFORM": "wayland",
            "GNOBLIN_COMPOSITOR_SOCKET": str(fixture / "absent-compositor.sock"),
        } | extra_env
        result = subprocess.run(
            [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture), "--no-color"],
            env=env,
            capture_output=True,
            text=True,
            timeout=30,
        )
        output = result.stdout + result.stderr
        print(f"--- {label} ---")
        print(output)
        if result.returncode or "WORKSPACE_CHANGE_TEST_PASSED" not in output:
            raise SystemExit(1)
