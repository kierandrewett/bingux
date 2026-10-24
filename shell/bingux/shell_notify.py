#!/usr/bin/env python3
"""Send a shell message through the desktop notification server.

Usage: shell_notify.py PAYLOAD_JSON

PAYLOAD_JSON is an object with `title` and optionally `body`, `icon`, and
`retryDesktopId` (a desktop entry ID to relaunch with --notify-errors when the
user presses Retry). Shell messages never use private popups.
"""

import argparse
import html
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bingux_runtime import HelperError, main, require_gi  # noqa: E402

TAG = "notify"
SERVICE = "org.freedesktop.Notifications"
OBJECT = "/org/freedesktop/Notifications"
RETRY_WAIT_SECONDS = 600
CALL_TIMEOUT_MS = 3000


def notify(payload):
    Gio, GLib = require_gi(TAG, Gio="2.0", GLib="2.0")
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    loop = GLib.MainLoop()
    identifier = 0
    retry = payload.get("retryDesktopId", "")
    if retry and ("/" in retry or "\0" in retry):
        raise HelperError("Retry requires a desktop entry ID", code="invalid")

    def give_up():
        # Nobody pressed Retry in time. Close the notification so a dead
        # Retry button is not left on screen, then let the helper exit.
        try:
            bus.call_sync(
                SERVICE,
                OBJECT,
                SERVICE,
                "CloseNotification",
                GLib.Variant("(u)", (identifier,)),
                None,
                Gio.DBusCallFlags.NONE,
                CALL_TIMEOUT_MS,
                None,
            )
        except GLib.Error:
            pass
        loop.quit()
        return False

    def on_signal(_bus, _sender, _path, _interface, signal, parameters):
        notice, value = parameters.unpack()
        if notice != identifier:
            return
        if signal == "NotificationClosed":
            loop.quit()
        elif value == "retry" and retry:
            subprocess.Popen(
                [sys.executable, str(Path(__file__).with_name("launch-application.py")), "--notify-errors", retry],
                start_new_session=True,
            )
            loop.quit()

    if retry:
        for signal in ("ActionInvoked", "NotificationClosed"):
            bus.signal_subscribe(SERVICE, SERVICE, signal, OBJECT, None, Gio.DBusSignalFlags.NONE, on_signal)
    try:
        identifier = bus.call_sync(
            SERVICE,
            OBJECT,
            SERVICE,
            "Notify",
            GLib.Variant(
                "(susssasa{sv}i)",
                (
                    "Bingux",
                    0,
                    payload.get("icon", "dialog-error-symbolic"),
                    str(payload["title"]),
                    html.escape(str(payload.get("body", ""))),
                    ["retry", "Retry"] if retry else [],
                    {"category": GLib.Variant("s", "device.error")},
                    -1,
                ),
            ),
            None,
            Gio.DBusCallFlags.NONE,
            CALL_TIMEOUT_MS,
            None,
        ).unpack()[0]
    except GLib.Error as error:
        raise HelperError(
            f"The notification server rejected the message: {error.message}", code="unavailable"
        ) from None
    if retry:
        # Bound the helper's lifetime: without a Retry press the wait ends
        # after RETRY_WAIT_SECONDS.
        GLib.timeout_add_seconds(RETRY_WAIT_SECONDS, give_up)
        loop.run()
    return identifier


def parse_payload(text):
    try:
        payload = json.loads(text)
    except ValueError:
        raise HelperError("The notification payload is not valid JSON.", code="protocol") from None
    if not isinstance(payload, dict) or "title" not in payload:
        raise HelperError("The notification payload needs a JSON object with a title.", code="protocol")
    return payload


@main(TAG)
def cli(argv=None):
    parser = argparse.ArgumentParser(
        description="Send a Bingux shell message through the desktop notification server.",
        epilog='Example: shell_notify.py \'{"title": "Capture failed", "body": "Try again."}\'',
    )
    parser.add_argument("payload", help="JSON object with title, and optionally body, icon, retryDesktopId")
    args = parser.parse_args(argv)
    notify(parse_payload(args.payload))


if __name__ == "__main__":
    cli()
