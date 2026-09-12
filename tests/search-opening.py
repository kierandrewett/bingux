#!/usr/bin/env python3
"""Private Wayland check: Search must not animate from its initial window size."""

import os
from pathlib import Path
import subprocess

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
source = Path(__file__).resolve()
fixture = Path(os.environ["XDG_CONFIG_HOME"]) / "search-opening.qml"
fixture.write_text(
    source.with_suffix(".qml").read_text().replace("../shell/bingux", (source.parents[1] / "shell/bingux").as_uri())
)
result = subprocess.run(
    [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture)], capture_output=True, text=True, timeout=12
)
print(result.stdout + result.stderr)
assert result.returncode == 0
assert (
    result.stdout.count("PASS: search appears at a fixed position")
    + result.stderr.count("PASS: search appears at a fixed position")
    == 3
)
