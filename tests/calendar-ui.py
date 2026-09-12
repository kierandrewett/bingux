"""Isolated calendar UI regression fixture; no interaction with the user's accounts."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="bingux-calendar-test-") as temporary:
    folder = Path(temporary)
    shutil.copytree(root / "shell/bingux", folder, dirs_exist_ok=True, ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy(root / "tests/calendar-ui.qml", folder / "shell.qml")
    report = folder / "result"
    result = subprocess.run(
        ["qs", "-p", str(folder)],
        capture_output=True,
        text=True,
        timeout=12,
        env=dict(
            os.environ,
            BINGUX_CALENDAR_TEST_RESULTS=str(report),
            BINGUX_CALENDAR_TEST_IMAGE="/tmp/bingux-calendar-review.png",
        ),
    )
    message = report.read_text() if report.exists() else "FAIL: no result"
    print(message)
    assert result.returncode == 0 and message.startswith("PASS:"), result.stdout + result.stderr
