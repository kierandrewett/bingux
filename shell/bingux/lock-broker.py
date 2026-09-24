#!/usr/bin/env python3
"""Report a Bingux lock boundary to the trusted Gnoblin lock broker."""

import argparse
import os
import sys

from gi.repository import Gio, GLib


DESTINATION = "org.gnoblin.Lock"
OBJECT_PATH = "/org/gnoblin/Lock"
INTERFACE = "org.gnoblin.Lock"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("event", choices=("presented",))
    args = parser.parse_args()
    token = os.environ.get("GNOBLIN_LOCK_TOKEN")
    if not token:
        print("bingux-lock: GNOBLIN_LOCK_TOKEN is required", file=sys.stderr)
        return 2
    method = {"presented": "ReportPresented"}[args.event]
    try:
        Gio.bus_get_sync(Gio.BusType.SESSION, None).call_sync(
            DESTINATION,
            OBJECT_PATH,
            INTERFACE,
            method,
            GLib.Variant("(s)", (token,)),
            None,
            Gio.DBusCallFlags.NONE,
            3000,
            None,
        )
    except GLib.Error as error:
        print(f"bingux-lock: {method} failed: {error.message}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
