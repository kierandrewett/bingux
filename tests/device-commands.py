#!/usr/bin/env python3
"""Test network process and scan ownership in a private compositor session."""

import json
import os
from pathlib import Path
import subprocess

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
repo = Path(__file__).resolve().parents[1]
config_dir = Path(os.environ["XDG_CONFIG_HOME"])
config = config_dir / "devices.qml"
config.write_text(
    Path(__file__).with_suffix(".qml").read_text().replace("../shell/bingux", (repo / "shell/bingux").as_uri())
)
bin_dir = config_dir / "bin"
bin_dir.mkdir()
nmcli = bin_dir / "nmcli"
nmcli.write_text("""#!/usr/bin/python3
import json, os, sys, time
from pathlib import Path
state = Path(os.environ["XDG_CONFIG_HOME"]) / "network-state"
args = sys.argv[1:]
with (state.parent / "network-calls").open("a") as log: log.write(json.dumps(args) + "\\n")
if "up" in args or "down" in args:
    time.sleep(.2)
    if args[-1].endswith("555555555555"): sys.exit(4)
    state.write_text("yes" if "up" in args else "no")
elif "connection" in args:
    device = "eth0" if state.exists() and state.read_text() == "yes" else ""
    print("00000000-1111-2222-3333-444444444444:802-3-ethernet:Test:" + device)
    print("00000000-1111-2222-3333-555555555555:802-3-ethernet:Failure:")
""")
nmcli.chmod(0o755)
env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ["PATH"])
result = subprocess.run(
    [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(config)], env=env, text=True, capture_output=True, timeout=40
)
print(result.stdout + result.stderr)
assert result.returncode == 0 and "DEVICE_COMMANDS_PASSED" in result.stdout + result.stderr

calls = [json.loads(line) for line in (config_dir / "network-calls").read_text().splitlines()]
changes = [call for call in calls if "up" in call or "down" in call]
assert changes == [
    ["--wait", "10", "connection", action, "uuid", identity]
    for action, identity in [
        ("up", "00000000-1111-2222-3333-444444444444"),
        ("down", "00000000-1111-2222-3333-444444444444"),
        ("up", "00000000-1111-2222-3333-555555555555"),
    ]
], changes
