#!/usr/bin/env python3
"""Exercise the real editor and moved popups at native compositor scales."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

from gi.repository import Gio

parser = argparse.ArgumentParser()
parser.add_argument("--scales", nargs="+", type=float, default=[1.25, 1.5, 2.0])
parser.add_argument("--capture-dir", type=Path)
parser.add_argument("--cases", nargs="+", choices=["editor-compact", "control-external"], default=["editor-compact", "control-external"])
args = parser.parse_args()
if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Only a private Gnoblin compositor can change scale for this test")
if not str(Path(os.environ["XDG_CONFIG_HOME"]).resolve()).startswith("/tmp/gnoblin-gs."):
    raise SystemExit("The test requires private compositor settings")
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)


def state():
    return bus.call_sync(
        "org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig", "GetCurrentState", None, None,
        Gio.DBusCallFlags.NONE, 5000, None,
    ).unpack()


subprocess.run([
    "gsettings", "set", "org.gnome.mutter", "experimental-features",
    "['scale-monitor-framebuffer']",
], check=True)
initial = state()
if len(initial[1]) != 1:
    raise SystemExit("Use one virtual monitor for the scale matrix")
monitor = initial[1][0]
connector = monitor[0][0]
mode = next(mode for mode in monitor[1] if mode[6].get("is-current"))
runner = Path(__file__).with_name("desktop-layout-live.py")
for scale in args.scales:
    subprocess.run([
        "gdctl", "set", "--layout-mode", "logical", "--logical-monitor",
        "--monitor", connector, "--primary", "--scale", str(scale),
    ], check=True)
    actual = state()
    if abs(actual[2][0][2] - scale) > 0.001:
        raise SystemExit("The compositor did not apply the requested scale")
    environment = os.environ | {
        "BINGUX_TEST_SCREEN_WIDTH": str(round(mode[1] / scale)),
        "BINGUX_TEST_SCREEN_HEIGHT": str(round(mode[2] / scale)),
    }
    if args.capture_dir:
        args.capture_dir.mkdir(parents=True, exist_ok=True)
        environment["BINGUX_GROUP_CAPTURE"] = str(args.capture_dir / f"scale-{scale}")
        environment["BINGUX_NATIVE_SCREENSHOT"] = str(args.capture_dir / f"editor-{scale}.png")
    print(f"SCALE {scale}: {environment['BINGUX_TEST_SCREEN_WIDTH']}x{environment['BINGUX_TEST_SCREEN_HEIGHT']}", flush=True)
    for case in args.cases:
        subprocess.run([sys.executable, str(runner), "--case", case], env=environment, check=True)
    print(f"PASS: {", ".join(args.cases)} at {scale}", flush=True)
