#!/usr/bin/env python3
"""Real Files windows and dock mouse events in an isolated Gnoblin session."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import time

root = Path(os.environ["XDG_CONFIG_HOME"]) / "gnoblin"
if not str(root).startswith("/tmp/gnoblin-gs."):
    raise SystemExit("Run through the isolated Gnoblin test runner.")
(root / "scripts").mkdir(parents=True, exist_ok=True)
fixture = root / "dock-focus-fixture"
fixture.mkdir()
source = Path(__file__).resolve().parent.parent / "shell/bingux"
for path in source.iterdir():
    if path.suffix in (".qml", ".js") or path.name == "qmldir":
        shutil.copy2(path, fixture / path.name)
dock_file = fixture / "Dock.qml"
dock_source = dock_file.read_text().replace(
    "id: dockButton", 'id: dockButton; objectName: "dock-test-button"; property alias testMouse: dockMouse'
)
dock_file.write_text(dock_source)
report = root / "dock-focus-report.json"
probe = root / "scripts/dock-focus-probe.js"


def snapshot(action=""):
    sequence = time.monotonic_ns() // 1000
    probe.write_text(
        """
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
function rect(r) { return {x:r.x, y:r.y, width:r.width, height:r.height}; }
export default function () {
    ACTION
    const area = global.workspace_manager.get_active_workspace().get_work_area_for_monitor(0);
    GLib.file_set_contents(REPORT, JSON.stringify({sequence: SEQUENCE, pointer: global.get_pointer(), area: rect(area),
        windows: global.get_window_actors().map(a => ({title: a.meta_window.get_title(),
            rect: rect(a.meta_window.get_frame_rect()), visible: a.visible, minimized: a.meta_window.minimized,
            pid: a.meta_window.get_pid(), maximized: a.meta_window.is_maximized(), layer: a.meta_window.get_window_type() === Meta.WindowType.DOCK,
            focus: a.meta_window === global.display.focus_window}))}));
}
""".replace("ACTION", action)
        .replace("REPORT", json.dumps(str(report)))
        .replace("SEQUENCE", str(sequence))
    )
    subprocess.run(["gnoblinctl", "script", "reload"], check=True, capture_output=True)
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        try:
            result = json.loads(report.read_text())
            if result.get("sequence") == sequence:
                return result
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(0.02)
    raise AssertionError("Compositor probe did not finish")


def wait_for(operation, description):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        result = operation()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError(f"{description}: {snapshot()}")


def ipc(method, *args):
    result = subprocess.run(
        ["qs", "-p", str(fixture), "ipc", "call", "dockProbe", method, *args], capture_output=True, text=True, timeout=5
    )
    if result.returncode:
        return None
    return json.loads(result.stdout) if method == "status" else True


(fixture / "shell.qml").write_text("""import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
ShellRoot {
    TestCase { id: input; when: false }
    Dock { id: dock; settings: QtObject { property var pinnedApps: [] } }
    function buttons(item) {
        if (item.objectName === "dock-test-button") {
            const point = item.mapToItem(dock.contentItem, item.width / 2, item.height / 2);
            return [{id: item.currentGroup.id, hovered:item.testMouse.containsMouse, enabled:item.testMouse.enabled, x: point.x, y: point.y, windows: item.currentGroup.windows.map(w => ({title:w.title, active:w.activated, minimized:w.minimized}))}];
        }
        let result = [];
        for (const child of item.children || []) result = result.concat(buttons(child));
        return result;
    }
    IpcHandler {
        target: "dockProbe"
        function click(): void { const button = buttons(dock.contentItem).find(b => b.windows.some(w => w.title === "Files last")); input.mouseClick(dock.contentItem, button.x, button.y, Qt.LeftButton); }
        function status(): string { return JSON.stringify({buttons: buttons(dock.contentItem), focused: ToplevelManager.activeToplevel?.title, height:dock.height, width:dock.width}); }
    }
}
""")
folders = [root / "Files first", root / "Files last"]
for folder in folders:
    folder.mkdir()
app = subprocess.Popen(["nautilus", "--new-window", str(folders[0])])
shell = None
try:
    wait_for(lambda: any(w["title"] == "Files first" for w in snapshot()["windows"]), "first Files window")
    shell = subprocess.Popen(["qs", "-p", str(fixture)])
    wait_for(lambda: ipc("status"), "dock IPC")
    wait_for(lambda: any(b["windows"] for b in ipc("status")["buttons"]), "initial dock window")
    subprocess.run(["nautilus", "--new-window", str(folders[1])], check=True, timeout=8)
    wait_for(lambda: any(w["title"] == "Files last" for w in snapshot()["windows"]), "last Files window")
    snapshot(
        "global.get_window_actors().find(a => a.meta_window.get_title() === 'Files last').meta_window.activate(global.get_current_time());"
    )
    wait_for(lambda: any(w["title"] == "Files last" and w["focus"] for w in snapshot()["windows"]), "last Files focus")
    time.sleep(0.3)
    state = ipc("status")
    group = next(b for b in state["buttons"] if any(w["title"] == "Files last" for w in b["windows"]))
    assert group["windows"][-1]["title"] == "Files last", state
    ipc("click")
    time.sleep(0.7)
    result = snapshot()
    last = next(w for w in result["windows"] if w["title"] == "Files last")
    files = [w for w in result["windows"] if w["title"] in ("Files first", "Files last")]
    assert len(files) == 2 and all(w["minimized"] and not w["focus"] for w in files), result
    ipc("click")
    time.sleep(0.7)
    result = snapshot()
    last = next(w for w in result["windows"] if w["title"] == "Files last")
    first = next(w for w in result["windows"] if w["title"] == "Files first")
    # This fixture injects Qt mouse events and checks real window visibility.
    # The dock interaction fixture separately checks the activation request.
    assert not last["minimized"] and last["visible"] and first["minimized"], result
    print("DOCK_WINDOW_FOCUS_PASSED", flush=True)
finally:
    if shell:
        shell.terminate()
        shell.wait(timeout=5)
    subprocess.run(["nautilus", "--quit"], timeout=5, check=False)
    if app.poll() is None:
        app.terminate()
        app.wait(timeout=5)
