#!/usr/bin/env python3
"""Test real MPRIS calls from the dock on Gnoblin's isolated session bus."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from gi.repository import Gio, GLib

if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin's GNOBLIN_TEST_DBUS_CLIENT launcher.")

ROOT = Path(__file__).resolve().parent.parent
art_requests = []
class Artwork(BaseHTTPRequestHandler):
    def do_GET(self):
        art_requests.append(self.path)
        self.send_response(200)
        self.send_header("Content-Type", "image/png")
        self.end_headers()
        self.wfile.write(base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEUlEQVR4nGNQONCAFTEMLQkASsZYAa5usi0AAAAASUVORK5CYII="))
    def log_message(self, *args):
        pass
art_server = ThreadingHTTPServer(("127.0.0.1", 0), Artwork)
threading.Thread(target=art_server.serve_forever, daemon=True).start()
PATH = "/org/mpris/MediaPlayer2"
ROOT_IFACE = "org.mpris.MediaPlayer2"
PLAYER_IFACE = ROOT_IFACE + ".Player"

def variant(value):
    if isinstance(value, bool): return GLib.Variant("b", value)
    if isinstance(value, float): return GLib.Variant("d", value)
    if isinstance(value, int): return GLib.Variant("x", value)
    if isinstance(value, dict): return GLib.Variant("a{sv}", value)
    if isinstance(value, list): return GLib.Variant("as", value)
    return GLib.Variant("s", value)

class Player:
    def __init__(self, suffix, desktop):
        self.calls = []
        self.root = {"CanQuit": True, "CanRaise": False, "HasTrackList": False,
                     "Identity": "Test player", "DesktopEntry": desktop,
                     "SupportedUriSchemes": [], "SupportedMimeTypes": []}
        self.player = {"PlaybackStatus": "Paused", "Rate": 1.0, "Volume": 1.0,
                       "MinimumRate": 1.0, "MaximumRate": 1.0, "Position": 0,
                       "CanGoNext": True, "CanGoPrevious": True, "CanPlay": True,
                       "CanPause": True, "CanSeek": True, "CanControl": True}
        self.metadata("Test track", "first")
        properties = lambda values: "".join(f'<property name="{key}" type="{variant(value).get_type_string()}" access="read"/>' for key, value in values.items())
        xml = f'''<node><interface name="{ROOT_IFACE}">{properties(self.root)}
        <method name="Quit"/><method name="Raise"/></interface>
        <interface name="{PLAYER_IFACE}">{properties(self.player)}
        <method name="Play"/><method name="Pause"/><method name="PlayPause"/>
        <method name="Next"/><method name="Previous"/><method name="Stop"/>
        <method name="Seek"><arg type="x" direction="in"/></method>
        <method name="SetPosition"><arg type="o" direction="in"/><arg type="x" direction="in"/></method>
        <signal name="Seeked"><arg type="x"/></signal></interface></node>'''
        self.bus = Gio.DBusConnection.new_for_address_sync(os.environ["DBUS_SESSION_BUS_ADDRESS"],
            Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)
        for interface in Gio.DBusNodeInfo.new_for_xml(xml).interfaces:
            self.bus.register_object(PATH, interface, self.call, self.get, None)
        self.bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "RequestName",
            GLib.Variant("(su)", (ROOT_IFACE + "." + suffix, 0)), None, Gio.DBusCallFlags.NONE, 2000, None)

    def metadata(self, title, track):
        self.player["Metadata"] = {"mpris:trackid": GLib.Variant("o", "/track/" + track),
            "mpris:length": GLib.Variant("x", 180000000), "xesam:title": variant(title),
            "mpris:artUrl": variant(f"http://127.0.0.1:{art_server.server_port}/art.png"),
            "xesam:artist": variant(["Test artist"])}

    def get(self, connection, sender, path, interface, name):
        return variant((self.root if interface == ROOT_IFACE else self.player)[name])

    def call(self, connection, sender, path, interface, method, parameters, invocation):
        args = parameters.unpack()
        self.calls.append((method, args))
        changed = {}
        if method in ("Play", "Pause", "PlayPause"):
            playing = method == "Play" or (method == "PlayPause" and self.player["PlaybackStatus"] != "Playing")
            changed["PlaybackStatus"] = "Playing" if playing else "Paused"
        elif method in ("Next", "Previous"):
            self.metadata("Second track" if method == "Next" else "Test track", "second" if method == "Next" else "first")
            changed = {"Metadata": self.player["Metadata"], "CanGoNext": method != "Next"}
        elif method == "SetPosition":
            if args[0] == self.player["Metadata"]["mpris:trackid"].unpack():
                self.player["Position"] = args[1]
                self.bus.emit_signal(None, PATH, PLAYER_IFACE, "Seeked", GLib.Variant("(x)", (args[1],)))
        self.player.update(changed)
        if changed:
            self.bus.emit_signal(None, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged",
                GLib.Variant("(sa{sv}as)", (PLAYER_IFACE, {key: variant(value) for key, value in changed.items()}, [])))
        invocation.return_value(None)
        if method == "Quit":
            GLib.idle_add(lambda: (self.bus.close_sync(None), False)[1])

players = [Player("bingux_test", "dock-media-test"), Player("unrelated", "another-player")]
loop = GLib.MainLoop()
thread = threading.Thread(target=loop.run, daemon=True)
thread.start()
try:
    with tempfile.TemporaryDirectory(prefix="bingux-media-test-") as directory:
        fixture = Path(directory)
        for file in (ROOT / "shell/bingux").iterdir():
            if file.suffix in (".qml", ".js", ".py") or file.name == "qmldir": shutil.copy2(file, fixture)
        dock = (fixture / "Dock.qml").read_text().replace("required property var settings", "required property var settings\n    property alias testItems: dockItems")
        dock = dock.replace("id: dockButton", "id: dockButton\n                    property alias testMenu: appMenu")
        (fixture / "Dock.qml").write_text(dock)
        shutil.copy2(ROOT / "tests/dock-media.qml", fixture / "shell.qml")
        applications = fixture / "data/applications"
        applications.mkdir(parents=True)
        (fixture / "data/icons").symlink_to("/usr/share/icons", target_is_directory=True)
        for name in ("dock-media-test", "no-media"):
            (applications / (name + ".desktop")).write_text(f"[Desktop Entry]\nType=Application\nName={name}\nExec=true\nIcon=audio-x-generic\n")
        discord_icon = fixture / "discord.png"
        discord_icon.write_bytes(base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEUlEQVR4nGNQONCAFTEMLQkASsZYAa5usi0AAAAASUVORK5CYII="))
        (applications / "discord-canary.desktop").write_text(f"[Desktop Entry]\nType=Application\nName=Discord Canary\nStartupWMClass=discord\nExec=true\nIcon={discord_icon}\n")
        result_file = fixture / "results.txt"
        environment = os.environ | {"XDG_DATA_HOME": str(fixture / "data"), "XDG_DATA_DIRS": str(fixture / "data"), "XDG_CONFIG_HOME": str(fixture / "config"),
                                    "BINGUX_MEDIA_TEST_RESULTS": str(result_file), "BINGUX_DISCORD_TEST_ICON": str(discord_icon)}
        result = subprocess.run([os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture), "--no-color"],
                                env=environment, capture_output=True, text=True, timeout=35)
        report = result_file.read_text() if result_file.exists() else ""
        print(report)
        if result.returncode != 0 or "FAILURES 0" not in report:
            print("MPRIS calls:", players[0].calls)
            print(result.stdout + result.stderr)
            raise SystemExit(1)
        methods = [name for name, args in players[0].calls]
        assert methods.count("Next") == 1, methods
        assert all(name in methods for name in ("Play", "Pause", "Previous", "SetPosition", "Quit")), methods
        assert not players[1].calls, players[1].calls
        assert art_requests == ["/art.png"], art_requests
        print("PASS: album art fetched once across menu reopen and metadata updates")
        print("PASS: real MPRIS calls, capability changes, seek, disconnect, and unrelated-player isolation")
finally:
    art_server.shutdown()
    loop.quit()
    thread.join(timeout=2)
    for player in players:
        if not player.bus.is_closed(): player.bus.close_sync(None)
