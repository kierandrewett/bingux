#!/usr/bin/env python3
"""Check that the dependency doctor reports success and useful failures."""

from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
TOOLS = ("qmake6", "cargo", "cc", "cmake", "ninja", "pkg-config", "systemctl")


class DependencyCheckTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in TOOLS:
            command = self.bin / name
            command.write_text("#!/bin/sh\nexit 0\n")
            command.chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": str(self.bin),
            "HOME": str(self.root),
            "XDG_DATA_HOME": str(self.root / "data"),
        }

    def tearDown(self):
        self.temp.cleanup()

    def run_check(self, cwd=ROOT):
        return subprocess.run(
            [sys.executable, str(ROOT / "scripts/check-dependencies.py"), "--install-user"],
            cwd=cwd,
            env=self.env,
            capture_output=True,
            text=True,
        )

    def run_install_check(self):
        # Exercise the real make prerequisite, but neither build nor install.
        make = shutil.which("make")
        self.assertIsNotNone(make)
        shim = self.bin / "python3"
        shim.write_text(
            f"#!{sys.executable}\n"
            "import os, pathlib, sys\n"
            "if sys.argv[1] == 'scripts/install-shell.py':\n"
            f"    pathlib.Path({str(self.root / 'installer-reached')!r}).touch()\n"
            "    sys.exit(0)\n"
            f"os.execv({sys.executable!r}, [{sys.executable!r}, *sys.argv[1:]])\n"
        )
        shim.chmod(0o755)
        return subprocess.run(
            [make, "--no-print-directory", "-o", "all", "install-user", f"USER_PREFIX={self.root / 'install'}"],
            cwd=ROOT,
            env={key: value for key, value in self.env.items() if key not in ("MAKEFLAGS", "MFLAGS", "MAKELEVEL")},
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_success(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("prerequisites are ready", result.stdout)

    def test_qmake_failure_is_actionable(self):
        qmake = self.bin / "qmake6"
        qmake.write_text("#!/bin/sh\necho 'Project ERROR: Unknown module(s) in QT: quick qml' >&2\nexit 1\n")
        qmake.chmod(0o755)
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Qt Quick/QML modules are unavailable", result.stdout)
        self.assertIn("make doctor", result.stdout)

    def test_missing_shell_build_tools_are_named(self):
        (self.bin / "cmake").unlink()
        (self.bin / "ninja").unlink()
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("CMake not found: cmake", result.stdout)
        self.assertIn("Ninja not found: ninja", result.stdout)

    def test_missing_shell_library_is_named(self):
        pkg_config = self.bin / "pkg-config"
        pkg_config.write_text('#!/bin/sh\n[ "$1" = --exists ] && [ "$2" = libarchive ] && exit 1\nexit 0\n')
        pkg_config.chmod(0o755)
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("pkg-config dependency missing: libarchive", result.stdout)

    def test_missing_systemctl_only_matters_for_user_installs(self):
        (self.bin / "systemctl").unlink()
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("systemctl not found", result.stdout)
        build_only = subprocess.run(
            [sys.executable, str(ROOT / "scripts/check-dependencies.py")],
            cwd=ROOT,
            env=self.env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(build_only.returncode, 0, build_only.stdout + build_only.stderr)

    def test_install_user_runs_the_doctor_first(self):
        (self.bin / "cmake").unlink()
        rejected = self.run_install_check()
        self.assertNotEqual(rejected.returncode, 0, rejected.stdout + rejected.stderr)
        self.assertIn("CMake not found", rejected.stdout)
        self.assertFalse((self.root / "installer-reached").exists())
        (self.bin / "cmake").write_text("#!/bin/sh\nexit 0\n")
        (self.bin / "cmake").chmod(0o755)
        accepted = self.run_install_check()
        self.assertEqual(accepted.returncode, 0, accepted.stdout + accepted.stderr)
        self.assertTrue((self.root / "installer-reached").exists())


if __name__ == "__main__":
    unittest.main()
