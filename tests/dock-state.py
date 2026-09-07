#!/usr/bin/env python3
"""Exercise Dock QML with deterministic events in Gnoblin's private test session."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


root = Path(__file__).resolve().parent.parent
if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin scripts/run-gnome-shell.sh with GNOBLIN_TEST_DBUS_CLIENT.")
with tempfile.TemporaryDirectory(prefix="bingux-dock-test-") as directory:
    fixture = Path(directory)
    source = root / "shell/bingux"
    # Copy local component definitions so shared tooltip/menu dependencies
    # evolve with the shell. Only the Dock is instantiated by this fixture.
    for component in source.iterdir():
        if component.suffix in (".qml", ".js") or component.name == "qmldir":
            shutil.copy2(component, fixture)
    # Inject only the compositor event source and an inspection alias. The
    # production grouping, timers, Repeater, layout and animations run intact.
    dock_file = fixture / "Dock.qml"
    dock = dock_file.read_text().replace("ToplevelManager", "root.testManager")
    dock = dock.replace("required property var settings", """required property var settings
    required property var testManager
    property alias testItems: dockItems
    property alias testSurface: dockSurface
    property alias testTooltip: dockTooltip
    property alias testLaunchEffect: dockLaunchEffect
    property alias testLaunchOverlay: launchOverlay""")
    dock = dock.replace("model: root.testManager.toplevels", "model: root.testManager.toplevels.values")
    dock = dock.replace("id: dockButton", """id: dockButton
                    property alias testMenu: appMenu
                    property alias testMouse: dockMouse""")
    dock_file.write_text(dock)
    shutil.copy2(root / "tests/dock-state.qml", fixture / "shell.qml")
    environment = os.environ | {
        "QT_QPA_PLATFORM": "wayland",
        "QT_QUICK_BACKEND": "software",
        "XDG_CONFIG_HOME": str(fixture / "config"),
    }
    command = [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture), "--no-color"]
    try:
        result = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=15)
    except subprocess.TimeoutExpired as error:
        print((error.stdout or b"").decode() + (error.stderr or b"").decode())
        raise SystemExit("Dock test timed out") from error
    output = result.stdout + result.stderr
    print(output)
    if result.returncode != 0 or "DOCK_TEST_PASSED" not in output:
        raise SystemExit(1)
