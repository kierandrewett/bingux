import importlib.util
import base64
import json
from pathlib import Path
import subprocess
import sys
import os
import selectors
import time
import tempfile
import unittest
from urllib.parse import quote

HELPER = Path(__file__).resolve().parents[1] / "shell/bingux/render-os-icons.py"
spec = importlib.util.spec_from_file_location("os_icons", HELPER)
icons = importlib.util.module_from_spec(spec)
spec.loader.exec_module(icons)


class IconRenderingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.cache = icons.RenderCache()
        self.theme = icons.Gtk.IconTheme.get_default()

    def test_real_gnome_web_keeps_transparent_corners(self):
        if self.theme.lookup_icon("org.gnome.Epiphany", icons.SIZE, 0) is None:
            self.skipTest("GNOME Web icon is not installed")
        uri = icons.resolve("image://icon/org.gnome.Epiphany", self.theme, self.cache)
        self.assertTrue(uri.startswith("data:image/png;base64,"))
        loader = icons.GdkPixbuf.PixbufLoader.new_with_type("png")
        loader.write(base64.b64decode(uri.split(",", 1)[1]))
        loader.close()
        image = loader.get_pixbuf()
        self.assertTrue(image.get_has_alpha())
        self.assertEqual(image.get_pixels()[3], 0, "top-left corner must not be opaque black")
        center = image.get_rowstride() * (image.get_height() // 2) + image.get_n_channels() * (image.get_width() // 2)
        self.assertEqual(image.get_pixels()[center + 3], 255, "globe artwork stays opaque")
        self.assertEqual(icons.resolve("image://icon/org.gnome.Epiphany", self.theme, self.cache), uri)
        self.assertEqual(list(self.directory.iterdir()), [], "rendering must not create PNG files")
        self.assertEqual(len(self.cache.entries), 1)

    def test_animation_raster_size_has_a_separate_cache_entry(self):
        path = self.directory / "gear.svg"
        path.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128"><circle cx="64" cy="64" r="50" fill="white"/></svg>')
        for size in [192, 224, 448]:
            uri = icons.resolve(path.as_uri() + "?bingux-size=" + str(size), self.theme, self.cache)
            loader = icons.GdkPixbuf.PixbufLoader.new_with_type("png")
            loader.write(base64.b64decode(uri.split(",", 1)[1]))
            loader.close()
            self.assertEqual(loader.get_pixbuf().get_width(), size)
        self.assertEqual(len(self.cache.entries), 3)
        self.assertEqual(icons.requested_size("file:///a.svg?bingux-size=999999"), 1024)

    def test_missing_icon_fallback_keeps_transparent_corners(self):
        for fallback in ["application-x-executable", "image-missing"]:
            source = "image://icon/bingux-definitely-missing-icon?fallback=" + fallback
            uri = icons.resolve(source, self.theme, self.cache)
            self.assertEqual(uri, icons.resolve("image://icon/" + fallback, self.theme, self.cache))
            self.assertTrue(uri.startswith("data:image/png;base64,"))
            loader = icons.GdkPixbuf.PixbufLoader.new_with_type("png")
            loader.write(base64.b64decode(uri.split(",", 1)[1]))
            loader.close()
            image = loader.get_pixbuf()
            self.assertTrue(image.get_has_alpha())
            self.assertEqual(image.get_pixels()[3], 0, "fallback corner must remain transparent")
        self.assertEqual(icons.resolve("image://icon/bingux-definitely-missing-icon", self.theme, self.cache),
                         icons.resolve("image://icon/application-x-executable", self.theme, self.cache))

    def test_raster_black_pixels_are_not_removed(self):
        path = self.directory / "black.png"
        image = icons.GdkPixbuf.Pixbuf.new(icons.GdkPixbuf.Colorspace.RGB, True, 8, 16, 16)
        image.fill(0x000000FF)
        image.savev(str(path), "png", [], [])
        self.assertEqual(icons.resolve(path.as_uri(), self.theme, self.cache), path.as_uri())
        self.assertEqual(icons.resolve("image://icon/" + str(path), self.theme, self.cache), path.as_uri())
        self.assertEqual(icons.resolve("image://icon/black?path=" + quote(str(self.directory)), self.theme, self.cache), path.as_uri())
        self.assertEqual(icons.GdkPixbuf.Pixbuf.new_from_file(str(path)).get_pixels()[3], 255)

    def test_changed_asset_gets_a_new_cache_entry(self):
        path = self.directory / "icon.svg"
        path.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="4" fill="black"/></svg>')
        first = icons.resolve(path.as_uri(), self.theme, self.cache)
        path.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="5" fill="red"/></svg>')
        self.assertNotEqual(icons.resolve(path.as_uri(), self.theme, self.cache), first)

    def test_files_and_folders_use_gio_full_colour_icons(self):
        for path in [self.directory, self.directory / "document.pdf"]:
            if path.suffix:
                path.write_bytes(b"%PDF-1.4\n")
            source = "file-preview:" + quote(str(path), safe="")
            resolved, file, info = icons.file_preview(source, self.theme)
            expected = self.theme.lookup_by_gicon(info.get_icon(), icons.SIZE, icons.Gtk.IconLookupFlags.FORCE_SIZE)
            self.assertEqual(resolved, Path(expected.get_filename()))
            self.assertNotIn("symbolic", str(resolved))
            self.assertEqual(file.get_path(), str(path))

    def test_image_and_pdf_previews_are_generated_and_saved_for_nautilus(self):
        if icons.GnomeDesktop is None:
            self.skipTest("GNOME thumbnail service is not installed")
        import cairo
        image_path = self.directory / "image with spaces.png"
        pixbuf = icons.GdkPixbuf.Pixbuf.new(icons.GdkPixbuf.Colorspace.RGB, False, 8, 160, 80)
        pixbuf.fill(0xE05030FF)
        pixbuf.savev(str(image_path), "png", [], [])
        pdf_path = self.directory / "preview.pdf"
        surface = cairo.PDFSurface(str(pdf_path), 160, 240)
        context = cairo.Context(surface)
        context.set_source_rgb(0.2, 0.5, 0.8)
        context.paint()
        surface.finish()
        env = dict(os.environ, XDG_CACHE_HOME=str(self.directory / "cache"))
        process = subprocess.Popen([sys.executable, str(HELPER)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, env=env)
        self.addCleanup(lambda: process.poll() is None and process.kill())
        selector = selectors.DefaultSelector()
        self.addCleanup(selector.close)
        selector.register(process.stdout, selectors.EVENT_READ)
        sources = ["file-preview:" + quote(str(path), safe="") for path in [image_path, pdf_path]]
        process.stdin.write("".join(json.dumps({"source": source}) + "\n" for source in sources).encode())
        process.stdin.flush()
        previews = set()
        pending = b""
        deadline = time.monotonic() + 10
        while len(previews) < 2 and time.monotonic() < deadline:
            if not selector.select(0.2):
                continue
            pending += os.read(process.stdout.fileno(), 65536)
            while b"\n" in pending:
                line, pending = pending.split(b"\n", 1)
                record = json.loads(line)
                # The full-colour fallback is also an SVG rendered to PNG.
                # A preview has the fixture's rectangular aspect ratio.
                resolved = record.get("resolved", "")
                if resolved.startswith("data:image/png;base64,"):
                    loader = icons.GdkPixbuf.PixbufLoader.new_with_type("png")
                    loader.write(base64.b64decode(resolved.split(",", 1)[1]))
                    loader.close()
                    image = loader.get_pixbuf()
                    if image.get_width() != image.get_height():
                        previews.add(record["source"])
        self.assertEqual(previews, set(sources), "both image and PDF must produce real previews")
        cache = self.directory / "cache/thumbnails/large"
        deadline = time.monotonic() + 3
        while len(list(cache.glob("*.png"))) < 2 and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertEqual(len(list(cache.glob("*.png"))), 2)
        process.stdin.close()
        process.wait(timeout=3)
        process.stdout.close()
        process.stderr.close()

    def test_palette_ignores_transparency_and_is_cached(self):
        path = self.directory / "palette.svg"
        path.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect x="8" y="8" width="16" height="16" fill="#1ed760"/></svg>')
        palette = icons.sample_palette(path.as_uri(), self.theme, self.cache)
        self.assertIn("#1ed760", palette)
        self.assertNotIn("#000000", palette)
        self.assertEqual(icons.sample_palette(path.as_uri(), self.theme, self.cache), palette)
        self.assertEqual(len(self.cache.entries), 1)

    def test_memory_cache_is_bounded_and_recent_entries_survive(self):
        cache = icons.RenderCache(limit=10)
        cache.put("a", "aaaa")
        cache.put("b", "bbbb")
        self.assertEqual(cache.get("a"), "aaaa")
        cache.put("c", "cccc")
        self.assertIsNone(cache.get("b"))
        self.assertEqual(cache.bytes, 8)
        cache.put("large", "x" * 11)
        self.assertIsNone(cache.get("large"))

    def test_persistent_worker_handles_batched_requests_and_invalid_sources(self):
        sources = ["image://icon/system-file-manager", "image://icon/org.gnome.Epiphany", "https://example.com/icon.svg"]
        result = subprocess.run([sys.executable, str(HELPER)], input="".join(json.dumps({"source": source}) + "\n" for source in sources), text=True, capture_output=True, timeout=5, check=True)
        responses = [json.loads(line) for line in result.stdout.splitlines()]
        rendered = {record["source"]: record for record in responses if "source" in record}
        self.assertEqual(set(rendered), set(sources))
        self.assertIn("error", rendered[sources[-1]])


if __name__ == "__main__":
    unittest.main()
