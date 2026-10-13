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
        self.env = {
            **os.environ,
            "PATH": str(self.bin),
            "HOME": str(self.root),
            "XDG_DATA_HOME": str(self.root / "data"),
        }
        self.env.pop("BINGUX_QUICKSHELL", None)
        self.write_quickshell(self.bin / "qs")

    def write_quickshell(self, path, build="6.10.3", runtime="6.10.3"):
        path.parent.mkdir(parents=True, exist_ok=True)
        # Refuse any invocation that could start a desktop instead of querying it.
        path.write_text(
            '#!/bin/sh\n[ "$*" = "--version -vv --no-color" ] || exit 99\n'
            f"echo 'Buildtime Qt Version: {build}' >&2\n"
            f"echo 'Runtime Qt Version: {runtime}' >&2\n"
        )
        path.chmod(0o755)

    def tearDown(self):
        self.temp.cleanup()

    def run_check(self):
        return subprocess.run(
            [sys.executable, str(ROOT / "scripts/check-dependencies.py"), "--install-user"],
            env=self.env,
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

    def test_qt_patch_mismatch_is_actionable(self):
        self.write_quickshell(self.bin / "qs", build="6.10.1")
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        for detail in (str(self.bin / "qs"), "6.10.1", "6.10.3", "rebuild", "BINGUX_QUICKSHELL"):
            self.assertIn(detail, result.stdout)

    def test_managed_runtime_precedes_mismatched_system_binary(self):
        self.write_quickshell(self.bin / "qs", build="6.10.1")
        self.write_quickshell(self.root / "data/gnoblin/quickshell-fedora/bin/quickshell")
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_mismatched_managed_runtime_does_not_fall_back(self):
        managed = self.root / "data/gnoblin/quickshell-fedora/bin/quickshell"
        self.write_quickshell(managed, build="6.10.1")
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(str(managed), result.stdout)

    def test_explicit_override_precedes_managed_runtime(self):
        self.write_quickshell(self.root / "data/gnoblin/quickshell-fedora/bin/quickshell")
        override = self.bin / "custom-qs"
        self.write_quickshell(override, build="6.10.1")
        self.env["BINGUX_QUICKSHELL"] = str(override)
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(str(override), result.stdout)

    def test_gnoblin_wrapper_precedes_system_binary(self):
        self.write_quickshell(self.bin / "qs", build="6.10.1")
        self.write_quickshell(self.bin / "gnoblin-quickshell")
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_broken_quickshell_is_actionable(self):
        (self.bin / "qs").write_text("#!/bin/sh\necho 'error loading libQt6Core.so.6' >&2\nexit 127\n")
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(str(self.bin / "qs"), result.stdout)
        self.assertIn("libQt6Core.so.6", result.stdout)

    def test_missing_version_diagnostics_are_not_success(self):
        (self.bin / "qs").write_text("#!/bin/sh\necho 'quickshell unknown'\n")
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("Qt versions", result.stdout)

    def test_invalid_override_does_not_fall_back(self):
        self.env["BINGUX_QUICKSHELL"] = str(self.root / "missing-qs")
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("BINGUX_QUICKSHELL is not executable", result.stdout)


if __name__ == "__main__":
    unittest.main()
