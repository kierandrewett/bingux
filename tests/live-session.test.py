#!/usr/bin/env python3
"""distro/installer/session: the live session loop and bingux-live-switch.

A fake compositor stands in for gnoblin. It logs its arguments and exits 0 on
TERM, so the loop's real restart, switch and exit paths run without a display.
"""

import os
import shutil
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SESSION = REPO / "distro/installer/session"
INSTALLER_LUA = REPO / "distro/installer/gnoblin/installer.lua"
FIXTURE_LUA = "/fixture/installer.lua"

FAKE_COMPOSITOR = """#!/bin/sh
echo "$*" >>"$FAKE_LOG"
trap 'exit 0' TERM
while :; do sleep 0.05; done
"""


def wait_for(predicate, timeout=10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.02)
    return False


class LiveSessionTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="live-session-"))
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.runtime = self.tmp / "runtime"
        self.runtime.mkdir()
        self.state = self.runtime / "bingux-live"
        self.log = self.tmp / "compositor.log"
        fake = self.tmp / "fake-gnoblin"
        fake.write_text(FAKE_COMPOSITOR)
        fake.chmod(0o755)
        self.env = dict(
            os.environ,
            XDG_RUNTIME_DIR=str(self.runtime),
            FAKE_LOG=str(self.log),
            BINGUX_LIVE_COMPOSITOR=str(fake),
            BINGUX_INSTALLER_CONFIG=FIXTURE_LUA,
        )
        self.session = None
        self.addCleanup(self.stop_session)

    def stop_session(self):
        if self.session is None:
            return
        if self.session.poll() is None:
            self.session.kill()
            self.session.wait()
        self.session.stdout.close()
        self.session.stderr.close()

    def start(self, *modes):
        self.session = subprocess.Popen(
            [str(SESSION / "bingux-live-session"), *modes],
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    def switch(self, mode):
        return subprocess.run(
            [str(SESSION / "bingux-live-switch"), *([mode] if mode else [])],
            env=self.env,
            capture_output=True,
            text=True,
        )

    def launches(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def compositor_pid(self):
        try:
            return int((self.state / "pid").read_text())
        except (OSError, ValueError):
            return None

    def wait_for_launch(self, count):
        self.assertTrue(
            wait_for(lambda: len(self.launches()) >= count),
            f"expected {count} compositor launches, got {self.launches()}",
        )

    def wait_for_new_pid(self, old):
        self.assertTrue(
            wait_for(lambda: self.compositor_pid() not in (None, old)),
            "the loop did not record the new compositor pid",
        )

    def test_installer_is_the_default_mode(self):
        self.start()
        self.wait_for_launch(1)
        self.assertEqual(self.launches(), [f"--config {FIXTURE_LUA}"])

    def test_switch_moves_between_installer_and_live(self):
        self.start()
        self.wait_for_launch(1)
        first = self.compositor_pid()

        result = self.switch("live")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.wait_for_launch(2)
        self.wait_for_new_pid(first)
        self.assertEqual(self.launches()[1], "")

        second = self.compositor_pid()
        result = self.switch("installer")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.wait_for_launch(3)
        self.wait_for_new_pid(second)
        self.assertEqual(self.launches()[2], f"--config {FIXTURE_LUA}")

    def test_compositor_exit_without_switch_ends_the_session(self):
        self.start("live")
        self.wait_for_launch(1)
        os.kill(self.compositor_pid(), signal.SIGTERM)
        self.session.wait(timeout=10)
        self.assertEqual(self.session.returncode, 0)
        self.assertEqual(len(self.launches()), 1)

    def test_switch_without_a_session_fails(self):
        result = self.switch("live")
        self.assertEqual(result.returncode, 1)
        self.assertIn("no live session", result.stderr)
        self.assertFalse((self.state / "next").exists())

    def test_switch_rejects_an_unknown_mode(self):
        result = self.switch("bogus")
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage", result.stderr)

    def test_session_rejects_an_unknown_mode(self):
        self.start("bogus")
        self.assertEqual(self.session.wait(timeout=10), 2)

    def test_session_scripts_are_executable(self):
        for name in ("bingux-live-session", "bingux-live-switch", "bingux-installer-ui"):
            with self.subTest(name=name):
                self.assertTrue(os.access(SESSION / name, os.X_OK))

    def test_installer_config_wires_the_installer_ui(self):
        text = INSTALLER_LUA.read_text()
        self.assertIn('"bingux-greeter"] = { name = "bingux-greeter", enable = false }', text)
        self.assertIn('command = { "/usr/bin/bingux-installer-ui" }', text)

    def test_installer_config_is_valid_lua(self):
        luac = shutil.which("luac")
        if luac is None:
            self.skipTest("luac is not installed")
        result = subprocess.run([luac, "-p", str(INSTALLER_LUA)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
