#!/usr/bin/env python3
"""Opt-in live checks: act only on a generated notification, player and window."""
import json
import os
import subprocess
import threading
import time
import uuid
from gi.repository import Gio, GLib

ctl = os.environ.get("BINGUXCTL", "binguxctl")
identity = "binguxctl-test-" + uuid.uuid4().hex[:10]
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
loop = GLib.MainLoop()
threading.Thread(target=loop.run, daemon=True).start()


def call(*args):
    result = subprocess.run([ctl, *map(str, args)], text=True, capture_output=True, timeout=12)
    assert result.returncode == 0, (args, result.stderr)
    return json.loads(result.stdout) if result.stdout.strip() else None


def until(get, predicate, timeout=6):
    deadline = time.monotonic() + timeout
    value = None
    while time.monotonic() < deadline:
        value = get()
        if predicate(value): return value
        time.sleep(.1)
    raise AssertionError(value)


path = "/org/mpris/MediaPlayer2"
interface = "org.mpris.MediaPlayer2"
player_name = interface + ".binguxctl_test_" + uuid.uuid4().hex
properties = {
    interface: {"Identity": GLib.Variant("s", identity), "DesktopEntry": GLib.Variant("s", identity),
        "CanQuit": GLib.Variant("b", False), "CanRaise": GLib.Variant("b", False), "HasTrackList": GLib.Variant("b", False),
        "SupportedUriSchemes": GLib.Variant("as", []), "SupportedMimeTypes": GLib.Variant("as", [])},
    interface + ".Player": {"PlaybackStatus": GLib.Variant("s", "Paused"), "Position": GLib.Variant("x", 0),
        "Rate": GLib.Variant("d", 1), "MinimumRate": GLib.Variant("d", 1), "MaximumRate": GLib.Variant("d", 1),
        "Volume": GLib.Variant("d", 1), "Metadata": GLib.Variant("a{sv}", {"mpris:trackid": GLib.Variant("o", "/track/test"),
            "mpris:length": GLib.Variant("x", 120000000), "xesam:title": GLib.Variant("s", "Test track")}),
        **{name: GLib.Variant("b", True) for name in ("CanGoNext", "CanGoPrevious", "CanPlay", "CanPause", "CanSeek", "CanControl")}}
}
methods = []
registrations = []


def player_call(connection, sender, object_path, iface, method, params, invocation):
    methods.append((method, params.unpack()))
    if method in ("Play", "Pause"):
        status = GLib.Variant("s", "Playing" if method == "Play" else "Paused")
        properties[iface]["PlaybackStatus"] = status
        bus.emit_signal(None, path, "org.freedesktop.DBus.Properties", "PropertiesChanged",
            GLib.Variant("(sa{sv}as)", (iface, {"PlaybackStatus": status}, [])))
    invocation.return_value(None)


for iface, values in properties.items():
    props = "".join(f'<property name="{name}" type="{value.get_type_string()}" access="read"/>' for name, value in values.items())
    ops = ''.join(f'<method name="{name}"/>' for name in ("Play", "Pause", "Next", "Previous")) if iface.endswith(".Player") else ""
    if ops: ops += '<method name="SetPosition"><arg type="o" direction="in"/><arg type="x" direction="in"/></method>'
    xml = Gio.DBusNodeInfo.new_for_xml(f'<node><interface name="{iface}">{props}{ops}</interface></node>')
    registrations.append(bus.register_object(path, xml.interfaces[0], player_call,
        lambda connection, sender, object_path, iface, name: properties[iface][name], None))
bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "RequestName",
    GLib.Variant("(su)", (player_name, 0)), None, Gio.DBusCallFlags.NONE, 2000, None)
window = None
notice_id = None
notice_key = None
try:
    until(lambda: call("media", "list"), lambda state: any(player["id"] == player_name for player in state["players"]))
    call("media", "play", "--player", player_name)
    until(lambda: methods, lambda calls: any(method == "Play" for method, _ in calls))
    call("media", "pause", "--player", player_name)
    call("media", "next", "--player", player_name)
    call("media", "seek", "12", "--player", player_name)
    until(lambda: methods, lambda calls: any(method == "SetPosition" and args == ("/track/test", 12000000) for method, args in calls))
    print("PASS: real MPRIS play, pause, next and seek reach the selected test player", flush=True)

    invoked = []
    subscription = bus.signal_subscribe("org.freedesktop.Notifications", "org.freedesktop.Notifications", "ActionInvoked",
        "/org/freedesktop/Notifications", None, Gio.DBusSignalFlags.NONE, lambda *args: invoked.append(args[-1].unpack()))
    notice_id = bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications", "Notify",
        GLib.Variant("(susssasa{sv}i)", ("Bingux CLI test", 0, "", identity, "Command verification", ["test", "Test action"],
            {"resident": GLib.Variant("b", True)}, 0)), None, Gio.DBusCallFlags.NONE, 2000, None).unpack()[0]
    state = until(lambda: call("notifications", "list"), lambda state: any(item["summary"] == identity for item in state["notifications"]))
    notice_key = next(item["id"] for item in state["notifications"] if item["summary"] == identity)
    call("notifications", "invoke", notice_key, "test")
    until(lambda: invoked, lambda values: (notice_id, "test") in values)
    call("notifications", "dismiss", notice_key)
    until(lambda: call("notifications", "list"), lambda state: all(item["id"] != notice_key for item in state["notifications"]))
    print("PASS: normal notification action and dismissal affect only the generated notice", flush=True)

    window = subprocess.Popen(["foot", "--app-id", identity, "--title", identity, "sleep", "60"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    def windows(): return call("windows", "list")["windows"]
    found = until(windows, lambda entries: any(entry["title"] == identity for entry in entries))
    window_id = next(entry["id"] for entry in found if entry["title"] == identity)
    call("windows", "minimize", window_id)
    until(windows, lambda entries: any(entry["id"] == window_id and entry["minimized"] for entry in entries))
    call("windows", "activate", window_id)
    until(windows, lambda entries: any(entry["id"] == window_id and entry["active"] and not entry["minimized"] for entry in entries))
    call("windows", "close", window_id)
    until(windows, lambda entries: all(entry["id"] != window_id for entry in entries))
    print("PASS: real test window minimises, activates and closes by stable ID", flush=True)
finally:
    if window and window.poll() is None: window.terminate(); window.wait(timeout=5)
    if notice_id:
        bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications", "CloseNotification",
            GLib.Variant("(u)", (notice_id,)), None, Gio.DBusCallFlags.NONE, 2000, None)
    bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "ReleaseName",
        GLib.Variant("(s)", (player_name,)), None, Gio.DBusCallFlags.NONE, 2000, None)
    for registration in registrations: bus.unregister_object(registration)
    loop.quit()
