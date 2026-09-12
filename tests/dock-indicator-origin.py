#!/usr/bin/env python3
"""Check that window removal and focus changes keep the dock pill inside its clip."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix="bingux-indicator-") as directory:
    fixture = Path(directory)
    for name in ("DockWindowIndicators", "Theme"):
        shutil.copy2(root / f"shell/bingux/{name}.qml", fixture)
    (fixture / "qmldir").write_text(
        "singleton Theme 1.0 Theme.qml\nDockWindowIndicators 1.0 DockWindowIndicators.qml\n"
    )
    shutil.copy2(root / "tests/dock-indicator-origin.qml", fixture / "shell.qml")
    result = subprocess.run(
        [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture), "--no-color"],
        env=os.environ | {"QT_QPA_PLATFORM": "offscreen", "QT_QUICK_BACKEND": "software"},
        capture_output=True,
        text=True,
        timeout=10,
    )
    output = result.stdout + result.stderr
    if result.returncode or "INDICATOR_TEST_PASSED" not in output or "PILL_CLIPPED" in output:
        raise SystemExit(output)
    print("PASS: active dock pill remains visible after window removal and focus changes")
