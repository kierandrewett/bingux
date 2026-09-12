#!/usr/bin/env python3
"""Read-only JSON-lines bridge to the desktop calendar service.

Recurrence and timezone expansion remain owned by Evolution Data Server.
"""

import json
import sys
import threading
from gi.repository import Gio, GLib

SERVICE = "org.gnome.Shell.CalendarServer"
PATH = "/org/gnome/Shell/CalendarServer"


class EventStore:
    def __init__(self):
        self.events = {}

    def update(self, appointments):
        parents = set()
        for uid, title, start, end, _extras in appointments:
            if not uid.endswith("\n"):
                parent = uid.rsplit("\n", 1)[0] + "\n"
                if parent not in parents:
                    self.remove_prefix(parent)
                    parents.add(parent)
            self.events[uid] = dict(id=uid, title=title or "Untitled event", start=start, end=end)

    def remove_prefix(self, prefix):
        self.events = {uid: event for uid, event in self.events.items() if not uid.startswith(prefix)}

    def within(self, since, until):
        return sorted(
            (
                event
                for event in self.events.values()
                if event["start"] < until and max(event["end"], event["start"] + 1) > since
            ),
            key=lambda event: (event["start"], event["end"], event["title"].casefold()),
        )


def main():
    loop = GLib.MainLoop()
    store = EventStore()
    state = dict(since=0, until=0, available=False, loading=False, error="")

    def emit():
        print(json.dumps(dict(state, events=store.within(state["since"], state["until"]))), flush=True)

    try:
        proxy = Gio.DBusProxy.new_for_bus_sync(
            Gio.BusType.SESSION, Gio.DBusProxyFlags.NONE, None, SERVICE, PATH, SERVICE, None
        )
        proxy.set_default_timeout(5000)
    except GLib.Error:
        print(json.dumps(dict(state, events=[], error="Calendar service unavailable")), flush=True)
        return

    def properties(*_args):
        value = proxy.get_cached_property("HasCalendars")
        state["available"] = bool(proxy.get_name_owner() and value and value.unpack())
        emit()

    def signal(_proxy, _sender, name, parameters):
        values = parameters.unpack()[0]
        if name == "EventsAddedOrUpdated":
            store.update(values)
        elif name == "EventsRemoved":
            for uid in values:
                store.remove_prefix(uid)
        elif name == "ClientDisappeared":
            store.remove_prefix(values + "\n")
        state["loading"] = False
        emit()

    def request(data):
        try:
            since, until = int(data["since"]), int(data["until"])
            if not 0 < until - since <= 62 * 86400:
                raise ValueError("Invalid calendar range")
        except (KeyError, TypeError, ValueError):
            return False
        # Keep the last snapshot while EDS refreshes. The persistent worker also
        # receives removals while the popup is closed, so cached rows stay live.
        state.update(since=since, until=until, loading=True, error="")
        emit()
        try:
            proxy.call_sync(
                "SetTimeRange", GLib.Variant("(xxb)", (since, until, True)), Gio.DBusCallFlags.NONE, 5000, None
            )
        except GLib.Error:
            state["error"] = "Could not load calendar events"
        state["loading"] = False
        properties()
        return False

    def read_requests():
        for line in sys.stdin:
            try:
                data = json.loads(line)
                if isinstance(data, dict):
                    GLib.idle_add(request, data)
            except ValueError:
                continue
        GLib.idle_add(loop.quit)

    proxy.connect("g-signal", signal)
    proxy.connect("g-properties-changed", properties)
    proxy.connect("notify::g-name-owner", properties)
    print(json.dumps(dict(ready=True)), flush=True)
    properties()
    threading.Thread(target=read_requests, daemon=True).start()
    loop.run()


if __name__ == "__main__":
    main()
