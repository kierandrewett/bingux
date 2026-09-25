#!/usr/bin/env python3
"""Keep backdrop blur inside real search, emoji and switcher materials."""

import json
import os
from pathlib import Path
import subprocess
import time
from PIL import Image, ImageChops, ImageStat

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
source = Path(__file__).resolve()
root = Path(os.environ["XDG_CONFIG_HOME"])
fixture = root / "popup-shadow-blur.qml"
qml = source.with_name("popup-shadows.qml").read_text()
qml = qml.replace("ShellRoot {", "ShellRoot { id: regression; property bool flat: false", 1)
qml = qml.replace(
    'color: "#8090a0"',
    """color: "#888888"
        Canvas { id: checker; anchors.fill: parent
            onPaint: {
                const ctx = getContext("2d");
                ctx.fillStyle = "#888888"; ctx.fillRect(0,0,width,height);
                if (!regression.flat) for (let y=0;y<height;y+=4) for(let x=0;x<width;x+=4) {
                    ctx.fillStyle = (x/4+y/4)%2 ? "#eeeeee" : "#222222";
                    ctx.fillRect(x,y,4,4);
                }
            }
            Connections { target: regression; function onFlatChanged() { checker.requestPaint(); } }
        }""",
)
qml = qml.replace(
    'target: "shadows"',
    """target: "shadows"
        function reference(value: bool): void { regression.flat = value; }""",
)
qml = qml.replace(
    "layers:item.layers.length,",
    """bounds: [item.surface.mapToItem(current,0,0).x,
                item.surface.mapToItem(current,0,0).y,item.surface.width,item.surface.height], layers:item.layers.length,""",
)
fixture.write_text(qml.replace("../shell/bingux", (source.parents[1] / "shell/bingux").as_uri()))
config = root / "gnoblin"
config.mkdir(exist_ok=True)
# Deliberately disable alpha inference: the standard region must exclude shadows.
ignore_shadows = "true" if os.environ.get("EXPECT_STANDARD") == "0" else "false"
(config / "init.lua").write_text(
    "return {shell={['layer-animation']='none'}, "
    "['window-rules']={{match={type='layer'},blur=24,['blur-ignore-shadows']=" + ignore_shadows + "}}}"
)
subprocess.run([str(source.parents[2] / "gnoblin/src/tools/gnoblinctl"), "reload"], check=True)
qs = os.environ.get("QS_TEST_BIN", "qs")


def ipc(method, *args):
    return subprocess.check_output([qs, "-p", str(fixture), "ipc", "call", "shadows", method, *args], text=True)


def capture(name):
    path = root / (name + ".png")
    subprocess.run(["grim", str(path)], check=True)
    return Image.open(path).convert("RGB")


with (root / "shadow-qml.log").open("w") as log:
    process = subprocess.Popen([qs, "-p", str(fixture)], stdout=log, stderr=log)
    try:
        time.sleep(1)
        for kind in ["search", "emoji", "switcher"]:
            ipc("reference", "false")
            ipc("present", kind)
            time.sleep(0.8)
            state = json.loads(ipc("shadow", "true"))
            assert state["layers"] == 3 and state["visible"] and state["opacity"] > 0.99, state
            time.sleep(0.15)
            on = capture(kind + "-shadow")
            ipc("shadow", "false")
            time.sleep(0.15)
            off = capture(kind + "-no-shadow")
            darkened = sum(sum(b) - sum(a) > 9 for a, b in zip(on.getdata(), off.getdata()))
            brightened = sum(max(a[i] - b[i] for i in range(3)) > 4 for a, b in zip(on.getdata(), off.getdata()))
            assert brightened < 10, (kind, "blur brightened black shadow pixels", brightened)
            assert darkened > 1000, (kind, "shadow did not darken exterior pixels", darkened)
            ipc("reference", "true")
            deadline = time.monotonic() + 5
            while True:
                time.sleep(0.1)
                reference = capture(kind + "-flat")
                difference = ImageChops.difference(off, reference)
                if max(ImageStat.Stat(difference.crop((0, 100, 100, 200))).mean) > 50:
                    break
                assert time.monotonic() < deadline, "Reference frame did not arrive"
            x, y, w, h = state["bounds"]
            interior = tuple(map(int, (x + 16, y + 16, x + w - 16, y + h - 16)))
            error = max(ImageStat.Stat(difference.crop(interior)).mean)
            assert error < 1, (kind, "panel interior lost backdrop blur", error)
            print(
                "PASS:",
                kind,
                "shadow brightening:",
                brightened,
                "darkened:",
                darkened,
                "interior backdrop error:",
                error,
                flush=True,
            )
    finally:
        process.terminate()
        process.wait(timeout=5)
        diagnostic = (root / "shadow-qml.log").read_text()
        if "Error:" in diagnostic or "failed" in diagnostic.lower():
            print(diagnostic)
