"""Notification action policy, without touching the desktop or user clipboard."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("capture_notify", Path(__file__).resolve().parents[1] / "shell/bingux/capture-notify.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class NotificationActions(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.path = Path(self.folder.name) / "capture.png"
        self.path.write_bytes(b"screenshot")
        self.notice = module.CaptureNotification.__new__(module.CaptureNotification)
        self.notice.kind = "screenshot"
        self.notice.path = self.path
        self.notice.identity = self.notice.fingerprint()
        self.notice.identifier = 42
        self.notice.chooser_subscription = 0
        self.notice.loop = Mock()
        self.notice.notify = Mock()
        self.notice.call = Mock()
        self.notice.bus = Mock()

    def invoke(self, action, identifier=42):
        self.notice.on_signal(None, None, None, None, "ActionInvoked", module.GLib.Variant("(us)", (identifier, action)))

    def test_preview_and_actions_are_standard_notification_fields(self):
        self.notice.call.return_value = (42,)
        module.CaptureNotification.notify(self.notice)
        values = self.notice.call.call_args.args[-1]
        self.assertEqual(values[5], ["default", "Open", "copy", "Copy", "save", "Save As…", "discard", "Discard"])
        self.assertEqual(values[6]["image-path"].unpack(), self.path.as_uri())
        self.assertTrue(values[6]["resident"].unpack())

    def test_recording_uses_normal_notification_without_image_or_copy(self):
        self.notice.kind = "recording"
        self.notice.call.return_value = (42,)
        module.CaptureNotification.notify(self.notice)
        values = self.notice.call.call_args.args[-1]
        self.assertEqual(values[2:4], ("media-record-symbolic", "Recording saved"))
        self.assertEqual(values[5], ["default", "Open", "save", "Save As…", "discard", "Discard"])
        self.assertNotIn("image-path", values[6])
        with patch.object(module.subprocess, "run") as run:
            self.invoke("copy")
            run.assert_not_called()

    def test_recording_opens_its_saved_file(self):
        self.notice.kind = "recording"
        with patch.object(module.Gio.AppInfo, "launch_default_for_uri") as launch:
            self.invoke("default")
            launch.assert_called_once_with(self.path.as_uri(), None)

    def test_copy_reads_this_capture_not_latest_capture(self):
        with patch.object(module.subprocess, "run") as run:
            self.invoke("copy")
            self.assertEqual(run.call_args.args[0], ["wl-copy", "--type", "image/png"])
            self.assertEqual(run.call_args.kwargs["stdin"].name, str(self.path))
        self.notice.notify.assert_called_with("Screenshot copied")

    def test_other_notifications_do_not_trigger_actions(self):
        self.invoke("discard", identifier=43)
        self.assertTrue(self.path.exists())
        self.notice.notify.assert_not_called()

    def test_changed_file_is_not_discarded(self):
        self.path.write_bytes(b"replacement must survive")
        self.invoke("discard")
        self.assertTrue(self.path.exists())
        self.assertEqual(self.notice.notify.call_args.args[0], "Screenshot action failed")

    def test_discard_uses_trash_not_unlink(self):
        with patch.object(module.Gio.File, "new_for_path") as file:
            self.invoke("discard")
            file.return_value.trash.assert_called_once_with(None)
        self.notice.loop.quit.assert_called_once()

    def test_save_cancel_leaves_original(self):
        self.notice.chooser_subscription = 12
        self.notice.on_save_response(None, None, None, None, None, module.GLib.Variant("(ua{sv})", (1, {})))
        self.assertTrue(self.path.exists())
        self.notice.notify.assert_not_called()

    def test_save_copies_to_selected_destination(self):
        destination = Path(self.folder.name) / "chosen.png"
        self.notice.chooser_subscription = 12
        self.notice.on_save_response(None, None, None, None, None,
            module.GLib.Variant("(ua{sv})", (0, {"uris": module.GLib.Variant("as", [destination.as_uri()])})))
        self.assertEqual(destination.read_bytes(), self.path.read_bytes())


if __name__ == "__main__":
    unittest.main()
