#!/usr/bin/env python3
"""Check real panel close frames against a fade of their settled composition."""

import os
from pathlib import Path
import shutil
import subprocess
import time

from PIL import Image, ImageChops, ImageStat

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
source = Path(__file__).resolve()
repo = source.parents[1]
gnoblin = repo.parent / "gnoblin"
config = Path(os.environ["XDG_CONFIG_HOME"])
root = config / "gnoblin"
scripts = root / "scripts"
scripts.mkdir(parents=True, exist_ok=True)
(scripts / "slow-panels.js").write_text("""import St from 'gi://St';
export default function(api) {
    const settings = St.Settings.get();
    const previous = settings.slow_down_factor;
    settings.slow_down_factor = 10;
    api.addCleanup(() => settings.slow_down_factor = previous);
}
""")
(root / "init.lua").write_text("""local g = require("gnoblin")
g.set({
    shell = { ["layer-animation"] = "none" },
    ["window-rules"] = {
        {match = {layer = "^(gnoblin-shell-popup|gnoblin-dock-tooltip|bingux-switcher)$"}, blur = 24},
        {match = {layer = "^gnoblin-shell-popup$"}, animation = {["in"] = "fade", out = "fade", duration = 120, easing = "ease-out-cubic"}},
        {match = {layer = "^gnoblin-dock-tooltip$"}, animation = {["in"] = "fade", out = "fade", duration = 160, easing = "ease-out-cubic"}},
        {match = {layer = "^bingux-switcher$"}, animation = {["in"] = "none", out = "fade", duration = 100, easing = "ease-out-cubic"}},
    },
})
""")
subprocess.run([str(gnoblin / "src/tools/gnoblinctl"), "reload"], check=True)
fixture = config / "panel-blur-fade.qml"
fixture.write_text(source.with_suffix(".qml").read_text().replace("../shell/bingux", (repo / "shell/bingux").as_uri()))
qs = os.environ.get("QS_TEST_BIN", "qs")


def ipc(method, *args):
    return subprocess.check_output([qs, "-p", str(fixture), "ipc", "call", "panels", method, *args], text=True).strip()


def capture():
    path = config / "frame.png"
    subprocess.run(["grim", str(path)], check=True)
    return Image.open(path).convert("RGB")


with (config / "panel-qml.log").open("w") as log:
    process = subprocess.Popen([qs, "-p", str(fixture)], stdout=log, stderr=log)
    try:
        time.sleep(1)
        for attempt in range(30):
            if ipc("ready") == "true":
                break
            time.sleep(0.1)
        else:
            raise AssertionError("Panel animation policies were not received")
        background = capture()
        for kind in ("popup", "tooltip", "switcher"):
            if kind == "switcher":
                ipc("seedWindow")
                time.sleep(2)
                background = capture()
            ipc("present", kind)
            if kind == "popup":
                assert float(ipc("popupOpacity")) == 1, "Popup still fades inside its buffer"
            time.sleep(2)
            settled = capture()
            bounds = ImageChops.difference(settled, background).getbbox()
            assert bounds, (kind, "panel did not render")
            ipc("dismiss", kind)
            samples = []
            for delay in (0.12, 0.18, 0.2):
                time.sleep(delay)
                actual = capture()
                # Fit the fade amount, then require all pixels (including blur)
                # to agree with that single amount. A client-only fade cannot.
                errors = []
                for step in range(101):
                    expected = Image.blend(background, settled, step / 100)
                    error = max(ImageStat.Stat(ImageChops.difference(actual, expected).crop(bounds)).mean)
                    errors.append(error)
                best = min(range(101), key=errors.__getitem__)
                samples.append(best)
                assert errors[best] < 2, (kind, best, errors[best])
            assert 0 < samples[0] < 100, (kind, "missing intermediate fade", samples)
            assert samples[-1] < samples[0], (kind, "fade did not advance", samples)
            time.sleep(2)
            assert max(ImageStat.Stat(ImageChops.difference(capture(), background)).mean) < 0.1, (
                kind,
                "blur remained after close",
            )
            print("PASS:", kind, "blur and content share the close fade", samples, flush=True)
    finally:
        process.terminate()
        process.wait(timeout=5)
        print((config / "panel-qml.log").read_text())
