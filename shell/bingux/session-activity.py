#!/usr/bin/env python3
"""Report AFK transitions from Mutter's session-wide idle monitor."""

import json
import sys

from gi.repository import Gio, GLib


BUS_NAME = "org.gnome.Mutter.IdleMonitor"
OBJECT_PATH = "/org/gnome/Mutter/IdleMonitor/Core"
INTERFACE = "org.gnome.Mutter.IdleMonitor"
DEFAULT_IDLE_THRESHOLD_MS = 2 * 60 * 1000


def main():
    threshold_ms = int(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_IDLE_THRESHOLD_MS
    if threshold_ms <= 0:
        raise ValueError("idle threshold must be positive")

    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    loop = GLib.MainLoop()
    watch_ids = {"idle": None, "active": None}
    current_afk = None

    def call(method, parameters, result_signature):
        reply = connection.call_sync(
            BUS_NAME,
            OBJECT_PATH,
            INTERFACE,
            method,
            parameters,
            GLib.VariantType.new(result_signature),
            Gio.DBusCallFlags.NONE,
            5000,
            None,
        )
        return reply.unpack()

    def publish(afk):
        nonlocal current_afk
        if current_afk == afk:
            return
        current_afk = afk
        print(json.dumps({"afk": afk}), flush=True)

    def add_active_watch():
        watch_ids["active"] = call("AddUserActiveWatch", GLib.Variant("()", ()), "(u)")[0]

    def on_watch_fired(_connection, _sender, _path, _interface, _signal, parameters):
        watch_id = parameters.unpack()[0]
        if watch_id == watch_ids["idle"]:
            publish(True)
        elif watch_id == watch_ids["active"]:
            watch_ids["active"] = None
            publish(False)
            add_active_watch()

    connection.signal_subscribe(
        BUS_NAME,
        INTERFACE,
        "WatchFired",
        OBJECT_PATH,
        None,
        Gio.DBusSignalFlags.NONE,
        on_watch_fired,
    )

    watch_ids["idle"] = call(
        "AddIdleWatch", GLib.Variant("(t)", (threshold_ms,)), "(u)"
    )[0]
    add_active_watch()
    idle_ms = call("GetIdletime", GLib.Variant("()", ()), "(t)")[0]
    publish(idle_ms >= threshold_ms)

    connection.connect("closed", lambda *_args: loop.quit())
    loop.run()


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"Unable to monitor session activity: {error}", file=sys.stderr, flush=True)
        sys.exit(1)
