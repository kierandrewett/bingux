#!/usr/bin/env python3
"""Check staged installation paths without installing into the host."""
from pathlib import Path
import json
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstallTest(unittest.TestCase):
    def test_stage(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, stage = base / "build", base / "stage"
            for filename in ("text/libbinguxtext.so", "settings/libbinguxsettings.so",
                             "bingux-audio-meter", "cargo/release/bingux-searchd", "cargo/release/bingux-statusd"):
                path = build / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("test fixture\n")
            subprocess.run(["python3", str(ROOT / "scripts/install-shell.py"), "--prefix", "/usr",
                            "--destdir", str(stage), "--build-dir", str(build)], check=True)
            for name in ("bingux", "binguxctl", "bingux-settings"):
                launcher = stage / "usr/bin" / name
                self.assertTrue(launcher.stat().st_mode & 0o111)
                subprocess.run(["sh", "-n", str(launcher)], check=True)
                self.assertNotIn(str(stage), launcher.read_text())
            config = json.loads((stage / "usr/share/bingux/search.json").read_text())
            self.assertEqual(config["commands"]["applicationLauncher"][1],
                             "/usr/share/bingux/shell/launch-application.py")
            self.assertTrue((stage / "usr/share/bingux/shell/ProfileSettings.qml").is_file())
            self.assertTrue((stage / "usr/lib/bingux/qml/Bingux/Text/qmldir").is_file())
            self.assertFalse((stage / "home").exists())
            self.assertFalse((stage / "usr/share/bingux/shell/__pycache__").exists())
            unit = (stage / "usr/lib/systemd/user/bingux.service").read_text()
            self.assertIn("LimitCORE=0", unit)
            self.assertIn("ExecStart=/usr/bin/bingux --no-color", unit)

    def test_missing_build_fails_before_install(self):
        with tempfile.TemporaryDirectory() as name:
            stage = Path(name) / "stage"
            result = subprocess.run(["python3", str(ROOT / "scripts/install-shell.py"),
                                     "--destdir", str(stage), "--build-dir", name], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(stage.exists())


if __name__ == "__main__":
    unittest.main()
