#!/usr/bin/env python3
"""Launch a generated desktop entry through the real dock in a private desktop."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
repo = Path(__file__).resolve().parents[1]
config = Path(os.environ["XDG_CONFIG_HOME"]) / "binguxctl-apps"
shutil.copytree(repo / "shell/bingux", config, ignore=shutil.ignore_patterns("__pycache__"))
fixture = config / "AppControlTest.qml"
fixture.write_text("""import Quickshell
ShellRoot {
    ProfileSettings { id: profile }
    Dock { id: dock; settings: profile }
    ShellCommands { indicators: null; mediaControls: null; notificationState: null; dockView: dock }
}
""")
app_id = "binguxctl-private-launch"
applications = Path(os.environ["XDG_DATA_HOME"]) / "applications"
applications.mkdir(parents=True, exist_ok=True)
entry = applications / (app_id + ".desktop")
launcher = config / "entry.py"
launch_log = config / "launch.json"
launcher.write_text(
    "import json, os, sys\nfrom pathlib import Path\n"
    + f"Path({str(launch_log)!r}).write_text(json.dumps({{'cwd': os.getcwd(), 'args': sys.argv[1:]}}))\n"
    + f"os.execv('/usr/bin/foot', ['foot', '--app-id', {app_id!r}, '--title', 'Bingux private launch', 'sleep', '60'])\n"
)
entry.write_text(f"""[Desktop Entry]
Type=Application
Name=Bingux private launch
Exec=/usr/bin/python3 {launcher} normal %k
Path={config}
Icon=utilities-terminal
Terminal=false
Actions=new-window;

[Desktop Action new-window]
Name=New window
Exec=/usr/bin/python3 {launcher} new-window
""")
qs = os.environ.get("QS_TEST_BIN", "qs")
command = [sys.executable, str(repo / "packages/binguxctl/binguxctl.py"), "--quickshell", qs, "--path", str(fixture)]
env = os.environ | {"GNOBLIN_COMPOSITOR_SOCKET": str(config / "private-compositor.sock")}
log_path = config / "quickshell.log"
log = log_path.open("w")
process = subprocess.Popen([qs, "-p", str(fixture)], env=env, stdout=log, stderr=subprocess.STDOUT)


def call(*args):
    result = subprocess.run(command + list(args), capture_output=True, text=True, timeout=10)
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    return json.loads(result.stdout)


def until(query, predicate):
    deadline = time.monotonic() + 10
    result = None
    while time.monotonic() < deadline:
        try:
            result = query()
            if predicate(result):
                return result
        except (AssertionError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    raise AssertionError(result)


try:
    until(lambda: call("apps", "list", app_id), lambda state: len(state["apps"]) == 1)
    for mode in ((), ("--new-window",)):
        launch_log.unlink(missing_ok=True)
        call("apps", "launch", app_id, *mode)
        until(lambda: launch_log.exists(), bool)
        windows = until(
            lambda: call("windows", "list")["windows"], lambda rows: any(row["appId"] == app_id for row in rows)
        )
        actual = json.loads(launch_log.read_text())
        if mode:
            assert actual["args"] == ["new-window"], actual
        else:
            assert actual["cwd"] == str(config) and actual["args"] == ["normal", str(entry)], actual
        for window in windows:
            if window["appId"] == app_id:
                call("windows", "close", window["id"])
        until(lambda: call("windows", "list")["windows"], lambda rows: all(row["appId"] != app_id for row in rows))
    print("PASS: installed app launch, working directory, desktop field codes, new-window action and real window close")
finally:
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
    log.close()
    print(log_path.read_text())
