"""Notification payload and action tests; never launches a real application."""
import sys
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "shell/bingux"))
import shell_notify


class ShellNotifications(unittest.TestCase):
    def test_plain_error_uses_standard_notify_and_escapes_body(self):
        bus = Mock()
        bus.call_sync.return_value.unpack.return_value = (42,)
        with patch.object(shell_notify.Gio, "bus_get_sync", return_value=bus):
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
        with (patch.object(shell_notify.Gio, "bus_get_sync", return_value=bus),
              patch.object(shell_notify.GLib, "MainLoop", return_value=loop),
              patch.object(shell_notify.subprocess, "Popen") as launch):
            shell_notify.notify({"title": "Could not open app", "retryDesktopId": "org.example.App"})
        self.assertEqual(bus.call_sync.call_args.args[4].unpack()[5], ["retry", "Retry"])
        self.assertEqual(launch.call_args.args[0][-2:], ["--notify-errors", "org.example.App"])
        self.assertNotIn("shell", launch.call_args.kwargs)
        loop.quit.assert_called_once()

    def test_invalid_retry_is_rejected(self):
        with patch.object(shell_notify.Gio, "bus_get_sync"):
            with self.assertRaises(ValueError):
                shell_notify.notify({"title": "Error", "retryDesktopId": "/tmp/not-an-app"})


if __name__ == "__main__":
    unittest.main()
