#!/usr/bin/env python3
"""Check staged installation paths without installing into the host."""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstallTest(unittest.TestCase):
    def test_stage(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, stage = base / "build", base / "stage"
            for filename in ("text/libbinguxtext.so", "settings/libbinguxsettings.so", "effects/libbinguxeffects.so",
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
            self.assertTrue((stage / "usr/lib/bingux/qml/Bingux/Effects/libbinguxeffects.so").is_file())
            self.assertFalse((stage / "home").exists())
            self.assertFalse((stage / "usr/share/bingux/shell/__pycache__").exists())
            unit = (stage / "usr/lib/systemd/user/bingux.service").read_text()
            self.assertIn("LimitCORE=0", unit)
            self.assertIn("ExecStart=/usr/bin/bingux --no-color", unit)
            search_unit = (stage / "usr/lib/systemd/user/bingux-search-ui.service").read_text()
            switcher_unit = (stage / "usr/lib/systemd/user/bingux-switcher-ui.service").read_text()
            self.assertIn("ExecStart=/usr/bin/bingux-search-ui", search_unit)
            self.assertIn("ExecStart=/usr/bin/bingux-switcher-ui", switcher_unit)
            for name in ("capture", "emoji"):
                unit = (stage / f"usr/lib/systemd/user/bingux-{name}-ui.service").read_text()
                self.assertIn(f"ExecStart=/usr/bin/bingux-{name}-ui", unit)
                self.assertTrue((stage / f"usr/bin/bingux-{name}-ui").is_file())
            searchd_unit = (stage / "usr/lib/systemd/user/bingux-searchd.service").read_text()
            self.assertIn("ExecStart=/usr/libexec/bingux/search-service", searchd_unit)

    def test_missing_build_fails_before_install(self):
        with tempfile.TemporaryDirectory() as name:
            stage = Path(name) / "stage"
            result = subprocess.run(["python3", str(ROOT / "scripts/install-shell.py"),
                                     "--destdir", str(stage), "--build-dir", name], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(stage.exists())

    def test_user_install_is_central_and_removable(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            home, config = base / "home", base / "config"
            for filename in ("text/libbinguxtext.so", "settings/libbinguxsettings.so", "effects/libbinguxeffects.so",
                             "bingux-audio-meter", "cargo/release/bingux-searchd", "cargo/release/bingux-statusd"):
                path = build / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("test fixture\n")
            environment = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(config)}
            subprocess.run(["python3", str(ROOT / "scripts/install-shell.py"), "--user", "--no-systemd",
                            "--prefix", str(prefix), "--build-dir", str(build)], env=environment, check=True)
            self.assertTrue((prefix / ".bingux-install.json").is_file())
            self.assertTrue((prefix / "share/bingux/shell/ProfileSettings.qml").is_file())
            launcher = home / ".local/bin/bingux"
            self.assertTrue(launcher.is_symlink())
            self.assertEqual(launcher.resolve(), (prefix / "bin/bingux").resolve())
            self.assertTrue((home / ".local/bin/bingux-uninstall").is_symlink())
            unit = config / "systemd/user/bingux.service"
            self.assertTrue(unit.is_symlink())
            self.assertIn(f"ExecStart={prefix}/bin/bingux --no-color", unit.read_text())

            subprocess.run(["python3", str(ROOT / "scripts/install-shell.py"), "--user", "--uninstall",
                            "--no-systemd", "--prefix", str(prefix)], env=environment, check=True)
            self.assertFalse(prefix.exists())
            self.assertFalse(launcher.exists())
            self.assertFalse((home / ".local/bin/bingux-uninstall").exists())
            self.assertFalse(unit.exists())
            self.assertFalse((config / "bingux").exists())

    def test_user_install_does_not_take_over_unmanaged_directory(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            prefix.mkdir(parents=True)
            marker = prefix / "keep-me"
            marker.write_text("unmanaged\n")
            result = subprocess.run(["python3", str(ROOT / "scripts/install-shell.py"), "--user",
                                     "--no-systemd", "--prefix", str(prefix), "--build-dir", str(build)],
                                    env={**os.environ, "HOME": str(base / "home")}, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unmanaged directory", result.stderr)
            self.assertEqual(marker.read_text(), "unmanaged\n")

    def test_user_install_rejects_external_qml_directory(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            result = subprocess.run(["python3", str(ROOT / "scripts/install-shell.py"), "--user",
                                     "--no-systemd", "--prefix", str(base / "install"),
                                     "--qml-dir", str(base / "outside-qml"), "--build-dir", str(base / "build")],
                                    env={**os.environ, "HOME": str(base / "home")}, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("inside --prefix", result.stderr)

    def test_user_install_manages_systemd_target(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            home, config, bin_dir = base / "home", base / "config", base / "bin"
            for filename in ("text/libbinguxtext.so", "settings/libbinguxsettings.so", "effects/libbinguxeffects.so",
                             "bingux-audio-meter", "cargo/release/bingux-searchd", "cargo/release/bingux-statusd"):
                path = build / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("test fixture\n")
            bin_dir.mkdir()
            log = base / "systemctl.log"
            fake_systemctl = bin_dir / "systemctl"
            fake_systemctl.write_text("#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$BINGUX_SYSTEMCTL_LOG\"\n")
            fake_systemctl.chmod(0o755)
            environment = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(config),
                           "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"],
                           "BINGUX_SYSTEMCTL_LOG": str(log)}
            subprocess.run(["python3", str(ROOT / "scripts/install-shell.py"), "--user",
                            "--prefix", str(prefix), "--build-dir", str(build)], env=environment, check=True)
            self.assertIn("daemon-reload", log.read_text())
            self.assertIn("enable --now bingux.target", log.read_text())
            wants = config / "systemd/user/graphical-session.target.wants"
            wants.mkdir(parents=True)
            enabled_target = wants / "bingux.target"
            enabled_target.symlink_to(prefix / "lib/systemd/user/bingux.target")
            subprocess.run([str(home / ".local/bin/bingux-uninstall")], env=environment, check=True)
            calls = log.read_text()
            self.assertIn("disable --now bingux.target", calls)
            self.assertFalse(enabled_target.exists())


if __name__ == "__main__":
    unittest.main()
