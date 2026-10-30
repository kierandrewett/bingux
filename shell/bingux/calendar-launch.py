#!/usr/bin/env python3
"""Open a Calendar event in its month, using the running app's date locale."""

import argparse
import datetime
import locale
import os
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bingux_runtime import require_gi  # noqa: E402

require_gi("calendar-launch", Gio="2.0", GLib="2.0")
from gi.repository import Gio, GLib  # noqa: E402 - require_gi selects the typelib versions first.
from application_spawn import spawn_detached  # noqa: E402


def calendar_locale(bus):
    env = os.environ
    try:
        reply = bus.call_sync(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            GLib.Variant("(s)", ("org.gnome.Calendar",)),
            GLib.VariantType.new("(u)"),
            Gio.DBusCallFlags.NONE,
            1000,
            None,
        )
        pid = reply.unpack()[0]
        env = dict(part.split("=", 1) for part in Path(f"/proc/{pid}/environ").read_text().split("\0") if "=" in part)
    except (GLib.Error, OSError, ValueError):
        pass
    return env.get("LC_ALL") or env.get("LC_TIME") or env.get("LANG") or "C"


GNOME_CALENDAR_ID = "org.gnome.Calendar.desktop"


def default_calendar():
    """The user's default text/calendar handler, or None when none is set."""
    return Gio.AppInfo.get_default_for_type("text/calendar", False)


def open_other_calendar(app):
    """Start the chosen calendar app. Only GNOME Calendar has date and event options."""
    spawn_detached(["gio", "launch", app.get_filename()])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--date")
    parser.add_argument("--uuid")
    parser.add_argument("--open", action="store_true", help="Start the default calendar without a date")
    args = parser.parse_args()
    if not args.open and not args.date:
        parser.error("--date is required unless --open is given")
    app = default_calendar()
    if app is not None and app.get_id() != GNOME_CALENDAR_ID and app.get_filename():
        open_other_calendar(app)
        return
    if args.open:
        spawn_detached(["gnome-calendar"])
        return
    date = datetime.date.fromisoformat(args.date)
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    try:
        locale.setlocale(locale.LC_TIME, calendar_locale(bus))
    except locale.Error:
        locale.setlocale(locale.LC_TIME, "")
    # Calendar's CLI uses EDS's locale date parser, not ISO 8601.
    date_format = locale.nl_langinfo(locale.D_FMT).replace("%y", "%Y")
    spawn_detached(
        ["gnome-calendar", "--date", date.strftime(date_format)],
    )
    if not args.uuid:
        return
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        reply = bus.call_sync(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            GLib.Variant("(s)", ("org.gnome.Calendar",)),
            GLib.VariantType.new("(b)"),
            Gio.DBusCallFlags.NONE,
            1000,
            None,
        )
        if reply.unpack()[0]:
            # Let the date command reach the application before the event action.
            time.sleep(0.2)
            spawn_detached(
                ["gnome-calendar", "--uuid", args.uuid],
            )
            return
        time.sleep(0.1)
    raise SystemExit("Calendar did not start")


if __name__ == "__main__":
    main()
