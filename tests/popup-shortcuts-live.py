"""Opt-in live test: Super release and direct popup shortcuts."""
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
        time.sleep(.05)
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
    time.sleep(.2)
    for _ in range(2):
        key(65515, True)  # Super_L
        time.sleep(.4)
        key(65515, False)
        wait_for(lambda: ctl("search", "status")["open"])
        key(65515, True)
        time.sleep(.2)
        key(65515, False)
        wait_for(lambda: not ctl("search", "status")["open"])
        time.sleep(.2)
    key(65515, True)
    time.sleep(.4)
    key(65515, False)
    # Type immediately after release, without waiting for the popup or its IPC.
    for symbol in [97, 98, 65288, 99, 100]:  # ab, Backspace, cd
        key(symbol, True)
        key(symbol, False)
    wait_for(lambda: ctl("ipc", "search", "status")["acceptingKeyboard"])
    wait_for(lambda: ctl("ipc", "search", "status")["query"] == "acd")
    ctl("search", "close")
    wait_for(lambda: not ctl("search", "status")["open"])
    time.sleep(.2)
    key(65515, True)
    key(46, True)  # period
    key(46, False)
    wait_for(lambda: ctl("emoji", "status")["visible"])
    key(65515, False)
    time.sleep(.3)
    assert not ctl("search", "status")["open"], "Super chord release must not open Search"
    ctl("emoji", "close")
    key(65513, True)  # Alt_L
    key(115, True)  # s
    key(115, False)
    key(65513, False)
    wait_for(lambda: ctl("capture", "status")["opened"])
    ctl("capture", "cancel")
    print("PASS: immediate typing survives opening; Super toggles only on release; Emoji and Capture use persistent shortcuts; Super chords do not open Search")
finally:
    for symbol in list(held):
        key(symbol, False)
    call(session, destination + ".Session", "Stop")
