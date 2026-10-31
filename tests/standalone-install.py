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

# Every build output the installer copies, as placeholder files.
BUILD_OUTPUTS = (
    "text/libbinguxtext.so",
    "effects/libbinguxeffects.so",
    "wayland-sync/libbinguxwaylandsync.so",
    "bingux-audio-meter",
    "bingux-image-clipboard",
    "bingux-frame",
    "bingux-shell/quickshell/src/bingux-shell",
    "bingux-shell.sxa",
    "cargo/release/bingux-searchd",
    "cargo/release/bingux-statusd",
    "cargo/release/bingux-session",
    "cargo/release/bingux-config",
    "cargo/release/bingux-helper",
    "cargo/release/bingux-settings",
    "cargo/release/bingux-files",
    "cargo/release/bingux-taxonomy",
    "cargo/release/bingux-preview-host",
    "cargo/release/bingux-effects",
    "cargo/release/xdg-desktop-portal-bingux",
    "cargo/release/bingux-global-shortcuts",
)


def write_build_fixtures(build):
    for filename in BUILD_OUTPUTS:
        path = build / filename
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("test fixture\n")


class InstallTest(unittest.TestCase):
    def test_user_upgrade_migrates_legacy_launcher_without_losing_search_settings(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            home, config = base / "home", base / "config"
            write_build_fixtures(build)
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
            environment = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(config),
            }
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
                [
                    sys.executable,
                    str(prefix / "libexec/bingux/shell/launch-application.py"),
                ],
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
                json.loads(search_config.read_text())["commands"]["applicationLauncher"],
                ["/custom/launch"],
            )

    def test_stage(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, stage = base / "build", base / "stage"
            write_build_fixtures(build)
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
            for name in ("binguxctl", "bingux-frame"):
                launcher = stage / "usr/bin" / name
                self.assertTrue(launcher.stat().st_mode & 0o111)
                subprocess.run(["sh", "-n", str(launcher)], check=True)
                self.assertNotIn(str(stage), launcher.read_text())
            # The shell surfaces are one binary; the names select the mode.
            self.assertTrue((stage / "usr/bin/bingux-shell").stat().st_mode & 0o111)
            for name in (
                "bingux",
                "bingux-lock",
                "bingux-greeter",
                "bingux-popouts",
                "bingux-wallpaper",
            ):
                link = stage / "usr/bin" / name
                self.assertTrue(link.is_symlink())
                self.assertEqual(os.readlink(link), "bingux-shell")
            self.assertIn(
                '"$BINGUX_QUICKSHELL" --shell-dir',
                (stage / "usr/bin/binguxctl").read_text(),
            )
            config = json.loads((stage / "usr/share/bingux/search.json").read_text())
            self.assertEqual(config["commands"]["applicationLauncher"][0], sys.executable)
            self.assertEqual(
                config["commands"]["applicationLauncher"][1],
                "/usr/libexec/bingux/shell/launch-application.py",
            )
            self.assertEqual(config["commands"]["fileOpener"], ["/usr/bin/xdg-open"])
            self.assertEqual(config["commands"]["clipboard"], ["/usr/bin/wl-copy"])
            search_service = stage / "usr/libexec/bingux/search-service"
            subprocess.run(["sh", "-n", str(search_service)], check=True)
            # The command points at the installed path, never the staging root.
            self.assertIn(
                "exec /usr/libexec/bingux/search-service",
                (stage / "usr/bin/bingux-search-service").read_text(),
            )
            self.assertIn(
                "/usr/libexec/bingux/shell/migrate-search-config.py",
                search_service.read_text(),
            )
            # QML and assets ship in the archive beside the QML plugins, not as loose files.
            self.assertTrue((stage / "usr/lib/bingux/bingux-shell.sxa").is_file())
            self.assertFalse((stage / "usr/share/bingux/shell").exists())
            self.assertTrue((stage / "usr/libexec/bingux/shell/bingux_runtime.py").is_file())
            # The shell's icons ship in the archive; no theme is installed beside it.
            self.assertFalse((stage / "usr/share/bingux/icons").exists())
            self.assertFalse((stage / "usr/share/icons/Adwaita").exists())
            self.assertTrue((stage / "usr/share/gnoblin/conf.d/bingux.lua").is_file())
            integration = (stage / "usr/share/gnoblin/conf.d/bingux.lua").read_text()
            self.assertIn('["ext-background-effect-v1"] = true', integration)
            self.assertIn('layer = "^bingux-capture$"', integration)
            self.assertIn('layer = "^bingux-capture-controls$"', integration)
            self.assertIn(
                'bingux = { "/usr/libexec/bingux/bingux-frame", "--compact" }',
                integration,
            )
            self.assertIn('mode = "auto"', integration)
            self.assertIn('["remove-csd"] = true', integration)
            self.assertTrue((stage / "usr/lib/bingux/qml/Bingux/Text/qmldir").is_file())
            self.assertTrue((stage / "usr/lib/bingux/qml/Bingux/Effects/libbinguxeffects.so").is_file())
            self.assertTrue((stage / "usr/lib/bingux/qml/Bingux/Wayland/libbinguxwaylandsync.so").is_file())
            self.assertFalse((stage / "home").exists())
            self.assertFalse((stage / "usr/libexec/bingux/shell/__pycache__").exists())
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
                self.assertTrue((stage / f"usr/bin/bingux-{name}-ui").is_symlink())
            lock_unit = (stage / "usr/lib/systemd/user/bingux-lock.service").read_text()
            self.assertIn("ExecStart=/usr/bin/bingux-lock", lock_unit)
            self.assertNotIn("WantedBy=", lock_unit)
            searchd_unit = (stage / "usr/lib/systemd/user/bingux-searchd.service").read_text()
            self.assertIn("ExecStart=/usr/libexec/bingux/search-service", searchd_unit)
            target_unit = (stage / "usr/lib/systemd/user/bingux.target").read_text()
            self.assertIn("WantedBy=gnoblin-session.target", target_unit)
            self.assertNotIn("gnome-session@gnoblin.target", target_unit)
            self.assertNotIn("WantedBy=graphical-session.target", target_unit)
            # Only the helper daemons and the greeter wallpaper export are pulled in. The
            # shell and its surfaces come from Gnoblin's Lua autostart, so wanting them
            # here would start a second copy.
            wants = next(line for line in target_unit.splitlines() if line.startswith("Wants="))
            self.assertEqual(
                set(wants[len("Wants=") :].split()),
                {
                    "bingux-searchd.service",
                    "bingux-statusd.service",
                    "xdg-desktop-portal-bingux.service",
                    "bingux-global-shortcuts.service",
                    "bingux-greeter-export.service",
                    "bingux-greeter-export.path",
                    "bingux-bsm-publish.service",
                    "bingux-bsm-publish.path",
                },
            )
            self.assertTrue((stage / "usr/lib/systemd/user/bingux-greeter-export.path").is_file())
            self.assertTrue((stage / "usr/lib/systemd/user/bingux-bsm-publish.path").is_file())
            self.assertTrue((stage / "usr/lib/systemd/user/bingux-bsm-publish.service").is_file())
            self.assertTrue((stage / "usr/lib/tmpfiles.d/bingux-greeter.conf").is_file())
            portal_unit = (stage / "usr/lib/systemd/user/xdg-desktop-portal-bingux.service").read_text()
            self.assertIn("ExecStart=/usr/libexec/bingux/xdg-desktop-portal-bingux", portal_unit)
            self.assertIn("Environment=BINGUXCTL=/usr/bin/binguxctl", portal_unit)
            portal_dbus = stage / "usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.bingux.service"
            self.assertIn(
                "Exec=/usr/libexec/bingux/xdg-desktop-portal-bingux",
                portal_dbus.read_text(),
            )

            # The files each package takes: applications, sound effects, metadata.
            for path in (
                "usr/bin/bingux-settings",
                "usr/bin/bingux-files",
                "usr/bin/bingux-taxonomy",
                "usr/bin/bingux-preview-host",
                "usr/bin/bingux-effects",
                "usr/bin/bingux-config",
                "usr/lib/systemd/user/bingux-effects@.service",
                "usr/lib/systemd/user-preset/90-bingux.preset",
                "usr/share/man/man1/bingux-settings.1",
                "usr/share/man/man1/bingux-files.1",
                "usr/share/man/man1/bingux-config.1",
                "usr/share/man/man1/bingux-effects.1",
                "usr/share/man/man1/bingux-taxonomy.1",
                "usr/share/metainfo/dev.drewett.bingux.Settings.metainfo.xml",
                "usr/share/metainfo/dev.drewett.bingux.Files.metainfo.xml",
                "usr/share/metainfo/dev.drewett.bingux.Taxonomy.metainfo.xml",
                "usr/share/applications/bingux-files.desktop",
                "usr/share/icons/hicolor/scalable/apps/bingux-files.svg",
            ):
                self.assertTrue((stage / path).is_file(), path)
            self.assertFalse((stage / "usr/lib/ladspa").exists(), "no plugin of Bingux's own is installed")
            effects_unit = (stage / "usr/lib/systemd/user/bingux-effects@.service").read_text()
            self.assertIn("ExecStart=/usr/bin/bingux-effects run %i", effects_unit)
            self.assertIn(
                "enable bingux.target",
                (stage / "usr/lib/systemd/user-preset/90-bingux.preset").read_text(),
            )
            # The shell draws only Bingux icons: no copy of the Adwaita theme is installed,
            # and the licence of the Bingux icons sits beside the other licences.
            self.assertFalse((stage / "usr/share/bingux/icons").exists())
            self.assertFalse((stage / "usr/share/icons/Adwaita").exists())
            self.assertTrue((stage / "usr/share/licenses/bingux/icons/COPYING").is_file())
            self.assertFalse((stage / "usr/share/licenses/adwaita-icon-theme").exists())
            self.assertFalse((stage / "usr/share/licenses/bingux/adwaita-icon-theme").exists())
            manifest = json.loads((stage / "usr/share/bingux/search-apps/bingux-settings.json").read_text())
            self.assertEqual(manifest["command"], ["/usr/bin/bingux-settings"])
            # A package builder stages below a build root; no installed file may name it
            # (rpm's check-buildroot rejects the build when one does).
            for path in stage.rglob("*"):
                if path.is_file() and not path.is_symlink():
                    self.assertNotIn(str(stage).encode(), path.read_bytes(), str(path))

    def test_portal_service_follows_a_non_usr_prefix(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, stage = base / "build", base / "stage"
            write_build_fixtures(build)
            subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--prefix",
                    "/opt/bingux",
                    "--destdir",
                    str(stage),
                    "--build-dir",
                    str(build),
                ],
                check=True,
            )
            prefix = stage / "opt/bingux"
            dbus = (prefix / "share/dbus-1/services/org.freedesktop.impl.portal.desktop.bingux.service").read_text()
            self.assertIn("Exec=/opt/bingux/libexec/bingux/xdg-desktop-portal-bingux", dbus)
            self.assertNotIn("/usr/libexec", dbus)
            unit = (prefix / "lib/systemd/user/xdg-desktop-portal-bingux.service").read_text()
            self.assertIn("ExecStart=/opt/bingux/libexec/bingux/xdg-desktop-portal-bingux", unit)
            self.assertIn("Environment=BINGUXCTL=/opt/bingux/bin/binguxctl", unit)

    def test_missing_build_fails_before_install(self):
        with tempfile.TemporaryDirectory() as name:
            stage = Path(name) / "stage"
            result = subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts/install-shell.py"),
                    "--destdir",
                    str(stage),
                    "--build-dir",
                    name,
                ],
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(stage.exists())

    def test_user_install_is_central_and_removable(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "install"
            home, config = base / "home", base / "config"
            write_build_fixtures(build)
            environment = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(config),
            }
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
            self.assertTrue((prefix / "lib/bingux/bingux-shell.sxa").is_file())
            self.assertTrue((prefix / "share/gnoblin/conf.d/bingux.lua").is_file())
            user_integration = (prefix / "share/gnoblin/conf.d/bingux.lua").read_text()
            self.assertIn(
                f'bingux = {{ "{prefix}/libexec/bingux/bingux-frame", "--compact" }}',
                user_integration,
            )
            self.assertIn('mode = "auto"', user_integration)
            self.assertIn('["remove-csd"] = true', user_integration)
            init = config / "gnoblin/init.lua"
            dropin = config / "gnoblin/conf.d/bingux.lua"
            self.assertTrue(init.is_file())
            self.assertIn('gnoblin.load("conf.d/**/*.lua")', init.read_text())
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
            write_build_fixtures(build)
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
            self.assertIn("disable quickshell.service bingux.service", log.read_text())
            self.assertIn("enable bingux.target", log.read_text())
            self.assertIn("is-active --quiet gnoblin-session.target", log.read_text())
            self.assertIn("start bingux.target", log.read_text())
            wants = config / "systemd/user/gnoblin-session.target.wants"
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
            write_build_fixtures(build)
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

    def test_apps_install_has_no_shell_and_uninstall_restores_everything(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, home = base / "build", base / "home"
            prefix = home / ".local"
            write_build_fixtures(build)
            # An older install of the same program, and a hand-made entry, are the user's.
            (prefix / "bin").mkdir(parents=True)
            (prefix / "share/applications").mkdir(parents=True)
            older = prefix / "bin/bingux-settings"
            older.symlink_to("/nonexistent/older-bingux-settings")
            entry = prefix / "share/applications/bingux-settings.desktop"
            entry.write_text("[Desktop Entry]\nName=Mine\n")
            before = sorted(str(p.relative_to(prefix)) for p in prefix.rglob("*"))
            environment = {**os.environ, "HOME": str(home)}
            command = [
                "python3",
                str(ROOT / "scripts/install-shell.py"),
                "--apps",
                "--no-strip",
                "--no-cache-update",
                "--prefix",
                str(prefix),
                "--build-dir",
                str(build),
            ]
            for _ in range(2):  # the second run is an upgrade and must not displace its own files
                subprocess.run(command, env=environment, check=True, capture_output=True)
            for program in (
                "bingux-config",
                "bingux-settings",
                "bingux-files",
                "bingux-preview-host",
                "bingux-taxonomy",
                "bingux-effects",
            ):
                self.assertTrue(os.access(prefix / "bin" / program, os.X_OK), program)
            self.assertFalse((prefix / "lib/ladspa").exists(), "no plugin of Bingux's own is installed")
            for app, mime in (
                ("bingux-settings", "x-scheme-handler/bingux-settings;"),
                ("bingux-files", "inode/directory;x-scheme-handler/bingux-files;"),
                ("bingux-taxonomy", "x-scheme-handler/bingux-taxonomy;"),
            ):
                desktop = (prefix / "share/applications" / (app + ".desktop")).read_text()
                self.assertIn(f"Exec={prefix}/bin/{app} ", desktop)
                self.assertIn(f"Exec={prefix}/bin/{app} ", desktop.split("[Desktop Action", 1)[1])
                self.assertIn("MimeType=" + mime, desktop)
                self.assertTrue((prefix / "share/icons/hicolor/scalable/apps" / (app + ".svg")).is_file())
            self.assertFalse((prefix / "share/metainfo").exists())
            self.assertTrue((prefix / "share/man/man1/bingux-settings.1").is_file())
            search = json.loads((prefix / "share/bingux/search-apps/bingux-settings.json").read_text())
            self.assertEqual(search["command"], [f"{prefix}/bin/bingux-settings"])
            unit = (prefix / "share/systemd/user/bingux-effects@.service").read_text()
            self.assertIn(f"ExecStart={prefix}/bin/bingux-effects run %i", unit)
            # What the user had stays reachable beside the install.
            self.assertTrue((prefix / "bin/bingux-settings.pre-bingux-apps.bak").is_symlink())
            backup = prefix / "share/applications/bingux-settings.desktop.pre-bingux-apps.bak"
            self.assertIn("Name=Mine", backup.read_text())
            # The shell is not part of this install, and the session is untouched.
            for shell_path in (
                "bin/bingux",
                "bin/bingux-searchd",
                "share/bingux/shell",
                "share/gnoblin",
                "lib/systemd/user",
                "share/icons/Adwaita",
            ):
                self.assertFalse((prefix / shell_path).exists(), shell_path)
            self.assertFalse((home / ".config").exists())
            uninstall = [str(prefix / "bin/bingux-apps-uninstall"), "--no-cache-update"]
            subprocess.run(uninstall, env=environment, check=True)
            self.assertEqual(sorted(str(p.relative_to(prefix)) for p in prefix.rglob("*")), before)
            self.assertEqual(os.readlink(older), "/nonexistent/older-bingux-settings")
            self.assertEqual(entry.read_text(), "[Desktop Entry]\nName=Mine\n")

    def test_apps_uninstall_keeps_a_file_changed_after_the_install(self):
        with tempfile.TemporaryDirectory() as name:
            base = Path(name)
            build, prefix = base / "build", base / "home/.local"
            write_build_fixtures(build)
            common = ["--apps", "--no-cache-update", "--prefix", str(prefix)]
            script = str(ROOT / "scripts/install-shell.py")
            subprocess.run(
                ["python3", script, *common, "--no-strip", "--build-dir", str(build)],
                check=True,
                capture_output=True,
            )
            edited = prefix / "share/applications/bingux-files.desktop"
            edited.write_text(edited.read_text() + "# edited by hand\n")
            result = subprocess.run(
                ["python3", script, *common, "--uninstall"],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("changed after the install", result.stderr)
            self.assertEqual(
                [p.name for p in prefix.rglob("*") if p.is_file()],
                ["bingux-files.desktop"],
            )


if __name__ == "__main__":
    unittest.main()
