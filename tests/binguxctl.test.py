"""Public command routing and safe IPC failure behaviour."""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock

spec = importlib.util.spec_from_file_location("binguxctl", Path(__file__).resolve().parents[1] / "packages/binguxctl/binguxctl.py")
ctl = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ctl)


class ControlCommands(unittest.TestCase):
    def route(self, words):
        cli = ctl.parser()
        return ctl.invocation(cli.parse_args(words), cli)

    def test_capture_show_is_idempotent_and_toggle_is_explicit(self):
        self.assertEqual(self.route(["capture"]), ["call", "--", "capture", "show", "", ""])
        self.assertEqual(self.route(["capture", "open", "--mode", "recording", "--target", "screen"]),
            ["call", "--", "capture", "show", "recording", "screen"])
        self.assertEqual(self.route(["capture", "toggle"]), ["call", "--", "capture", "open"])

    def test_text_stays_one_argument_without_shell_interpretation(self):
        text = 'notes; $(touch /tmp/never) "quoted"'
        self.assertEqual(self.route(["search", "query", text]), ["call", "--", "search", "query", text])
        self.assertEqual(self.route(["ipc", "example", "method", text]), ["call", "--", "example", "method", text])

    def test_invalid_options_fail_before_ipc(self):
        for words in (["capture", "take", "--mode", "recording"], ["sidebar", "edge", "bottom"],
                      ["sidebar", "open", "notes"], ["search", "query"], ["search", "open", "text"]):
            with self.subTest(words=words), contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                self.route(words)

    def test_explicit_not_ready_can_retry(self):
        run = Mock(side_effect=[subprocess.CompletedProcess([], 0, "Not ready to accept queries yet.\n", ""),
            subprocess.CompletedProcess([], 0, '{"ok":true}\n', "")])
        with contextlib.redirect_stdout(io.StringIO()): self.assertEqual(ctl.execute(["qs"], run, Mock()), 0)
        self.assertEqual(run.call_count, 2)

    def test_unknown_outcome_is_never_repeated(self):
        for failure in (subprocess.TimeoutExpired("qs", 4), OSError("disconnected")):
            run = Mock(side_effect=failure)
            with self.assertRaises(type(failure)): ctl.execute(["qs"], run, Mock())
            self.assertEqual(run.call_count, 1)

    def test_missing_method_and_rejected_action_are_errors(self):
        for output in ('No running instances for /missing/shell.qml', 'Function not found.', '{"ok":false,"error":"Capture already running"}'):
            run = Mock(return_value=subprocess.CompletedProcess([], 0, output, ""))
            with self.assertRaises(RuntimeError): ctl.execute(["qs"], run, Mock())
            self.assertEqual(run.call_count, 1)


if __name__ == "__main__": unittest.main()
