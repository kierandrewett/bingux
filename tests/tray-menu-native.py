#!/usr/bin/env python3
"""Serve a real StatusNotifierItem/DBusMenu for tray-menu-native.qml.

Run inside the already-running test desktop. This fixture owns only its Gio
bus objects and its optional Quickshell fixture process; it never manages a VM
or desktop session. The external driver supplies real pointer/keyboard input.
--peer-count defaults to one independent SNI/DBusMenu peer; use zero for a
target-only run. Result JSON records the child PID and staged QML root.
"""

import argparse
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import time
import traceback

import gi

gi.require_version("Gio", "2.0")
gi.require_version("GLib", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
SNI = "org.kde.StatusNotifierItem"
MENU = "com.canonical.dbusmenu"
TEST = "dev.bingux.TrayMenuFixture1"
CONTROL_NAME = "dev.bingux.TrayMenuFixture"
BUS_NAME = "org.kde.StatusNotifierItem-{}-1".format(os.getpid())
ITEM_PATH = "/StatusNotifierItem"
MENU_PATH = "/Menu"
FIXTURE_PATH = "/dev/bingux/TrayMenuFixture"


def request_name(connection, name):
    result = connection.call_sync(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
        GLib.Variant("(su)", (name, 4)),  # DBUS_NAME_FLAG_DO_NOT_QUEUE
        GLib.VariantType.new("(u)"),
        Gio.DBusCallFlags.NONE,
        3000,
        None,
    ).unpack()[0]
    if result not in (1, 4):
        raise RuntimeError(f"Cannot own {name}: RequestName returned {result}; another owner may be running")


def release_name(connection, name):
    result = connection.call_sync(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ReleaseName",
        GLib.Variant("(s)", (name,)),
        GLib.VariantType.new("(u)"),
        Gio.DBusCallFlags.NONE,
        3000,
        None,
    ).unpack()[0]
    if result not in (1, 2):
        raise RuntimeError(f"Cannot release {name}: ReleaseName returned {result}")


SNI_XML = f'''<node><interface name="{SNI}">
 <method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
 <method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
 <method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
 <method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
 <property name="Category" type="s" access="read"/><property name="Id" type="s" access="read"/>
 <property name="Title" type="s" access="read"/><property name="Status" type="s" access="read"/>
 <property name="WindowId" type="u" access="read"/><property name="IconName" type="s" access="read"/>
 <property name="IconPixmap" type="a(iiay)" access="read"/><property name="OverlayIconName" type="s" access="read"/>
 <property name="OverlayIconPixmap" type="a(iiay)" access="read"/><property name="AttentionIconName" type="s" access="read"/>
 <property name="AttentionIconPixmap" type="a(iiay)" access="read"/><property name="AttentionMovieName" type="s" access="read"/>
 <property name="ToolTip" type="(sa(iiay)ss)" access="read"/><property name="ItemIsMenu" type="b" access="read"/>
 <property name="Menu" type="o" access="read"/>
</interface></node>'''
MENU_XML = f'''<node><interface name="{MENU}">
 <method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{{sv}}av)" direction="out"/></method>
 <method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{{sv}})" direction="out"/></method>
 <method name="GetProperty"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
 <method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
 <method name="EventGroup"><arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/></method>
 <method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
 <method name="AboutToShowGroup"><arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/></method>
 <signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
 <signal name="ItemsPropertiesUpdated"><arg type="a(ia{{sv}})"/><arg type="a(ias)"/></signal>
 <property name="Version" type="u" access="read"/><property name="TextDirection" type="s" access="read"/>
 <property name="Status" type="s" access="read"/><property name="IconThemePath" type="s" access="read"/>
</interface></node>'''
TEST_XML = f'''<node><interface name="{TEST}">
 <method name="DelayLayout"><arg type="u" direction="in"/></method>
 <method name="ReleaseLayout"/><method name="ReplaceItem"/><method name="RemoveItem"/>
 <method name="Snapshot"><arg type="s" direction="out"/></method>
 <signal name="Receipt"><arg type="s"/><arg type="i"/></signal>
</interface></node>'''


class Fixture:
    def __init__(self, connection, item_connection):
        self.connection = connection
        self.item_connection = item_connection
        self.delay_ms = 1200
        self.pending = []
        self.receipts = []
        self.generation = 1
        self.visible = True
        self.sni_info = Gio.DBusNodeInfo.new_for_xml(SNI_XML).interfaces[0]
        self.menu_info = Gio.DBusNodeInfo.new_for_xml(MENU_XML).interfaces[0]
        self.test_info = Gio.DBusNodeInfo.new_for_xml(TEST_XML).interfaces[0]
        self.providers = {}
        self.control_registration = 0

    def _export(self):
        self.add_provider(self.item_connection, BUS_NAME, "tray-menu-native-target")
        self.control_registration = self.connection.register_object(FIXTURE_PATH, self.test_info, self.call, None, None)

    def add_provider(self, connection, name, item_id):
        provider = {
            "id": item_id,
            "busName": name,
            "connection": connection.get_unique_name(),
            "registrations": [],
            "bus": connection,
        }
        self.providers[connection.get_unique_name()] = provider
        for path, info in ((ITEM_PATH, self.sni_info), (MENU_PATH, self.menu_info)):
            provider["registrations"].append(connection.register_object(path, info, self.call, self.get_property, None))

    def provider_receipt(self, connection, sender, path):
        provider = self.providers.get(connection.get_unique_name())
        return {
            "providerId": provider["id"] if provider else None,
            "connection": connection.get_unique_name(),
            "sender": sender,
            "path": path,
        }

    def peers(self):
        return [
            {key: provider[key] for key in ("id", "busName", "connection")}
            for provider in self.providers.values()
            if provider["bus"] is not self.item_connection
        ]

    def get_property(self, connection, sender, path, interface, name):
        return self.properties(interface, connection).get(name)

    def properties(self, interface, connection=None):
        provider = self.providers[(connection or self.item_connection).get_unique_name()]
        is_target = provider["bus"] is self.item_connection
        title = "Native delayed menu" if is_target else provider["id"]
        if interface == SNI:
            values = {
                "Category": GLib.Variant("s", "ApplicationStatus"),
                "Id": GLib.Variant("s", provider["id"]),
                "Title": GLib.Variant("s", title),
                "Status": GLib.Variant("s", "Active" if not is_target or self.visible else "Passive"),
                "WindowId": GLib.Variant("u", 0),
                "IconName": GLib.Variant("s", ""),
                "IconPixmap": GLib.Variant("a(iiay)", [(16, 16, bytes((255, 70, 180, 240)) * 256)]),
                "OverlayIconName": GLib.Variant("s", ""),
                "OverlayIconPixmap": GLib.Variant("a(iiay)", []),
                "AttentionIconName": GLib.Variant("s", ""),
                "AttentionIconPixmap": GLib.Variant("a(iiay)", []),
                "AttentionMovieName": GLib.Variant("s", ""),
                "ToolTip": GLib.Variant("(sa(iiay)ss)", ("", [], title, provider["id"])),
                "ItemIsMenu": GLib.Variant("b", True),
                "Menu": GLib.Variant("o", MENU_PATH),
            }
            return values
        if interface == MENU:
            return {
                "Version": GLib.Variant("u", 3),
                "TextDirection": GLib.Variant("s", "ltr"),
                "Status": GLib.Variant("s", "normal"),
                "IconThemePath": GLib.Variant("s", ""),
            }
        return {}

    def layout(self, connection=None):
        provider = self.providers[(connection or self.item_connection).get_unique_name()]
        is_target = provider["bus"] is self.item_connection
        row = {
            "label": GLib.Variant("s", "Native delayed action" if is_target else provider["id"] + " action"),
            "enabled": GLib.Variant("b", True),
            "visible": GLib.Variant("b", True),
            "type": GLib.Variant("s", "standard"),
        }
        child = GLib.Variant("(ia{sv}av)", (1, row, []))
        # The av container supplies each element's single variant wrapper.
        return (self.generation if is_target else 1, (0, {}, [child]))

    def call(self, connection, sender, path, interface, method, parameters, invocation):
        self.receipts.append(
            {
                "kind": "method",
                "interface": interface,
                "method": method,
                **self.provider_receipt(connection, sender, path),
                "time": time.time(),
            }
        )
        try:
            self._call(connection, sender, path, interface, method, parameters, invocation)
        except Exception as error:
            traceback.print_exc(file=sys.stderr)
            invocation.return_dbus_error("dev.bingux.TrayMenuFixture.Error", str(error))

    def _call(self, connection, sender, path, interface, method, parameters, invocation):
        if interface == "org.freedesktop.DBus.Properties":
            if method == "Get":
                iface, name = parameters.unpack()
                value = self.properties(iface, connection).get(name)
                if value is None:
                    invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownProperty", name)
                else:
                    invocation.return_value(GLib.Variant("(v)", (value,)))
            elif method == "GetAll":
                iface = parameters.unpack()[0]
                invocation.return_value(GLib.Variant("(a{sv})", (self.properties(iface, connection),)))
            else:
                invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", method)
            return
        args = parameters.unpack()
        if interface == SNI:
            invocation.return_value(None)
        elif interface == MENU:
            if method == "GetLayout":
                delay = self.delay_ms if connection.get_unique_name() == self.item_connection.get_unique_name() else 0
                self.receipts.append(
                    {
                        "kind": "get-layout",
                        "time": time.time(),
                        "delayed": bool(delay),
                        **self.provider_receipt(connection, sender, path),
                    }
                )
                if delay:
                    self.pending.append(invocation)
                    GLib.timeout_add(self.delay_ms, self._release_one, invocation)
                else:
                    invocation.return_value(GLib.Variant("(u(ia{sv}av))", self.layout(connection)))
            elif method == "Event":
                item_id, event = args[0], args[1]
                self.receipts.append(
                    {
                        "kind": "menu-event",
                        "event": event,
                        "item": item_id,
                        "time": time.time(),
                        **self.provider_receipt(connection, sender, path),
                    }
                )
                if event == "clicked":
                    self.receipts.append(
                        {
                            "kind": "action",
                            "item": item_id,
                            "time": time.time(),
                            **self.provider_receipt(connection, sender, path),
                        }
                    )
                    self.connection.emit_signal(
                        None, FIXTURE_PATH, TEST, "Receipt", GLib.Variant("(si)", ("action", item_id))
                    )
                invocation.return_value(None)
            elif method == "EventGroup":
                invocation.return_value(GLib.Variant("(ai)", ([],)))
            elif method in ("GetGroupProperties",):
                invocation.return_value(GLib.Variant("(a(ia{sv}))", ([],)))
            elif method == "GetProperty":
                invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownProperty", str(args[1]))
            elif method == "AboutToShow":
                invocation.return_value(GLib.Variant("(b)", (False,)))
            elif method == "AboutToShowGroup":
                invocation.return_value(GLib.Variant("(aiai)", ([], [])))
            else:
                invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", method)
        elif interface == TEST:
            if method == "DelayLayout":
                self.delay_ms = max(0, min(int(args[0]), 10000))
                self.receipts.append({"kind": "delay-configured", "milliseconds": self.delay_ms, "time": time.time()})
                invocation.return_value(None)
            elif method == "ReleaseLayout":
                self.receipts.append({"kind": "layout-released", "time": time.time()})
                self._release_one()
                invocation.return_value(None)
            elif method == "ReplaceItem":
                self.generation += 1
                self.visible = True
                self.receipts.append(
                    {"kind": "replacement-requested", "generation": self.generation, "time": time.time()}
                )
                self.cancel_pending_layouts("provider replaced")
                self.release_item_name()
                # Let the bus deliver NameOwnerChanged so the watcher removes
                # the old item before the provider returns under the same name.
                GLib.MainContext.default().iteration(False)
                self.acquire_item_name()
                self.watcher.call_sync(
                    "RegisterStatusNotifierItem", GLib.Variant("(s)", (BUS_NAME,)), Gio.DBusCallFlags.NONE, 3000, None
                )
                self.receipts.append(
                    {"kind": "replacement-registered", "generation": self.generation, "time": time.time()}
                )
                invocation.return_value(None)
            elif method == "RemoveItem":
                self.visible = False
                self.receipts.append({"kind": "removed", "time": time.time()})
                self.cancel_pending_layouts("provider removed")
                self.release_item_name()
                invocation.return_value(None)
            elif method == "Snapshot":
                invocation.return_value(
                    GLib.Variant(
                        "(s)",
                        (
                            json.dumps(
                                {
                                    "id": "tray-menu-native-target",
                                    "visible": self.visible,
                                    "generation": self.generation,
                                    "receipts": self.receipts,
                                    "pendingLayouts": len(self.pending),
                                    "peers": self.peers(),
                                }
                            ),
                        ),
                    )
                )
            else:
                invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", method)

    def _release_one(self, invocation=None):
        if invocation is None and self.pending:
            invocation = self.pending[0]
        if invocation in self.pending:
            self.pending.remove(invocation)
            invocation.return_value(GLib.Variant("(u(ia{sv}av))", self.layout()))
        return GLib.SOURCE_REMOVE

    def cancel_pending_layouts(self, reason):
        while self.pending:
            invocation = self.pending.pop(0)
            invocation.return_dbus_error("org.freedesktop.DBus.Error.Disconnected", reason)
        self.receipts.append({"kind": "pending-layouts-cancelled", "reason": reason, "time": time.time()})

    def release_item_name(self):
        release_name(self.item_connection, BUS_NAME)

    def acquire_item_name(self):
        request_name(self.item_connection, BUS_NAME)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, required=True, help="new directory for result JSON and service log")
    parser.add_argument("--quickshell", default=os.environ.get("BINGUX_QUICKSHELL", "quickshell"))
    parser.add_argument("--serve-only", action="store_true", help="export the provider without launching QML")
    parser.add_argument("--peer-count", type=int, default=1, help="independent peer items, 0 to 24 (default: 1)")
    parser.add_argument("--timeout", type=int, default=180, help="maximum run time in seconds (default: 180)")
    args = parser.parse_args()
    args.artifacts = args.artifacts.resolve()
    if not 0 <= args.peer_count <= 24:
        parser.error("--peer-count must be between 0 and 24")
    if not 1 <= args.timeout <= 3600:
        parser.error("--timeout must be between 1 and 3600 seconds")
    if args.artifacts.exists():
        parser.error(f"artifact directory already exists: {args.artifacts}")
    if os.environ.get("BINGUX_DISPOSABLE_QEMU_GUEST") != "1":
        parser.error("set BINGUX_DISPOSABLE_QEMU_GUEST=1 only inside the disposable test guest")
    try:
        virtual = subprocess.run(
            ["systemd-detect-virt", "--vm"], check=True, capture_output=True, text=True, timeout=4
        ).stdout.strip()
        product = Path("/sys/class/dmi/id/product_name").read_text(errors="replace").strip().lower()
    except (OSError, subprocess.SubprocessError) as error:
        parser.error(f"cannot verify disposable QEMU guest identity: {error}")
    if virtual not in {"qemu", "kvm"} or ("qemu" not in product and "standard pc" not in product):
        parser.error(f"expected a QEMU/KVM guest and QEMU DMI product, got {virtual!r}, {product!r}")
    if not os.environ.get("WAYLAND_DISPLAY") or not os.environ.get("XDG_RUNTIME_DIR"):
        parser.error("run inside the already active guest Wayland session")
    args.artifacts.mkdir(parents=True)
    connections = []
    owned_names = []
    sources = []
    fixture = None
    child = None
    result = {
        "fixture": "tray-menu-native",
        "provider": BUS_NAME,
        "itemId": "tray-menu-native-target",
        "menuPath": MENU_PATH,
        "peerCount": args.peer_count,
        "peers": [],
        "quickshellPid": None,
        "qmlRoot": None,
        "started": time.time(),
        "receipts": [],
        "exit": "starting",
    }

    def save_result():
        (args.artifacts / "result.json").write_text(json.dumps(result, indent=2) + "\n")

    def new_bus(address):
        connection = Gio.DBusConnection.new_for_address_sync(
            address,
            Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
            None,
            None,
        )
        connections.append(connection)
        return connection

    def own_name(connection, name):
        request_name(connection, name)
        owned_names.append((connection, name))

    try:
        address = Gio.dbus_address_get_for_bus_sync(Gio.BusType.SESSION, None)
        connection = new_bus(address)
        # A second fixture must fail here without taking or queueing its control name.
        own_name(connection, CONTROL_NAME)
        item_connection = new_bus(address)
        own_name(item_connection, BUS_NAME)
        fixture = Fixture(connection, item_connection)
        result["receipts"] = fixture.receipts
        result["providerConnection"] = item_connection.get_unique_name()
        fixture._export()
        watcher = Gio.DBusProxy.new_sync(
            connection,
            Gio.DBusProxyFlags.NONE,
            None,
            "org.kde.StatusNotifierWatcher",
            "/StatusNotifierWatcher",
            "org.kde.StatusNotifierWatcher",
            None,
        )
        fixture.watcher = watcher
        watcher.call_sync(
            "RegisterStatusNotifierItem", GLib.Variant("(s)", (BUS_NAME,)), Gio.DBusCallFlags.NONE, 3000, None
        )
        for number in range(1, args.peer_count + 1):
            peer_connection = new_bus(address)
            name = f"org.kde.StatusNotifierItem-{os.getpid()}-{number + 1}"
            own_name(peer_connection, name)
            fixture.add_provider(peer_connection, name, f"tray-menu-native-peer-{number}")
            watcher.call_sync(
                "RegisterStatusNotifierItem", GLib.Variant("(s)", (name,)), Gio.DBusCallFlags.NONE, 3000, None
            )
        result["peers"] = fixture.peers()
        if not args.serve_only:
            # Preserve the production type-resolution path in the staged config root.
            qml_root = args.artifacts / "qml"
            result["qmlRoot"] = str(qml_root.resolve())
            shutil.copytree(REPO / "shell/bingux", qml_root)
            fixture_text = (REPO / "tests/tray-menu-native.qml").read_text()
            fixture_text = fixture_text.replace('import "../shell/bingux"\n', "")
            (qml_root / "shell.qml").write_text(fixture_text)
            with (args.artifacts / "quickshell.log").open("w") as logfile:
                child = subprocess.Popen(
                    [args.quickshell, "-p", str(qml_root), "--no-color"],
                    cwd=REPO,
                    stdout=logfile,
                    stderr=subprocess.STDOUT,
                    env=os.environ.copy(),
                )
            result["quickshellPid"] = child.pid
        result["exit"] = "running"
        save_result()
        loop = GLib.MainLoop()

        def stop(*_):
            result["exit"] = "stopped"
            loop.quit()
            return GLib.SOURCE_REMOVE

        sources.append(GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGINT, stop))
        sources.append(GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM, stop))
        sources.append(GLib.timeout_add_seconds(args.timeout, stop))

        def check_child():
            if child.poll() is not None:
                result["exit"] = "child-exited"
                loop.quit()
                return GLib.SOURCE_REMOVE
            return GLib.SOURCE_CONTINUE

        if child:
            sources.append(GLib.timeout_add_seconds(1, check_child))
        loop.run()
    except Exception as error:
        result.update({"exit": "error", "error": str(error)})
        traceback.print_exc(file=sys.stderr)
    finally:
        for source in sources:
            if GLib.MainContext.default().find_source_by_id(source):
                GLib.source_remove(source)
        if child:
            try:
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=3)
                result["quickshellExitCode"] = child.returncode
            except Exception:
                traceback.print_exc(file=sys.stderr)
        if fixture:
            result.update(
                {"pendingLayouts": len(fixture.pending), "generation": fixture.generation, "peers": fixture.peers()}
            )
            try:
                fixture.cancel_pending_layouts("fixture stopped")
                for provider in fixture.providers.values():
                    for registration in provider["registrations"]:
                        provider["bus"].unregister_object(registration)
                if fixture.control_registration:
                    fixture.connection.unregister_object(fixture.control_registration)
            except Exception:
                traceback.print_exc(file=sys.stderr)
        # Each resource is recorded immediately after acquisition, including on start failures.
        for connection, name in reversed(owned_names):
            try:
                if not connection.is_closed():
                    release_name(connection, name)
            except Exception:
                traceback.print_exc(file=sys.stderr)
        for connection in reversed(connections):
            try:
                if not connection.is_closed():
                    connection.close_sync(None)
            except Exception:
                traceback.print_exc(file=sys.stderr)
        result["finished"] = time.time()
        save_result()
    return 1 if result["exit"] in ("error", "child-exited") else 0


if __name__ == "__main__":
    raise SystemExit(main())
