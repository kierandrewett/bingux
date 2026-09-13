#!/usr/bin/env python3
"""Pixel regression of real items fading independently inside client buffers."""

import os
from pathlib import Path
import subprocess
import time

from PIL import Image, ImageChops, ImageStat

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
source = Path(__file__).resolve()
repo = source.parents[1]
config = Path(os.environ["XDG_CONFIG_HOME"])
root = config / "gnoblin"
root.mkdir(exist_ok=True)
(root / "init.lua").write_text("""local g = require("gnoblin")
g.set({
    shell = { ["layer-animation"] = "none" },
    ["window-rules"] = {
        {match = {type = "layer"}, blur = 24, ["blur-ignore-shadows"] = true},
        {match = {title = "^Floating panel fade regression$"}, blur = 24,
            ["blur-ignore-shadows"] = true, animation = "none"},
        {match = {layer = "^bingux-popup-dismiss$"}, blur = 0},
    },
})
""")
fixture = config / "shared-buffer-fades.qml"
fixture.write_text(source.with_suffix(".qml").read_text().replace("../shell/bingux", (repo / "shell/bingux").as_uri()))
qs = os.environ.get("QS_TEST_BIN", "qs")


def ipc(method, *args):
    return subprocess.check_output([qs, "-p", str(fixture), "ipc", "call", "fades", method, *map(str, args)], text=True)


def capture():
    path = config / "frame.png"
    subprocess.run(["grim", str(path)], check=True)
    return Image.open(path).convert("RGB")


with (config / "shared-fades.log").open("w") as log:
    process = subprocess.Popen([qs, "-p", str(fixture)], stdout=log, stderr=log)
    try:
        time.sleep(1)
        for kind in os.environ.get(
            "FADE_TEST_ITEMS", "inline shadowed floating tooltip corner notification search preview capture"
        ).split():
            ipc("present", kind)
            time.sleep(1)
            ipc("prepare")
            ipc("alpha", 1)
            time.sleep(1)
            settled = capture()
            print(ipc("diagnostic"), flush=True)
            ipc("alpha", 0)
            time.sleep(1)
            background = capture()
            bounds = ImageChops.difference(settled, background).getbbox()
            assert bounds, (kind, "No visible test surface")
            for alpha in (0.75, 0.5, 0.25, 0.1, 0, 0.5, 1):
                ipc("alpha", alpha)
                time.sleep(1)
                actual = capture()
                expected = Image.blend(background, settled, alpha)
                error = max(ImageStat.Stat(ImageChops.difference(actual, expected).crop(bounds)).mean)
                print(kind, alpha, error, flush=True)
                if error >= 2:
                    actual.save("/tmp/shared-fade-actual.png")
                    expected.save("/tmp/shared-fade-expected.png")
                    settled.save("/tmp/shared-fade-settled.png")
                assert error < 2, (kind, alpha, error)
            print("PASS:", kind, "shared-buffer blur fade and reversal", flush=True)
    finally:
        process.terminate()
        process.wait(timeout=5)
        diagnostic = (config / "shared-fades.log").read_text()
        if "Error:" in diagnostic or "failed" in diagnostic.lower():
            print(diagnostic)
