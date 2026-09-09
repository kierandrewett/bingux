#!/usr/bin/env python3
"""Read the final report even when a test process exits between polls."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from private_shell import run_reported_shell


class FinalReportTests(unittest.TestCase):
    def test_completed_report_from_an_exited_process(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory)
            runner = fixture / "runner"
            runner.write_text('#!/bin/sh\nprintf PASS > "$BINGUX_TEST_REPORT"\n')
            runner.chmod(0o700)
            with patch.dict("os.environ", {"QS_TEST_BIN": str(runner)}):
                report, _ = run_reported_shell(fixture, {}, "BINGUX_TEST_REPORT", timeout=2)
            self.assertEqual(report, "PASS")

    def test_exit_without_a_report_remains_a_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory)
            runner = fixture / "runner"
            runner.write_text("#!/bin/sh\nexit 7\n")
            runner.chmod(0o700)
            with patch.dict("os.environ", {"QS_TEST_BIN": str(runner)}):
                report, _ = run_reported_shell(fixture, {}, "BINGUX_TEST_REPORT", timeout=2)
            self.assertIn("Shell exited before completion (exit status 7)", report)


if __name__ == "__main__":
    unittest.main()
