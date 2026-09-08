"""Run top-bar component checks from an isolated Quickshell config."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="bingux-bar-controls-") as temporary:
    folder = Path(temporary)
    shutil.copytree(root / "shell/bingux", folder, dirs_exist_ok=True, ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy(root / "tests/top-bar-controls.qml", folder / "shell.qml")
    result = folder / "results"
    process = subprocess.run(["qs", "-p", str(folder)], env=dict(os.environ, BINGUX_TOP_BAR_TEST_RESULTS=str(result)),
                             capture_output=True, text=True, timeout=12)
    report = result.read_text() if result.exists() else "FAIL: no report"
    print(report)
    assert process.returncode == 0 and report.startswith("PASS:"), process.stdout + process.stderr
