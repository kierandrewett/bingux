#!/usr/bin/env python3
"""Check pointer input through the panel fade composition."""

import os
from pathlib import Path
import subprocess
import json
from PIL import Image

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
source = Path(__file__).resolve()
fixture = Path(os.environ["XDG_CONFIG_HOME"]) / "popup-fade-input.qml"
fixture.write_text(
    source.with_suffix(".qml").read_text().replace("../shell/bingux", (source.parents[1] / "shell/bingux").as_uri())
)
result = subprocess.run(
    [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture)], capture_output=True, text=True, timeout=15
)
print(result.stdout + result.stderr)
assert result.returncode == 0
assert "PASS: native, inline and shadowed popup buttons work during fades" in result.stdout + result.stderr
for line in (result.stdout + result.stderr).splitlines():
    if "SWATCH " not in line:
        continue
    sample = json.loads(line.split("SWATCH ", 1)[1])
    image = Image.open(fixture.parent / ("popup-" + sample["mode"] + ".png")).convert("RGB")
    pixel = image.getpixel((sample["x"], sample["y"]))
    assert min(pixel) >= 240, ("Panel background obscures opaque content", sample["mode"], pixel)
print("PASS: opaque panel content stays opaque in all three popup modes")
