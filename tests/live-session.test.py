#!/usr/bin/env python3
"""distro/installer/session: the live session loop and bingux-live-switch.

A fake compositor stands in for gnoblin. It logs its arguments and exits 0 on
TERM, so the loop's real restart, switch and exit paths run without a display.
The failure paths use fake compositors that exit with an error, stay up
without a control socket, or report a configuration fallback, as Gnoblin does
on a busy machine (E2E runs 1, 3, 4 and 5 on 5f53de8.iso).
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

# Behaviour comes from the file $FAKE_PLAN: one word per launch, in order, and
# the last word again for every later launch.
#   run       start, create the control socket, stay up
#   fail      exit with status 1 at once (Gnoblin: runtime configuration timed out)
#   nosocket  stay up and never create the control socket
#   fallback  create the control socket and the "defaults" fallback note
#   lost      create the control socket and the "Lua runtime recovery failed" note
FAKE_PLANNED = """#!/usr/bin/env python3
import os, signal, socket, sys, time
from pathlib import Path

with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(" ".join(sys.argv[1:]) + "\\n")
plan = Path(os.environ["FAKE_PLAN"]).read_text().split()
counter = Path(os.environ["FAKE_LOG"]).read_text().count("\\n")
step = plan[min(counter, len(plan)) - 1]
runtime = Path(os.environ["XDG_RUNTIME_DIR"]) / "gnoblin"
runtime.mkdir(exist_ok=True)
if step == "fail":
    sys.exit(1)
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
if step in ("run", "fallback", "lost"):
    server = socket.socket(socket.AF_UNIX)
    server.bind(str(runtime / "compositor-v1.sock"))
if step == "fallback":
    (runtime / "config-fallback").write_text("defaults\\nThe Lua runtime restarted\\n")
if step == "lost":
    (runtime / "config-fallback").write_text("notice\\nLua runtime recovery failed. The compositor is still running.\\n")
while True:
    time.sleep(0.05)
"""

FAKE_SYSTEMD_CAT = """#!/bin/sh
# Records its use, then does what systemd-cat does: runs the command.
echo "systemd-cat $*" >>"$FAKE_CAT_LOG"
[ "${FAKE_CAT_BROKEN:-}" = 1 ] && exit 1
shift 2
exec "$@"
"""


def wait_for(predicate, timeout=10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.02)
    return False


class SessionCase(unittest.TestCase):
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
            BINGUX_LIVE_JOURNAL="0",
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

    def plan(self, *steps, **settings):
        """Use the planned fake compositor; `settings` become BINGUX_LIVE_* variables."""
        fake = self.tmp / "fake-planned"
        fake.write_text(FAKE_PLANNED)
        fake.chmod(0o755)
        (self.tmp / "plan").write_text(" ".join(steps))
        self.env.update(BINGUX_LIVE_COMPOSITOR=str(fake), FAKE_PLAN=str(self.tmp / "plan"))
        for name, value in settings.items():
            self.env[f"BINGUX_LIVE_{name.upper()}"] = str(value)

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


class LiveSessionTest(SessionCase):
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


class StartRetryTest(SessionCase):
    """The loop starts a compositor that did not come up again (5f53de8.iso black boots)."""

    def test_early_failure_starts_the_compositor_again(self):
        # Gnoblin exits 1 when it waited too long for its runtime configuration.
        self.plan("fail", "run")
        self.start()
        self.wait_for_launch(2)
        self.assertTrue(wait_for(lambda: (self.runtime / "gnoblin/compositor-v1.sock").exists()))
        self.assertIsNone(self.session.poll(), "the session ended after the second try came up")
        self.assertEqual(self.launches(), [f"--config {FIXTURE_LUA}"] * 2)

    def test_repeated_failure_ends_the_session_with_the_error_status(self):
        self.plan("fail")
        self.start("live")
        self.assertEqual(self.session.wait(timeout=20), 1)
        self.assertEqual(len(self.launches()), 3)

    def test_attempt_limit_is_a_setting(self):
        self.plan("fail", max_attempts=2)
        self.start()
        self.assertEqual(self.session.wait(timeout=20), 1)
        self.assertEqual(len(self.launches()), 2)

    def test_late_failure_is_a_logout_not_a_failed_start(self):
        self.plan("fail", start_window=0)
        self.start()
        self.assertEqual(self.session.wait(timeout=20), 1)
        self.assertEqual(len(self.launches()), 1)

    def test_compositor_without_a_socket_is_stopped_and_started_again(self):
        # Gnoblin stays up on its defaults: no control socket, no installer.
        self.plan("nosocket", "run", watch=1)
        self.start()
        self.wait_for_launch(2)
        self.assertTrue(wait_for(lambda: (self.runtime / "gnoblin/compositor-v1.sock").exists()))
        self.assertIsNone(self.session.poll())
        self.assertEqual(len(self.launches()), 2)

    def test_fallback_to_defaults_is_stopped_and_started_again(self):
        self.plan("fallback", "run", watch=30)
        self.start()
        self.wait_for_launch(2)
        self.assertTrue(
            wait_for(
                lambda: (self.runtime / "gnoblin/compositor-v1.sock").exists()
                and not (self.runtime / "gnoblin/config-fallback").exists()
            )
        )
        self.assertEqual(len(self.launches()), 2)

    def test_lost_lua_runtime_is_stopped_and_started_again(self):
        # Seen on 5f53de8.iso under load: the socket exists, the note says the
        # runtime recovery failed, and the installer never starts.
        self.plan("lost", "run", watch=30)
        self.start()
        self.wait_for_launch(2)
        self.assertTrue(
            wait_for(
                lambda: (self.runtime / "gnoblin/compositor-v1.sock").exists()
                and not (self.runtime / "gnoblin/config-fallback").exists()
            )
        )
        self.assertEqual(len(self.launches()), 2)

    def test_other_notices_are_left_alone(self):
        self.runtime.joinpath("gnoblin").mkdir()
        self.runtime.joinpath("gnoblin/config-fallback").write_text("notice\nSome settings were ignored\n")
        self.plan("run", watch=1)
        self.start()
        self.wait_for_launch(1)
        time.sleep(2.5)
        self.assertEqual(len(self.launches()), 1)

    def test_healthy_compositor_is_left_alone(self):
        self.plan("run", watch=1)
        self.start()
        self.wait_for_launch(1)
        time.sleep(2.5)
        self.assertEqual(len(self.launches()), 1)
        self.assertIsNone(self.session.poll())

    def test_last_try_is_not_stopped_by_the_watch(self):
        self.plan("nosocket", max_attempts=1, watch=1)
        self.start()
        self.wait_for_launch(1)
        time.sleep(2.5)
        self.assertEqual(len(self.launches()), 1)
        self.assertIsNone(self.session.poll())

    def test_switch_after_a_retry_still_reaches_the_new_compositor(self):
        self.plan("fail", "run")
        self.start()
        self.wait_for_launch(2)
        first = self.compositor_pid()
        self.assertIsNotNone(first)
        self.assertEqual(self.switch("live").returncode, 0)
        self.wait_for_launch(3)
        self.wait_for_new_pid(first)
        self.assertEqual(self.launches()[2], "")


class JournalTest(SessionCase):
    """The loop and the compositor print to the journal when it is there."""

    def fake_cat(self, broken=False):
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir(exist_ok=True)
        cat = bin_dir / "systemd-cat"
        cat.write_text(FAKE_SYSTEMD_CAT)
        cat.chmod(0o755)
        self.cat_log = self.tmp / "cat.log"
        self.env.update(
            BINGUX_LIVE_JOURNAL="1",
            FAKE_CAT_LOG=str(self.cat_log),
            FAKE_CAT_BROKEN="1" if broken else "0",
            PATH=f"{bin_dir}:{self.env['PATH']}",
        )

    def test_session_runs_under_systemd_cat(self):
        self.fake_cat()
        self.start()
        self.wait_for_launch(1)
        lines = self.cat_log.read_text().splitlines()
        self.assertEqual(lines[0], "systemd-cat -t bingux-live-session true")
        self.assertTrue(lines[1].startswith("systemd-cat -t bingux-live-session /"), lines)
        self.assertEqual(len(lines), 2, "the session must not re-exec itself twice")

    def test_session_runs_without_a_journal(self):
        self.fake_cat(broken=True)
        self.start()
        self.wait_for_launch(1)
        self.assertEqual(self.cat_log.read_text().splitlines(), ["systemd-cat -t bingux-live-session true"])
        self.assertIsNone(self.session.poll())


if __name__ == "__main__":
    unittest.main()
