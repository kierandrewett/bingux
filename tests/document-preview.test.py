import importlib.util
from pathlib import Path
import tempfile
import unittest

import cairo
from PIL import Image, ImageCms

HELPER = Path(__file__).resolve().parents[1] / "shell/bingux/preview-document.py"
spec = importlib.util.spec_from_file_location("document_preview", HELPER)
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


class DocumentPreviewTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def test_pdf_metadata_and_individual_pages(self):
        path = self.directory / "two pages.pdf"
        surface = cairo.PDFSurface(str(path), 210, 297)
        context = cairo.Context(surface)
        for colour in [(0.2, 0.4, 0.8), (0.8, 0.4, 0.2)]:
            context.set_source_rgb(*colour)
            context.paint()
            context.show_page()
        surface.finish()
        metadata = preview.metadata(path)
        self.assertEqual(metadata["kind"], "pdf")
        self.assertEqual(metadata["file"]["sizeBytes"], path.stat().st_size)
        self.assertIn("modified", metadata["file"])
        self.assertIn("PDF", metadata["file"]["type"])
        self.assertEqual(len(metadata["pages"]), 2)
        first = preview.render(path, 0, 400)["image"]
        second = preview.render(path, 1, 400)["image"]
        self.assertTrue(first.startswith("data:image/png;base64,"))
        self.assertNotEqual(first, second)
        with self.assertRaises(ValueError):
            preview.render(path, 2, 400)

    def test_image_preview_and_bounded_render_size(self):
        path = self.directory / "image.png"
        image = cairo.ImageSurface(cairo.FORMAT_ARGB32, 120, 60)
        image.write_to_png(str(path))
        self.assertEqual(preview.metadata(path)["pages"], [{"width": 120, "height": 60}])
        self.assertTrue(preview.render(path, 0, 100000)["image"].startswith("data:image/png;base64,"))

    def test_embedded_image_information(self):
        path = self.directory / "photo.jpg"
        image = Image.new("RGB", (240, 160), "green")
        exif = Image.Exif()
        exif[34665] = {36867: "2022:04:29 11:04:00"}
        profile = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()
        image.save(path, dpi=(72, 72), exif=exif, icc_profile=profile)
        details = preview.metadata(path)["image"]
        self.assertEqual(details["colourSpace"], "RGB")
        self.assertEqual(details["resolution"], "72 × 72 dpi")
        self.assertIn("sRGB", details["colourProfile"])
        self.assertEqual(details["contentCreated"], "2022-04-29T11:04:00")

    def test_text_is_literal_and_bounded(self):
        path = self.directory / "notes.md"
        path.write_text("<script>literal text</script>\n")
        self.assertEqual(preview.metadata(path)["text"], path.read_text())
        path.write_bytes(b"x" * (preview.MAX_TEXT_BYTES + 10))
        result = preview.metadata(path)
        self.assertTrue(result["truncated"])
        self.assertEqual(len(result["text"]), preview.MAX_TEXT_BYTES)

    def test_missing_binary_and_oversize_files_report_errors(self):
        with self.assertRaises(ValueError):
            preview.metadata(self.directory / "missing.pdf")
        path = self.directory / "binary.txt"
        path.write_bytes(b"\x00\x01\x02")
        with self.assertRaises(ValueError):
            preview.metadata(path)
        with path.open("wb") as stream:
            stream.truncate(preview.MAX_FILE_BYTES + 1)
        with self.assertRaises(ValueError):
            preview.metadata(path)


if __name__ == "__main__":
    unittest.main()
