#!/usr/bin/env python3
"""Launch a desktop entry through GIO, including its optional new-window action."""
import argparse
import sys
import gi

gi.require_version("Gio", "2.0")
gi.require_version("GioUnix", "2.0")
from gi.repository import Gio, GioUnix, GLib


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--new-window", action="store_true")
    parser.add_argument("desktop_id")
    args = parser.parse_args(argv)
    identity = args.desktop_id
    if "/" in identity or not identity:
        parser.error("Expected a desktop entry ID")
    if not identity.endswith(".desktop"):
        identity += ".desktop"
    try:
        entry = GioUnix.DesktopAppInfo.new(identity)
    except TypeError:
        entry = None
    if entry is None:
        print("Installed application not found", file=sys.stderr)
        return 1
    try:
        context = Gio.AppLaunchContext()
        if args.new_window and "new-window" in entry.list_actions():
            entry.launch_action("new-window", context)
        elif not entry.launch([], context):
            raise RuntimeError("Application launch was rejected")
    except (GLib.Error, RuntimeError) as error:
        print(f"Could not launch application: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
