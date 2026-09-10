#!/usr/bin/env python3
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


extensions = load("extensions", ROOT / "shell/bingux/extensions.py")
settings = load("settings", ROOT / "shell/bingux/settings-backend.py")


class Extensions(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.environment = patch.dict(os.environ, {"XDG_DATA_HOME": str(self.base / "user"),
            "XDG_DATA_DIRS": str(self.base / "system"), "XDG_CONFIG_HOME": str(self.base / "config"),
            "BINGUX_NO_EXTENSIONS": "0"})
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def fixture(self, identifier="example", system=False, **extra):
        folder = self.base / ("system" if system else "user") / "bingux/extensions" / identifier
        folder.mkdir(parents=True)
        (folder / "Widget.qml").write_text("import QtQuick\nItem {}\n")
        data = {"id": identifier, "name": "Example", "apiVersion": 1,
            "widgets": [{"id": "sample", "name": "Sample", "component": "Widget.qml"}], **extra}
        (folder / "extension.json").write_text(json.dumps(data))
        return folder

    def test_explicit_enable_disable_and_safe_mode(self):
        self.fixture(customMetadata={"anything": True}, version="9000")
        self.assertEqual(extensions.discover()["widgets"], [])
        extensions.set_enabled("example", True)
        self.assertEqual(extensions.discover()["widgets"][0]["id"], "extension:example/sample")
        with patch.dict(os.environ, {"BINGUX_NO_EXTENSIONS": "1"}):
            self.assertEqual(extensions.discover()["widgets"], [])
        extensions.set_enabled("example", False)
        self.assertEqual(extensions.discover()["widgets"], [])

    def test_broken_manifest_does_not_hide_other_extensions(self):
        bad = self.fixture("broken")
        (bad / "extension.json").write_text("{")
        self.fixture("good")
        extensions.set_enabled("good", True)
        result = extensions.discover()
        self.assertEqual(len(result["widgets"]), 1)
        self.assertEqual(result["errors"][0]["id"], "broken")

    def test_user_copy_wins_and_api_version_is_not_shell_version(self):
        self.fixture(system=True)
        self.fixture(apiVersion=2)
        result = extensions.discover()
        self.assertEqual(result["extensions"], [])
        self.assertIn("API version", result["errors"][0]["message"])

    def test_invalid_component_and_duplicate_ids(self):
        folder = self.fixture()
        for widgets in ([{"id": "sample", "name": "Sample", "component": "../outside.qml"}],
                        [{"id": "sample", "name": "Sample", "component": "Widget.qml"}] * 2):
            data = json.loads((folder / "extension.json").read_text())
            data["widgets"] = widgets
            (folder / "extension.json").write_text(json.dumps(data))
            self.assertTrue(extensions.discover()["errors"])

    def test_missing_widget_placement_is_preserved_and_duplicates_rejected(self):
        desktop = copy.deepcopy(settings.DEFAULTS["desktop"])
        desktop["layout"] = {"top-left": [], "top-center": [], "top-right": ["extension:missing/widget"], "dock": [], "sidebar": []}
        settings.validate_desktop(desktop)
        desktop["layout"]["dock"] = ["extension:missing/widget"]
        with self.assertRaises(ValueError):
            settings.validate_desktop(desktop)


if __name__ == "__main__":
    unittest.main()
