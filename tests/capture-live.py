"""Opt-in real screen tests. Writes only to a fresh temporary directory; no audio."""

import json
from pathlib import Path
import selectors
import subprocess
import tempfile
import time

folder = tempfile.mkdtemp(prefix="bingux-capture-proof-")
backend = Path(__file__).resolve().parents[1] / "shell/bingux/capture_backend.py"
worker = subprocess.Popen(["python3", "-u", str(backend)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
selector = selectors.DefaultSelector()
selector.register(worker.stdout, selectors.EVENT_READ)
jobs = iter(
    [
        dict(kind="recording", encoder="cpu"),
        dict(kind="recording", encoder="auto"),
        dict(kind="screenshot", format="png"),
        dict(kind="screenshot", format="jpeg", cursor=True),
    ]
)
stop_at = None
job = None


def send(record):
    worker.stdin.write(json.dumps(record) + "\n")
    worker.stdin.flush()


try:
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        if stop_at and time.monotonic() >= stop_at:
            send(dict(command="stop"))
            stop_at = None
        if not selector.select(0.1):
            continue
        event = json.loads(worker.stdout.readline())
        print(event, flush=True)
        if event["event"] == "recording":
            assert job is not None, "Received recording event before a capture job was submitted"
            if job.get("encoder") == "cpu":
                assert event["encoder"] in ("x264enc", "openh264enc")
            stop_at = time.monotonic() + 3
        elif event["event"] == "error":
            raise RuntimeError(event)
        elif event["event"] == "saved":
            info = json.loads(
                subprocess.check_output(
                    [
                        "ffprobe",
                        "-v",
                        "error",
                        "-show_entries",
                        "stream=codec_name,width,height:format=duration,size",
                        "-of",
                        "json",
                        event["path"],
                    ]
                )
            )
            print(info, flush=True)
            stream = info["streams"][0]
            assert (stream["width"], stream["height"]) == (640, 360)
            if event["kind"] == "recording":
                assert stream["codec_name"] == "h264"
                assert 2.8 <= float(info["format"]["duration"]) <= 5, info
            subprocess.run(["ffmpeg", "-v", "error", "-i", event["path"], "-f", "null", "-"], check=True)
        if event["event"] in ("ready", "saved"):
            job = next(jobs, None)
            if job is None:
                print("ALL CAPTURES AND FULL DECODES PASSED:", folder)
                break
            send(
                dict(
                    command="capture",
                    target="region",
                    region=dict(x=200, y=200, width=640, height=360),
                    directory=folder,
                    copy=False,
                    **job,
                )
            )
    else:
        raise TimeoutError("Capture did not finish")
finally:
    worker.terminate()
    worker.wait(timeout=5)
