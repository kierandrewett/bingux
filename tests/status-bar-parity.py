#!/usr/bin/env python3
"""Check that StatusBar draws the same top bar as the production desktop.

Runs tests/status-bar.qml (StatusBar alone, with the same hermetic session bus and
fake metrics as desktop-baseline.py) under headless
sway and compares the top strip with tests/fixtures/desktop-baseline/top-bar.png,
the bar of the production shell. The clock and the CPU/RAM readout are masked
the same way desktop-baseline.py masks them.

Needs sway, grim and the Quickshell binary (QS_TEST_BIN, default qs), and the
Bingux.Wayland plugin on QML_IMPORT_PATH, as for desktop-baseline.py.
"""

import argparse
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT / "shell/bingux"
HARNESS = ROOT / "tests/status-bar.qml"
REFERENCE = ROOT / "tests/fixtures/desktop-baseline/top-bar.png"
QS = os.environ.get("QS_TEST_BIN", "qs")
OUTPUT = "HEADLESS-1"
SIZE = (1280, 800)


def load_baseline():
    spec = importlib.util.spec_from_file_location("desktop_baseline", ROOT / "tests/desktop-baseline.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


baseline = load_baseline()


def capture_bar(shot=None):
    """Run the StatusBar harness under headless sway; return its top strip."""
    home = Path(tempfile.mkdtemp(prefix="bingux-status-bar-"))
    work = home / "work"
    work.mkdir()
    runtime = home / "run"
    runtime.mkdir(mode=0o700)
    metrics = baseline.fake_metrics(runtime)
    sway_config = home / "sway.conf"
    sway_config.write_text(f"output {OUTPUT} resolution {SIZE[0]}x{SIZE[1]}\n")
    base = os.environ | {
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(runtime),
        "WLR_BACKENDS": "headless",
        "WLR_RENDERER": "gles2",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
        "DBUS_SYSTEM_BUS_ADDRESS": baseline.SYSTEM_BUS,
    }
    base.pop("DISPLAY", None)
    base.pop("WAYLAND_DISPLAY", None)
    sway = subprocess.Popen(["sway", "-c", str(sway_config)], env=base, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    client = None
    try:
        if not baseline.wait_for(lambda: any(not p.name.endswith(".lock") for p in runtime.glob("wayland-*"))):
            raise SystemExit("status-bar-parity: headless sway did not start")
        wayland = sorted(p.name for p in runtime.glob("wayland-*") if not p.name.endswith(".lock"))[0]
        ipc = sorted(runtime.glob("sway-ipc.*.sock"))
        if not ipc:
            raise SystemExit("status-bar-parity: headless sway IPC socket did not appear")
        environment = base | {"WAYLAND_DISPLAY": wayland, "SWAYSOCK": str(ipc[0]), "QT_QPA_PLATFORM": "wayland"}
        qml = home / "status-bar.qml"
        qml.write_text(HARNESS.read_text().replace("../shell/bingux", SHELL.as_uri()))
        log = work / "shell.log"
        handle = log.open("w")
        client = subprocess.Popen([QS, "-p", str(qml)], env=environment, stdout=handle, stderr=subprocess.STDOUT)
        full = baseline.settled_desktop(environment, work, log)
        if shot:
            full.save(shot)
        return full.crop((0, 0, SIZE[0], baseline.bar_height()))
    finally:
        if client is not None and client.poll() is None:
            client.kill()
            client.wait(timeout=5)
        sway.terminate()
        sway.wait(timeout=5)
        metrics.stop.set()
        shutil.rmtree(home, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--shot", metavar="PATH", help="also save the full harness frame to PATH")
    baseline.ensure_hermetic()
    arguments = parser.parse_args()
    if not REFERENCE.is_file():
        raise SystemExit(f"status-bar-parity: missing reference {REFERENCE}")
    actual = capture_bar(arguments.shot)
    fraction, count = baseline.changed_fraction(Image.open(REFERENCE).convert("RGB"), actual)
    status = "ok  " if fraction <= baseline.FRACTION_TOLERANCE else "FAIL"
    print(f"{status} top-bar: {count} px changed ({fraction:.5%}, tolerance {baseline.FRACTION_TOLERANCE:.3%})")
    if fraction > baseline.FRACTION_TOLERANCE:
        raise SystemExit("status-bar-parity: StatusBar drifted from the desktop top bar")


if __name__ == "__main__":
    sys.exit(main())
