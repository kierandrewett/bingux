"""Opt-in real compositor pointer test. No screenshot is saved."""
import json
from pathlib import Path
import subprocess
import time
from gi.repository import Gio, GLib

shell = Path(__file__).resolve().parents[1] / "shell/bingux"
command = ["qs", "ipc", "--any-display", "-p", str(shell), "call", "capture"]
def ipc(method):
    return subprocess.check_output(command + [method], text=True, timeout=2)
def status():
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        try: return json.loads(ipc("status"))
        except ValueError: time.sleep(.08)
    raise AssertionError("Capture IPC unavailable")

assert not status()["opened"] and status()["state"] in ("idle", "saved", "error")
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
destination = "org.gnome.Mutter.RemoteDesktop"
def call(path, interface, method, args=None):
    return bus.call_sync(destination, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 3000, None)
session = call("/org/gnome/Mutter/RemoteDesktop", destination, "CreateSession").unpack()[0]
def move(x, y):
    call(session, destination + ".Session", "NotifyPointerMotionRelative", GLib.Variant("(dd)", (x, y)))
    time.sleep(.04)
def button(down):
    call(session, destination + ".Session", "NotifyPointerButton", GLib.Variant("(ib)", (272, down)))
    time.sleep(.04)
try:
    call(session, destination + ".Session", "Start")
    ipc("open")
    deadline = time.monotonic() + 3
    while not status()["opened"] and time.monotonic() < deadline: time.sleep(.04)
    assert status()["opened"]
    move(-10000, -10000)
    move(0, 100)
    button(True)
    move(10000, 0)
    selected = status()
    assert selected["dragging"], selected
    assert selected["region"]["x"] == 0 and selected["region"]["width"] == selected["screen"]["width"], selected
    button(False)
    print("PASS: real pointer draws from sidebar edge to final screen column without losing grab")
finally:
    button(False)
    call(session, destination + ".Session", "Stop")
    ipc("cancel")
