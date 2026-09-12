#!/usr/bin/env python3
"""Apply an editor layout to the actual shell in a private compositor."""

import os
import argparse
from pathlib import Path
import shutil
import tempfile
import subprocess
import signal
import resource
import time
from private_shell import run_reported_shell, stage_compositor_bridge

parser = argparse.ArgumentParser()
parser.add_argument(
    "--case",
    choices=(
        "extensions",
        "desktop-layout",
        "control-layout",
        "control-audio",
        "control-media",
        "control-groups",
        "control-connectivity",
        "control-utilities",
        "control-external",
        "control-capture",
        "settings-recovery",
        "sidebar-layout",
        "sidebar-controls",
        "sidebar-panels",
        "editor-reset",
        "dock-unpin",
        "panel-placement",
        "editor-compact",
        "editor-save-input",
        "metrics-placement",
        "tray-placement",
        "overflow-edit",
        "overflow-detached",
    ),
    default="desktop-layout",
)
case = parser.parse_args().case
repo = Path(__file__).resolve().parent.parent
if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin's private test session")
with tempfile.TemporaryDirectory(prefix="bingux-layout-live-") as directory:
    fixture = Path(directory)
    for source in (repo / "shell/bingux").iterdir():
        if source.suffix in (".qml", ".js", ".py") or source.name == "qmldir":
            shutil.copy2(source, fixture)
    shell = (
        (fixture / "shell.qml")
        .read_text()
        .replace("import QtQuick\n", "import QtQuick\nimport QtQuick.Window\nimport QtTest\n", 1)
    )
    if os.environ.get("BINGUX_TEST_SCREEN_NAME"):
        shell = shell.replace(
            "id: topBar\n",
            'id: topBar\n        screen: Quickshell.screens.find(screen => screen.name === Quickshell.env("BINGUX_TEST_SCREEN_NAME"))\n',
            1,
        )
    if case in ("extensions", "control-layout", "control-groups", "control-utilities", "sidebar-controls"):
        shell = 'import "ControlLayout.js" as ControlLayout\n' + shell
    if case == "control-connectivity":
        shell = "import Quickshell.Bluetooth\n" + shell
    base_shell = shell
    shell = shell.rstrip()[:-1] + (repo / ("tests/" + case + "-live.inc.qml")).read_text() + "\n}\n"
    (fixture / "shell.qml").write_text(shell)
    for folder in ("icons", "preview-assets"):
        shutil.copytree(repo / "shell/bingux" / folder, fixture / folder)
    (fixture / "bin").mkdir()
    systemctl = fixture / "bin/systemctl"
    systemctl.write_text("#!/bin/sh\nexit 0\n")
    systemctl.chmod(0o700)
    environment = os.environ | {
        "BINGUX_TEST_COMPOSITOR_CONFIG": os.environ["XDG_CONFIG_HOME"],
        "BINGUX_TEST_NATIVE_INPUT": str(repo / "tests/customise-native-input.py"),
        "QT_QPA_PLATFORM": "wayland",
        "XDG_CONFIG_HOME": str(fixture / "config"),
        "XDG_STATE_HOME": str(fixture / "state"),
        "BINGUX_LAYOUT_IMPORT": "0",
        "PATH": str(fixture / "bin") + ":" + os.environ["PATH"],
    }
    if case in ("control-layout", "control-groups", "control-utilities", "sidebar-controls"):
        (fixture / "action-avatar.svg").write_text(
            '<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><circle cx="24" cy="24" r="24" fill="#77aaff"/></svg>'
        )
        environment["BINGUX_ACTION_REPORT"] = str(fixture / "actions.jsonl")
        for name in ("gnome-control-center", "loginctl"):
            helper = fixture / "bin" / name
            helper.write_text(
                '#!/usr/bin/python3\nimport json,os,sys\nwith open(os.environ["BINGUX_ACTION_REPORT"],"a") as output: output.write(json.dumps({"command":os.path.basename(sys.argv[0]),"arguments":sys.argv[1:]})+"\\n")\n'
            )
            helper.chmod(0o700)
    if case == "control-capture":
        environment["BINGUX_CAPTURE_HELPER"] = str(repo / "tests/customise-capture-helper.py")
        environment["BINGUX_CAPTURE_ACTION_REPORT"] = str(fixture / "capture-actions.jsonl")
    if case == "extensions":
        extension_root = fixture / "data/bingux/extensions"
        extension_root.mkdir(parents=True)
        shutil.copytree(repo / "examples/extensions/org.bingux.example", extension_root / "org.bingux.example")
        (fixture / "config/bingux").mkdir(parents=True, exist_ok=True)
        (fixture / "config/bingux/extensions.json").write_text('{"enabled":["org.bingux.example"]}')
        environment["XDG_DATA_HOME"] = str(fixture / "data")
        environment["BINGUX_NO_EXTENSIONS"] = "0"
    environment.pop("BINGUX_SETTINGS_HELPER", None)
    stage_compositor_bridge(repo, os.environ["XDG_CONFIG_HOME"])
    subprocess.run(["python3", str(repo / "tests/customise-native-input.py"), "--prepare"], env=environment, check=True)
    time.sleep(0.3)
    # Search is a separate UI session, so exercise its actual receiver too.
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    search_process = None
    with (fixture / "search-runtime.log").open("w") as search_log:
        if case == "control-external":
            search_process = subprocess.Popen(
                [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture / "SearchShell.qml"), "--no-color"],
                env=environment,
                stdout=search_log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        try:
            # Recovery deliberately rewrites and reloads the settings file for
            # every malformed-case probe; give its nine-file matrix enough
            # time to finish on a cold private compositor.
            timeout = 90 if case == "settings-recovery" else 55
            report, output = run_reported_shell(fixture, environment, "BINGUX_LAYOUT_REPORT", timeout=timeout)
            print(output)
            if report != "PASS" or any(
                error in output
                for error in (
                    "CUSTOMISE_TEST_FAILED",
                    "TypeError",
                    "ReferenceError",
                    "has crashed",
                    'property "maximumPopupHeight"',
                    'property "preferredY"',
                    "Cannot use same item on different windows",
                    "Updates can only be scheduled",
                    "QGridLayoutEngine::addItem",
                    "Binding loop detected",
                )
            ):
                raise SystemExit(report if report != "PASS" else "Runtime errors during layout test")
            if case in (
                "extensions",
                "control-groups",
                "control-connectivity",
                "control-utilities",
                "control-external",
                "control-capture",
                "settings-recovery",
                "sidebar-layout",
                "sidebar-controls",
                "editor-reset",
                "panel-placement",
                "metrics-placement",
                "tray-placement",
                "overflow-edit",
            ):
                (fixture / "shell.qml").write_text(
                    base_shell.rstrip()[:-1] + (repo / ("tests/" + case + "-reload.inc.qml")).read_text() + "\n}\n"
                )
                (fixture / "report").unlink()
                report, output = run_reported_shell(fixture, environment, "BINGUX_LAYOUT_REPORT", timeout=30)
                print(output)
                if report != "PASS" or any(
                    error in output
                    for error in (
                        "CUSTOMISE_TEST_FAILED",
                        "TypeError",
                        "ReferenceError",
                        "Binding loop detected",
                        "has crashed",
                    )
                ):
                    raise SystemExit(report if report != "PASS" else "Runtime errors after restoring saved groups")
                print("PASS: saved widget placements and settings survive a fresh shell process")
        finally:
            if search_process:
                try:
                    os.killpg(search_process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                search_process.wait(timeout=5)
                search_log.flush()
                search_output = (fixture / "search-runtime.log").read_text()
                if any(
                    error in search_output
                    for error in ("TypeError", "ReferenceError", "Binding loop detected", "has crashed")
                ):
                    print(search_output)
                    raise SystemExit("Runtime errors in the real search UI session")
