"""Run with dbus-run-session: real notification preview/actions, test file only."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import gi
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf

root = Path(__file__).resolve().parents[1] / "shell/bingux"
fixture = root / "CaptureNotificationTest.qml"
with tempfile.TemporaryDirectory(prefix=".capture-notify-proof-", dir=Path.home()) as temporary:
    path = Path(temporary) / "Screenshot preview.png"
    pixbuf = GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB, False, 8, 640, 160)
    pixbuf.fill(0x345ABAFF)
    pixbuf.savev(str(path), "png", [], [])
    qs = os.environ.get("QS_TEST_BIN", "qs")
    process = subprocess.Popen([qs, "-p", str(fixture)])
    notice = None
    command = [qs, "ipc", "-p", str(fixture), "call", "test"]
    def snapshot():
        return json.loads(subprocess.check_output(command + ["snapshot"], text=True, timeout=2))
    try:
        time.sleep(.6)
        notice = subprocess.Popen(["python3", str(root / "capture-notify.py"), str(path)],
                                  env=dict(os.environ, GIO_USE_VFS="local"))
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline:
            data = snapshot()
            if data: break
            time.sleep(.08)
        assert data and data[0]["image"], data
        assert data[0]["actions"] == ["default", "copy", "save", "discard"], data
        print("PASS: real Notify delivery, image preview and action labels", flush=True)
        time.sleep(.4)
        preview = json.loads(subprocess.check_output(command + ["preview"], text=True, timeout=2))
        assert preview and preview["ready"] and abs(preview["height"] - preview["paintedHeight"]) < 1, preview
        assert preview["height"] < 100, preview
        print("PASS: wide preview has no letterbox spacing", flush=True)
        buttons = json.loads(subprocess.check_output(command + ["buttonGeometry"], text=True, timeout=2))
        assert len(buttons) == 3, buttons
        for button in buttons:
            assert abs(button["left"] - button["right"]) < 1, button
            assert abs(button["iconCenter"] - button["center"]) < 1, button
            assert button["gap"] == 8 and button["left"] >= 8, button
        print("PASS: action icons centered with equal padding and 8px label gaps", flush=True)
        subprocess.run(["grim", "-g", "3020,40 420x330", "/tmp/bingux-capture-notification.png"], check=True)
        subprocess.run(command + ["invoke", "discard"], check=True, timeout=2)
        deadline = time.monotonic() + 8
        while notice.poll() is None and time.monotonic() < deadline:
            time.sleep(.1)
        assert not path.exists(), snapshot()
        assert notice.poll() == 0, snapshot()
        print("PASS: Discard moved the generated test screenshot to Trash")
        subprocess.run(command + ["dismiss"], check=True, timeout=2)
        recording = Path(temporary) / "Recording notification.mp4"
        recording.write_bytes(b"notification-action-test")
        notice = subprocess.Popen(["python3", str(root / "capture-notify.py"), str(recording), "recording"],
                                  env=dict(os.environ, GIO_USE_VFS="local"))
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline:
            data = snapshot()
            if data and data[0]["summary"] == "Recording saved": break
            time.sleep(.08)
        assert data[0]["summary"] == "Recording saved", data
        assert not data[0]["image"], data
        assert data[0]["actions"] == ["default", "save", "discard"], data
        subprocess.run(command + ["invoke", "discard"], check=True, timeout=2)
        notice.wait(timeout=5)
        assert notice.returncode == 0 and not recording.exists()
        print("PASS: Recording saved uses the normal notification surface and working actions")
    finally:
        if notice and notice.poll() is None:
            notice.terminate()
            notice.wait(timeout=3)
        process.terminate()
        process.wait(timeout=3)
