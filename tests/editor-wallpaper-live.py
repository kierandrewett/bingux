#!/usr/bin/env python3
"""Check bounded editor wallpaper decoding in a disposable Gnoblin VM seat.

The caller installs the candidate shell before running this test. It adds a
private object name and IPC probe to that installed copy, reloads it, then
restores both files byte-for-byte. Preview wallpaper changes are cancelled and
never saved to the user's settings.
"""

import argparse
import json
import math
from pathlib import Path
import socket
import subprocess
import time

from PIL import Image


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("runner", type=Path)
parser.add_argument("run", type=Path)
parser.add_argument("--quickshell", required=True)
parser.add_argument("--shell", default="/home/luna/.local/share/bingux/share/bingux/shell")
parser.add_argument("--report", type=Path, required=True)
parser.add_argument("--expected-dpr", type=float, default=1)
args = parser.parse_args()
args.report.mkdir(parents=True, exist_ok=False)

env = [
    "env",
    "XDG_RUNTIME_DIR=/run/user/1000",
    "WAYLAND_DISPLAY=wayland-0",
    "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus",
]
unit = "editor-wallpaper-" + str(time.time_ns())
desktop_source = args.shell + "/DesktopCustomise.qml"
shell_source = args.shell + "/shell.qml"
desktop_backup = "/tmp/" + unit + "-DesktopCustomise.qml"
shell_backup = "/tmp/" + unit + "-shell.qml"
sock = None
stream = None


def guest(*command):
    return subprocess.check_output([str(args.runner), "guest", str(args.run), "--", *env, *command], timeout=30)


def ipc(target, method, *argv):
    reply = guest(args.quickshell, "ipc", "-p", args.shell, "call", target, method, *argv)
    return json.loads(reply) if method == "status" else None


def wait(predicate, message):
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError(message)


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


probe = r"""
    IpcHandler {
        target: "editor-wallpaper-test"
        function find(item, objectName) {
            if (!item) return null;
            if (item.objectName === objectName) return item;
            for (const child of item.children || []) {
                const found = find(child, objectName);
                if (found) return found;
            }
            return null;
        }
        function open(): void {
            root.customiserItem().open();
        }
        function setWallpaper(url: string): void {
            root.customiserItem().wallpaper = url;
        }
        function cancel(): void {
            root.customiserItem().cancel();
        }
        function status(): string {
            const editor = root.customiserObject;
            const image = editor ? find(editor.contentItem, "editor-wallpaper-live-probe") : null;
            return JSON.stringify({
                visible: editor?.visible ?? false,
                mapped: editor?.nativeWindow.visible ?? false,
                canvas: editor?.preview ? {width: editor.preview.width, height: editor.preview.height} : null,
                image: image ? {
                    source: image.source.toString(), status: image.status,
                    ready: image.status === Image.Ready,
                    dpr: image.probeDpr,
                    sourceWidth: image.sourceSize.width, sourceHeight: image.sourceSize.height,
                    implicitWidth: image.implicitWidth, implicitHeight: image.implicitHeight
                } : null
            });
        }
    }
"""

# Produce solid 8K PNG fixtures inside the guest without adding a package
# dependency. Their differing aspect ratios expose sourceSize's crop behavior.
png_writer = r"""
from pathlib import Path
import struct
import zlib
def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
def image(path, width, height, rgb):
    row = bytes([0]) + bytes(rgb) * width
    data = zlib.compress(row * height, 9)
    Path(path).write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) + chunk(b"IDAT", data) + chunk(b"IEND", b""))
image("/tmp/EDITOR_WALLPAPER_FIXTURE/landscape.png", 7680, 4320, (233, 78, 78))
image("/tmp/EDITOR_WALLPAPER_FIXTURE/portrait.png", 4320, 7680, (78, 121, 233))
""".replace("EDITOR_WALLPAPER_FIXTURE", unit)


def instrument():
    script = (
        "from pathlib import Path; "
        f"desktop=Path({desktop_source!r}); shell=Path({shell_source!r}); "
        f"desktop_backup=Path({desktop_backup!r}); shell_backup=Path({shell_backup!r}); "
        "desktop_backup.write_bytes(desktop.read_bytes()); shell_backup.write_bytes(shell.read_bytes()); "
        "text=desktop.read_text(); anchor='            source: canvas.width > 0 && canvas.height > 0 ? root.wallpaper : \\\"\\\"\\n'; "
        "assert text.count(anchor) == 1, 'candidate editor Image anchor is not unique'; "
        "desktop.write_text(text.replace(anchor, '            objectName: \\\"editor-wallpaper-live-probe\\\"\\n            property real probeDpr: Screen.devicePixelRatio\\n' + anchor)); "
        "text=shell.read_text(); assert text.rstrip().endswith('}'); "
        f"shell.write_text(text.rstrip()[:-1] + {probe!r} + '\\n}}\\n')"
    )
    guest("python3", "-c", script)


def restore():
    script = (
        "from pathlib import Path; "
        f"pairs=[(Path({desktop_source!r}), Path({desktop_backup!r})), (Path({shell_source!r}), Path({shell_backup!r}))]; "
        "[(target.write_bytes(backup.read_bytes()), backup.unlink()) for target, backup in pairs if backup.exists()]"
    )
    guest("python3", "-c", script)


def exact(value, expected, label):
    assert value == expected, f"{label}: expected {expected}, got {value}"


def decoded_size(canvas_width, canvas_height, dpr, image_width, image_height):
    scale = max(canvas_width * dpr / image_width, canvas_height * dpr / image_height)
    # Qt rounds the aspect-preserving decoded size to the nearest pixel.
    return math.floor(image_width * scale + 0.5), math.floor(image_height * scale + 0.5)


def capture(label, colour):
    raw = args.report / (label + ".ppm")
    qmp("screendump", filename=str(raw.resolve()))
    image = Image.open(raw).convert("RGB")
    image.save(args.report / (label + ".png"))
    raw.unlink()
    pixel = image.getpixel((max(1, image.width // 100), image.height // 2))
    if colour == "red":
        assert pixel[0] > pixel[1] * 1.5 and pixel[0] > pixel[2] * 1.5, (label, pixel)
    else:
        assert pixel[2] > pixel[0] * 1.5 and pixel[2] > pixel[1] * 1.5, (label, pixel)
    return list(pixel)


def ready_image(url, image_width, image_height, label, colour):
    ipc("editor-wallpaper-test", "setWallpaper", url)
    state = wait(
        lambda: (
            (candidate := ipc("editor-wallpaper-test", "status"))["image"]
            and candidate["image"]["ready"]
            and candidate["image"]["source"] == url
            and candidate
        ),
        label + " wallpaper did not become ready",
    )
    image = state["image"]
    canvas = state["canvas"]
    physical_width = math.ceil(canvas["width"] * image["dpr"])
    physical_height = math.ceil(canvas["height"] * image["dpr"])
    expected_width, expected_height = decoded_size(
        canvas["width"], canvas["height"], image["dpr"], image_width, image_height
    )
    exact(image["sourceWidth"], physical_width, label + " source width")
    exact(image["sourceHeight"], physical_height, label + " source height")
    exact(image["implicitWidth"], expected_width, label + " decoded width")
    exact(image["implicitHeight"], expected_height, label + " decoded height")
    state["backgroundPixel"] = capture(label, colour)
    return state


primary_failure = None
try:
    instrument()
    ipc("shell", "reload")
    wait(lambda: ipc("editor-wallpaper-test", "status"), "private editor wallpaper probe did not load")

    sock = socket.socket(socket.AF_UNIX)
    sock.settimeout(10)
    sock.connect(str(args.run / "qmp.sock"))
    stream = sock.makefile("rw")
    stream.readline()
    qmp("qmp_capabilities")
    assert qmp("query-status")["running"], "VM is not running"

    guest("mkdir", "-p", "/tmp/" + unit)
    guest("python3", "-c", png_writer)
    landscape = "file:///tmp/" + unit + "/landscape.png"
    portrait = "file:///tmp/" + unit + "/portrait.png"

    ipc("editor-wallpaper-test", "open")
    opened = wait(
        lambda: (
            (state := ipc("editor-wallpaper-test", "status"))["visible"]
            and state["mapped"]
            and state["canvas"]
            and state
        ),
        "editor did not open and map",
    )
    canvas = opened["canvas"]
    assert opened["image"], opened
    assert abs(opened["image"]["dpr"] - args.expected_dpr) < 0.01, opened
    physical_width = math.ceil(canvas["width"] * opened["image"]["dpr"])
    physical_height = math.ceil(canvas["height"] * opened["image"]["dpr"])
    assert physical_width > 0 and physical_height > 0, opened

    evidence = {
        "canvas": canvas,
        "expectedDpr": args.expected_dpr,
        "landscape": ready_image(landscape, 7680, 4320, "landscape", "red"),
        "portrait": ready_image(portrait, 4320, 7680, "portrait", "blue"),
    }
    ipc("editor-wallpaper-test", "cancel")
    hidden = wait(
        lambda: (
            (state := ipc("editor-wallpaper-test", "status"))["visible"] is False and state["mapped"] is False and state
        ),
        "editor cancel did not unmap the native window",
    )
    evidence["cancelled"] = hidden
    (args.report / "editor-wallpaper.json").write_text(json.dumps(evidence, indent=2))
    print("PASS: mapped editor bounded 8K landscape/portrait decode and cancelled unmap", json.dumps(evidence))
except BaseException as error:
    primary_failure = error
    try:
        (args.report / "failure.txt").write_text(str(error) + "\n")
    except OSError:
        pass
    raise
finally:
    cleanup_failures = []

    def cleanup(label, action):
        try:
            action()
        except Exception as error:
            cleanup_failures.append(f"{label}: {error}")

    cleanup("cancel editor", lambda: ipc("editor-wallpaper-test", "cancel"))
    cleanup("restore installed sources", restore)
    cleanup("reload restored shell", lambda: ipc("shell", "reload"))
    cleanup("remove guest fixtures", lambda: guest("rm", "-rf", "/tmp/" + unit))
    if stream is not None:
        cleanup("close QMP stream", stream.close)
    if sock is not None:
        cleanup("close QMP socket", sock.close)
    if cleanup_failures:
        message = "Cleanup failures: " + "; ".join(cleanup_failures)
        if primary_failure is None:
            raise RuntimeError(message)
        print(message)
