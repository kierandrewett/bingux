"""Opt-in: exercise real Alt+S press/hold/release in an idle desktop session."""
import json
from pathlib import Path
import subprocess
import time
from gi.repository import Gio, GLib

shell = Path(__file__).resolve().parents[1] / "shell/bingux"
command = ["qs", "ipc", "--any-display", "-p", str(shell), "call", "capture"]


def status():
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        result = subprocess.check_output(command + ["status"], text=True, timeout=2)
        try: return json.loads(result)
        except ValueError: time.sleep(.05)
    raise RuntimeError("Capture IPC unavailable")


initial = status()
assert not initial["opened"] and initial["state"] in ("idle", "saved", "error"), "Leave the user's active capture untouched"
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
destination = "org.gnome.Mutter.RemoteDesktop"


def call(path, interface, method, args=None):
    return bus.call_sync(destination, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 3000, None)


session = call("/org/gnome/Mutter/RemoteDesktop", destination, "CreateSession").unpack()[0]


def key(code, pressed):
    call(session, destination + ".Session", "NotifyKeyboardKeycode", GLib.Variant("(ub)", (code, pressed)))


def press(hold):
    key(56, True)
    key(31, True)
    time.sleep(hold)
    key(31, False)
    key(56, False)


try:
    call(session, destination + ".Session", "Start")
    for panel in ["search", "calendar", "controls"] * 3:
        subprocess.run(command[:-1] + ["shell", panel], check=True, timeout=3)
        time.sleep(.25)
        press(.08)
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline and not status()["opened"]: time.sleep(.03)
        assert status()["opened"], f"Alt+S failed with {panel} focused"
        subprocess.run(command + ["cancel"], check=True, timeout=3)
        print(f"PASS: Alt+S over {panel}", flush=True)
        time.sleep(.2)
    for hold in [.04, .5, .8, 1.2] * 3:
        press(hold)
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline and not status()["opened"]: time.sleep(.03)
        assert status()["opened"], f"Alt+S held for {hold}s did not open capture"
        press(.04)
        deadline = time.monotonic() + 1
        while time.monotonic() < deadline and status()["opened"]: time.sleep(.03)
        assert not status()["opened"], "Second physical press did not close capture"
        print(f"PASS: hold={hold}s; one press opens, next press closes", flush=True)
        time.sleep(.08)
finally:
    key(31, False)
    key(56, False)
    call(session, destination + ".Session", "Stop")
    subprocess.run(command + ["cancel"], check=True, timeout=3)
