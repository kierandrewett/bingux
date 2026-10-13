#!/usr/bin/env python3
"""Check Settings' production callback without a display or host session bus.

The GNOME 49.9 guard is in shell/cc-application.c:179-222:
https://gitlab.gnome.org/GNOME/gnome-control-center/-/blob/49.9/shell/cc-application.c
Use --real-guard to also run the installed binary's headless --list path.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

SHELL = Path(__file__).resolve().parents[1] / "shell/bingux"
QML = r"""
const fs = require("fs"), vm = require("vm"), url = require("url");
const shell = process.argv[1], commands = [];
const source = fs.readFileSync(shell+"/ControlCentre.qml", "utf8");
const handler = source.match(/^    function settings\([^\n]*\) \{\n.*?^    \}/ms);
if (!handler) throw new Error("Settings callback not found");
const context = {
    visible: true,
    Quickshell: {execDetached: argv => commands.push(Array.from(argv))},
    Qt: {resolvedUrl: name => url.pathToFileURL(shell+"/"+name)}
};
vm.createContext(context);
vm.runInContext(handler[0], context);
context.settings(process.argv[2]);
console.log(JSON.stringify({commands, visible:context.visible}));
"""
PROBE = r"""
import json, os, sys
from pathlib import Path
# Match the upstream desktop token guard, including its current diagnostic.
if "gnome" not in os.environ.get("XDG_CURRENT_DESKTOP", "").lower().split(":"):
    print("Running gnome-control-center is only supported under GNOME and Unity, exiting", file=sys.stderr)
    raise SystemExit(1)
Path(os.environ["SETTINGS_RECORD"]).write_text(json.dumps({
    "argv":sys.argv[1:], "desktop":os.environ.get("XDG_CURRENT_DESKTOP"),
    "session":os.environ.get("XDG_SESSION_DESKTOP"), "sentinel":os.environ.get("SETTINGS_SENTINEL"),
    "nice":os.getpriority(os.PRIO_PROCESS,0), "policy":os.sched_getscheduler(0)
}))
"""


def callback(panel):
    result = subprocess.run(["node", "-e", QML, str(SHELL), panel], capture_output=True, text=True, check=True)
    state = json.loads(result.stdout)
    assert state["visible"] is False and len(state["commands"]) == 1, state
    return state["commands"][0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--real-guard", action="store_true")
    args = parser.parse_args()
    original_desktop = os.environ.get("XDG_CURRENT_DESKTOP")
    binary = shutil.which("gnome-control-center")
    if args.real_guard and not binary:
        parser.error("--real-guard requires an installed gnome-control-center")
    with tempfile.TemporaryDirectory(prefix="bingux-settings-launch-") as temporary:
        root = Path(temporary)
        probe = root / "gnome-control-center"
        probe.write_text(f"#!{sys.executable}\n" + PROBE)
        probe.chmod(0o700)
        record = root / "record.json"
        env = os.environ | {
            "XDG_CURRENT_DESKTOP": "Gnoblin",
            "XDG_SESSION_DESKTOP": "gnoblin",
            "SETTINGS_RECORD": str(record),
            "SETTINGS_SENTINEL": "literal $value ' \\",
            "PATH": str(root) + os.pathsep + os.environ["PATH"],
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(root / "no-session-bus"),
        }
        for key in ("DISPLAY", "WAYLAND_DISPLAY"):
            env.pop(key, None)
        failures = []
        for panel in ("network", "network; $(touch forbidden) ' \\"):
            record.unlink(missing_ok=True)
            parent_nice = os.getpriority(os.PRIO_PROCESS, 0)
            result = subprocess.run(callback(panel), env=env, cwd=root, capture_output=True, text=True, timeout=5)
            if result.returncode:
                failures.append(f"Settings callback {panel!r}: exit {result.returncode}: {result.stderr.strip()}")
                continue
            state = json.loads(record.read_text())
            assert state["argv"] == [panel], state
            assert state["desktop"] == "GNOME", state
            assert state["session"] == "gnoblin" and state["sentinel"] == env["SETTINGS_SENTINEL"], state
            assert state["nice"] == max(0, parent_nice), state
            assert state["policy"] == os.sched_getscheduler(0), state
            assert env["XDG_CURRENT_DESKTOP"] == "Gnoblin", "parent launch environment changed"
            assert not (root / "forbidden").exists(), "panel interpreted as shell code"
            print("Settings child:", json.dumps(state))
        if args.real_guard:
            real_env = env | {"PATH": os.environ["PATH"], "GDK_BACKEND": "none"}
            # Upstream handles --list after the guard but before application
            # startup. Both display variables and the host bus are unavailable.
            result = subprocess.run(
                callback("--list"), env=real_env, cwd=root, capture_output=True, text=True, timeout=8
            )
            if result.returncode or "network" not in result.stdout:
                failures.append(f"Installed Settings --list: exit {result.returncode}: {result.stderr.strip()}")
            else:
                print("PASS: installed Settings --list accepts callback environment and lists network")
        assert os.environ.get("XDG_CURRENT_DESKTOP") == original_desktop
        if failures:
            print("FAIL:\n" + "\n".join(failures), file=sys.stderr)
            return 1
        print(
            "PASS: production Settings callback; child-only desktop environment; panel argv; inherited priority/policy"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
