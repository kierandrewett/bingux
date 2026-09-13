"""Opt-in live regression: Super must focus Search without a pointer click."""

import json
import subprocess
import time
from gi.repository import Gio, GLib


def ctl(*args):
    return json.loads(subprocess.check_output(["binguxctl", *args], text=True, timeout=5) or "{}")


def wait_for(predicate):
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.05)
    raise AssertionError("Popup did not reach its expected state")


bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
destination = "org.gnome.Mutter.RemoteDesktop"


def call(path, interface, method, args=None):
    return bus.call_sync(destination, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 3000, None)


session = call("/org/gnome/Mutter/RemoteDesktop", destination, "CreateSession").unpack()[0]
held = set()


def key(symbol, down):
    call(session, destination + ".Session", "NotifyKeyboardKeysym", GLib.Variant("(ub)", (symbol, down)))
    if down:
        held.add(symbol)
    else:
        held.discard(symbol)


try:
    call(session, destination + ".Session", "Start")
    ctl("search", "close")
    ctl("emoji", "close")
    wait_for(lambda: not ctl("search", "status")["open"])
    time.sleep(0.2)
    key(65515, True)
    time.sleep(0.4)
    key(65515, False)
    wait_for(lambda: ctl("ipc", "search", "status")["acceptingKeyboard"])
    # Send native key events without clicking the search field.
    for symbol in [97, 98, 65288, 99, 100]:  # ab, Backspace, cd
        key(symbol, True)
        key(symbol, False)
    wait_for(lambda: ctl("ipc", "search", "status")["acceptingKeyboard"])
    wait_for(lambda: ctl("ipc", "search", "status")["query"] == "acd")
    print("PASS: Super focuses search and accepts typing and Backspace without a click")
    ctl("search", "close")
    wait_for(lambda: not ctl("search", "status")["open"])
    time.sleep(0.2)
finally:
    for symbol in list(held):
        key(symbol, False)
    call(session, destination + ".Session", "Stop")
