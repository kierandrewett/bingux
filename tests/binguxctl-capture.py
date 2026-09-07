#!/usr/bin/env python3
"""Private desktop: capture through binguxctl and play the real shutter event."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
repo = Path(__file__).resolve().parents[1]
config = Path(os.environ["XDG_CONFIG_HOME"]) / "binguxctl-capture"
shutil.copytree(repo / "shell/bingux", config, ignore=shutil.ignore_patterns("__pycache__"))
output = config / "captures"
output.mkdir()
settings = config / "capture.ini"
settings.write_text("[capture]\nkind=screenshot\ncopy=false\ndirectory=" + str(output) + "\n")
fixture = config / "CaptureControlTest.qml"
fixture.write_text('import Quickshell\nShellRoot { CaptureTool { screen: Quickshell.screens[0] } }\n')
player = shutil.which("canberra-gtk-play")
assert player, "Install the GNOME event sound player for the audio integration test"
probe = config / "bin"
probe.mkdir()
sound_log = config / "sound.json"
(probe / "canberra-gtk-play").write_text("#!/usr/bin/python3\nimport json, subprocess, sys\nfrom pathlib import Path\n"
    + "result = subprocess.run(" + repr([player]) + " + sys.argv[1:])\n"
    + "Path(" + repr(str(sound_log)) + ").write_text(json.dumps({'args': sys.argv[1:], 'code': result.returncode}))\n")
(probe / "canberra-gtk-play").chmod(0o755)
environment = os.environ | {"BINGUX_CAPTURE_SETTINGS_PATH": settings.as_uri(), "PATH": str(probe) + ":" + os.environ["PATH"]}
qs = os.environ.get("QS_TEST_BIN", "qs")
command = [sys.executable, str(repo / "packages/binguxctl/binguxctl.py"), "--quickshell", qs, "--path", str(fixture)]
log = (config / "quickshell.log").open("w")
process = subprocess.Popen([qs, "-p", str(fixture)], env=environment, stdout=log, stderr=subprocess.STDOUT)


def call(*args):
    result = subprocess.run(command + list(args), text=True, capture_output=True, timeout=12)
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    return json.loads(result.stdout) if result.stdout.strip() else None


def wait(predicate):
    deadline = time.monotonic() + 12
    state = None
    while time.monotonic() < deadline:
        try:
            state = call("capture", "status")
            if predicate(state): return state
        except AssertionError:
            pass
        time.sleep(.1)
    raise AssertionError(state)


try:
    wait(lambda state: state["ready"])
    call("capture", "open", "--mode", "screenshot", "--target", "region")
    wait(lambda state: state["opened"])
    call("capture", "open")
    assert call("capture", "status")["opened"]
    call("capture", "take")
    saved = wait(lambda state: state["state"] == "saved")
    assert Path(saved["savedPath"]).parent == output and Path(saved["savedPath"]).stat().st_size > 0, saved
    deadline = time.monotonic() + 4
    while not sound_log.exists() and time.monotonic() < deadline: time.sleep(.05)
    sound = json.loads(sound_log.read_text())
    assert sound["code"] == 0 and sound["args"][:2] == ["--id", "screen-capture"], sound
    print("PASS: binguxctl opens selector, repeated open stays open, take saves screenshot, GNOME shutter player succeeds")
finally:
    process.terminate()
    process.wait(timeout=5)
    log.close()
    print((config / "quickshell.log").read_text())
