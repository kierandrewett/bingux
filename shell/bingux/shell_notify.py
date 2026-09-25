#!/usr/bin/env python3
"""Shell messages use the desktop notification server, never private popups."""

import html
import json
from pathlib import Path
import subprocess
import sys

from gi.repository import Gio, GLib

SERVICE = "org.freedesktop.Notifications"
OBJECT = "/org/freedesktop/Notifications"


def notify(payload):
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    loop = GLib.MainLoop()
    identifier = 0
    retry = payload.get("retryDesktopId", "")
    if retry and ("/" in retry or "\0" in retry):
        raise ValueError("Retry requires a desktop entry ID")

    def on_signal(_bus, _sender, _path, _interface, signal, parameters):
        notice, value = parameters.unpack()
        if notice != identifier:
            return
        if signal == "NotificationClosed":
            loop.quit()
        elif value == "retry" and retry:
            subprocess.Popen(
                [sys.executable, str(Path(__file__).with_name("launch-application.py")), "--notify-errors", retry],
                start_new_session=True,
            )
            loop.quit()

    if retry:
        for signal in ("ActionInvoked", "NotificationClosed"):
            bus.signal_subscribe(SERVICE, SERVICE, signal, OBJECT, None, Gio.DBusSignalFlags.NONE, on_signal)
    identifier = bus.call_sync(
        SERVICE, OBJECT, SERVICE, "Notify",
        GLib.Variant("(susssasa{sv}i)", (
            "Bingux", 0, payload.get("icon", "dialog-error-symbolic"),
            str(payload["title"]), html.escape(str(payload.get("body", ""))),
            ["retry", "Retry"] if retry else [],
            {"category": GLib.Variant("s", "device.error")}, -1,
        )), None, Gio.DBusCallFlags.NONE, 3000, None,
    ).unpack()[0]
    if retry:
        loop.run()
    return identifier


if __name__ == "__main__":
    try:
        notify(json.loads(sys.argv[1]))
    except (GLib.Error, ValueError, KeyError, OSError) as error:
        print(f"Desktop notification failed: {error}", file=sys.stderr)
        sys.exit(1)
