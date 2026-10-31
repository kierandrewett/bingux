#!/usr/bin/python3
"""Synthetic keyboard and pointer input for an isolated session.

Runs inside the sandbox, started by `Session` with the session's own D-Bus
address. It opens one `org.gnome.Mutter.RemoteDesktop` session (Gnoblin's
compositor is Mutter) and keeps it open, because the session ends when the
connection that created it closes. The harness writes one JSON object a line
on stdin and reads one JSON reply a line on stdout:

    {"op": "key", "keysym": 65293, "down": true}
    {"op": "button", "button": 272, "down": true}
    {"op": "rel", "dx": 10.0, "dy": 0.0}
    {"op": "warp", "x": 640, "y": 360}
    {"op": "axis", "dx": 0.0, "dy": 15.0}

`warp` has no absolute coordinates to use: a RemoteDesktop session without a
screen-cast stream takes only relative motion. It clamps the pointer into the
top-left corner of the stage with a very large negative move, then moves by the
target offset. The stage must therefore start at (0, 0), which holds for the
single virtual monitor the session runs.

Needs PyGObject. It is a test instrument: it is not part of the desktop under
test and does not use anything from Bingux.
"""

import json
import sys

from gi.repository import Gio, GLib

DESTINATION = "org.gnome.Mutter.RemoteDesktop"
SESSION_INTERFACE = "org.gnome.Mutter.RemoteDesktop.Session"
CLAMP = -20000.0


def main() -> int:
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

    def call(path, interface, method, args=None):
        return bus.call_sync(DESTINATION, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 10000, None)

    session = call("/org/gnome/Mutter/RemoteDesktop", DESTINATION, "CreateSession").unpack()[0]
    call(session, SESSION_INTERFACE, "Start")

    def notify(method, signature, *values):
        call(session, SESSION_INTERFACE, method, GLib.Variant(signature, values))

    handlers = {
        "key": lambda m: notify("NotifyKeyboardKeysym", "(ub)", m["keysym"], m["down"]),
        "keycode": lambda m: notify("NotifyKeyboardKeycode", "(ub)", m["keycode"], m["down"]),
        "button": lambda m: notify("NotifyPointerButton", "(ib)", m["button"], m["down"]),
        "rel": lambda m: notify("NotifyPointerMotionRelative", "(dd)", float(m["dx"]), float(m["dy"])),
        "axis": lambda m: notify("NotifyPointerAxis", "(ddu)", float(m["dx"]), float(m["dy"]), m.get("flags", 0)),
    }

    def warp(message):
        notify("NotifyPointerMotionRelative", "(dd)", CLAMP, CLAMP)
        notify("NotifyPointerMotionRelative", "(dd)", float(message["x"]), float(message["y"]))

    handlers["warp"] = warp

    print(json.dumps({"ok": True, "ready": True}), flush=True)
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
            handlers[message["op"]](message)
            reply = {"ok": True}
        except Exception as error:  # reported to the harness, which raises it in the test
            reply = {"ok": False, "error": f"{type(error).__name__}: {error}"}
        print(json.dumps(reply), flush=True)
    try:
        call(session, SESSION_INTERFACE, "Stop")
    except GLib.Error:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
