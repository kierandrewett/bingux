#!/usr/bin/env python3
"""Check shared app badges, tooltips, and switcher motion in a private session."""

import os
from pathlib import Path
import subprocess

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
config = Path(os.environ["XDG_CONFIG_HOME"]) / "bingux"
config.mkdir(parents=True, exist_ok=True)
(config / "switcher.json").write_text('{"enabled":false,"showDelay":0}')
environment = os.environ | {"GNOBLIN_COMPOSITOR_SOCKET": "/tmp/bingux-switcher-ui-unused.sock"}
qml = config / "switcher-ui.qml"
qml.write_text(
    Path(__file__)
    .with_suffix(".qml")
    .read_text()
    .replace("../shell/bingux", (Path(__file__).resolve().parent.parent / "shell/bingux").as_uri())
)
try:
    result = subprocess.run(
        [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(qml)],
        env=environment,
        text=True,
        capture_output=True,
        timeout=8,
    )
except subprocess.TimeoutExpired as error:
    print((error.stdout or b"").decode() + (error.stderr or b"").decode())
    raise
output = result.stdout + result.stderr
print(output)
assert result.returncode == 0 and "SWITCHER_UI_PASSED" in output
