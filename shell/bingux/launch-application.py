#!/usr/bin/env python3
"""Launch a desktop entry through GIO, including its optional new-window action."""
import argparse
import sys
import os
import time
import gi

gi.require_version("Gio", "2.0")
gi.require_version("GioUnix", "2.0")
from gi.repository import Gio, GioUnix, GLib


def report_failure(name, message, notify):
    print(f"Could not launch {name}: {message}", file=sys.stderr, flush=True)
    if notify:
        try:
            bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications", "Notify",
                GLib.Variant("(susssasa{sv}i)", ("Bingux", 0, "dialog-error",
                    f"Could not open {name}", message[:1000], [], {}, -1)),
                None, Gio.DBusCallFlags.NONE, 3000, None)
        except GLib.Error as error:
            print(f"Launch error notification failed: {error.message}", file=sys.stderr)
    return 1


def launch_and_watch(entry, context):
    children = []
    accepted = entry.launch_uris_as_manager_with_fds([], context,
        GLib.SpawnFlags.SEARCH_PATH | GLib.SpawnFlags.DO_NOT_REAP_CHILD,
        None, None, lambda _app, pid, _data: children.append(pid), None, -1, -1, -1)
    if not accepted:
        raise RuntimeError("Application launch was rejected")
    # Some launchers hand off to an existing instance and exit successfully.
    # Only a non-zero exit or signal is a confirmed failure.
    deadline = time.monotonic() + 4
    while children and time.monotonic() < deadline:
        for pid in children[:]:
            done, status = os.waitpid(pid, os.WNOHANG)
            if not done:
                continue
            children.remove(pid)
            code = os.waitstatus_to_exitcode(status)
            if code:
                reason = f"The application exited with code {code}." if code > 0 else f"The application stopped with signal {-code}."
                raise RuntimeError(reason + " Diagnostic output is in the Quickshell service log.")
        if children:
            time.sleep(.05)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report-timeout", action="store_true")
    parser.add_argument("--notify-errors", action="store_true")
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
        return report_failure(identity, "The installed desktop entry could not be found.", args.notify_errors)
    if args.report_timeout:
        return report_failure(entry.get_display_name(), "No application window appeared before the launch timeout. The application may still be starting or running in the background.", True)
    try:
        context = Gio.AppLaunchContext()
        if args.new_window and "new-window" in entry.list_actions():
            entry.launch_action("new-window", context)
        else:
            launch_and_watch(entry, context)
    except (GLib.Error, RuntimeError) as error:
        return report_failure(entry.get_display_name(), str(error), args.notify_errors)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
