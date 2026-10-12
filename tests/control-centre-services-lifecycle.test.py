#!/usr/bin/env python3
"""Exercise the control helper lifecycle through its JSON protocol."""

import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import time
import unittest


DEFAULT_SOURCE = Path(__file__).resolve().parents[1] / "shell/bingux/control-centre-services.py"
SOURCE = Path(os.environ.get("BINGUX_CONTROLS_SOURCE", DEFAULT_SOURCE))
WRAPPER = r"""
import importlib.util
import os
from pathlib import Path

from gi.repository import GLib

source = Path(os.environ["BINGUX_CONTROLS_SOURCE"])
spec = importlib.util.spec_from_file_location("control_centre_services", source)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

original_init = module.Controls.__init__
def instrumented_init(self):
    original_init(self)
    def external_dnd_change():
        if self.notifications and self.notifications.is_writable("show-banners"):
            self.notifications.set_boolean("show-banners", False)
            self.notifications.sync()
        return False
    GLib.timeout_add(100, external_dnd_change)
module.Controls.__init__ = instrumented_init

calls = 0
def read(self):
    global calls
    calls += 1
    return {"probe": calls}
module.Controls.read = read
module.main()
"""


class ControlCentreLifecycleTest(unittest.TestCase):
    def setUp(self):
        environment = os.environ | {
            "BINGUX_CONTROLS_SOURCE": str(SOURCE),
            "GSETTINGS_BACKEND": "memory",
        }
        self.process = subprocess.Popen(
            [sys.executable, "-u", "-c", WRAPPER],
            env=environment,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=False,
        )
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.records = []
        self.pending = bytearray()
        self.unread = []

    def tearDown(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                try:
                    self.process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=3)
        self.selector.close()
        self.process.stdout.close()
        self.process.stderr.close()

    def diagnostic(self):
        if self.process.poll() is None:
            return "helper still running"
        return self.process.stderr.read().decode(errors="replace")

    def send(self, record):
        self.process.stdin.write((json.dumps(record) + "\n").encode())
        self.process.stdin.flush()

    def next_record(self, timeout):
        if self.unread:
            return self.unread.pop(0)
        for key, _ in self.selector.select(timeout):
            chunk = os.read(key.fileobj.fileno(), 65536)
            self.assertTrue(chunk, self.diagnostic())
            self.pending.extend(chunk)
            while b"\n" in self.pending:
                line, _, self.pending = self.pending.partition(b"\n")
                self.unread.append(json.loads(line))
            if self.unread:
                record = self.unread.pop(0)
                self.records.append(record)
                return record
        return None

    def receive_until(self, predicate, description, timeout=3):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            record = self.next_record(max(0, deadline - time.monotonic()))
            if record and predicate(record):
                return record
        self.fail(description + ": " + self.diagnostic())

    def assert_no_probe(self, duration=0.25):
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            record = self.next_record(max(0, deadline - time.monotonic()))
            if record:
                self.assertNotIn("probe", record.get("state", {}), record)

    def test_inactive_helper_defers_probes_but_keeps_dnd_live(self):
        ready = self.receive_until(lambda record: record.get("ready"), "helper did not become ready")
        self.assertIn("doNotDisturb", ready["state"])
        self.receive_until(
            lambda record: record.get("state", {}).get("doNotDisturb") is True,
            "external private-memory DND change was not emitted",
        )
        for record in self.records:
            self.assertNotIn("probe", record.get("state", {}), record)
        self.assert_no_probe()

        self.send({"op": "action", "kind": "dnd", "enabled": False})
        self.receive_until(lambda record: record.get("actionDone"), "inactive DND action did not finish")
        self.receive_until(
            lambda record: record.get("state", {}).get("doNotDisturb") is False,
            "inactive DND action did not emit its state",
        )
        self.assert_no_probe()

        self.send({"op": "active", "value": True})
        self.receive_until(lambda record: record.get("state", {}).get("probe") == 1, "activation did not probe")
        self.send({"op": "active", "value": False})

        self.send({"op": "action", "kind": "dnd", "enabled": True})
        self.receive_until(lambda record: record.get("actionDone"), "second inactive DND action did not finish")
        self.assert_no_probe()

        self.send({"op": "active", "value": True})
        self.receive_until(lambda record: record.get("state", {}).get("probe") == 2, "reactivation did not probe")
        self.send({"op": "action", "kind": "dnd", "enabled": False})
        self.receive_until(lambda record: record.get("actionDone"), "active DND action did not finish")
        self.receive_until(
            lambda record: record.get("state", {}).get("probe") == 3,
            "active DND action did not refresh",
        )


if __name__ == "__main__":
    unittest.main()
