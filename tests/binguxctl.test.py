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

    def test_extended_commands_target_the_shared_actions(self):
        self.assertEqual(self.route(["audio", "mute", "--input"]), ["call", "--", "actions", "audio", "mute", "true", "0"])
        self.assertEqual(self.route(["media", "seek", "30", "--player", "org.mpris.MediaPlayer2.test"]),
            ["call", "--", "actions", "media", "seek", "org.mpris.MediaPlayer2.test", "30.0"])
        self.assertEqual(self.route(["notifications", "invoke", "session:3", "open"]),
            ["call", "--", "actions", "notification", "invoke", "session:3", "open"])
        self.assertEqual(self.route(["windows", "close", "instance:2"]), ["call", "--", "actions", "window", "close", "instance:2"])
        self.assertEqual(self.route(["dock", "move", "app", "2"]), ["call", "--", "actions", "dock", "move", "app", "2"])

    def test_desktop_controls(self):
        cases = [
            (["controls", "page", "audio", "--input"], ["page", "audio", "true"]),
            (["audio", "devices"], ["device", "list", "false", ""]),
            (["audio", "select", "alsa.device", "--input"], ["device", "select", "true", "alsa.device"]),
            (["audio", "volume", "30"], ["audio", "volume", "false", "30.0"]),
            (["bluetooth", "scan", "on"], ["bluetooth", "scan", "on"]),
            (["bluetooth", "connect", "AA:BB:CC:DD:EE:FF"], ["bluetooth", "connect", "AA:BB:CC:DD:EE:FF"]),
            (["network", "connect", "00000000-1111-2222-3333-444444444444"], ["network", "connect", "00000000-1111-2222-3333-444444444444"]),
            (["power", "set", "balanced"], ["service", "power", "set", "balanced"]),
            (["night-light", "off"], ["service", "night-light", "off", ""]),
            (["awake", "toggle"], ["service", "awake", "toggle", ""]),
        ]
        for words, call in cases:
            with self.subTest(words=words): self.assertEqual(self.route(words), ["call", "--", "actions", *call])

    def test_network_refresh_waits_for_data_and_reports_failure(self):
        query = Mock(side_effect=[{"pending": True}, {"busy": True}, {"busy": False, "ready": True, "connections": []}])
        state = ctl.refresh_network(["qs", "ipc"], query=query, sleep=Mock())
        self.assertEqual(state["connections"], [])
        self.assertEqual(query.call_count, 3)
        query = Mock(side_effect=[{}, {"busy": False, "ready": True, "error": "nmcli failed"}])
        with self.assertRaisesRegex(RuntimeError, "nmcli failed"): ctl.refresh_network([], query=query)
        query = Mock(return_value={"busy": True})
        with self.assertRaisesRegex(RuntimeError, "timed out"):
            ctl.refresh_network([], query=query, monotonic=Mock(side_effect=[0, 16]))

    def test_capture_options_preserve_false_and_zero(self):
        import json
        call = self.route(["capture", "configure", "--no-copy", "--delay", "0", "--region", "0", "0", "20", "30"])
        self.assertEqual(call[:4], ["call", "--", "capture", "configure"])
        self.assertEqual(json.loads(call[4]), {"copy": False, "delay": 0, "region": {"x": 0, "y": 0, "width": 20, "height": 30}})

    def test_invalid_options_fail_before_ipc(self):
        for words in (["capture", "take", "--mode", "recording"], ["sidebar", "edge", "bottom"],
                      ["sidebar", "open", "notes"], ["search", "query"], ["search", "open", "text"], ["audio", "volume", "nan"], ["media", "seek", "inf"],
                      ["notifications", "clear", "1"], ["notifications", "invoke", "1"], ["windows", "close"],
                      ["dock", "move", "app", "-1"], ["capture", "configure"],
                      ["controls", "page"], ["controls", "open", "audio"], ["controls", "page", "network", "--input"],
                      ["audio", "select"], ["audio", "devices", "bad"], ["audio", "volume", "bad"],
                      ["bluetooth", "scan", "toggle"], ["bluetooth", "connect", "other"], ["network", "connect", "bad"],
                      ["power", "set"], ["night-light", "on", "bad"]):
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
        for output in ('No running instances for /missing/shell.qml', 'Function not found.', ' ERROR quickshell.ipc: Error occurred while waiting for response.', '{"ok":false,"error":"Capture already running"}'):
            run = Mock(return_value=subprocess.CompletedProcess([], 0, output, ""))
            with self.assertRaises(RuntimeError): ctl.execute(["qs"], run, Mock())
            self.assertEqual(run.call_count, 1)


if __name__ == "__main__": unittest.main()
