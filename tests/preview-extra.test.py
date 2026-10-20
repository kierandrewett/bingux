import importlib.util
import os
import shutil
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("preview", ROOT / "shell/bingux/preview-document.py")
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


class ExtraPreviewTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.path = Path(temporary.name)
        patcher = patch.dict(os.environ, {"XDG_CONFIG_HOME": str(self.path / "config")})
        patcher.start()
        self.addCleanup(patcher.stop)

    def file(self, name, contents):
        path = self.path / name
        path.write_text(contents)
        return path

    def test_yaml_frontmatter_is_metadata_not_document_body(self):
        data = preview.metadata(
            self.file(
                "note.md", "---\ntitle: A note\ntags: [one, two]\ndate: 2026-09-08\n---\n# Actual heading\n\nBody"
            )
        )
        self.assertEqual(data["frontmatter"]["title"], "A note")
        self.assertIn("one", data["frontmatter"]["tags"])
        self.assertNotIn("title:", data["renderedText"])
        self.assertIn("<h1>Actual heading</h1>", data["renderedText"])

    def test_toml_and_malformed_frontmatter(self):
        data = preview.metadata(self.file("note.md", '+++\ntitle = "Hello"\n+++\nBody'))
        self.assertEqual(data["frontmatter"]["title"], "Hello")
        for content in ("---\na: [broken\n---\nBody", "---\na: &a [*a]\n---\nBody"):
            data = preview.metadata(self.file("bad.md", content))
            self.assertEqual(data["text"], content)
            self.assertTrue(data["frontmatterWarning"])

    def test_csv_quotes_and_html_escaping(self):
        data = preview.metadata(self.file("data.csv", 'Name,Value\n"Two, words","<script>"\n'))
        self.assertIn("Two, words", data["renderedText"])
        self.assertIn("&lt;script&gt;", data["renderedText"])
        self.assertNotIn("<script>", data["renderedText"])

    def test_archive_listing_never_extracts(self):
        path = self.path / "archive.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("../escape.txt", "never extract")
            archive.writestr("<img src=x>", "untrusted")
        data = preview.metadata(path)
        self.assertIn("../escape.txt", data["renderedText"])
        self.assertIn("&lt;img", data["renderedText"])
        self.assertEqual(data["format"], "archive")
        self.assertFalse((self.path.parent / "escape.txt").exists())

    def test_email_headers_and_plain_body(self):
        data = preview.metadata(
            self.file(
                "message.eml",
                "From: person@example.test\nTo: you@example.test\nSubject: Hello\nContent-Type: text/plain; charset=utf-8\n\nA message.",
            )
        )
        self.assertEqual(data["frontmatter"]["Subject"], "Hello")
        self.assertIn("A message.", data["renderedText"])

    def test_user_size_limit_and_hard_cap(self):
        config = self.path / "config/bingux/bingux.toml"
        config.parent.mkdir(parents=True)
        config.write_text("[previews]\nmax_megabytes = 1\n")
        binary = os.environ.get("BINGUX_CONFIG_BIN") or shutil.which("bingux-config")
        self.assertIsNotNone(binary, "set BINGUX_CONFIG_BIN to the bingux-config binary")
        binary_environment = patch.dict(os.environ, {"BINGUX_CONFIG_BIN": binary})
        binary_environment.start()
        self.addCleanup(binary_environment.stop)
        path = self.path / "large.csv"
        with path.open("wb") as stream:
            stream.truncate(1_000_001)
        with self.assertRaisesRegex(ValueError, "1 MB limit"):
            preview.metadata(path)
        config.write_text("[previews]\nmax_megabytes = 200\n")
        with path.open("wb") as stream:
            stream.truncate(20_000_001)
        with self.assertRaisesRegex(ValueError, "20 MB limit"):
            preview.metadata(path)


if __name__ == "__main__":
    unittest.main()
