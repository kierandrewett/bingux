"""CPU-only capture start/cleanup proof; no desktop, portal, clipboard or encoder.

Run: PYTHONDONTWRITEBYTECODE=1 python3 tests/capture-pipeline-lifecycle.test.py
The media pipeline is replaced by a bounded four-byte GStreamer source/filesink.
Real backend bus registration, message dispatch and cleanup remain under test.
"""

import gc
import importlib.util
import os
from pathlib import Path
import sys
import time
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
shell = Path(__file__).resolve().parents[1] / "shell/bingux"
source = Path(os.environ.get("BINGUX_CAPTURE_BACKEND_TEST_SOURCE", str(shell / "capture_backend.py")))
sys.path.insert(0, str(source.parent))
spec = importlib.util.spec_from_file_location("capture_backend", source)
backend = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backend)
Gst, GLib = backend.Gst, backend.GLib
Gst.init(None)
parse_launch = Gst.parse_launch


def dispatch_until(predicate, timeout=1):
    deadline = time.monotonic() + timeout
    context = GLib.MainContext.default()
    while not predicate() and time.monotonic() < deadline:
        if context.pending():
            context.iteration(False)
        else:
            time.sleep(0.001)
    return predicate()


class Worker(backend.Capture):
    def __init__(self):
        # Avoid Capture.__init__'s desktop discovery. Supply its lifecycle fields.
        self.pipeline = None
        self.pipeline_bus = None
        self.pipeline_bus_handler = 0
        self.timer = self.stop_timer = 0
        self.session = ""
        self.subscriptions = []
        self.remote_fd = -1
        self.temporary = Path("/dev/null")
        self.job = None
        self.started = False
        self.events = []
        self.messages = 0

    def message(self, bus, message):
        self.messages += 1
        super().message(bus, message)

    def complete(self):
        self.cleanup()
        self.events.append("eos")

    def fail(self, message):
        self.cleanup()
        self.events.append("error")

    def begin(self):
        self.started = False
        self.job = {"kind": "screenshot", "target": "screen", "format": "png", "encoder": "auto"}
        self.start_pipeline(0)


class PipelineLifecycle(unittest.TestCase):
    def setUp(self):
        self.worker = Worker()
        self.destroyed_buses = []
        self.destroyed_pipelines = []

    def producer(self, description, error=False):
        pipeline = parse_launch(
            "fakesrc num-buffers=1 sizetype=fixed sizemax=4 ! "
            + ("identity error-after=1 ! " if error else "")
            + "filesink name=output"
        )
        pipeline.weak_ref(lambda: self.destroyed_pipelines.append(True))
        pipeline.get_bus().weak_ref(lambda: self.destroyed_buses.append(True))
        return pipeline

    def assert_released(self, count):
        gc.collect()
        self.assertTrue(dispatch_until(lambda: len(self.destroyed_pipelines) == count), "pipeline retained")
        self.assertTrue(
            dispatch_until(lambda: len(self.destroyed_buses) == count),
            f"bus/source retained: {len(self.destroyed_buses)} of {count} destroyed",
        )

    def test_six_completed_jobs_release_each_bus_and_pipeline(self):
        with patch.object(Gst, "parse_launch", self.producer):
            for cycle in range(6):
                self.worker.begin()
                self.assertTrue(dispatch_until(lambda: len(self.worker.events) == cycle + 1))
                self.assertEqual(self.worker.events[-1], "eos")
            self.assert_released(6)

    def test_error_dispatch_releases_bus_and_allows_next_job(self):
        with patch.object(Gst, "parse_launch", lambda description: self.producer(description, error=True)):
            self.worker.begin()
            self.assertTrue(dispatch_until(lambda: bool(self.worker.events)))
            self.assertEqual(self.worker.events, ["error"])
            self.assert_released(1)
        with patch.object(Gst, "parse_launch", self.producer):
            self.worker.begin()
            self.assertTrue(dispatch_until(lambda: len(self.worker.events) == 2))
            self.assertEqual(self.worker.events[-1], "eos")
            self.assert_released(2)

    def test_cancel_disconnects_old_bus_and_cleanup_is_idempotent(self):
        with patch.object(Gst, "parse_launch", self.producer):
            self.worker.begin()
            bus = self.worker.pipeline.get_bus()
            self.worker.cleanup()
            self.worker.cleanup()
            before = self.worker.messages
            bus.set_flushing(False)
            bus.post(Gst.Message.new_application(None, Gst.Structure.new_empty("after-cleanup")))
            dispatch_until(lambda: self.worker.messages > before, timeout=0.02)
            self.assertEqual(self.worker.messages, before, "old bus still calls backend after cleanup")
            bus = None
            self.assert_released(1)

    def test_playing_failure_releases_installed_watch(self):
        set_state = Gst.Pipeline.set_state

        def fail_playing(pipeline, state):
            if state == Gst.State.PLAYING:
                return Gst.StateChangeReturn.FAILURE
            return set_state(pipeline, state)

        with patch.object(Gst, "parse_launch", self.producer), patch.object(Gst.Pipeline, "set_state", fail_playing):
            self.worker.begin()
            self.assertEqual(self.worker.events, ["error"])
        self.assert_released(1)

    def test_parse_failure_and_unused_cleanup_need_no_watch(self):
        self.worker.cleanup()
        with patch.object(Gst, "parse_launch", side_effect=GLib.Error("fixture parse failure")):
            self.worker.begin()
        self.worker.cleanup()
        self.assertEqual(self.worker.events, ["error"])
        self.assertIsNone(self.worker.pipeline)


if __name__ == "__main__":
    unittest.main()
