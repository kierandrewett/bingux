#!/usr/bin/env python3
"""Exercise Dock QML with deterministic events in Gnoblin's private test session."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


root = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / "tests/lib"))
import dock_fixture  # noqa: E402

test_name = os.environ.get("BINGUX_SHELL_TEST", "dock-state")
if test_name not in (
    "dock-state",
    "dock-focus-indicators",
    "dock-pinning",
    "dock-behaviour",
    "dock-refresh",
    "dock-order",
    "dock-overflow",
    "search-launch-cursor",
    "dock-launch-timeout",
    "dock-startup",
    "dock-menu-lazy",
    "dock-window-actions",
):
    raise SystemExit("Unknown shell test")
if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin scripts/run-gnome-shell.sh with GNOBLIN_TEST_DBUS_CLIENT.")
with tempfile.TemporaryDirectory(prefix="bingux-dock-test-") as directory:
    fixture = Path(directory)
    # Local component copies, so shared tooltip/menu dependencies evolve with
    # the shell. Only the Dock is instantiated by this fixture; the compositor
    # event source and inspection aliases are injected, and the production
    # grouping, timers, Repeater, layout and animations run intact.
    environment = (
        os.environ
        | dock_fixture.prepare(fixture, root / ("tests/" + test_name + ".qml"))
        | {
            "QT_QPA_PLATFORM": "wayland",
            "QT_QUICK_BACKEND": "software",
        }
    )
    if test_name == "search-launch-cursor":
        shutil.copy2(root / "tests/search-launch-socket.qml", fixture / "SearchSocket.qml")
        overlay_file = fixture / "SearchOverlay.qml"
        overlay_file.write_text(
            overlay_file.read_text().replace("id: root", "id: root\n    property alias testSocket: searchSocket", 1)
        )
    if test_name == "dock-launch-timeout":
        environment["BINGUX_APP_LAUNCHER_HELPER"] = "/usr/bin/true"
    command = [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture), "--no-color"]
    try:
        result = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=50)
    except subprocess.TimeoutExpired as error:
        print((error.stdout or b"").decode() + (error.stderr or b"").decode())
        raise SystemExit("Dock test timed out") from error
    output = result.stdout + result.stderr
    if test_name == "dock-startup" and result.returncode == 0 and "DOCK_TEST_PASSED" in output:
        # A second process reads the real Settings cache written by the first.
        result = subprocess.run(
            command, env=environment | {"DOCK_REPLAY": "1"}, capture_output=True, text=True, timeout=50
        )
        output += result.stdout + result.stderr
        if "DOCK_TEST_PASSED" not in result.stdout + result.stderr:
            output += "\nDOCK_BEHAVIOUR_FAILED restart did not complete\n"
    print(output)
    print(f"Shell exit status: {result.returncode}")
    if (
        "DOCK_BEHAVIOUR_FAILED" in output
        or result.returncode != 0
        or ("SEARCH_CURSOR_PASSED" if test_name == "search-launch-cursor" else "DOCK_TEST_PASSED") not in output
    ):
        raise SystemExit(1)
