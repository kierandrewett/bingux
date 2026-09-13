#!/usr/bin/env python3
"""Private-session pixel check of shadows on the three real popup components."""

import json
import os
from pathlib import Path
import subprocess
import time
from PIL import Image

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
source = Path(__file__).resolve()
root = Path(os.environ["XDG_CONFIG_HOME"])
fixture = root / "popup-shadows.qml"
fixture.write_text(
    source.with_suffix(".qml").read_text().replace("../shell/bingux", (source.parents[1] / "shell/bingux").as_uri())
)
qs = os.environ.get("QS_TEST_BIN", "qs")
with (root / "shadow-qml.log").open("w") as log:
    process = subprocess.Popen([qs, "-p", str(fixture)], stdout=log, stderr=log)

    def ipc(*args):
        return subprocess.check_output(
            [qs, "-p", str(fixture), "ipc", "call", "shadows", args[0], "--", *args[1:]], text=True
        )

    try:
        time.sleep(1)
        for kind in ["search", "emoji", "switcher"]:
            ipc("present", kind)
            time.sleep(0.8)
            state = json.loads(ipc("shadow", "true"))
            assert state["layers"] == 3 and state["visible"] and state["opacity"] > 0.99, state
            time.sleep(0.15)
            on = Path("/tmp") / ("bingux-shadow-" + kind + ".png")
            subprocess.run(["grim", str(on)], check=True)
            ipc("shadow", "false")
            time.sleep(0.15)
            off = root / "without-shadow.png"
            subprocess.run(["grim", str(off)], check=True)
            a, b = Image.open(on).convert("RGB"), Image.open(off).convert("RGB")
            darkened = sum(sum(y) - sum(x) > 9 for x, y in zip(a.getdata(), b.getdata()))
            assert darkened > 1000, (kind, "shadow did not darken exterior pixels", darkened)
            print("PASS:", kind, "shadow rendered;", darkened, "pixels darkened")
    finally:
        process.terminate()
        process.wait(timeout=5)
        print((root / "shadow-qml.log").read_text())
