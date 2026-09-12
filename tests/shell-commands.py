#!/usr/bin/env python3
"""Exercise the shell action API on isolated QML models."""

import os
from pathlib import Path
import subprocess

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
repo = Path(__file__).resolve().parents[1]
config = Path(os.environ["XDG_CONFIG_HOME"]) / "commands.qml"
config.write_text(
    Path(__file__).with_suffix(".qml").read_text().replace("../shell/bingux", (repo / "shell/bingux").as_uri())
)
result = subprocess.run(
    [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(config)], text=True, capture_output=True, timeout=10
)
print(result.stdout + result.stderr)
assert result.returncode == 0 and "SHELL_COMMANDS_PASSED" in result.stdout + result.stderr
