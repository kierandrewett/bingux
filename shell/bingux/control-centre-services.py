#!/usr/bin/env python3
"""Read desktop control state; apply only explicit, allowlisted UI actions."""

from concurrent.futures import ThreadPoolExecutor
import json
import ipaddress
import os
import shutil
import subprocess
import sys

from gi.repository import Gio, GLib

POWER_SERVICES = [
    ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles"),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
]


def run(command, timeout=3):
    result = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise ValueError("The service could not complete the request. Check its settings or permissions.")
    return result.stdout.strip()


def properties(bus_type, name, path):
    bus = Gio.bus_get_sync(bus_type, None)
    response = bus.call_sync(
        name,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        GLib.Variant("(s)", (name,)),
        GLib.VariantType.new("(a{sv})"),
        Gio.DBusCallFlags.NONE,
        1500,
        None,
    )
    return response.unpack()[0]


def power_state():
    for name, path in POWER_SERVICES:
        try:
            data = properties(Gio.BusType.SYSTEM, name, path)
            return {
                "available": True,
                "profile": data["ActiveProfile"],
                "profiles": [profile["Profile"] for profile in data["Profiles"]],
                "service": name,
                "path": path,
            }
        except GLib.Error:
            continue
    return {"available": False, "profile": "", "profiles": []}


def mullvad_state(data):
    state = data.get("state", "unknown")
    location = data.get("details", {}).get("location") or {}
    subtitle = {
        "connected": "Connected",
        "connecting": "Connecting",
        "disconnecting": "Disconnecting",
        "disconnected": "Disconnected",
        "error": "Connection error",
    }.get(state, "Unavailable")
    if state == "connected" and location.get("country"):
        subtitle += " - " + location["country"]
    if state == "disconnected" and data.get("details", {}).get("locked_down"):
        subtitle = "Disconnected - traffic blocked"
    return {
        "id": "mullvad",
        "name": "Mullvad",
        "connected": state == "connected",
        "subtitle": subtitle,
        "canToggle": state in ("connected", "disconnected", "error"),
    }


def tailscale_state(data, prefs=None):
    state = data.get("BackendState", "")
    connected = state == "Running"
    exit_node = data.get("ExitNodeStatus")
    subtitle = (
        "Private network connected"
        if connected
        else "Disconnected"
        if state == "Stopped"
        else "Sign in required"
        if state == "NeedsLogin"
        else "Unavailable"
    )
    if connected and exit_node:
        subtitle = "Exit node active" if exit_node.get("Online") else "Exit node unavailable"
    nodes = []
    for peer in [data.get("Self") or {}] + list((data.get("Peer") or {}).values()):
        if not peer.get("ID"):
            continue
        location = peer.get("Location") or {}
        provider = (
            "Mullvad" if "tag:mullvad-exit-node" in (peer.get("Tags") or []) else "VPN provider" if location else ""
        )
        nodes.append(
            {
                "id": peer["ID"],
                "name": peer.get("HostName") or peer.get("DNSName", "").rstrip("."),
                "dns": peer.get("DNSName", "").rstrip("."),
                "ips": peer.get("TailscaleIPs") or [],
                "os": peer.get("OS", ""),
                "online": bool(peer.get("Online")),
                "self": peer is data.get("Self"),
                "exitOption": bool(peer.get("ExitNodeOption")),
                "exit": bool(peer.get("ExitNode")),
                "provider": provider,
                "country": location.get("Country", ""),
                "countryCode": location.get("CountryCode", ""),
                "city": location.get("City", ""),
            }
        )
    nodes.sort(key=lambda node: (not node["self"], not node["online"], node["name"].casefold()))
    flags = {
        "accept-dns": "CorpDNS",
        "accept-routes": "RouteAll",
        "shields-up": "ShieldsUp",
        "exit-node-allow-lan-access": "ExitNodeAllowLANAccess",
    }
    return {
        "id": "tailscale",
        "name": "Tailscale",
        "connected": connected,
        "subtitle": subtitle,
        "canToggle": state in ("Running", "Stopped"),
        "nodes": nodes,
        "exitNode": (prefs or {}).get("ExitNodeID") or (exit_node or {}).get("ID", ""),
        "preferences": {
            flag: prefs[key] for flag, key in flags.items() if prefs is not None and isinstance(prefs.get(key), bool)
        },
    }


def split_nm(line):
    fields, field, escaped = [], "", False
    for character in line:
        if escaped:
            field += character
            escaped = False
        elif character == "\\":
            escaped = True
        elif character == ":":
            fields.append(field)
            field = ""
        else:
            field += character
    return fields + [field]


def vpn_state():
    rows = []
    for command, parser in [("mullvad", mullvad_state), ("tailscale", tailscale_state)]:
        if not shutil.which(command):
            continue
        try:
            data = json.loads(run([command, "status", "--json"]))
            if command == "tailscale":
                try:
                    prefs = json.loads(run(["tailscale", "debug", "prefs"]))
                except (ValueError, OSError, subprocess.TimeoutExpired):
                    prefs = None
                rows.append(tailscale_state(data, prefs))
            else:
                rows.append(parser(data))
        except (ValueError, OSError, subprocess.TimeoutExpired):
            rows.append(
                {
                    "id": command,
                    "name": command.capitalize(),
                    "connected": False,
                    "subtitle": "Service unavailable",
                    "canToggle": False,
                }
            )
    if shutil.which("nmcli"):
        try:
            for line in run(
                ["nmcli", "-t", "-e", "yes", "-f", "UUID,TYPE,NAME,DEVICE", "connection", "show"]
            ).splitlines():
                fields = split_nm(line)
                if len(fields) != 4 or fields[1] not in ("vpn", "wireguard"):
                    continue
                if fields[3] == "wg-mullvad" and any(row["id"] == "mullvad" for row in rows):
                    continue
                connected = fields[3] not in ("", "--")
                rows.append(
                    {
                        "id": "nm:" + fields[0],
                        "name": fields[2],
                        "connected": connected,
                        "subtitle": "Connected" if connected else "Disconnected",
                        "canToggle": True,
                    }
                )
        except (ValueError, OSError, subprocess.TimeoutExpired):
            pass
    return rows


def settings(schema):
    source = Gio.SettingsSchemaSource.get_default()
    found = source.lookup(schema, True) if source else None
    return Gio.Settings.new_full(found, None, None) if found else None


class Controls:
    def __init__(self):
        self.notifications = settings("org.gnome.desktop.notifications")
        self.colour = settings("org.gnome.settings-daemon.plugins.color")
        self.executor = ThreadPoolExecutor(max_workers=1)
        self.active = False
        self.busy = False
        self.refresh_pending = False
        self.snapshot = {}

    def emit(self, value):
        print(json.dumps(value), flush=True)

    def settings_state(self):
        return {
            "doNotDisturb": not self.notifications.get_boolean("show-banners") if self.notifications else False,
            "dndAvailable": bool(self.notifications and self.notifications.is_writable("show-banners")),
            "nightLight": self.colour.get_boolean("night-light-enabled") if self.colour else False,
            "nightLightAvailable": bool(self.colour and self.colour.is_writable("night-light-enabled")),
        }

    def changed(self, *_):
        self.emit({"state": self.settings_state()})

    def read(self):
        result = {"vpns": vpn_state(), "power": power_state()}
        try:
            bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            result["awakeAvailable"] = (
                bool(shutil.which("gnome-session-inhibit"))
                and bus.call_sync(
                    "org.freedesktop.DBus",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "NameHasOwner",
                    GLib.Variant("(s)", ("org.gnome.SessionManager",)),
                    None,
                    Gio.DBusCallFlags.NONE,
                    1000,
                    None,
                ).unpack()[0]
            )
        except GLib.Error:
            result["awakeAvailable"] = False
        try:
            result["nightLightActive"] = properties(
                Gio.BusType.SESSION, "org.gnome.SettingsDaemon.Color", "/org/gnome/SettingsDaemon/Color"
            ).get("NightLightActive", False)
        except GLib.Error:
            result["nightLightActive"] = False
        return result

    def refresh(self):
        if self.busy:
            self.refresh_pending = True
            return
        self.busy = True
        self.executor.submit(self.read).add_done_callback(lambda future: GLib.idle_add(self.refreshed, future))

    def refreshed(self, future):
        self.busy = False
        try:
            self.snapshot = future.result()
            self.emit({"state": self.snapshot | self.settings_state()})
        except Exception:
            self.emit({"error": "Some controls are unavailable."})
        if self.refresh_pending:
            self.refresh_pending = False
            self.refresh()
        return False

    def apply(self, request):
        kind = request.get("kind")
        enabled = request.get("enabled")
        if kind in ("dnd", "nightLight") and isinstance(enabled, bool):
            target, key = (
                (self.notifications, "show-banners") if kind == "dnd" else (self.colour, "night-light-enabled")
            )
            if (
                target is None
                or not target.is_writable(key)
                or not target.set_boolean(key, not enabled if kind == "dnd" else enabled)
            ):
                raise ValueError("This setting is unavailable.")
            Gio.Settings.sync()
        elif kind == "vpn" and isinstance(enabled, bool):
            identity = request.get("id")
            row = next(
                (row for row in self.snapshot.get("vpns", []) if row["id"] == identity and row["canToggle"]), None
            )
            if not row:
                raise ValueError("This connection is unavailable.")
            if identity == "mullvad":
                run(["mullvad", "connect" if enabled else "disconnect"], timeout=12)
            elif identity == "tailscale":
                run(["tailscale", "up" if enabled else "down"], timeout=12)
            else:
                run(
                    ["nmcli", "--wait", "10", "connection", "up" if enabled else "down", "uuid", identity[3:]],
                    timeout=12,
                )
        elif kind == "power":
            power = power_state()
            profile = request.get("profile")
            if profile not in power["profiles"]:
                raise ValueError("This power mode is unavailable.")
            Gio.bus_get_sync(Gio.BusType.SYSTEM, None).call_sync(
                power["service"],
                power["path"],
                "org.freedesktop.DBus.Properties",
                "Set",
                GLib.Variant("(ssv)", (power["service"], "ActiveProfile", GLib.Variant("s", profile))),
                None,
                Gio.DBusCallFlags.NONE,
                3000,
                None,
            )
        elif kind == "tailscale":
            # Revalidate targets against current daemon state, not a stale UI snapshot.
            data = json.loads(run(["tailscale", "status", "--json"]))
            if data.get("BackendState") not in ("Running", "Stopped"):
                raise ValueError("Tailscale must be signed in before changing settings.")
            setting = request.get("setting")
            if setting == "exit-node":
                identity = request.get("value")
                if identity == "":
                    value = ""
                else:
                    peer = next(
                        (
                            peer
                            for peer in (data.get("Peer") or {}).values()
                            if peer.get("ID") == identity and peer.get("ExitNodeOption") and peer.get("Online")
                        ),
                        None,
                    )
                    if not peer or not peer.get("TailscaleIPs"):
                        raise ValueError("That exit node is no longer available.")
                    value = str(ipaddress.ip_address(peer["TailscaleIPs"][0]))
                run(["tailscale", "set", "--exit-node=" + value], timeout=12)
            elif setting in ("accept-dns", "accept-routes", "shields-up", "exit-node-allow-lan-access") and isinstance(
                enabled, bool
            ):
                run(["tailscale", "set", "--" + setting + "=" + str(enabled).lower()], timeout=12)
            else:
                raise ValueError("Unknown Tailscale setting.")
        else:
            raise ValueError("Unknown control action.")

    def action(self, request):
        self.executor.submit(self.apply, request).add_done_callback(lambda future: GLib.idle_add(self.applied, future))

    def applied(self, future):
        error = ""
        try:
            future.result()
        except Exception as failure:
            error = str(failure) if isinstance(failure, ValueError) else "The service could not complete this change."
        self.emit({"actionDone": True, "error": error})
        self.changed()
        self.refresh()
        return False


def main():
    controls = Controls()
    if sys.argv[1:] == ["snapshot"]:
        print(json.dumps(controls.read() | controls.settings_state()))
        return
    loop = GLib.MainLoop()
    for target in (controls.notifications, controls.colour):
        if target:
            target.connect("changed", controls.changed)
    pending = bytearray()

    def read(_stream, _condition):
        chunk = os.read(sys.stdin.fileno(), 8192)
        if not chunk:
            loop.quit()
            return False
        pending.extend(chunk)
        while b"\n" in pending:
            line, _, rest = pending.partition(b"\n")
            pending[:] = rest
            try:
                request = json.loads(line)
                if request.get("op") == "active":
                    controls.active = bool(request.get("value"))
                    if controls.active:
                        controls.refresh()
                elif request.get("op") == "action":
                    controls.action(request)
            except (ValueError, TypeError):
                continue
        return True

    def tick():
        if controls.active:
            controls.refresh()
        return True

    GLib.io_add_watch(sys.stdin, GLib.IO_IN | GLib.IO_HUP, read)
    GLib.timeout_add_seconds(4, tick)
    controls.emit({"ready": True, "state": controls.settings_state()})
    controls.refresh()
    loop.run()
    controls.executor.shutdown(wait=False, cancel_futures=True)


if __name__ == "__main__":
    main()
