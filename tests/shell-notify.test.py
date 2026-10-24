"""Notification payload and action tests; never launches a real application."""

import sys
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "shell/bingux"))
import shell_notify
from bingux_runtime import HelperError

from gi.repository import Gio, GLib


class ShellNotifications(unittest.TestCase):
    def test_plain_error_uses_standard_notify_and_escapes_body(self):
        bus = Mock()
        bus.call_sync.return_value.unpack.return_value = (42,)
        with patch.object(Gio, "bus_get_sync", return_value=bus):
            self.assertEqual(shell_notify.notify({"title": "Capture failed", "body": "Missing <file> & path"}), 42)
        values = bus.call_sync.call_args.args[4].unpack()
        self.assertEqual(values[0], "Bingux")
        self.assertEqual(values[3:6], ("Capture failed", "Missing &lt;file&gt; &amp; path", []))
        bus.signal_subscribe.assert_not_called()

    def test_retry_action_uses_desktop_launcher_without_a_shell(self):
        bus, loop = Mock(), Mock()
        bus.call_sync.return_value.unpack.return_value = (42,)
        callbacks = {}
        bus.signal_subscribe.side_effect = lambda *args: callbacks.update({args[2]: args[-1]})
        parameters = Mock()
        parameters.unpack.return_value = (42, "retry")
        loop.run.side_effect = lambda: callbacks["ActionInvoked"](None, None, None, None, "ActionInvoked", parameters)
        with (
            patch.object(Gio, "bus_get_sync", return_value=bus),
            patch.object(GLib, "MainLoop", return_value=loop),
            patch.object(GLib, "timeout_add_seconds"),
            patch.object(shell_notify.subprocess, "Popen") as launch,
        ):
            shell_notify.notify({"title": "Could not open app", "retryDesktopId": "org.example.App"})
        self.assertEqual(bus.call_sync.call_args.args[4].unpack()[5], ["retry", "Retry"])
        self.assertEqual(launch.call_args.args[0][-2:], ["--notify-errors", "org.example.App"])
        self.assertNotIn("shell", launch.call_args.kwargs)
        loop.quit.assert_called_once()

    def test_retry_wait_ends_after_timeout_without_notification_closed(self):
        bus, loop = Mock(), Mock()
        bus.call_sync.return_value.unpack.return_value = (42,)
        bus.signal_subscribe.side_effect = lambda *args: None
        timer = {}
        loop.run.side_effect = lambda: timer["callback"]()
        with (
            patch.object(Gio, "bus_get_sync", return_value=bus),
            patch.object(GLib, "MainLoop", return_value=loop),
            patch.object(
                GLib,
                "timeout_add_seconds",
                side_effect=lambda seconds, callback: timer.update(seconds=seconds, callback=callback),
            ) as add,
            patch.object(shell_notify.subprocess, "Popen") as launch,
        ):
            self.assertEqual(
                shell_notify.notify({"title": "Could not open app", "retryDesktopId": "org.example.App"}), 42
            )
        self.assertEqual(add.call_args.args[0], shell_notify.RETRY_WAIT_SECONDS)
        self.assertEqual(bus.call_sync.call_args.args[3], "CloseNotification")
        self.assertEqual(bus.call_sync.call_args.args[4].unpack(), (42,))
        loop.quit.assert_called_once()
        launch.assert_not_called()

    def test_invalid_retry_is_rejected(self):
        with patch.object(Gio, "bus_get_sync"):
            with self.assertRaises(HelperError) as caught:
                shell_notify.notify({"title": "Error", "retryDesktopId": "/tmp/not-an-app"})
        self.assertEqual(caught.exception.code, "invalid")

    def test_payload_must_be_an_object_with_a_title(self):
        with self.assertRaises(HelperError) as caught:
            shell_notify.parse_payload("[1]")
        self.assertEqual(caught.exception.code, "protocol")
        with self.assertRaises(HelperError):
            shell_notify.parse_payload("{not json")

    def test_help_and_bogus_argument_exit_codes(self):
        script = str(Path(__file__).resolve().parents[1] / "shell/bingux/shell_notify.py")
        self.assertEqual(subprocess.run([sys.executable, script, "--help"], capture_output=True).returncode, 0)
        self.assertEqual(subprocess.run([sys.executable, script, "--bogus"], capture_output=True).returncode, 2)


if __name__ == "__main__":
    unittest.main()
