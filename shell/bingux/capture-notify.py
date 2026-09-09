#!/usr/bin/env python3
"""A capture's notification/actions outlive capture-worker and shell reloads."""
from pathlib import Path
import subprocess
import sys
import uuid

from gi.repository import Gio, GLib

NOTIFICATIONS = "org.freedesktop.Notifications"
NOTIFICATION_PATH = "/org/freedesktop/Notifications"
PORTAL = "org.freedesktop.portal.Desktop"


class CaptureNotification:
    def __init__(self, path, kind="screenshot"):
        if kind not in ("screenshot", "recording"):
            raise ValueError("Unknown capture kind")
        self.kind = kind
        self.path = Path(path).absolute()
        self.identity = self.fingerprint()
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.loop = GLib.MainLoop()
        self.identifier = 0
        self.chooser_subscription = 0
        for signal in ("ActionInvoked", "NotificationClosed"):
            self.bus.signal_subscribe(NOTIFICATIONS, NOTIFICATIONS, signal, NOTIFICATION_PATH,
                                      None, Gio.DBusSignalFlags.NONE, self.on_signal)

    def fingerprint(self):
        stat = self.path.lstat()
        if self.path.is_symlink() or not self.path.is_file():
            raise ValueError("Capture is no longer a regular file")
        return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns

    def validate(self):
        if self.fingerprint() != self.identity:
            raise ValueError("Capture changed since capture; leaving it untouched")

    def call(self, destination, path, interface, method, signature, values):
        return self.bus.call_sync(destination, path, interface, method, GLib.Variant(signature, values),
                                  None, Gio.DBusCallFlags.NONE, 5000, None).unpack()

    @property
    def title(self):
        return "Recording" if self.kind == "recording" else "Screenshot"

    def notify(self, summary=None, body=None):
        hints = {"resident": GLib.Variant("b", True),
                 "category": GLib.Variant("s", "transfer.complete")}
        actions = ["default", "Open"]
        if self.kind == "screenshot":
            hints["image-path"] = GLib.Variant("s", self.path.as_uri())
            actions += ["copy", "Copy"]
        actions += ["save", "Save As…", "discard", "Discard"]
        self.identifier = self.call(NOTIFICATIONS, NOTIFICATION_PATH, NOTIFICATIONS, "Notify",
            "(susssasa{sv}i)", ("Capture", self.identifier,
                "media-record-symbolic" if self.kind == "recording" else "screenshot-selection-symbolic",
                summary if summary is not None else self.title + " saved",
                body if body is not None else self.path.name, actions, hints, -1))[0]

    def on_signal(self, bus, sender, path, interface, signal, parameters):
        identifier, value = parameters.unpack()
        if identifier != self.identifier:
            return
        if signal == "NotificationClosed":
            if not self.chooser_subscription:
                self.loop.quit()
            return
        try:
            self.validate()
            if value == "copy" and self.kind == "screenshot":
                mime = "image/jpeg" if self.path.suffix.lower() in (".jpg", ".jpeg") else "image/png"
                with self.path.open("rb") as image:
                    subprocess.run(["wl-copy", "--type", mime], stdin=image, check=True, timeout=4)
                self.notify("Screenshot copied")
            elif value == "save":
                self.save_as()
            elif value == "discard":
                # Never permanently delete if the filesystem has no trash support.
                Gio.File.new_for_path(str(self.path)).trash(None)
                self.call(NOTIFICATIONS, NOTIFICATION_PATH, NOTIFICATIONS, "CloseNotification", "(u)", (self.identifier,))
                self.loop.quit()
            elif value == "default":
                Gio.AppInfo.launch_default_for_uri(self.path.as_uri(), None)
        except (OSError, ValueError, GLib.Error, subprocess.SubprocessError) as error:
            self.notify(self.title + " action failed", str(error))

    def save_as(self):
        if self.chooser_subscription:
            return
        token = "capture_" + uuid.uuid4().hex
        sender = self.bus.get_unique_name()[1:].replace(".", "_")
        request = "/org/freedesktop/portal/desktop/request/" + sender + "/" + token
        self.chooser_subscription = self.bus.signal_subscribe(PORTAL, "org.freedesktop.portal.Request",
            "Response", request, None, Gio.DBusSignalFlags.NONE, self.on_save_response)
        try:
            self.call(PORTAL, "/org/freedesktop/portal/desktop", "org.freedesktop.portal.FileChooser", "SaveFile",
                "(ssa{sv})", ("", "Save " + self.kind, {"handle_token": GLib.Variant("s", token),
                    "current_name": GLib.Variant("s", self.path.name), "modal": GLib.Variant("b", True)}))
        except Exception:
            self.bus.signal_unsubscribe(self.chooser_subscription)
            self.chooser_subscription = 0
            raise

    def on_save_response(self, bus, sender, path, interface, signal, parameters):
        self.bus.signal_unsubscribe(self.chooser_subscription)
        self.chooser_subscription = 0
        response, results = parameters.unpack()
        if response != 0:
            return
        try:
            self.validate()
            uris = results.get("uris", [])
            if len(uris) != 1:
                raise ValueError("The file picker did not return a destination")
            source = Gio.File.new_for_path(str(self.path))
            destination = Gio.File.new_for_uri(uris[0])
            if not source.equal(destination):
                # Overwrite is explicitly confirmed by the native SaveFile dialog.
                source.copy(destination, Gio.FileCopyFlags.OVERWRITE, None, None, None)
            self.notify(self.title + " saved", "Saved to " + destination.get_parse_name())
        except (OSError, ValueError, GLib.Error) as error:
            self.notify("Could not save " + self.kind, str(error))

    def run(self):
        self.notify()
        try:
            print("notified", flush=True)
        except BrokenPipeError:
            # A slow server may accept after the capture worker's acknowledgement timeout.
            # Keep the notification actions alive even if the worker moved on.
            sys.stdout = None
        self.loop.run()


if __name__ == "__main__":
    CaptureNotification(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else "screenshot").run()
