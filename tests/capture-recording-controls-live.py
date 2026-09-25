"""Real region guide and top-bar hover/leave/click test. Records no audio."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from PIL import Image

repo = Path(__file__).resolve().parents[1]
folder = Path(tempfile.mkdtemp(prefix="bingux-recording-controls-"))
shell = folder / "shell"
shutil.copytree(repo / "shell/bingux", shell, ignore=shutil.ignore_patterns("__pycache__"))
report = folder / "report.json"
result = subprocess.run(
    [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(shell / "CaptureRecordingTest.qml")],
    env=os.environ | {
        "BINGUX_CAPTURE_SETTINGS_PATH": (folder / "settings.ini").as_uri(),
        "BINGUX_CAPTURE_TEST_RESULTS": str(report),
        "BINGUX_CAPTURE_TEST_DIRECTORY": str(folder),
        "BINGUX_CAPTURE_TEST_GUIDE": str(folder / "guide.png"),
    },
    text=True, capture_output=True, timeout=25,
)
print(result.stdout + result.stderr)
evidence = json.loads(report.read_text())
assert result.returncode == 0 and evidence["result"] == "PASS", evidence
with Image.open(folder / "guide.png") as guide:
    assert guide.getpixel((300, 300))[:3] == (128, 128, 128), "Recorded area must stay clear"
    assert guide.getpixel((100, 100))[:3] == (102, 102, 102), "Outside backdrop must be exactly 20 percent black"
    assert guide.getpixel((199, 250))[:3] == (255, 255, 255), "Recording outline must match selection"
    assert min(guide.getpixel((195, 195))[:3]) > 225, "Recording corner marker must remain visible above applications"
info = json.loads(subprocess.check_output([
    "ffprobe", "-v", "error", "-show_entries", "format=duration:stream=width,height,codec_type", "-of", "json", evidence["path"],
]))
assert abs(float(info["format"]["duration"]) - evidence["expected"]) < 0.15, (info, evidence)
assert len(info["streams"]) == 1 and info["streams"][0]["width"] == 640, info
subprocess.run(["ffmpeg", "-v", "error", "-i", evidence["path"], "-f", "null", "-"], check=True)
print("PASS: region guide, hover reset, click trims to hover time, and full decode", evidence, folder)
