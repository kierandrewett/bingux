#!/usr/bin/env python3
"""Check staged installation paths without installing into the host."""

from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstallTest(unittest.TestCase):
    def test_user_upgrade_migrates_legacy_launcher_without_losing_search_settings(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            home, config = base / "home", base / "config"
            for filename in (
                "text/libbinguxtext.so",
                "settings/libbinguxsettings.so",
                "effects/libbinguxeffects.so",
                "wayland-sync/libbinguxwaylandsync.so",
                "bingux-audio-meter",
                "bingux-image-clipboard",
                "bingux-frame",
                "cargo/release/bingux-searchd",
                "cargo/release/bingux-statusd",
            ):
                path = build / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("test fixture\n")
            search_config = config / "bingux/search.json"
            search_config.parent.mkdir(parents=True)
            search_config.write_text(
                json.dumps(
                    {
                        "protocolVersion": 1,
                        "providerManifestPaths": ["/custom/provider.json"],
                        "commands": {
                            "applicationLauncher": ["/usr/bin/gtk-launch"],
                            "fileOpener": ["/custom/open"],
                            "clipboard": ["/custom/copy"],
                        },
                    }
                )
                + "\n"
            )
            environment = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(config)}
            subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--user",
                    "--no-systemd",
                    "--prefix",
                    str(prefix),
                    "--build-dir",
                    str(build),
                ],
                env=environment,
                check=True,
            )
            migrated = json.loads(search_config.read_text())
            self.assertEqual(
                migrated["commands"]["applicationLauncher"],
                [sys.executable, str(prefix / "share/bingux/shell/launch-application.py")],
            )
            self.assertEqual(migrated["commands"]["fileOpener"], ["/custom/open"])
            self.assertEqual(migrated["commands"]["clipboard"], ["/custom/copy"])
            self.assertEqual(migrated["providerManifestPaths"], ["/custom/provider.json"])
            migrated["commands"]["applicationLauncher"] = ["/custom/launch"]
            search_config.write_text(json.dumps(migrated) + "\n")
            subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--user",
                    "--no-systemd",
                    "--prefix",
                    str(prefix),
                    "--build-dir",
                    str(build),
                ],
                env=environment,
                check=True,
            )
            self.assertEqual(
                json.loads(search_config.read_text())["commands"]["applicationLauncher"], ["/custom/launch"]
            )

    def test_stage(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, stage = base / "build", base / "stage"
            for filename in (
                "text/libbinguxtext.so",
                "settings/libbinguxsettings.so",
                "effects/libbinguxeffects.so",
                "wayland-sync/libbinguxwaylandsync.so",
                "bingux-audio-meter",
                "bingux-image-clipboard",
                "bingux-frame",
                "cargo/release/bingux-searchd",
                "cargo/release/bingux-statusd",
            ):
                path = build / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("test fixture\n")
            subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--prefix",
                    "/usr",
                    "--destdir",
                    str(stage),
                    "--build-dir",
                    str(build),
                ],
                check=True,
            )
            for name in ("bingux", "binguxctl", "bingux-settings", "bingux-lock", "bingux-frame"):
                launcher = stage / "usr/bin" / name
                self.assertTrue(launcher.stat().st_mode & 0o111)
                subprocess.run(["sh", "-n", str(launcher)], check=True)
                self.assertNotIn(str(stage), launcher.read_text())
            config = json.loads((stage / "usr/share/bingux/search.json").read_text())
            self.assertEqual(config["commands"]["applicationLauncher"][0], sys.executable)
            self.assertEqual(
                config["commands"]["applicationLauncher"][1], "/usr/share/bingux/shell/launch-application.py"
            )
            self.assertEqual(config["commands"]["fileOpener"], ["/usr/bin/xdg-open"])
            self.assertEqual(config["commands"]["clipboard"], ["/usr/bin/wl-copy"])
            search_service = stage / "usr/libexec/bingux/search-service"
            subprocess.run(["sh", "-n", str(search_service)], check=True)
            self.assertIn("/usr/share/bingux/shell/migrate-search-config.py", search_service.read_text())
            self.assertTrue((stage / "usr/share/bingux/shell/ProfileSettings.qml").is_file())
            self.assertTrue((stage / "usr/share/gnoblin/conf.d/bingux.lua").is_file())
            integration = (stage / "usr/share/gnoblin/conf.d/bingux.lua").read_text()
            self.assertIn('["ext-background-effect-v1"] = true', integration)
            self.assertIn('layer = "^bingux-capture$"', integration)
            self.assertIn('layer = "^bingux-capture-controls$"', integration)
            self.assertIn('bingux = { "bingux-frame", "--compact" }', integration)
            self.assertIn('mode = "auto"', integration)
            self.assertNotIn('["remove-csd"] = true', integration)
            self.assertTrue((stage / "usr/lib/bingux/qml/Bingux/Text/qmldir").is_file())
            self.assertTrue((stage / "usr/lib/bingux/qml/Bingux/Effects/libbinguxeffects.so").is_file())
            self.assertTrue((stage / "usr/lib/bingux/qml/Bingux/Wayland/libbinguxwaylandsync.so").is_file())
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
            lock_unit = (stage / "usr/lib/systemd/user/bingux-lock.service").read_text()
            self.assertIn("ExecStart=/usr/bin/bingux-lock", lock_unit)
            self.assertNotIn("WantedBy=", lock_unit)
            searchd_unit = (stage / "usr/lib/systemd/user/bingux-searchd.service").read_text()
            self.assertIn("ExecStart=/usr/libexec/bingux/search-service", searchd_unit)
            target_unit = (stage / "usr/lib/systemd/user/bingux.target").read_text()
            self.assertIn("WantedBy=gnome-session@gnoblin.target", target_unit)
            self.assertNotIn("WantedBy=graphical-session.target", target_unit)

    def test_missing_build_fails_before_install(self):
        with tempfile.TemporaryDirectory() as name:
            stage = Path(name) / "stage"
            result = subprocess.run(
                ["python3", str(ROOT / "scripts/install-shell.py"), "--destdir", str(stage), "--build-dir", name],
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(stage.exists())

    def test_user_install_is_central_and_removable(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            home, config = base / "home", base / "config"
            for filename in (
                "text/libbinguxtext.so",
                "settings/libbinguxsettings.so",
                "effects/libbinguxeffects.so",
                "wayland-sync/libbinguxwaylandsync.so",
                "bingux-audio-meter",
                "bingux-image-clipboard",
                "bingux-frame",
                "cargo/release/bingux-searchd",
                "cargo/release/bingux-statusd",
            ):
                path = build / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("test fixture\n")
            environment = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(config)}
            subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--user",
                    "--no-systemd",
                    "--prefix",
                    str(prefix),
                    "--build-dir",
                    str(build),
                ],
                env=environment,
                check=True,
            )
            self.assertTrue((prefix / ".bingux-install.json").is_file())
            self.assertTrue((prefix / "share/bingux/shell/ProfileSettings.qml").is_file())
            self.assertTrue((prefix / "share/gnoblin/conf.d/bingux.lua").is_file())
            user_integration = (prefix / "share/gnoblin/conf.d/bingux.lua").read_text()
            self.assertIn('bingux = { "bingux-frame", "--compact" }', user_integration)
            self.assertIn('mode = "auto"', user_integration)
            self.assertNotIn('["remove-csd"] = true', user_integration)
            init = config / "gnoblin/init.lua"
            dropin = config / "gnoblin/conf.d/bingux.lua"
            self.assertTrue(init.is_file())
            self.assertIn('g.load("conf.d/**/*.lua")', init.read_text())
            self.assertTrue(dropin.is_symlink())
            self.assertEqual(dropin.resolve(), (prefix / "share/gnoblin/conf.d/bingux.lua").resolve())
            launcher = home / ".local/bin/bingux"
            self.assertTrue(launcher.is_symlink())
            self.assertEqual(launcher.resolve(), (prefix / "bin/bingux").resolve())
            frame_command = home / ".local/bin/bingux-frame"
            self.assertTrue(frame_command.is_symlink())
            self.assertEqual(frame_command.resolve(), (prefix / "bin/bingux-frame").resolve())
            self.assertTrue((home / ".local/bin/bingux-uninstall").is_symlink())
            unit = config / "systemd/user/bingux.service"
            self.assertTrue(unit.is_symlink())
            self.assertIn(f"ExecStart={prefix}/bin/bingux --no-color", unit.read_text())

            subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--user",
                    "--uninstall",
                    "--no-systemd",
                    "--prefix",
                    str(prefix),
                ],
                env=environment,
                check=True,
            )
            self.assertFalse(prefix.exists())
            self.assertFalse(launcher.exists())
            self.assertFalse(frame_command.exists())
            self.assertFalse((home / ".local/bin/bingux-uninstall").exists())
            self.assertFalse(unit.exists())
            self.assertFalse((config / "bingux").exists())
            self.assertFalse(dropin.exists())
            self.assertTrue(init.exists())

    def test_user_install_does_not_take_over_unmanaged_directory(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            prefix.mkdir(parents=True)
            marker = prefix / "keep-me"
            marker.write_text("unmanaged\n")
            result = subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--user",
                    "--no-systemd",
                    "--prefix",
                    str(prefix),
                    "--build-dir",
                    str(build),
                ],
                env={**os.environ, "HOME": str(base / "home")},
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unmanaged directory", result.stderr)
            self.assertEqual(marker.read_text(), "unmanaged\n")

    def test_user_install_rejects_external_qml_directory(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            result = subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--user",
                    "--no-systemd",
                    "--prefix",
                    str(base / "install"),
                    "--qml-dir",
                    str(base / "outside-qml"),
                    "--build-dir",
                    str(base / "build"),
                ],
                env={**os.environ, "HOME": str(base / "home")},
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("inside --prefix", result.stderr)

    def test_user_install_manages_systemd_target(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            home, config, bin_dir = base / "home", base / "config", base / "bin"
            for filename in (
                "text/libbinguxtext.so",
                "settings/libbinguxsettings.so",
                "effects/libbinguxeffects.so",
                "wayland-sync/libbinguxwaylandsync.so",
                "bingux-audio-meter",
                "bingux-image-clipboard",
                "bingux-frame",
                "cargo/release/bingux-searchd",
                "cargo/release/bingux-statusd",
            ):
                path = build / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("test fixture\n")
            bin_dir.mkdir()
            log = base / "systemctl.log"
            fake_systemctl = bin_dir / "systemctl"
            fake_systemctl.write_text('#!/bin/sh\nprintf \'%s\\n\' "$*" >> "$BINGUX_SYSTEMCTL_LOG"\n')
            fake_systemctl.chmod(0o755)
            environment = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(config),
                "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"],
                "BINGUX_SYSTEMCTL_LOG": str(log),
            }
            subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--user",
                    "--prefix",
                    str(prefix),
                    "--build-dir",
                    str(build),
                ],
                env=environment,
                check=True,
            )
            self.assertIn("daemon-reload", log.read_text())
            self.assertIn("disable bingux.target", log.read_text())
            self.assertIn("enable bingux.target", log.read_text())
            self.assertIn("is-active --quiet gnome-session@gnoblin.target", log.read_text())
            self.assertIn("start bingux.target", log.read_text())
            wants = config / "systemd/user/gnome-session@gnoblin.target.wants"
            wants.mkdir(parents=True)
            enabled_target = wants / "bingux.target"
            enabled_target.symlink_to(prefix / "lib/systemd/user/bingux.target")
            subprocess.run([str(home / ".local/bin/bingux-uninstall")], env=environment, check=True)
            calls = log.read_text()
            self.assertIn("disable --now bingux.target", calls)
            self.assertFalse(enabled_target.exists())

    def test_installed_uninstaller_survives_missing_systemctl(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            home, bin_dir = base / "home", base / "bin"
            for filename in (
                "text/libbinguxtext.so",
                "settings/libbinguxsettings.so",
                "effects/libbinguxeffects.so",
                "wayland-sync/libbinguxwaylandsync.so",
                "bingux-audio-meter",
                "bingux-image-clipboard",
                "bingux-frame",
                "cargo/release/bingux-searchd",
                "cargo/release/bingux-statusd",
            ):
                path = build / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("test fixture\n")
            bin_dir.mkdir()
            (bin_dir / "python3").symlink_to(sys.executable)
            environment = {**os.environ, "HOME": str(home), "PATH": str(bin_dir)}
            subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--user",
                    "--no-systemd",
                    "--prefix",
                    str(prefix),
                    "--build-dir",
                    str(build),
                ],
                env=environment,
                check=True,
            )
            subprocess.run([str(home / ".local/bin/bingux-uninstall")], env=environment, check=True)
            self.assertFalse(prefix.exists())


if __name__ == "__main__":
    unittest.main()
