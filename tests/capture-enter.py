#!/usr/bin/env python3
"""Private compositor: Return and keypad Enter save with toolbar focus."""
import os
from pathlib import Path
import shutil
import subprocess

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
repo = Path(__file__).resolve().parents[1]
config = Path(os.environ["XDG_CONFIG_HOME"]) / "capture-enter"
shutil.copytree(repo / "shell/bingux", config, ignore=shutil.ignore_patterns("__pycache__"))
output = config / "captures"
output.mkdir()
settings = config / "capture.ini"
settings.write_text("[capture]\nkind=screenshot\ncopy=false\ndirectory=" + str(output) + "\n")
fixture = config / "CaptureEnterTest.qml"
fixture.write_text(Path(__file__).with_suffix(".qml").read_text().replace('import "../shell/bingux"', 'import "."'))
result = subprocess.run([os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture)],
    env=os.environ | {"BINGUX_CAPTURE_SETTINGS_PATH": settings.as_uri(), "CAPTURE_ENTER_OUTPUT": str(output), "CAPTURE_ENTER_REPORT": str(config / "report.txt")},
    text=True, capture_output=True, timeout=20)
print(result.stdout + result.stderr)
report = (config / "report.txt").read_text()
print(report)
assert result.returncode == 0 and "CAPTURE_ENTER_PASSED" in report
assert len(list(output.glob("*.png"))) == 2
