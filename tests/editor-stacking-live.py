#!/usr/bin/env python3
"""Check retained editor focus and visible companions on the disposable VM seat.

Requires one monitor with the sidebar on the right, foot and an installed Bingux
shell. Preview changes are cancelled; settings are not saved.
"""

import argparse
import json
from pathlib import Path
import socket
import subprocess
import time

from PIL import Image, ImageChops, ImageStat

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("runner", type=Path)
parser.add_argument("run", type=Path)
parser.add_argument("--quickshell", required=True)
parser.add_argument("--shell", default="/home/luna/.local/share/bingux/share/bingux/shell")
parser.add_argument("--report", type=Path, required=True)
parser.add_argument("--cycles", type=int, default=3)
parser.add_argument("--fullscreen", action="store_true")
parser.add_argument("--keyboard-first", action="store_true", help="Close cold options with Escape before clicking")
args = parser.parse_args()
assert 1 <= args.cycles <= 20
args.report.mkdir(parents=True, exist_ok=False)
env = [
    "env",
    "XDG_RUNTIME_DIR=/run/user/1000",
    "WAYLAND_DISPLAY=wayland-0",
    "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus",
]
control = "/usr/local/lib/gnoblin/bin/gnoblinctl"
rows = []
unit = "editor-stack-" + str(time.time_ns())
unit_started = False
backup = "/tmp/" + unit + ".qml"
sock = None
stream = None


def guest(*command):
    return subprocess.check_output([str(args.runner), "guest", str(args.run), "--", *env, *command], timeout=20)


def ctl(*command):
    return json.loads(guest(control, *command))


def ipc(target, method, *argv):
    reply = guest(args.quickshell, "ipc", "-p", args.shell, "call", target, method, *argv)
    return json.loads(reply) if reply.strip() else None


def wait(predicate, message):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError(message)


monitors = ctl("monitor", "list")["monitors"]
assert len(monitors) == 1 and monitors[0]["width"] > 0 and monitors[0]["height"] > 0
screen_width = monitors[0]["width"]
screen_height = monitors[0]["height"]
assert ipc("sidebar", "status")["edge"] == "right"
initial_shell = ipc("shell", "status")
assert not initial_shell["controls"], "Start with desktop panels closed"
assert not initial_shell["loadedPanels"]["controls"], "Control Centre must be cold before this lazy-load check"
# Add a private probe to the installed VM shell, then reload that exact shell.
# Restore its source on exit; no diagnostic IPC is shipped to users.
probe = r"""
    IpcHandler {
        target: "editor-stack-test"
        function status(): string {
            const editor = root.customiserObject;
            function find(item, match) {
                if (match(item)) return item;
                for (const child of item.children || []) { const found = find(child, match); if (found) return found; }
                return null;
            }
            const inspector = editor ? find(editor.optionsContentItem, item => item.objectName === "customiseInspector") : null;
            const button = inspector ? find(inspector, item => item.label === "Close options") : null;
            return JSON.stringify({visible: editor?.visible ?? false, options: editor?.optionsPage ?? "",
                inspectorReady: editor?.inspectorReady ?? false,
                mainMapped: editor?.nativeWindow.visible ?? false, inspectorMapped: editor?.optionsWindow.visible ?? false,
                inspector: inspector ? {visible: inspector.visible, x: inspector.x, y: inspector.y, width: inspector.width, height: inspector.height} : null,
                closeHovered: button?.hovered ?? false, close: button ? DesktopEditing.point(button, editor.optionsWindow, button.width / 2, button.height / 2) : null});
        }
    }
"""


def qmp(command, **arguments):
    if stream is None:
        raise RuntimeError("QMP is not connected")
    stream.write(json.dumps({"execute": command, "arguments": arguments}) + "\n")
    stream.flush()
    while True:
        reply = json.loads(stream.readline())
        if "error" in reply:
            raise RuntimeError(reply)
        if "return" in reply:
            return reply["return"]


def key(code):
    for down in (True, False):
        qmp(
            "input-send-event", events=[{"type": "key", "data": {"down": down, "key": {"type": "qcode", "data": code}}}]
        )
        time.sleep(0.05)


def move(x, y):
    assert 0 <= x < screen_width and 0 <= y < screen_height
    qmp(
        "input-send-event",
        events=[{"type": "abs", "data": {"axis": "x", "value": int((x + 1) / screen_width * 32767)}}],
    )
    qmp(
        "input-send-event",
        events=[
            {"type": "abs", "data": {"axis": "x", "value": int(x / screen_width * 32767)}},
            {"type": "abs", "data": {"axis": "y", "value": int(y / screen_height * 32767)}},
        ],
    )
    time.sleep(0.1)


def click(x, y):
    move(x, y)
    for down in (True, False):
        qmp("input-send-event", events=[{"type": "btn", "data": {"down": down, "button": "left"}}])
        time.sleep(0.04)


def capture(label):
    time.sleep(2)
    raw = args.report / (label + ".ppm")
    qmp("screendump", filename=str(raw.resolve()))
    image = Image.open(raw).convert("RGB")
    image.save(args.report / (label + ".png"))
    raw.unlink()
    rows.append({"label": label, "shell": ipc("shell", "status"), "layers": ctl("layer", "list")})
    (args.report / "timeline.json").write_text(json.dumps(rows, indent=2))
    return image


def fullscreen_window():
    return next((w for w in ctl("window", "list")["windows"] if w["title"] == unit), None)


def crop(fractions):
    left, top, right, bottom = fractions
    return (
        round(screen_width * left),
        round(screen_height * top),
        round(screen_width * right),
        round(screen_height * bottom),
    )


sidebar_crop = crop((1045 / 1280, 60 / 800, 1260 / 1280, 650 / 800))
cards_crop = crop((610 / 1280, 230 / 800, 980 / 1280, 640 / 800))
background_press = (round(screen_width * 310 / 1280), round(screen_height * 520 / 800))


def assert_companions_visible(image, label):
    bright = sum(min(pixel) > 100 for pixel in image.crop(cards_crop).getdata())
    assert bright > 150, f"Control Centre content disappeared after {label}: {bright}"


def inspector_closed():
    status = ipc("editor-stack-test", "status")
    return status if status["options"] == "" and not status["inspectorReady"] else None


primary_failure = None
try:
    guest(
        "python3",
        "-c",
        "from pathlib import Path; p=Path("
        + repr(args.shell + "/shell.qml")
        + "); b=Path("
        + repr(backup)
        + "); b.write_bytes(p.read_bytes()); p.write_text(p.read_text().rstrip()[:-1] + "
        + repr(probe)
        + " + "
        + repr("\n}\n")
        + ")",
    )
    ipc("shell", "reload")
    wait(lambda: ipc("editor-stack-test", "status"), "Private editor probe did not load")
    sock = socket.socket(socket.AF_UNIX)
    sock.settimeout(10)
    sock.connect(str(args.run / "qmp.sock"))
    stream = sock.makefile("rw")
    stream.readline()
    qmp("qmp_capabilities")
    if args.fullscreen:
        guest(
            "systemd-run",
            "--user",
            "--unit=" + unit,
            "--collect",
            "/usr/bin/foot",
            "-T",
            unit,
            "--",
            "sleep",
            str(max(180, args.cycles * 30 + 30)),
        )
        unit_started = True
        window = wait(fullscreen_window, "Fullscreen fixture did not map")
        ctl("window", "fullscreen", window["id"])
        wait(lambda: fullscreen_window()["fullscreen"], "Fixture did not become fullscreen")
    for cycle in range(args.cycles):
        ipc("shell", "customiseContainer", "control-centre")
        wait(lambda: ipc("shell", "status")["controls"], "Editor did not open Control Centre")
        opened = capture(f"{cycle}-inspector")
        state = ipc("editor-stack-test", "status")
        assert state["visible"] and state["options"] and state["inspector"]["visible"], state
        assert state["close"], "Inspector close control was not laid out"
        # Closing the options surface must return keyboard input without raising
        # the editor wallpaper over the already-mapped sidebar and card.
        keyboard_close = bool(cycle % 2 or (args.keyboard_first and cycle == 0))
        if keyboard_close:
            key("esc")
        else:
            print(f"Inspector close geometry: {state['close']}", flush=True)
            move(state["close"]["x"], state["close"]["y"])
            wait(
                lambda: ipc("editor-stack-test", "status")["closeHovered"],
                "Pointer did not reach the visible close control",
            )
            click(state["close"]["x"], state["close"]["y"])
        wait(inspector_closed, "Visible inspector did not release Exclusive keyboard focus")
        closed = capture(f"{cycle}-closed-inspector")
        r = state["inspector"]
        area = (int(r["x"] + 16), int(r["y"] + 16), int(r["x"] + r["width"] - 16), int(r["y"] + r["height"] - 16))
        delta = ImageStat.Stat(ImageChops.difference(opened.crop(area), closed.crop(area))).mean
        assert max(delta) > 2, f"Inspector was reported open but its pixels were covered: {delta}"
        # A background press must not raise the editor above native containers.
        click(*background_press)
        assert ipc("shell", "status")["controls"]
        after_background = capture(f"{cycle}-background-press")
        difference = ImageStat.Stat(
            ImageChops.difference(closed.crop(sidebar_crop), after_background.crop(sidebar_crop))
        ).mean
        assert max(difference) < 4, f"Inspector close covered the visible sidebar: {difference}"
        # This crop contains card labels, not the inspector or pointer.
        assert_companions_visible(closed, "closing the inspector")
        assert_companions_visible(after_background, "a background press")
        key("esc")
        wait(
            lambda: not ipc("editor-stack-test", "status")["visible"], "Escape did not return to the editor and cancel"
        )
        assert not ipc("shell", "status")["controls"]
        hidden = ipc("editor-stack-test", "status")
        assert not hidden["mainMapped"] and not hidden["inspectorMapped"], (
            "Closed editor retained mapped fullscreen surfaces: " + str(hidden)
        )
        capture(f"{cycle}-cancelled")
        if args.fullscreen:
            assert fullscreen_window()["fullscreen"], "Editing changed the application fullscreen state"
        print(
            f"PASS cycle {cycle + 1}: {'Escape' if keyboard_close else 'mouse'} options close, "
            "companion pixels and cancellation",
            flush=True,
        )
except BaseException as error:
    primary_failure = error
    raise
finally:
    cleanup_failures = []

    def cleanup(label, action):
        try:
            action()
        except Exception as error:
            cleanup_failures.append(f"{label}: {error}")

    if stream is not None:
        cleanup("first Escape", lambda: key("esc"))
        cleanup("second Escape", lambda: key("esc"))
    if unit_started:
        cleanup("fixture stop", lambda: guest("systemctl", "--user", "stop", unit + ".service"))
    cleanup(
        "source restore",
        lambda: guest(
            "python3",
            "-c",
            "from pathlib import Path; p=Path("
            + repr(args.shell + "/shell.qml")
            + "); b=Path("
            + repr(backup)
            + ");\nif b.exists():\n p.write_bytes(b.read_bytes())\n b.unlink()",
        ),
    )
    cleanup("shell reload after restore", lambda: ipc("shell", "reload"))
    if stream is not None:
        cleanup("QMP stream close", stream.close)
    if sock is not None:
        cleanup("QMP socket close", sock.close)
    if cleanup_failures:
        message = "Cleanup failures: " + "; ".join(cleanup_failures)
        if primary_failure is None:
            raise RuntimeError(message)
        print(message, flush=True)
