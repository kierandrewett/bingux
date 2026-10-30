#!/usr/bin/env python3
"""scripts/app_env.py: every script that starts a Bingux app gives it the memory
GSettings backend and no desktop, and every such script goes through the helper."""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS))

import app_env  # noqa: E402

# The scripts that start an app, directly or through a harness session, and where each one
# builds that app's environment. A new script that runs an app belongs in this list.
LAUNCHERS = [
    "probe_lib.py",
    "layout-lint.py",
    "layout-stability.py",
    "frame-gate.py",
    "frame-walk.py",
    "frame-time.py",
    "loop-check.py",
    "nav-slide.py",
    "number-roll-check.py",
    "mobile-walk.py",
    "files-shift.py",
    "focus-ring-sheet.py",
    "state-check.py",
    "rules-check.py",
]
# Scripts that reach an app only through probe_lib.run_job or layout-lint.run_job.
THROUGH = ["pixel-check.py", "structure-stability.py", "scroll-continuity.py", "copy-lint.py", "overflow-check.py",
           "focus-ring-check.py", "tooltip-check.py"]


class IsolatedEnvironment(unittest.TestCase):
    def test_the_memory_backend_is_forced_over_the_callers_value(self):
        env = app_env.isolated({"GSETTINGS_BACKEND": "dconf", "PATH": "/usr/bin"})
        self.assertEqual(env["GSETTINGS_BACKEND"], "memory")
        self.assertEqual(env["PATH"], "/usr/bin")

    def test_the_desktop_is_not_inherited(self):
        env = app_env.isolated(
            {"WAYLAND_DISPLAY": "wayland-0", "DISPLAY": ":0", "DCONF_PROFILE": "user", "HOME": "/h"}
        )
        for name in ("WAYLAND_DISPLAY", "DISPLAY", "DCONF_PROFILE"):
            self.assertNotIn(name, env)
        self.assertEqual(env["HOME"], "/h")

    def test_the_callers_mapping_is_not_changed(self):
        before = {"WAYLAND_DISPLAY": "wayland-0"}
        app_env.isolated(before)
        self.assertEqual(before, {"WAYLAND_DISPLAY": "wayland-0"})

    def test_the_process_environment_is_the_default(self):
        self.assertEqual(app_env.isolated()["GSETTINGS_BACKEND"], "memory")


class EveryLauncherUsesIt(unittest.TestCase):
    def test_each_launcher_builds_its_environment_through_the_helper(self):
        for name in LAUNCHERS:
            text = (SCRIPTS / name).read_text()
            self.assertRegex(text, r"app_env|isolated\(", f"{name} starts an app without scripts/app_env.py")
            self.assertTrue(
                re.search(r"\bisolated\(", text), f"{name} imports app_env but never calls isolated()"
            )

    def test_the_scripts_that_go_through_a_runner_import_one_that_does(self):
        for name in THROUGH:
            text = (SCRIPTS / name).read_text()
            self.assertRegex(text, r"probe_lib|layout-lint|lint", f"{name} reaches no runner")

    def test_no_script_pops_the_display_by_hand_without_the_helper(self):
        for path in sorted(SCRIPTS.glob("*.py")):
            text = path.read_text()
            if path.name == "app_env.py":
                continue
            if re.search(r'pop\("WAYLAND_DISPLAY"', text):
                self.assertIn("isolated(", text, f"{path.name} drops the display but not the dconf")


class RunnersGiveTheAppAnIsolatedEnvironment(unittest.TestCase):
    """The two runners every capture goes through, run with `env` as the "app": what it prints is
    what a Bingux app would have inherited."""

    DIRTY = {"WAYLAND_DISPLAY": "wayland-0", "DISPLAY": ":0", "GSETTINGS_BACKEND": "dconf"}

    def printed(self, runner) -> str:
        import os
        import tempfile
        from unittest import mock

        with tempfile.TemporaryDirectory() as scratch, mock.patch.dict(os.environ, self.DIRTY):
            result = runner(Path(scratch))
            return Path(result["log"]).read_text()

    def check(self, text: str) -> None:
        lines = text.splitlines()
        self.assertIn("GSETTINGS_BACKEND=memory", lines)
        self.assertNotIn("GSETTINGS_BACKEND=dconf", lines)
        for name in ("WAYLAND_DISPLAY", "DISPLAY"):
            self.assertFalse([line for line in lines if line.startswith(name + "=")], f"{name} reached the app")

    def test_layout_lint_run_job(self):
        import importlib.util

        spec = importlib.util.spec_from_file_location("layout_lint_for_test", SCRIPTS / "layout-lint.py")
        lint = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(lint)
        job = {"name": "env", "argv": ["sh", "-c", "env"], "pages": []}
        self.check(self.printed(lambda scratch: lint.run_job(job, scratch, 30)))

    def test_probe_lib_run_job(self):
        import probe_lib

        job = {"name": "env", "argv": ["sh", "-c", "env"], "pages": []}
        variant = probe_lib.parse_variants("dark")[0]
        self.check(self.printed(lambda scratch: probe_lib.run_job(job, scratch, 30, variant)))

if __name__ == "__main__":
    unittest.main()
