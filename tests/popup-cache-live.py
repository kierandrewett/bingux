#!/usr/bin/env python3
"""Verify popup caching with real QMP input in an existing disposable Gnoblin VM."""

import argparse
import io
import json
from pathlib import Path
import socket
import subprocess
import tarfile
import time

from PIL import Image, ImageChops, ImageStat


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("runner", type=Path, help="Gnoblin scripts/qemu-e2e")
parser.add_argument("run", type=Path)
parser.add_argument("--quickshell", required=True, help="Matching executable inside the guest")
parser.add_argument("--import-path", required=True, help="Installed Bingux QML plugins inside the guest")
parser.add_argument("--runtime", default="/run/user/1000")
parser.add_argument("--display", default="wayland-0")
parser.add_argument("--cycles", type=int, default=12)
args = parser.parse_args()
assert 1 <= args.cycles <= 100
repo = Path(__file__).resolve().parents[1]
env = [
    "env",
    f"XDG_RUNTIME_DIR={args.runtime}",
    f"WAYLAND_DISPLAY={args.display}",
    "QT_QPA_PLATFORM=wayland",
    f"QML_IMPORT_PATH={args.import_path}",
]


def guest(*command, payload=None):
    return subprocess.check_output(
        [str(args.runner), "guest", str(args.run), "--", *env, *command], input=payload, timeout=20
    )


fixture = guest("mktemp", "-d", "/tmp/bingux-popup-cache.XXXXXX").decode().strip()
payload = io.BytesIO()
with tarfile.open(fileobj=payload, mode="w") as archive:
    for source in (repo / "shell/bingux").iterdir():
        if source.suffix not in (".qml", ".js") and source.name != "qmldir":
            continue
        if source.name == "shell.qml":
            data = (repo / "tests/popup-cache.qml").read_bytes()
        elif source.name == "ShellPopup.qml":
            data = source.read_bytes().replace(
                b"    id: root\n",
                b"    id: root\n"
                b"    property alias testDismissContentItem: dismissWindow.contentItem\n"
                b"    property alias testDismissWindow: dismissWindow\n",
                1,
            )
        elif source.name in ("DesktopEditing.qml", "PopupTransitions.qml"):
            data = (repo / "tests/fixtures" / source.name).read_bytes()
        else:
            data = source.read_bytes()
        entry = tarfile.TarInfo(source.name)
        entry.size = len(data)
        archive.addfile(entry, io.BytesIO(data))
guest("tar", "-C", fixture, "-xf", "-", payload=payload.getvalue())


def ctl(*command):
    result = guest(args.quickshell, "-p", fixture, "ipc", "call", "fixture", *command).decode().strip()
    return json.loads(result) if result else None


sock = socket.socket(socket.AF_UNIX)
sock.settimeout(10)
sock.connect(str(args.run / "qmp.sock"))
stream = sock.makefile("rw")
stream.readline()


def qmp(command, **arguments):
    stream.write(json.dumps({"execute": command, "arguments": arguments}) + "\n")
    stream.flush()
    while True:
        reply = json.loads(stream.readline())
        if "error" in reply:
            raise RuntimeError(reply)
        if "return" in reply:
            return reply["return"]


qmp("qmp_capabilities")
monitors = json.loads(guest("gnoblinctl", "monitor", "list"))["monitors"]
width = max(m["x"] + m["width"] for m in monitors)
height = max(m["y"] + m["height"] for m in monitors)


def click(x, y):
    qmp(
        "input-send-event",
        events=[
            {"type": "abs", "data": {"axis": "x", "value": int(x / width * 32767)}},
            {"type": "abs", "data": {"axis": "y", "value": int(y / height * 32767)}},
        ],
    )
    time.sleep(0.15)
    for down in (True, False):
        qmp("input-send-event", events=[{"type": "btn", "data": {"down": down, "button": "left"}}])
        time.sleep(0.05)
    time.sleep(0.2)


def escape():
    for down in (True, False):
        qmp(
            "input-send-event",
            events=[{"type": "key", "data": {"down": down, "key": {"type": "qcode", "data": "esc"}}}],
        )
    time.sleep(0.5)


def capture():
    return Image.open(io.BytesIO(guest("grim", "-"))).convert("RGB")


def pattern_gain(frame, background, region):
    # Automatic blur reaches full strength at 80% material alpha. Client fades
    # therefore cannot be compared with a linear blend of settled screenshots.
    source = list(background.crop(region).convert("L").getdata())
    actual = list(frame.crop(region).convert("L").getdata())
    mean_source = sum(source) / len(source)
    mean_actual = sum(actual) / len(actual)
    return sum((a - mean_actual) * (b - mean_source) for a, b in zip(actual, source)) / sum(
        (b - mean_source) ** 2 for b in source
    )


try:
    guest(args.quickshell, "-d", "-p", fixture, "--no-color")
    time.sleep(0.5)
    baseline = capture()
    artifact = args.run / "popup-cache-proof"
    artifact.mkdir(exist_ok=True)
    for cycle in range(args.cycles):
        ctl("open", "main")
        time.sleep(0.4)
        if cycle == 0:
            ctl("remember")
        state = ctl("status")
        assert state["open"] and state["opacity"] == 1 and state["sameCard"] and state["sameDismiss"], state
        count = state["cardPresses"]
        click(250, 220)
        assert ctl("status")["cardPresses"] == count + 1, "Cached card missed a real click"
        escape()
        state = ctl("status")
        assert not any(
            state[key] for key in ("open", "drawn", "inputWidth", "inputHeight", "dismissWidth", "keyboard")
        ), state
        count = state["backgroundPresses"]
        click(250, 220)
        assert ctl("status")["backgroundPresses"] == count + 1, "Closed popup blocked click-through"
        ctl("open", "other")
        time.sleep(0.3)
        click(1100, 500)
        assert not ctl("status")["other"], "Outside press did not dismiss the other popup"
    ctl("open", "main")
    ctl("open", "child")
    time.sleep(0.4)
    click(250, 275)
    assert ctl("status")["childPresses"] == 1, "Inline child was blocked by a cached dismissal surface"
    escape()
    assert not ctl("status")["child"], "Child menu missed Escape"
    ctl("close")
    time.sleep(0.5)
    closed = capture()
    area = (160, 140, 500, 390)
    difference = ImageStat.Stat(ImageChops.difference(baseline.crop(area), closed.crop(area))).mean
    assert max(difference) < 0.2, f"Closed popup left pixels or blur: {difference}"
    print("PASS: repeated real input, inline child menu and clean closed pixels", flush=True)
    ctl("slow")
    ctl("open", "main")
    time.sleep(1.4)
    settled = capture()
    baseline.save(artifact / "before.png")
    settled.save(artifact / "settled.png")
    interior = (195, 175, 430, 325)
    assert max(ImageStat.Stat(ImageChops.difference(settled, baseline).crop(interior)).mean) > 5, (
        "Fade check needs a visible panel"
    )
    ctl("close")
    samples = []
    material_region = (200, 285, 430, 320)
    settled_gain = pattern_gain(settled, baseline, material_region)
    unblurred_gain = 1 - ctl("status")["materialAlpha"]
    assert settled_gain < unblurred_gain * 0.6, f"Settled panel did not blur the checkerboard: {settled_gain}"
    for index in range(3):
        time.sleep(0.15)
        frame = capture()
        frame.save(artifact / f"fade-{index}.png")
        samples.append(pattern_gain(frame, baseline, material_region))
    assert settled_gain < samples[0] < 0.95 and samples[-1] > samples[0] + 0.1, (
        f"Background contrast did not recover during the blur fade: {samples}"
    )
    time.sleep(1.3)
    ctl("open", "main")
    time.sleep(1.4)
    ctl("close")
    time.sleep(0.15)
    state = ctl("status")
    assert 0 < state["opacity"] < 1 and not state["inputWidth"], state
    ctl("open", "main")
    time.sleep(1.4)
    state = ctl("status")
    assert state["open"] and state["opacity"] == 1 and state["sameCard"], state
    escape()
    time.sleep(1.3)
    print(f"PASS: automatic blur fade contrast {samples}, interrupted-close reopen", flush=True)
    if len(monitors) > 1:
        ctl("screen", "1")
        ctl("open", "main")
        time.sleep(1.4)
        state = ctl("status")
        count = state["cardPresses"]
        click(monitors[1]["x"] + 250, monitors[1]["y"] + 220)
        assert ctl("status")["cardPresses"] == count + 1, "Moved popup missed clicks on the second screen"
        escape()
        time.sleep(1.3)
        ctl("screen", "0")
    # History can be the first presentation, with no toast-created window.
    ctl("open", "history")
    time.sleep(0.6)
    state = ctl("status")
    assert state["history"] and state["notificationDrawn"] and state["notificationInput"], state
    escape()
    state = ctl("status")
    assert not any(
        state[key]
        for key in ("history", "historyRetained", "notificationDrawn", "notificationInput", "notificationKeyboard")
    ), state
    ctl("notify")
    time.sleep(0.6)
    state = ctl("status")
    assert state["notifications"] == 1 and state["renderedNotifications"] == 1, state
    ctl("open", "history")
    time.sleep(0.6)
    ctl("clear")
    time.sleep(0.6)
    state = ctl("status")
    assert not any(
        state[key]
        for key in (
            "notifications",
            "history",
            "notificationPresented",
            "notificationDrawn",
            "notificationInput",
            "notificationKeyboard",
        )
    ), state
    ctl("notify")
    time.sleep(0.6)
    assert ctl("status")["renderedNotifications"] == 1, "Cached notification surface did not show a new toast"
    ctl("clear")
    time.sleep(0.6)
    print("PASS: first empty history, Escape, toast, clear and new toast", flush=True)
    baseline.save(artifact / "before.png")
    closed.save(artifact / "closed.png")
    print(
        f"PASS: {args.cycles} reopen cycles, real clicks, Escape, outside dismissal, click-through, child menu and clean closed pixels"
    )
    print(f"Screen move tested: {len(monitors) > 1}; closed pixel difference: {difference}")
finally:
    try:
        # Graceful Qt.quit/IPC kill can block in graphics teardown. Terminate
        # only the process launched with this unique temporary fixture path.
        pids = guest("pgrep", "-f", f"^.*quickshell -d -p {fixture} --no-color$").decode().split()
        guest("kill", "-TERM", *pids)
    finally:
        stream.close()
        sock.close()
