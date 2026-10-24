#!/usr/bin/env python3
"""Run the monitor identify labels in a headless Sway session with two outputs.

tests/identify.qml drives the production IdentifyState and IdentifySurface:
replacement, numbering by request order, expiry, reduced motion, click-through
and hotplug. Headless Sway starts with one output, so the runner creates the
second over IPC and then disables and re-enables it to exercise hotplug.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
QS = os.environ.get("QS_TEST_BIN", "qs")
MODES = (
    ("normal motion", {}),
    ("reduced motion", {"BINGUX_REDUCED_MOTION": "1"}),
)
FIXTURE_FILES = ("Theme", "PanelOutline", "IdentifyState", "IdentifySurface")


def wait_for(predicate, timeout=10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.1)
    return False


def outputs(environment):
    result = subprocess.run(
        ["swaymsg", "-t", "get_outputs"],
        env=environment,
        capture_output=True,
        text=True,
        check=True,
    )
    return {entry["name"] for entry in json.loads(result.stdout)}


def run_mode(label, extra_env):
    home = Path(tempfile.mkdtemp(prefix="bingux-identify-"))
    runtime = home / "run"
    runtime.mkdir(mode=0o700)
    fixture = home / "fixture"
    fixture.mkdir()
    for name in FIXTURE_FILES:
        shutil.copy2(ROOT / f"shell/bingux/{name}.qml", fixture)
    shutil.copy2(ROOT / "tests/identify.qml", fixture / "shell.qml")
    (fixture / "qmldir").write_text(
        "singleton Theme 1.0 Theme.qml\n"
        + "PanelOutline 1.0 PanelOutline.qml\n"
        + "IdentifyState 1.0 IdentifyState.qml\n"
        + "IdentifySurface 1.0 IdentifySurface.qml\n"
    )
    sway_config = home / "sway.conf"
    sway_config.write_text("output HEADLESS-1 resolution 1280x800\n")
    base = os.environ | {
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(runtime),
        "WLR_BACKENDS": "headless",
        "WLR_RENDERER": "gles2",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
    }
    base.pop("DISPLAY", None)
    base.pop("WAYLAND_DISPLAY", None)
    sway = subprocess.Popen(
        ["sway", "-c", str(sway_config)],
        env=base,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    client = None
    try:
        if not wait_for(lambda: any(not p.name.endswith(".lock") for p in runtime.glob("wayland-*"))):
            print("headless sway did not start")
            return False
        wayland = sorted(p.name for p in runtime.glob("wayland-*") if not p.name.endswith(".lock"))[0]
        ipc = sorted(runtime.glob("sway-ipc.*.sock"))
        if not ipc:
            print("headless sway IPC socket did not appear")
            return False
        environment = base | {
            "WAYLAND_DISPLAY": wayland,
            "SWAYSOCK": str(ipc[0]),
            "QT_QPA_PLATFORM": "wayland",
        } | extra_env
        subprocess.run(["swaymsg", "create_output"], env=environment, check=True, capture_output=True)
        if not wait_for(lambda: {"HEADLESS-1", "HEADLESS-2"} <= outputs(environment)):
            print("second headless output did not appear")
            return False
        subprocess.run(
            ["swaymsg", "output", "HEADLESS-2", "resolution", "1280x800"],
            env=environment,
            check=True,
            capture_output=True,
        )
        client = subprocess.Popen(
            [QS, "-p", str(fixture), "--no-color"],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        try:
            output, _ = client.communicate(timeout=90)
        except subprocess.TimeoutExpired:
            client.kill()
            output, _ = client.communicate()
        print(f"--- {label} ---")
        print(output)
        passed = client.returncode == 0 and "IDENTIFY_TEST_PASSED" in output and "FAIL!" not in output
        print(("ok   " if passed else "FAIL ") + label)
        return passed
    finally:
        if client is not None and client.poll() is None:
            client.kill()
        sway.terminate()
        sway.wait(timeout=5)
        shutil.rmtree(home, ignore_errors=True)


def main():
    for tool in ("sway", "swaymsg", QS):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    results = [run_mode(label, extra_env) for label, extra_env in MODES]
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
