"""Exercise GIO desktop launch semantics without altering installed applications."""
import json
import io
import importlib.util
from unittest.mock import Mock, patch
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

launcher = Path(__file__).resolve().parents[1] / "shell/bingux/launch-application.py"


class ApplicationLaunch(unittest.TestCase):
    def test_application_id_ending_in_desktop(self):
        with tempfile.TemporaryDirectory() as temporary:
            apps = Path(temporary) / "applications"
            apps.mkdir()
            record = Path(temporary) / "launched"
            (apps / "org.test.desktop.desktop").write_text(
                f"[Desktop Entry]\nType=Application\nName=Test\nExec=/usr/bin/touch {record}\n")
            for identity in ("org.test.desktop", "org.test.desktop.desktop"):
                record.unlink(missing_ok=True)
                result = subprocess.run([sys.executable, str(launcher), identity],
                    env=os.environ | {"XDG_DATA_HOME": temporary, "XDG_DATA_DIRS": temporary},
                    capture_output=True, text=True, timeout=6)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(record.exists())

    def test_missing_and_path_ids_return_clean_errors(self):
        for identity in ("bingux-no-such-test-app-984a5", "../invalid"):
            result = subprocess.run([sys.executable, str(launcher), identity], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("Traceback", result.stderr)

    def test_dock_launch_uses_host_manager(self):
        spec = importlib.util.spec_from_file_location("launcher", launcher)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with patch.object(module.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")) as run:
            self.assertEqual(module.main(["--notify-errors", "com.spotify.Client"]), 0)
        command = run.call_args.args[0]
        self.assertEqual(command[:4], ["systemd-run", "--user", "--collect", "--quiet"])
        self.assertIn("--property=ExitType=cgroup", command)
        self.assertIn("--host-launch", command)
        self.assertEqual(command[-1], "com.spotify.Client")

    def test_host_manager_timeout_is_reported(self):
        spec = importlib.util.spec_from_file_location("launcher_host_timeout", launcher)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with patch.object(module.subprocess, "run", side_effect=subprocess.TimeoutExpired("systemd-run", 5)), \
                patch("sys.stderr", new_callable=io.StringIO) as stderr:
            result = module.main(["--dock-feedback", "com.spotify.Client"])
        self.assertEqual(result, 1)
        self.assertIn("host application launcher did not respond", stderr.getvalue())

    def test_dock_launch_falls_back_when_host_manager_rejects_unit(self):
        spec = importlib.util.spec_from_file_location("launcher_host_fallback", launcher)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        entry = Mock()
        entry.get_display_name.return_value = "Fallback app"
        entry.list_actions.return_value = []
        entry.launch_uris_as_manager_with_fds.return_value = True
        with patch.object(module.subprocess, "run",
                          return_value=subprocess.CompletedProcess([], 1, "No space left on device", "")), \
                patch.object(module.GioUnix.DesktopAppInfo, "new", return_value=entry):
            self.assertEqual(module.main(["--dock-feedback", "com.example.Fallback"]), 0)
        entry.launch_uris_as_manager_with_fds.assert_called_once()

    def test_host_feedback_reports_missing_entry(self):
        if not os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
            self.skipTest("Requires a running user manager")
        result = subprocess.run([sys.executable, str(launcher), "--dock-feedback", "bingux-no-such-test-app-984a5"],
            capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 1, result.stderr)
        line = next(line for line in result.stdout.splitlines() if line.startswith("BINGUX_LAUNCH_ERROR "))
        self.assertIn("could not be found", json.loads(line[20:])["message"])

    def test_early_process_failure_is_reported(self):
        with tempfile.TemporaryDirectory() as temporary:
            apps = Path(temporary) / "applications"
            apps.mkdir()
            (apps / "bingux-failure.desktop").write_text(
                "[Desktop Entry]\nType=Application\nName=Failing app\nExec=/bin/sh -c 'exit 42'\n")
            result = subprocess.run([sys.executable, str(launcher), "bingux-failure"],
                env=os.environ | {"XDG_DATA_HOME": temporary, "XDG_DATA_DIRS": temporary},
                capture_output=True, text=True, timeout=6)
            self.assertEqual(result.returncode, 1)
            self.assertIn("exited with code 42", result.stderr)
            self.assertIn("Failing app", result.stderr)

    def test_blocked_gio_activation_is_reported_and_does_not_stick(self):
        spec = importlib.util.spec_from_file_location("launcher_timeout", launcher)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        entry = Mock()
        entry.get_display_name.return_value = "Hanging app"
        entry.list_actions.return_value = []

        def blocked_activation(*_args):
            time.sleep(.25)
            return True

        entry.launch_uris_as_manager_with_fds.side_effect = blocked_activation
        module.LAUNCH_ACCEPT_TIMEOUT = .05
        with patch.object(module.GioUnix.DesktopAppInfo, "new", return_value=entry), \
                patch("sys.stderr", new_callable=io.StringIO) as stderr:
            result = module.main(["bingux-hanging-test"])
        self.assertEqual(result, 1)
        self.assertIn("did not respond", stderr.getvalue())

    def test_missing_executable_and_crash_are_reported(self):
        for command, expected in (("/bingux-no-such-executable", "executable is unavailable"),
                ("/bin/sh -c 'kill -TERM $$'", "signal 15")):
            with self.subTest(command=command), tempfile.TemporaryDirectory() as temporary:
                apps = Path(temporary) / "applications"
                apps.mkdir()
                (apps / "bingux-broken.desktop").write_text(
                    f"[Desktop Entry]\nType=Application\nName=Broken app\nExec={command}\n")
                result = subprocess.run([sys.executable, str(launcher), "bingux-broken"],
                    env=os.environ | {"XDG_DATA_HOME": temporary, "XDG_DATA_DIRS": temporary},
                    capture_output=True, text=True, timeout=6)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(expected, result.stderr)
                self.assertIn("BINGUX_LAUNCH_ERROR ", result.stdout)

    def test_desktop_action_and_field_codes(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            apps = directory / "applications"
            apps.mkdir()
            record = directory / "launched.json"
            entrypoint = directory / "entry.py"
            entrypoint.write_text("import json, os, sys\nfrom pathlib import Path\n"
                + f"Path({str(record)!r}).write_text(json.dumps({{'args': sys.argv[1:], 'cwd': os.getcwd()}}))\n")
            desktop = apps / "bingux-launch-test.desktop"
            desktop.write_text(f"[Desktop Entry]\nType=Application\nName=Test\nPath={directory}\n"
                + f"Exec={sys.executable} {entrypoint} normal %k\nActions=new-window;\n"
                + f"[Desktop Action new-window]\nName=New\nExec={sys.executable} {entrypoint} new-window %k\n")
            env = os.environ | {"XDG_DATA_HOME": temporary, "XDG_DATA_DIRS": temporary}
            for flags, expected in (([], "normal"), (["--new-window"], "new-window")):
                record.unlink(missing_ok=True)
                result = subprocess.run([sys.executable, str(launcher), *flags, "bingux-launch-test"], env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                deadline = time.monotonic() + 3
                while not record.exists() and time.monotonic() < deadline: time.sleep(.01)
                self.assertTrue(record.exists(), result.stderr)
                state = json.loads(record.read_text())
                self.assertEqual(state, {"args": [expected, str(desktop)], "cwd": temporary})


if __name__ == "__main__": unittest.main()
