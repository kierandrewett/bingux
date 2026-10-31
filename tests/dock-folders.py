#!/usr/bin/env python3
"""Run the dock folder scenarios of tests/dock-folders.qml.

model    the dock's folder operations, the one-app collapse and the saved file.
legacy   the same without a saved dock_apps layout (pins in the dock's state file).
pointer  the same through real pointer and keyboard input in a private headless
         Sway session. The QML test prints DOCK_FOLDERS_STEP lines; this runner
         moves Sway's cursor and types with wtype, then the QML test checks.

The Sway session is virtual: nothing appears on the host display and the host's
input devices are not touched. Screenshots (the `shot` steps) go to
--output. Each scenario also runs in reduced motion, and the pointer scenario
at three dock sizes and alignments (written to a private bingux.toml).

    QS_TEST_BIN=build/bingux-shell/quickshell/src/bingux-shell \\
    QML_IMPORT_PATH=build/qml python3 tests/dock-folders.py [--output DIR] [--scenario NAME]
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests/lib"))
import dock_fixture  # noqa: E402
import headless_sway  # noqa: E402

QS = os.environ.get("QS_TEST_BIN", "qs")
MODES = (
    ("normal motion", {}),
    ("reduced motion", {"BINGUX_REDUCED_MOTION": "1"}),
)
# (dock icon size, dock alignment): the dock sits at the bottom of the screen
# and aligns left, centre or right; its folder card must follow it.
LAYOUTS = ((56, "center"), (36, "left"), (80, "right"))
KEY_NAMES = {"Return": "Return", "Escape": "Escape", "Left": "Left", "Right": "Right", "Up": "Up", "Down": "Down"}


def wtype(session, *arguments):
    subprocess.run(["wtype", *arguments], env=session.environment, check=True, capture_output=True)


def press_keys(session, keys):
    for key in keys:
        modifiers = key.split("+")
        name = modifiers.pop()
        command = []
        for modifier in modifiers:
            command += ["-M", modifier]
        command += ["-k", KEY_NAMES.get(name, name)]
        for modifier in reversed(modifiers):
            command += ["-m", modifier]
        wtype(session, *command)
        time.sleep(0.12)


def move(session, x, y):
    session.pointer(x, y)


def drag(session, start, end, hold):
    """Press at start, move in small steps to end, wait `hold` ms, release.

    With no hold the drag is quick (a few steps, no pauses), so a test of "a
    quick drag only reorders" does not depend on this runner's own speed."""
    move(session, *start)
    time.sleep(0.1)
    session.button("press")
    time.sleep(0.1)
    steps, pause = (16, 0.03) if hold else (6, 0)
    for index in range(1, steps + 1):
        move(session, start[0] + (end[0] - start[0]) * index / steps, start[1] + (end[1] - start[1]) * index / steps)
        time.sleep(pause)
    # A jiggle at the end keeps the pointer events flowing during the dwell.
    deadline = time.monotonic() + hold / 1000
    wobble = 0
    while time.monotonic() < deadline:
        wobble = 1 - wobble
        move(session, end[0] + wobble, end[1])
        time.sleep(0.05)
    session.button("release")


def perform(session, action, output, label):
    kind = action["do"]
    if kind == "drag":
        drag(session, action["from"], action["to"], action.get("hold", 0))
    elif kind == "click":
        move(session, action["x"], action["y"])
        time.sleep(0.1)
        session.button("press")
        time.sleep(0.06)
        session.button("release")
    elif kind == "pointer":
        move(session, action["x"], action["y"])
    elif kind == "type":
        # The popup's keyboard focus arrives a moment after its field reports it.
        time.sleep(0.8)
        wtype(session, action["text"])
    elif kind == "keys":
        time.sleep(0.4)
        press_keys(session, action["keys"])
    elif kind == "shot":
        if output:
            output.mkdir(parents=True, exist_ok=True)
            session.screenshot(output / f"{label}-{action['name']}.png")
    else:
        raise SystemExit("unknown action " + kind)


def run_scenario(scenario, label, extra_env, size, alignment, output):
    with headless_sway.session(parent=os.environ.get("BINGUX_TEST_TMP")) as session:
        holder = None
        if scenario == "pointer":
            # A held virtual keyboard keeps the seat's keyboard capability.
            holder = subprocess.Popen(
                ["wtype", "-s", "120000", "-k", "F24"], env=session.environment, stderr=subprocess.DEVNULL
            )
            headless_sway.wait_for(lambda: int(json.loads(session.swaymsg("-t", "get_seats"))[0]["capabilities"]) & 2)
        with tempfile.TemporaryDirectory(prefix="dock-folders-", dir=os.environ.get("BINGUX_TEST_TMP")) as directory:
            fixture = Path(directory)
            environment = dock_fixture.prepare(
                fixture,
                ROOT / "tests/dock-folders.qml",
                config_toml=f'[desktop]\ndock_size = {size}\ndock_alignment = "{alignment}"\n',
            )
            environment |= {
                "DOCK_FOLDERS_SCENARIO": scenario,
                "BINGUX_CONFIG_BIN": os.environ.get("BINGUX_CONFIG_BIN", "bingux-config"),
                "BINGUX_HELPER_DIR": str(ROOT / "shell/bingux"),
            }
            client = subprocess.Popen(
                [QS, "-p", str(fixture), "--no-color"],
                env=session.environment | environment | extra_env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            lines = []

            def pump():
                for line in client.stdout:
                    lines.append(line)
                    if "DOCK_FOLDERS_STEP " in line:
                        action = json.loads(line.split("DOCK_FOLDERS_STEP ", 1)[1])
                        try:
                            perform(session, action, output, label)
                        except (subprocess.CalledProcessError, OSError) as error:
                            lines.append(f"RUNNER FAILED: {error}\n")

            reader = threading.Thread(target=pump, daemon=True)
            reader.start()
            try:
                client.wait(timeout=240)
            except subprocess.TimeoutExpired:
                client.kill()
            reader.join(timeout=5)
            if holder:
                holder.kill()
            text = "".join(lines)
            passed = client.returncode == 0 and f"DOCK_FOLDERS_PASSED {scenario}" in text and "FAIL" not in text
            print(f"--- {scenario}, {label}, dock size {size}, aligned {alignment} ---")
            print(
                "\n".join(
                    line for line in text.splitlines() if "DOCK_FOLDERS" in line or "FAIL" in line or "ERROR" in line
                )
            )
            print(("ok   " if passed else "FAIL ") + f"{scenario}, {label}, dock size {size}, aligned {alignment}")
            return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--output", type=Path, help="directory for the pointer scenario's screenshots")
    parser.add_argument("--scenario", choices=("model", "legacy", "pointer"))
    parser.add_argument("--first", action="store_true", help="only normal motion and the first dock layout")
    args = parser.parse_args()
    for tool in ("sway", "swaymsg", "grim", "wtype", QS):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    results = []
    for scenario in (args.scenario,) if args.scenario else ("model", "legacy", "pointer"):
        for mode, extra in MODES[:1] if args.first else MODES:
            for size, alignment in LAYOUTS[:1] if args.first or scenario != "pointer" else LAYOUTS:
                label = mode.replace(" ", "-") + f"-{size}-{alignment}"
                results.append(run_scenario(scenario, label, extra, size, alignment, args.output))
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
