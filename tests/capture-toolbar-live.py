"""Opt-in live test for capture toolbar hit testing and default image copying."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from gi.repository import Gio, GLib

ctl = os.environ.get("BINGUXCTL", "binguxctl")
if not (Path(ctl).is_file() or shutil.which(ctl)):
    raise SystemExit("Set BINGUXCTL to the installed binguxctl executable")


def run(*args, timeout=5):
    return subprocess.check_output([ctl, "capture", *args], text=True, timeout=timeout)


def status():
    return json.loads(run("status"))


initial = status()
if initial["opened"] or initial["state"] not in ("idle", "saved", "error"):
    raise SystemExit("Leave the user's active capture untouched")
options = json.loads(run("options"))
if options.get("copy") is not True:
    raise AssertionError("Screenshot copying is disabled in the current capture preferences")
original_target = options["target"]

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
destination = "org.gnome.Mutter.RemoteDesktop"


def call(path, interface, method, args=None):
    return bus.call_sync(destination, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 3000, None)


session = call("/org/gnome/Mutter/RemoteDesktop", destination, "CreateSession").unpack()[0]
session_interface = destination + ".Session"


def click(x, y):
    call(session, session_interface, "NotifyPointerMotionRelative", GLib.Variant("(dd)", (-10000.0, -10000.0)))
    time.sleep(0.1)
    call(session, session_interface, "NotifyPointerMotionRelative", GLib.Variant("(dd)", (x, y)))
    time.sleep(0.1)
    call(session, session_interface, "NotifyPointerButton", GLib.Variant("(ib)", (0x110, True)))
    time.sleep(0.05)
    call(session, session_interface, "NotifyPointerButton", GLib.Variant("(ib)", (0x110, False)))


try:
    call(session, session_interface, "Start")
    run("configure", "--target", "region")
    run("open")
    deadline = time.monotonic() + 5
    while not status()["opened"] and time.monotonic() < deadline:
        time.sleep(0.05)
    current = status()
    assert current["opened"], current
    # The toolbar finishes its opening layout and input-region update as soon as status reports opened.
    time.sleep(1.0)

    width = current["screen"]["width"]
    height = current["screen"]["height"]
    # The toolbar is centered at the bottom by default; Screen is the middle
    # target, and Capture is the large shutter below it.
    click(width / 2, height - 140)
    deadline = time.monotonic() + 3
    while status()["target"] != "screen" and time.monotonic() < deadline:
        time.sleep(0.05)
    assert status()["target"] == "screen", "Toolbar click fell through to the full-screen preview"
    print("PASS: screen target button receives a real pointer click", flush=True)

    click(width / 2, height - 64)
    deadline = time.monotonic() + 15
    result = status()
    while result["state"] != "saved" and time.monotonic() < deadline:
        time.sleep(0.1)
        result = status()
    assert result["state"] == "saved", result
    assert "Copied to clipboard" in result["message"], result
    path = Path(result["savedPath"])
    image = path.read_bytes()
    assert image.startswith(b"\x89PNG\r\n\x1a\n"), path
    pasted = subprocess.check_output(["wl-paste", "--type", "image/png"], timeout=10)
    assert pasted == image, f"Clipboard image differs from {path}"
    print(f"PASS: screenshot copied by default; clipboard matches {path.stat().st_size} saved PNG bytes", flush=True)
finally:
    try:
        if status()["opened"]:
            run("cancel")
    finally:
        try:
            run("configure", "--target", original_target)
        finally:
            call(session, session_interface, "Stop")
