#!/usr/bin/env python3
"""Check recording controls, privacy indicators, and timer transitions."""
import os
from pathlib import Path
import subprocess

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
config = Path(os.environ["XDG_CONFIG_HOME"]) / "bingux"
config.mkdir(parents=True, exist_ok=True)
environment = os.environ | {"GNOBLIN_COMPOSITOR_SOCKET": "/tmp/bingux-privacy-test-unused.sock"}
qml = config / "privacy-indicators.qml"
qml.write_text(Path(__file__).with_suffix(".qml").read_text().replace("../shell/bingux", (Path(__file__).resolve().parent.parent / "shell/bingux").as_uri()))
try:
    result = subprocess.run([os.environ.get("QS_TEST_BIN", "qs"), "-p",
        str(qml)], env=environment, text=True, capture_output=True, timeout=12)
except subprocess.TimeoutExpired as error:
    print((error.stdout or b"").decode() + (error.stderr or b"").decode())
    raise
output = result.stdout + result.stderr
print(output)
assert result.returncode == 0 and "PRIVACY_UI_PASSED" in output
