#!/usr/bin/env python3
"""Exercise the one-byte native admission barrier for bingux-lock."""

import os
from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
GATE = ROOT / "shell/bingux/lock-start-gate.py"


class LockStartGateTest(unittest.TestCase):
    def run_gate(self, environment, pass_fds=()):
        env = dict(os.environ)
        env.pop("GNOBLIN_LOCK_START_FD", None)
        env.update(environment)
        return subprocess.run(
            [sys.executable, str(GATE), "/usr/bin/printf", "released"],
            env=env,
            pass_fds=pass_fds,
            capture_output=True,
            text=True,
        )

    def test_runs_without_a_native_barrier(self):
        result = self.run_gate({"GNOBLIN_LOCK_START_FD": ""})
        self.assertNotEqual(result.returncode, 0)
        result = self.run_gate({})
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "released")

    def test_releases_after_exactly_one_byte(self):
        read_fd, write_fd = os.pipe()
        try:
            os.write(write_fd, b"1")
            result = self.run_gate({"GNOBLIN_LOCK_START_FD": str(read_fd)}, (read_fd,))
        finally:
            os.close(read_fd)
            os.close(write_fd)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "released")

    def test_refuses_eof_and_invalid_descriptors(self):
        read_fd, write_fd = os.pipe()
        os.close(write_fd)
        try:
            eof = self.run_gate({"GNOBLIN_LOCK_START_FD": str(read_fd)}, (read_fd,))
        finally:
            os.close(read_fd)
        self.assertNotEqual(eof.returncode, 0)
        self.assertIn("closed the startup gate", eof.stderr)
        invalid = self.run_gate({"GNOBLIN_LOCK_START_FD": "not-a-fd"})
        self.assertNotEqual(invalid.returncode, 0)


if __name__ == "__main__":
    unittest.main()
