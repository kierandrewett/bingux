#!/usr/bin/env python3
"""Native image clipboard ownership for capture results.

Gnoblin's focus-independent ext-data-control protocol keeps a small native
owner process alive. GDK is retained as a fallback for compositors that do not
provide that protocol.
"""

from pathlib import Path
import os
import select
import shutil
import subprocess

import gi

gi.require_version("Gdk", "4.0")
gi.require_version("Gtk", "4.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402 - Select GI versions before importing their modules.


class ClipboardError(RuntimeError):
    """The native Wayland clipboard could not accept the image source."""


class NativeClipboard:
    def __init__(self):
        # GDK refuses display access before GTK has been initialized.  This is
        # deliberately lazy: importing capture policy tests must not require
        # a graphical session or change the user's clipboard.
        Gtk.init()
        display = Gdk.Display.get_default()
        if display is None:
            raise ClipboardError("No graphical display is available for the clipboard")
        self.display = display
        self.clipboard = display.get_clipboard()
        self.bytes = None
        self.provider = None

    def set_image(self, path, mime):
        path = Path(path)
        self.bytes = GLib.Bytes.new(path.read_bytes())
        self.provider = Gdk.ContentProvider.new_for_bytes(mime, self.bytes)
        if not self.clipboard.set_content(self.provider):
            raise ClipboardError("The Wayland clipboard rejected the image")
        return True


_native_clipboard = None
_owner_process = None


def _owner_binary():
    configured = os.environ.get("BINGUX_IMAGE_CLIPBOARD")
    candidates = [Path(configured)] if configured else []
    candidates += [
        Path("/usr/libexec/bingux/bingux-image-clipboard"),
        Path("/usr/local/libexec/bingux/bingux-image-clipboard"),
        Path(__file__).resolve().parents[2] / "build/bingux-image-clipboard",
    ]
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
    return shutil.which("bingux-image-clipboard")


def _stop_owner(process):
    if process and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=2)


def _copy_with_native_owner(path, mime, binary):
    global _owner_process
    previous = _owner_process if _owner_process and _owner_process.poll() is None else None
    candidate = subprocess.Popen(
        [binary, mime, str(path)],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        start_new_session=True,
    )
    try:
        if not select.select([candidate.stdout], [], [], 4)[0] or candidate.stdout.readline().strip() != "ready":
            raise ClipboardError("Native Wayland clipboard owner did not start")
    except (ClipboardError, OSError, subprocess.SubprocessError):
        _stop_owner(candidate)
        raise
    _owner_process = candidate
    _stop_owner(previous)
    return True


def copy_image_to_clipboard(path, mime):
    """Publish an image through the native Wayland clipboard owner."""
    binary = _owner_binary()
    if binary:
        try:
            return _copy_with_native_owner(Path(path), mime, binary)
        except ClipboardError:
            pass
    global _native_clipboard
    if _native_clipboard is None:
        _native_clipboard = NativeClipboard()
    return _native_clipboard.set_image(path, mime)
