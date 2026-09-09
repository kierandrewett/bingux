#!/usr/bin/env python3
"""Exercise the editor on two private virtual displays with different scales.

Start Gnoblin with two 1920x1200 virtual monitors. Only the private test
configuration is changed; no display settings are made persistent.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

from private_shell import display_state

parser = argparse.ArgumentParser()
parser.add_argument("--cases", nargs="+", choices=["editor-compact", "sidebar-layout", "overflow-edit", "dock-unpin"],
                    default=["editor-compact", "sidebar-layout", "overflow-edit"])
parser.add_argument("--arrangements", nargs="+", choices=["horizontal", "vertical"], default=["horizontal", "vertical"])
parser.add_argument("--capture-dir", type=Path)
args = parser.parse_args()
if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Only a private Gnoblin compositor can change displays for this test")
if not str(Path(os.environ["XDG_CONFIG_HOME"]).resolve()).startswith("/tmp/gnoblin-gs."):
    raise SystemExit("The test requires private compositor settings")
subprocess.run(["gsettings", "set", "org.gnome.mutter", "experimental-features",
                "['scale-monitor-framebuffer']"], check=True)
initial = display_state()
if len(initial[1]) != 2:
    raise SystemExit("Start the private compositor with two virtual monitors")
monitors = sorted(initial[1], key=lambda monitor: monitor[0][0])
connectors = [monitor[0][0] for monitor in monitors]
modes = [next(mode for mode in monitor[1] if mode[6].get("is-current")) for monitor in monitors]
scales = [1.0, 1.5]
for mode, scale in zip(modes, scales):
    if not any(abs(scale - supported) < 0.001 for supported in mode[5]):
        raise SystemExit(f"Mode {mode[1]}x{mode[2]} does not support scale {scale}")
sizes = [(round(mode[1] / scale), round(mode[2] / scale)) for mode, scale in zip(modes, scales)]
runner = Path(__file__).with_name("desktop-layout-live.py")
for arrangement in args.arrangements:
    # Change which display is primary and test nonzero origins on both axes.
    positions = [(0, 0), (sizes[0][0], 0)] if arrangement == "horizontal" else [(0, sizes[1][1]), (0, 0)]
    primary = 0 if arrangement == "horizontal" else 1
    command = ["gdctl", "set", "--layout-mode", "logical"]
    for index, connector in enumerate(connectors):
        command += ["--logical-monitor", "--monitor", connector, "--scale", str(scales[index]),
                    "--x", str(positions[index][0]), "--y", str(positions[index][1])]
        if index == primary:
            command.append("--primary")
    subprocess.run(command, check=True)
    actual = display_state()
    if len(actual[2]) != 2:
        raise SystemExit("The compositor did not create two separate logical displays")
    for index, connector in enumerate(connectors):
        logical = next(item for item in actual[2] if any(monitor[0] == connector for monitor in item[5]))
        if tuple(logical[:2]) != positions[index] or abs(logical[2] - scales[index]) > 0.001 or logical[4] != (index == primary):
            raise SystemExit(f"Unexpected logical display for {connector}: {logical}")
        environment = os.environ | {
            "BINGUX_TEST_SCREEN_NAME": connector,
            "BINGUX_TEST_SCREEN_WIDTH": str(sizes[index][0]),
            "BINGUX_TEST_SCREEN_HEIGHT": str(sizes[index][1]),
            "BINGUX_TEST_INPUT_ORIGIN": json.dumps(positions[index]),
        }
        if args.capture_dir:
            args.capture_dir.mkdir(parents=True, exist_ok=True)
            environment["BINGUX_GROUP_CAPTURE"] = str(args.capture_dir / f"{arrangement}-{connector}")
            environment["BINGUX_NATIVE_SCREENSHOT"] = str(args.capture_dir / f"{arrangement}-{connector}-editor.png")
        print(f"DISPLAY {arrangement} {connector}: origin={positions[index]} size={sizes[index]} scale={scales[index]} primary={index == primary}", flush=True)
        for case in args.cases:
            subprocess.run([sys.executable, str(runner), "--case", case], env=environment, check=True)
        print(f"PASS: {', '.join(args.cases)} on {arrangement} {connector}", flush=True)
