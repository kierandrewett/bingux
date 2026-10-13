#!/usr/bin/env python3
"""Exercise the real privacy helper with a controlled pw-dump process."""

import json
import os
from pathlib import Path
import selectors
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "shell/bingux/camera-status.py"
CAMERA = {
    "id": 12,
    "type": "PipeWire:Interface:Node",
    "info": {"state": "running", "props": {"media.role": "Camera", "application.name": "Browser"}},
}
SHARING = {
    "id": 13,
    "type": "PipeWire:Interface:Client",
    "info": {
        "props": {
            "pipewire.access": "portal",
            "pipewire.access.portal.media_roles": "",
            "pipewire.access.portal.app_id": "org.example.Sharing",
            "application.name": "Share",
        }
    },
}
EMPTY = {"available": True, "captures": [], "screenSharingCaptures": []}
UNAVAILABLE = dict(EMPTY, available=False)
FAKE = r"""#!/usr/bin/env python3
import json, os, socket, sys, time
from pathlib import Path
root = Path(os.environ["CAMERA_FIXTURE"])
with (root/"children").open("a") as log:
    log.write(json.dumps({"pid": os.getpid(), "args":sys.argv[1:]})+"\n")
baseline = (root/"baseline").read_bytes()
sys.stdout.buffer.write(baseline)
sys.stdout.buffer.flush()
if "--monitor" not in sys.argv and "-m" not in sys.argv:
    raise SystemExit(0)
path = root/"control.sock"
path.unlink(missing_ok=True)
server = socket.socket(socket.AF_UNIX)
server.bind(str(path))
server.listen(1)
while True:
    connection, _ = server.accept()
    with connection:
        request = json.loads(connection.makefile("rb").readline())
        if "exit" in request:
            raise SystemExit(request["exit"])
        if request.get("ignore_term"):
            import signal
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
        data = request.get("write", "").encode()
        width = request.get("fragment", len(data) or 1)
        for start in range(0, len(data), width):
            sys.stdout.buffer.write(data[start:start+width])
            sys.stdout.buffer.flush()
            if request.get("pause"):
                time.sleep(request["pause"])
        connection.sendall(b"done\n")
"""


class CameraMonitorTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bingux-camera-test-")
        self.root = Path(self.temporary.name)
        binary = self.root / "pw-dump"
        binary.write_text(FAKE)
        binary.chmod(0o700)
        (self.root / "baseline").write_text(json.dumps([]) + "\n")
        self.process = None
        self.selector = selectors.DefaultSelector()
        self.pending = bytearray()
        self.records = []

    def start(self, baseline=None):
        if baseline is not None:
            (self.root / "baseline").write_text(json.dumps(baseline) + "\n")
        env = os.environ | {"CAMERA_FIXTURE": str(self.root), "PATH": str(self.root) + os.pathsep + os.environ["PATH"]}
        self.process = subprocess.Popen(
            [sys.executable, str(SOURCE)], env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
        )
        self.selector.register(self.process.stdout, selectors.EVENT_READ)

    def next_record(self, timeout=3):
        deadline = time.monotonic() + timeout
        while not self.records and time.monotonic() < deadline:
            for key, _ in self.selector.select(max(0, deadline - time.monotonic())):
                chunk = os.read(key.fileobj.fileno(), 65536)
                self.assertTrue(chunk, "helper exited before publishing state")
                self.pending.extend(chunk)
                while b"\n" in self.pending:
                    line, _, tail = self.pending.partition(b"\n")
                    self.pending = bytearray(tail)
                    self.records.append(json.loads(line))
        self.assertTrue(self.records, "helper state timed out")
        return self.records.pop(0)

    def send(self, **request):
        deadline = time.monotonic() + 3
        while True:
            try:
                with socket.socket(socket.AF_UNIX) as connection:
                    connection.settimeout(3)
                    connection.connect(str(self.root / "control.sock"))
                    connection.sendall(json.dumps(request).encode() + b"\n")
                    if "exit" not in request:
                        reply = connection.recv(16)
                        if request.get("may_exit"):
                            self.assertIn(reply, (b"done\n", b""))
                        else:
                            self.assertEqual(reply, b"done\n")
                return
            except (FileNotFoundError, ConnectionRefusedError):
                self.assertLess(time.monotonic(), deadline, "monitor control unavailable")
                time.sleep(0.01)

    def frame(self, objects, **options):
        self.send(write=json.dumps(objects) + "\n", **options)

    def children(self):
        path = self.root / "children"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def tearDown(self):
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=2)
            self.process.stdout.close()
        self.selector.close()
        for child in self.children():
            try:
                os.kill(child["pid"], signal.SIGKILL)
            except ProcessLookupError:
                pass
        self.temporary.cleanup()

    def test_idle_uses_one_persistent_monitor(self):
        self.start()
        self.assertEqual(self.next_record(), EMPTY)
        time.sleep(1.25)
        self.assertEqual(len(self.children()), 1, "idle helper relaunched pw-dump")
        self.assertIn("--monitor", self.children()[0]["args"])

    def test_baseline_updates_removals_and_fragmented_frames(self):
        self.start([CAMERA, SHARING])
        initial = self.next_record()
        self.assertEqual(initial["captures"], [{"app": "Browser", "appId": "", "device": "Camera"}])
        self.assertEqual(
            initial["screenSharingCaptures"],
            [{"app": "Share", "appId": "org.example.Sharing", "device": "Screen sharing"}],
        )
        # Full replacement: state stops, and removed properties must not survive.
        stopped = dict(CAMERA, info={"state": "suspended", "props": CAMERA["info"]["props"]})
        self.frame([stopped], fragment=3)
        self.assertEqual(self.next_record()["captures"], [])
        camera_client = dict(
            SHARING, info={"props": {"pipewire.access": "portal", "pipewire.access.portal.media_roles": "Camera"}}
        )
        self.frame([camera_client])
        self.assertEqual(self.next_record(), EMPTY)
        self.frame([CAMERA, SHARING], fragment=1)
        self.assertEqual(self.next_record(), initial)
        self.frame([{"id": CAMERA["id"], "info": None}])
        removed = self.next_record()
        self.assertEqual(removed["captures"], [])
        self.assertEqual(removed["screenSharingCaptures"], initial["screenSharingCaptures"])
        self.frame([{"id": SHARING["id"], "info": None}])
        self.assertEqual(self.next_record(), EMPTY)
        self.frame([{"id": 90, "props": {"unrelated": True}}])
        self.frame([{"id": 90, "props": None}])
        self.frame([{"id": 999, "info": None}])
        self.frame([])
        self.assertFalse(self.records)
        self.assertFalse(self.selector.select(0.1), "unchanged graph published redundant output")

    def test_native_scalar_properties_do_not_disable_monitoring(self):
        # pw-dump's put_value converts numeric/boolean-looking property strings.
        camera = dict(
            CAMERA,
            info={
                "state": "running",
                "props": {
                    "media.role": "Camera",
                    "application.name": 123,
                    "application.id": None,
                    "node.description": 1.5,
                },
            },
        )
        client = dict(SHARING, info={"props": dict(SHARING["info"]["props"], **{"application.name": True})})
        self.start([camera, client])
        state = self.next_record()
        self.assertTrue(state["available"], "valid pw-dump scalars disabled privacy monitoring")
        self.assertEqual(state["captures"], [{"app": 123, "appId": "", "device": 1.5}])
        self.assertEqual(state["screenSharingCaptures"][0]["app"], True)
        self.frame([{"id": camera["id"], "info": None}, {"id": client["id"], "info": None}])
        self.assertEqual(self.next_record(), EMPTY)

    def test_unchanged_owner_update_does_not_reorder_output(self):
        second = dict(
            CAMERA,
            id=14,
            info={
                "state": "running",
                "props": {
                    "media.role": "Camera",
                    "application.name": "Second browser",
                },
            },
        )
        self.start([CAMERA, second])
        self.assertEqual(len(self.next_record()["captures"]), 2)
        self.frame([CAMERA])
        self.assertFalse(self.selector.select(0.2), "unchanged owners were reordered and republished")

    def test_quoted_brackets_unicode_and_coalesced_arrays(self):
        self.start()
        self.assertEqual(self.next_record(), EMPTY)
        camera = dict(
            CAMERA,
            info={"state": "running", "props": {"media.role": "Camera", "application.name": 'A [camera] "name" —'}},
        )
        # UTF-8 byte fragments and adjacent arrays, not a newline protocol.
        self.send(write=json.dumps([camera], ensure_ascii=False) + json.dumps([SHARING]), fragment=1)
        self.assertEqual(self.next_record()["captures"][0]["app"], 'A [camera] "name" —')
        self.assertTrue(self.next_record()["screenSharingCaptures"])

    def test_child_exit_clears_state_and_reconnects_with_new_baseline(self):
        self.start([CAMERA])
        self.assertTrue(self.next_record()["captures"])
        (self.root / "baseline").write_text("[]\n")
        self.send(exit=7)
        self.assertEqual(self.next_record(), UNAVAILABLE)
        self.assertEqual(self.next_record(timeout=4), EMPTY)
        self.assertEqual(len(self.children()), 2)

    def test_malformed_frame_is_unavailable_and_recovers(self):
        self.start([CAMERA])
        self.next_record()
        (self.root / "baseline").write_text("[]\n")
        self.send(write="[{broken}]\n")
        self.assertEqual(self.next_record(), UNAVAILABLE)
        self.assertEqual(self.next_record(timeout=4), EMPTY)

    def test_frame_bound(self):
        self.start()
        self.next_record()
        self.send(write='["' + "x" * (8 * 1024 * 1024), may_exit=True)
        self.assertEqual(self.next_record(), UNAVAILABLE)
        self.assertEqual(self.next_record(timeout=4), EMPTY)

    def test_no_baseline_is_not_claimed_available(self):
        (self.root / "baseline").write_text("")
        self.start()
        self.assertEqual(self.next_record(timeout=5), UNAVAILABLE)

    def test_partial_frame_deadline_and_deep_json_fail_unavailable(self):
        self.start()
        self.next_record()
        self.send(write='[{"id":')
        self.assertEqual(self.next_record(timeout=5), UNAVAILABLE)
        self.assertEqual(self.next_record(timeout=4), EMPTY)
        self.send(write="[" * 1500 + "]" * 1500)
        self.assertEqual(self.next_record(), UNAVAILABLE)

    def test_retained_graph_bytes_are_bounded_across_frames(self):
        self.start()
        self.next_record()
        camera = dict(
            CAMERA,
            info={"state": "running", "props": {"media.role": "Camera", "application.name": "x" * (1024 * 1024)}},
        )
        for identity in range(3):
            self.frame([dict(camera, id=identity)])
            if identity == 0:
                self.next_record()
        self.frame([dict(camera, id=3)])
        self.assertEqual(self.next_record(), UNAVAILABLE)

    def test_graph_object_bound(self):
        self.start()
        self.next_record()
        # Relevant objects are bounded even when every frame is individually small.
        self.frame([dict(CAMERA, id=index) for index in range(4097)])
        self.assertEqual(self.next_record(), UNAVAILABLE)

    def test_sigterm_reaps_even_an_uncooperative_monitor(self):
        self.start()
        self.next_record()
        self.send(ignore_term=True)
        pid = self.children()[0]["pid"]
        self.process.terminate()
        self.process.wait(timeout=3)
        self.assertEqual(self.process.returncode, 0)
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)

    def test_closed_output_stops_and_reaps_monitor(self):
        self.start()
        self.next_record()
        pid = self.children()[0]["pid"]
        self.process.stdout.close()
        self.frame([CAMERA])
        self.process.wait(timeout=3)
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)


if __name__ == "__main__":
    unittest.main()
