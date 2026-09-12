"""Pure capture policy regressions; no screen or microphone access."""

import importlib.util
from pathlib import Path
import unittest
import tempfile
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location(
    "capture", Path(__file__).resolve().parents[1] / "shell/bingux/capture_backend.py"
)
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


class CapturePolicy(unittest.TestCase):
    def worker(self, available, usable):
        worker = capture.Capture.__new__(capture.Capture)
        worker.encoders = available
        worker.encoder_works = Mock(side_effect=lambda name: name in usable)
        return worker

    def test_window_crop_fallback_removes_monitor_padding(self):
        capture.Gst.init(None)
        buffer = capture.Gst.Buffer.new()
        bounds = capture.window_crop(buffer, 3440, 1440, {"bufferWidth": 1280, "bufferHeight": 786})
        self.assertEqual(bounds, {"left": 0, "top": 0, "right": 2160, "bottom": 654})
        with self.assertRaises(ValueError):
            capture.window_crop(buffer, 3440, 1440)

    def test_window_id_validation(self):
        self.assertEqual(capture.settings({"target": "window", "windowId": "1447104021"})["windowId"], "1447104021")
        for identity in ("wrong", "-1", "0", str(2**64)):
            with self.assertRaises(ValueError):
                capture.settings({"target": "window", "windowId": identity})

    def test_native_window_uses_selected_identity(self):
        worker = capture.Capture.__new__(capture.Capture)
        worker.job = {"target": "window", "windowId": "1447104021", "cursor": False, "kind": "screenshot"}
        worker.call = Mock(side_effect=[("/session",), ("/stream",), ()])
        worker.bus = Mock()
        worker.refresh_window_bounds = Mock()
        worker.subscriptions = []
        worker.native_stream()
        request = worker.call.call_args_list[1].args
        self.assertEqual(request[3], "RecordWindow")
        self.assertEqual(request[4], "(a{sv})")
        self.assertEqual(request[5][0]["window-id"].unpack(), 1447104021)

    def test_shutter_respects_event_sound_preference(self):
        for enabled in (False, True):
            with (
                self.subTest(enabled=enabled),
                patch.object(capture.shutil, "which", return_value="/usr/bin/canberra-gtk-play"),
                patch.object(capture.Gio.SettingsSchemaSource, "get_default"),
                patch.object(capture.Gio.Settings, "new_full") as settings,
                patch.object(capture.subprocess, "Popen") as player,
            ):
                settings.return_value.get_boolean.return_value = enabled
                settings.return_value.get_string.return_value = "freedesktop"
                capture.play_shutter()
                self.assertEqual(player.call_count, int(enabled))
                if enabled:
                    self.assertIn("screen-capture", player.call_args.args[0])
                    self.assertIn("canberra.xdg-theme.name=freedesktop", player.call_args.args[0])

    def test_only_successful_screenshot_save_plays_shutter(self):
        for kind, has_output in (("screenshot", True), ("recording", True), ("screenshot", False)):
            with self.subTest(kind=kind, has_output=has_output), tempfile.TemporaryDirectory() as directory:
                worker = capture.Capture.__new__(capture.Capture)
                worker.job = {"kind": kind, "copy": False}
                worker.final = Path(directory) / "saved"
                worker.temporary = Path(directory) / "partial"
                if has_output:
                    worker.temporary.write_bytes(b"capture")
                worker.cleanup = Mock()
                worker.clear_preview = Mock()
                worker.emit = Mock()
                with (
                    patch.object(capture, "play_shutter") as shutter,
                    patch.object(capture.subprocess, "Popen"),
                    patch.object(capture.select, "select", return_value=([], [], [])),
                ):
                    worker.complete()
                    self.assertEqual(shutter.call_count, int(kind == "screenshot" and has_output))

    def test_software_only_never_probes_hardware(self):
        worker = self.worker(["vah264enc", "nvh264enc", "x264enc", "openh264enc"], {"vah264enc", "openh264enc"})
        self.assertEqual(worker.choose_encoder("cpu"), "openh264enc")
        self.assertEqual([call.args[0] for call in worker.encoder_works.call_args_list], ["x264enc", "openh264enc"])

    def test_installed_but_unusable_gpu_falls_back(self):
        worker = self.worker(["vah264enc", "nvh264enc", "openh264enc"], {"openh264enc"})
        self.assertEqual(worker.choose_encoder("auto"), "openh264enc")

    def test_no_gpu_plugins_required(self):
        worker = self.worker(["x264enc"], {"x264enc"})
        self.assertEqual(worker.choose_encoder("auto"), "x264enc")

    def test_missing_encoders(self):
        self.assertIsNone(self.worker([], set()).choose_encoder("auto"))

    def test_hardware_when_verified(self):
        worker = self.worker(["vah264enc", "openh264enc"], {"vah264enc", "openh264enc"})
        self.assertEqual(worker.choose_encoder("auto"), "vah264enc")

    def test_compressed_defaults(self):
        options = capture.settings({"target": "screen"})
        self.assertEqual(
            (options["encoder"], options["fps"], options["maxHeight"], options["audio"]), ("auto", 30, 1080, "none")
        )

    def test_even_dimensions_and_no_upscale(self):
        self.assertEqual(capture.video_size(3440, 1440, 1080), (2580, 1080))
        self.assertEqual(capture.video_size(641, 361, 1080), (640, 360))

    def test_invalid_region_and_encoder(self):
        for options in ({}, {"target": "screen", "encoder": "raw"}):
            with self.assertRaises(ValueError):
                capture.settings(options)

    def test_bitrate_scales_with_quality_and_resolution(self):
        self.assertLess(capture.bitrate("compact", 1920, 1080, 30), capture.bitrate("high", 1920, 1080, 30))
        self.assertLess(capture.bitrate("balanced", 640, 360, 30), capture.bitrate("balanced", 1920, 1080, 30))

    def test_hidpi_region_and_negative_monitor_origin(self):
        region = dict(x=-900, y=100, width=400, height=300)
        metadata = dict(position=(-1000, 0), size=(1000, 800))
        self.assertEqual(
            capture.region_crop(region, metadata, 2000, 1600), dict(left=200, top=200, right=1000, bottom=800)
        )

    def test_wrong_monitor_rejected(self):
        with self.assertRaises(ValueError):
            capture.region_crop(
                dict(x=2000, y=0, width=400, height=300), dict(position=(0, 0), size=(1920, 1080)), 1920, 1080
            )

    def test_frozen_capture_uses_selected_cursor_variant_and_crop(self):
        with tempfile.TemporaryDirectory(prefix="capture-frozen-test-") as directory:
            paths = {}
            for name, color in (("plain", 0xFF0000FF), ("cursor", 0x0000FFFF)):
                path = Path(directory) / (name + ".png")
                image = capture.GdkPixbuf.Pixbuf.new(capture.GdkPixbuf.Colorspace.RGB, False, 8, 20, 16)
                image.fill(color)
                image.savev(str(path), "png", [], [])
                paths[name] = path
            worker = capture.Capture.__new__(capture.Capture)
            worker.preview_token = "test-frame"
            worker.previews = {"screen": dict(paths=paths, geometry=dict(x=0, y=0, width=10, height=8))}
            worker.temporary = Path(directory) / "result.png"
            for visible, expected in ((False, (255, 0, 0)), (True, (0, 0, 255))):
                worker.job = capture.settings(
                    dict(
                        previewToken="test-frame",
                        screen="screen",
                        cursor=visible,
                        region=dict(x=2, y=1, width=4, height=2),
                    )
                )
                self.assertTrue(worker.frozen_screenshot())
                result = capture.GdkPixbuf.Pixbuf.new_from_file(str(worker.temporary))
                self.assertEqual((result.get_width(), result.get_height()), (8, 4))
                self.assertEqual(tuple(result.get_pixels()[:3]), expected)
            worker.job["previewToken"] = "stale"
            self.assertFalse(worker.frozen_screenshot())


if __name__ == "__main__":
    unittest.main()
