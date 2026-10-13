#!/usr/bin/env python3
"""Check that the dependency doctor reports success and useful failures."""

from pathlib import Path
import json
import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
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

    def write_quickshell(self, path, build="6.10.3", runtime="6.10.3", probe="pass"):
        path.parent.mkdir(parents=True, exist_ok=True)
        observed = {
            "id": "org.bingux.ActionIdentity",
            "actions": [
                {"id": "aproof", "name": "Same label"},
                {"id": "bproof", "name": "Same label"},
                {"id": "proof", "name": "Open proof"},
                {"id": "x", "name": "Single character"},
            ],
        }
        if probe == "old":
            observed["actions"] = [{"id": "roof", "name": "Open proof"}]
        output = "BINGUX_ACTION_IDENTITY=" + json.dumps(observed)
        # Stubs cover failure handling and environment isolation. The opt-in
        # real executable tests below cover the actual C++ parser/QML model.
        path.write_text(
            f"#!{sys.executable}\n"
            + textwrap.dedent(f"""\
                import os, pathlib, sys, time
                if sys.argv[1:] == ["--version", "-vv", "--no-color"]:
                    print("Buildtime Qt Version: {build}", file=sys.stderr)
                    print("Runtime Qt Version: {runtime}", file=sys.stderr)
                    sys.exit(0)
                assert sys.argv[1:5] == ["--no-color", "--log-rules", "qml.info=true", "--path"]
                root = pathlib.Path(sys.argv[5]).parent
                assert root != pathlib.Path({str(self.root)!r})
                for key in ("HOME", "XDG_DATA_HOME", "XDG_DATA_DIRS", "XDG_CONFIG_HOME", "XDG_CONFIG_DIRS",
                            "XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR"):
                    assert pathlib.Path(os.environ[key]).is_relative_to(root), key
                assert os.environ["QT_QPA_PLATFORM"] == "offscreen"
                assert os.environ["QT_QUICK_BACKEND"] == "software"
                for key in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "SESSION_MANAGER", "QS_CONFIG_PATH"):
                    assert key not in os.environ, key
                for key in ("DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS"):
                    address = os.environ[key].removeprefix("unix:path=")
                    assert pathlib.Path(address).is_relative_to(root)
                    assert not pathlib.Path(address).exists()
                assert (root / "shell.qml").is_file()
                assert (root / "data/applications/org.bingux.ActionIdentity.desktop").is_file()
                pathlib.Path({str(self.root / "probe-path")!r}).write_text(str(root))
                if {probe!r} == "timeout":
                    child = os.fork()
                    if child == 0:
                        time.sleep(11)
                        pathlib.Path({str(self.root / "leaked-child")!r}).touch()
                        os._exit(0)
                    time.sleep(30)
                if {probe!r} == "crash":
                    print("fixture QML startup failed", file=sys.stderr)
                    sys.exit(3)
                if {probe!r} == "missing":
                    sys.exit(0)
                if {probe!r} == "malformed":
                    print("BINGUX_ACTION_IDENTITY=invalid-json")
                else:
                    print({output!r})
                """)
        )
        path.chmod(0o755)

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
        self.assertFalse((self.root / "probe-path").exists(), "must reject Qt mismatch before loading QML")

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

    def test_relative_explicit_override_uses_same_executable(self):
        self.env["BINGUX_QUICKSHELL"] = "./bin/qs"
        result = self.run_check(cwd=self.root)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

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

    def test_old_action_ids_are_actionable(self):
        self.write_quickshell(self.bin / "qs", probe="old")
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        for detail in (
            "desktop-action IDs are incorrect",
            str(self.bin / "qs"),
            "BINGUX_QUICKSHELL",
            "patches/quickshell",
        ):
            self.assertIn(detail, result.stdout)

    def test_install_user_requires_successful_probe(self):
        self.write_quickshell(self.bin / "qs", probe="old")
        rejected = self.run_install_check()
        self.assertNotEqual(rejected.returncode, 0, rejected.stdout + rejected.stderr)
        self.assertFalse((self.root / "installer-reached").exists())
        self.write_quickshell(self.bin / "qs")
        accepted = self.run_install_check()
        self.assertEqual(accepted.returncode, 0, accepted.stdout + accepted.stderr)
        self.assertTrue((self.root / "installer-reached").exists())

    def test_probe_is_private_and_cleans_up(self):
        self.env.update(
            DISPLAY=":12345",
            WAYLAND_DISPLAY="real-session-sentinel",
            WAYLAND_SOCKET="9876",
            SESSION_MANAGER="real-session-sentinel",
            QS_CONFIG_PATH="real-config-sentinel",
            DBUS_SESSION_BUS_ADDRESS="real-session-sentinel",
            DBUS_SYSTEM_BUS_ADDRESS="real-system-sentinel",
        )
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        directory = Path((self.root / "probe-path").read_text())
        self.assertFalse(directory.exists())

    def test_failed_probe_is_not_success(self):
        for behavior in ("crash", "missing", "malformed"):
            with self.subTest(behavior=behavior):
                self.write_quickshell(self.bin / "qs", probe=behavior)
                result = self.run_check()
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("Cannot verify Quickshell desktop-action IDs", result.stdout)

    def test_probe_timeout_kills_its_process_group(self):
        self.write_quickshell(self.bin / "qs", probe="timeout")
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("timed out", result.stdout)
        subprocess.run([sys.executable, "-c", "import time; time.sleep(1.5)"], check=True)
        self.assertFalse((self.root / "leaked-child").exists())
        self.assertFalse(Path((self.root / "probe-path").read_text()).exists())

    @unittest.skipUnless(
        os.environ.get("BINGUX_TEST_OLD_QUICKSHELL"), "set BINGUX_TEST_OLD_QUICKSHELL for real-runtime red"
    )
    def test_real_old_runtime_is_rejected(self):
        self.env["BINGUX_QUICKSHELL"] = os.environ["BINGUX_TEST_OLD_QUICKSHELL"]
        result = self.run_check()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("desktop-action", result.stdout)
        self.assertIn("patches/quickshell", result.stdout)
        install = self.run_install_check()
        self.assertNotEqual(install.returncode, 0, install.stdout + install.stderr)
        self.assertIn("desktop-action", install.stdout)
        self.assertFalse((self.root / "installer-reached").exists())

    @unittest.skipUnless(os.environ.get("BINGUX_TEST_QUICKSHELL"), "set BINGUX_TEST_QUICKSHELL for real-runtime green")
    def test_real_corrected_runtime_is_accepted(self):
        self.env["BINGUX_QUICKSHELL"] = os.environ["BINGUX_TEST_QUICKSHELL"]
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        install = self.run_install_check()
        self.assertEqual(install.returncode, 0, install.stdout + install.stderr)
        self.assertTrue((self.root / "installer-reached").exists())


if __name__ == "__main__":
    unittest.main()
