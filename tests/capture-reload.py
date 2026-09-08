"""Isolated config copy: concurrent shell edits must not invalidate the reload test."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

source = Path(__file__).resolve().parents[1] / "shell/bingux"
with tempfile.TemporaryDirectory(prefix="capture-reload-") as temporary:
    root = Path(temporary)
    shutil.copytree(source, root / "config", ignore=shutil.ignore_patterns("__pycache__"))
    fixture = root / "config/CaptureReloadTest.qml"
    command = ["qs", "ipc", "--any-display", "-p", str(fixture), "call"]
    env = dict(os.environ, BINGUX_CAPTURE_SETTINGS_PATH=(root / "capture.ini").as_uri())
    process = subprocess.Popen(["qs", "-p", str(fixture)], env=env)
    def ipc(target, method):
        return subprocess.check_output(command + [target, method], text=True, timeout=2)
    def await_status(predicate):
        deadline = time.monotonic() + 6
        last = None
        while time.monotonic() < deadline:
            try:
                last = json.loads(ipc("capture", "status"))
                if predicate(last): return last
            except (ValueError, subprocess.SubprocessError): pass
            time.sleep(.08)
        raise AssertionError(f"Capture did not reach expected state: {last}")
    try:
        await_status(lambda s: s["ready"])
        ipc("capture", "open")
        await_status(lambda s: s["opened"])
        ipc("test", "setRegion")
        region = ipc("test", "region")
        for _ in range(3):
            ipc("test", "reload")
            time.sleep(.35)
            await_status(lambda s: s["opened"])
            assert ipc("test", "region") == region
        ipc("capture", "cancel")
        ipc("test", "reload")
        time.sleep(.35)
        await_status(lambda s: s["ready"] and not s["opened"] and not s["requested"])
        print("PASS: three reloads retain selector and region; cancelled selector stays closed")
    finally:
        process.terminate()
        process.wait(timeout=5)
