#!/usr/bin/python3
"""Open HTTP(S) through the desktop portal; preserve normal local-file opening."""
import os
import sys
import uuid


def main():
    if len(sys.argv) != 2:
        return 2
    target = sys.argv[1]
    if not target.startswith(("https://", "http://")):
        os.execv("/usr/bin/xdg-open", ["xdg-open", target])
    from gi.repository import Gio, GLib
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    token = "bingux" + uuid.uuid4().hex
    path = "/org/freedesktop/portal/desktop/request/" + connection.get_unique_name()[1:].replace(".", "_") + "/" + token
    loop = GLib.MainLoop()
    outcome = [1]

    def response(_connection, _sender, _path, _interface, _signal, parameters):
        code, _details = parameters.unpack()
        outcome[0] = 0 if code == 0 else 1
        loop.quit()

    subscription = connection.signal_subscribe("org.freedesktop.portal.Desktop", "org.freedesktop.portal.Request", "Response", path, None, Gio.DBusSignalFlags.NONE, response)
    connection.call_sync("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.OpenURI", "OpenURI",
                         GLib.Variant("(ssa{sv})", ("", target, {"handle_token": GLib.Variant("s", token)})), None, Gio.DBusCallFlags.NONE, 5000, None)
    def expire():
        loop.quit()
        return False
    timeout = GLib.timeout_add_seconds(10, expire)
    loop.run()
    connection.signal_unsubscribe(subscription)
    if outcome[0] == 0:
        GLib.source_remove(timeout)
    return outcome[0]


if __name__ == "__main__":
    raise SystemExit(main())
