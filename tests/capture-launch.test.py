import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock

spec = importlib.util.spec_from_file_location(
    "launch", Path(__file__).resolve().parents[1] / "shell/bingux/capture-launch.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class CaptureLaunch(unittest.TestCase):
    def test_retry_only_explicitly_undispatched_calls(self):
        run = Mock(
            side_effect=[
                subprocess.CompletedProcess([], 0, "Not ready to accept queries yet.\n", ""),
                subprocess.CompletedProcess([], 0, "", ""),
            ]
        )
        sleep = Mock()
        self.assertEqual(module.launch(["qs"], run, sleep), 0)
        self.assertEqual(run.call_count, 2)
        sleep.assert_called_once_with(0.1)

    def test_success_and_unknown_failure_never_repeat(self):
        for code, output in ((0, ""), (1, "Unexpected failure")):
            run = Mock(return_value=subprocess.CompletedProcess([], code, output, ""))
            self.assertEqual(module.launch(["qs"], run, Mock()), code)
            self.assertEqual(run.call_count, 1)

    def test_timeout_is_not_retried(self):
        run = Mock(side_effect=subprocess.TimeoutExpired("qs", 3))
        with self.assertRaises(subprocess.TimeoutExpired):
            module.launch(["qs"], run, Mock())
        self.assertEqual(run.call_count, 1)

    def test_retries_are_bounded(self):
        run = Mock(return_value=subprocess.CompletedProcess([], 0, "Function not found.", ""))
        self.assertEqual(module.launch(["qs"], run, Mock()), 1)
        self.assertEqual(run.call_count, 12)


if __name__ == "__main__":
    unittest.main()
