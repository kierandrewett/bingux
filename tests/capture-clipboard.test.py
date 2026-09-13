"""Native clipboard policy tests; use fake GDK objects and never alter the desktop clipboard."""

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

shell = Path(__file__).resolve().parents[1] / "shell/bingux"
sys.path.insert(0, str(shell))
spec = importlib.util.spec_from_file_location("capture_clipboard", shell / "capture_clipboard.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class NativeClipboardPolicy(unittest.TestCase):
    def test_publishes_final_image_bytes_with_requested_mime(self):
        display = Mock()
        clipboard = display.get_clipboard.return_value
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "saved.jpg"
            path.write_bytes(b"complete image")
            with (
                patch.object(module.Gtk, "init"),
                patch.object(module.Gdk.Display, "get_default", return_value=display),
            ):
                owner = module.NativeClipboard()
                self.assertTrue(owner.set_image(path, "image/jpeg"))

            provider = clipboard.set_content.call_args.args[0]
            self.assertTrue(provider.ref_formats().contain_mime_type("image/jpeg"))
            self.assertEqual(owner.bytes.get_data(), b"complete image")
            self.assertIs(provider, owner.provider)
            clipboard.set_content.assert_called_once_with(provider)

    def test_missing_display_is_a_recoverable_clipboard_error(self):
        with patch.object(module.Gtk, "init"), patch.object(module.Gdk.Display, "get_default", return_value=None):
            with self.assertRaises(module.ClipboardError):
                module.NativeClipboard()


if __name__ == "__main__":
    unittest.main()
