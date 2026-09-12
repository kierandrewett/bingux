#!/usr/bin/env python3
"""Check notification history and toast motion beside the sidebar."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile

repo = Path(__file__).resolve().parent.parent
if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin's private test session")
with tempfile.TemporaryDirectory(prefix="bingux-notification-sidebar-") as directory:
    fixture = Path(directory)
    for source in (repo / "shell/bingux").iterdir():
        if source.suffix in (".qml", ".js") or source.name == "qmldir":
            shutil.copy2(source, fixture)
    shutil.copy2(repo / "tests/notification-sidebar.qml", fixture / "shell.qml")
    result = subprocess.run(
        [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture), "--no-color"],
        env=os.environ | {"QT_QPA_PLATFORM": "wayland", "XDG_STATE_HOME": str(fixture / "state")},
        capture_output=True,
        text=True,
        timeout=20,
    )
    output = result.stdout + result.stderr
    print(output)
    if result.returncode or "NOTIFICATION_SIDEBAR_PASSED" not in output:
        raise SystemExit(1)
