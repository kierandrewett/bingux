"""Real screen tests in a temporary directory. --audio also tests default audio devices."""

import argparse
import json
from pathlib import Path
import selectors
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--audio", action="store_true", help="Record system audio, microphone, and both for three seconds each")
args = parser.parse_args()
folder = tempfile.mkdtemp(prefix="bingux-capture-proof-")
backend = Path(__file__).resolve().parents[1] / "shell/bingux/capture_backend.py"
worker = subprocess.Popen(["python3", "-u", str(backend)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
selector = selectors.DefaultSelector()
selector.register(worker.stdout, selectors.EVENT_READ)
jobs = iter(
    [
        dict(kind="recording", encoder="cpu"),
        dict(kind="recording", encoder="auto"),
        dict(kind="recording", clean_stop=True),
        *([dict(kind="recording", audio=mode) for mode in ("system", "microphone", "both")] if args.audio else []),
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
            send(dict(command="stop", hoveredAt=hovered_at))
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
            hovered_at = (event["started"] + 1.5) * 1000 if job.get("clean_stop") else 0
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
                        "stream=codec_name,codec_type,width,height,duration:format=duration,size",
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
                lower, upper = (1.4, 1.7) if job.get("clean_stop") else (2.8, 5)
                assert lower <= float(info["format"]["duration"]) <= upper, info
                audio = [stream for stream in info["streams"] if stream["codec_type"] == "audio"]
                assert len(audio) == (0 if job.get("audio", "none") == "none" else 1), info
                if audio:
                    assert audio[0]["codec_name"] == "aac", info
                    assert abs(float(audio[0]["duration"]) - float(stream["duration"])) < 0.5, info
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
                    **{key: value for key, value in job.items() if key != "clean_stop"},
                )
            )
    else:
        raise TimeoutError("Capture did not finish")
finally:
    worker.terminate()
    worker.wait(timeout=5)
