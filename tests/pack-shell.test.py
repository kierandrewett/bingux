#!/usr/bin/env python3
"""scripts/pack-shell.py: overlays, extra entries, tree output and determinism.

Runs the packer as a subprocess on small fixture trees. The base-only archive
is compared byte for byte with the one the committed (HEAD) packer writes for
the same source, so adding overlay support cannot change what ships today.
"""

import hashlib
import io
import json
import lzma
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
PACKER = REPO / "scripts/pack-shell.py"
ENTRY_FILES = {
    "shell.qml",
    "LockShell.qml",
    "GreeterShell.qml",
    "PopoutsShell.qml",
    "WallpaperShell.qml",
    "SearchShell.qml",
    "SwitcherShell.qml",
    "EmojiShell.qml",
    "CaptureShell.qml",
}


def write(root, relative, text, executable=False):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    if executable:
        path.chmod(0o755)
    return path


def base_tree(root):
    """A base tree with every built-in entry point, a Theme singleton and a qmldir."""
    source = root / "base"
    for name in sorted(ENTRY_FILES):
        write(source, name, f"import QtQuick\n// {name}\n")
    write(source, "Theme.qml", "pragma Singleton\nimport QtQuick\nQtObject {}\n")
    lines = [f"singleton Theme 1.0 Theme.qml"]
    lines += [f"{Path(name).stem} 1.0 {name}" for name in sorted(ENTRY_FILES)]
    write(source, "qmldir", "\n".join(lines) + "\n")
    write(source, "helpers/run.py", "print('not packed')\n", executable=True)
    return source


def overlay_tree(root, name="installer"):
    overlay = root / name
    write(overlay, "InstallerShell.qml", "import QtQuick\nItem {}\n")
    write(overlay, "InstallerWizard.qml", "import QtQuick\nItem {}\n")
    write(overlay, "InstallerState.qml", "pragma Singleton\nimport QtQuick\nQtObject {}\n")
    write(overlay, "qmldir.fragment", "singleton InstallerState 1.0 InstallerState.qml\nInstallerWizard 1.0 InstallerWizard.qml\n")
    write(overlay, "data/schema.js", "var schema = {};\n")
    return overlay


def pack(*arguments, expect_ok=True):
    result = subprocess.run(
        [sys.executable, str(PACKER), *map(str, arguments)], capture_output=True, text=True
    )
    if expect_ok and result.returncode != 0:
        raise AssertionError(f"packer failed: {result.stderr}")
    if not expect_ok and result.returncode == 0:
        raise AssertionError("packer succeeded but a failure was expected")
    return result


def members(archive):
    with lzma.open(archive) as stream:
        data = stream.read()
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        out = {}
        for member in tar.getmembers():
            if member.isfile():
                out[member.name] = tar.extractfile(member).read()
            else:
                out[member.name] = None
        return out


class PackShell(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pack-shell-test-"))
        self.source = base_tree(self.root)

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_base_archive_matches_committed_packer(self):
        head = subprocess.run(
            ["git", "-C", str(REPO), "show", "HEAD:scripts/pack-shell.py"],
            capture_output=True,
            check=False,
        )
        if head.returncode != 0:
            self.skipTest("no git HEAD to compare against")
        old = self.root / "pack-head.py"
        old.write_bytes(head.stdout)
        real_source = REPO / "shell/bingux"
        new_out = self.root / "new.sxa"
        old_out = self.root / "old.sxa"
        pack("--source", real_source, new_out)
        built = subprocess.run(
            [sys.executable, str(old), "--source", str(real_source), str(old_out)],
            capture_output=True,
            text=True,
        )
        self.assertEqual(built.returncode, 0, built.stderr)
        self.assertEqual(new_out.read_bytes(), old_out.read_bytes())

    def test_overlay_merges_files_and_fragment(self):
        overlay = overlay_tree(self.root)
        out = self.root / "merged.sxa"
        pack("--source", self.source, "--overlay", overlay, "--entries", '{"installer": "InstallerShell.qml"}', out)
        entries = members(out)
        manifest = json.loads(entries["manifest.json"])
        self.assertEqual(manifest["entries"]["installer"], "InstallerShell.qml")
        self.assertEqual(manifest["entries"]["desktop"], "shell.qml")
        self.assertIn("shell/InstallerWizard.qml", entries)
        self.assertIn("shell/data/schema.js", entries)
        qmldir = entries["shell/qmldir"].decode()
        self.assertIn("singleton InstallerState 1.0 InstallerState.qml", qmldir)
        self.assertIn("InstallerWizard 1.0 InstallerWizard.qml", qmldir)
        self.assertNotIn("qmldir.fragment", " ".join(entries))
        self.assertNotIn("shell/helpers/run.py", entries)
        self.assertEqual(qmldir.count("singleton Theme 1.0 Theme.qml"), 1)

    def test_collision_with_base_file_fails(self):
        overlay = overlay_tree(self.root)
        write(overlay, "Theme.qml", "pragma Singleton\nimport QtQuick\nQtObject {}\n")
        result = pack("--source", self.source, "--overlay", overlay, self.root / "x.sxa", expect_ok=False)
        self.assertIn("collides", result.stderr)
        self.assertIn("Theme.qml", result.stderr)

    def test_overlay_cannot_replace_qmldir(self):
        overlay = overlay_tree(self.root)
        write(overlay, "qmldir", "Ghost 1.0 Ghost.qml\n")
        result = pack("--source", self.source, "--overlay", overlay, self.root / "x.sxa", expect_ok=False)
        self.assertIn("qmldir", result.stderr)

    def test_fragment_repeating_a_base_type_fails(self):
        overlay = overlay_tree(self.root)
        write(overlay, "qmldir.fragment", "Theme 1.0 InstallerWizard.qml\n")
        result = pack("--source", self.source, "--overlay", overlay, self.root / "x.sxa", expect_ok=False)
        self.assertIn("repeats", result.stderr)

    def test_fragment_naming_a_missing_file_fails(self):
        overlay = overlay_tree(self.root)
        write(overlay, "qmldir.fragment", "InstallerGhost 1.0 InstallerGhost.qml\n")
        result = pack("--source", self.source, "--overlay", overlay, self.root / "x.sxa", expect_ok=False)
        self.assertIn("not in the tree", result.stderr)

    def test_qml_without_qmldir_fails(self):
        overlay = overlay_tree(self.root)
        write(overlay, "newdir/Loose.qml", "import QtQuick\nItem {}\n")
        result = pack("--source", self.source, "--overlay", overlay, self.root / "x.sxa", expect_ok=False)
        self.assertIn("no qmldir", result.stderr)

    def test_entry_for_built_in_mode_fails(self):
        result = pack(
            "--source", self.source, "--entries", '{"desktop": "InstallerShell.qml"}', self.root / "x.sxa", expect_ok=False
        )
        self.assertIn("repeats built-in mode desktop", result.stderr)

    def test_entry_file_must_exist(self):
        result = pack(
            "--source", self.source, "--entries", '{"installer": "Missing.qml"}', self.root / "x.sxa", expect_ok=False
        )
        self.assertIn("has no Missing.qml for mode installer", result.stderr)

    def test_entries_must_be_a_json_object(self):
        result = pack("--source", self.source, "--entries", '["x"]', self.root / "x.sxa", expect_ok=False)
        self.assertIn("JSON object", result.stderr)

    def test_repack_is_byte_identical(self):
        overlay = overlay_tree(self.root)
        first = self.root / "one.sxa"
        second = self.root / "two.sxa"
        arguments = ["--source", self.source, "--overlay", overlay, "--entries", '{"installer": "InstallerShell.qml"}']
        pack(*arguments, first)
        pack(*arguments, second)
        self.assertEqual(hashlib.sha256(first.read_bytes()).hexdigest(), hashlib.sha256(second.read_bytes()).hexdigest())

    def test_tree_out_matches_archive_contents(self):
        overlay = overlay_tree(self.root)
        out = self.root / "merged.sxa"
        tree = self.root / "tree"
        pack("--source", self.source, "--overlay", overlay, "--tree-out", tree, out)
        entries = members(out)
        for name, data in entries.items():
            if data is None or not name.startswith("shell/"):
                continue
            self.assertEqual((tree / name[len("shell/") :]).read_bytes(), data, name)
        self.assertTrue((tree / "InstallerShell.qml").is_file())
        self.assertFalse((tree / "helpers/run.py").exists())

    def test_tree_out_refuses_foreign_directory(self):
        foreign = self.root / "foreign"
        write(foreign, "keep.txt", "not ours\n")
        result = pack("--source", self.source, "--tree-out", foreign, self.root / "x.sxa", expect_ok=False)
        self.assertIn("not empty", result.stderr)
        self.assertTrue((foreign / "keep.txt").is_file())

    def test_tree_out_replaces_previous_output(self):
        overlay = overlay_tree(self.root)
        tree = self.root / "tree"
        pack("--source", self.source, "--overlay", overlay, "--tree-out", tree, self.root / "a.sxa")
        (tree / "InstallerShell.qml").unlink()
        pack("--source", self.source, "--overlay", overlay, "--tree-out", tree, self.root / "b.sxa")
        self.assertTrue((tree / "InstallerShell.qml").is_file())


if __name__ == "__main__":
    unittest.main()
