#!/usr/bin/env python3
"""Check that the dependency doctor reports success and useful failures."""

from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class DependencyCheckTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("qmake6", "cargo", "cc", "pkg-config", "qs", "systemctl"):
            command = self.bin / name
            command.write_text("#!/bin/sh\nexit 0\n")
            command.chmod(0o755)

    def tearDown(self):
        self.temp.cleanup()

    def run_check(self):
        return subprocess.run(
            [sys.executable, str(ROOT / "scripts/check-dependencies.py"), "--install-user"],
            env={**os.environ, "PATH": str(self.bin)},
            capture_output=True,
            text=True,
        )

    def test_success(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("prerequisites are ready", result.stdout)

    def test_qmake_failure_is_actionable(self):
        qmake = self.bin / "qmake6"
        qmake.write_text("#!/bin/sh\necho 'Project ERROR: Unknown module(s) in QT: quick qml' >&2\nexit 1\n")
        qmake.chmod(0o755)
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Qt Quick/QML modules are unavailable", result.stdout)
        self.assertIn("make doctor", result.stdout)


if __name__ == "__main__":
    unittest.main()
