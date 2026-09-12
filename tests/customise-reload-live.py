#!/usr/bin/env python3
"""Reload the real shell after editor drags without losing windows or IPC."""

import json
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import tempfile
import time
from private_shell import stage_compositor_bridge


if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin's private test session")
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
repo = Path(__file__).resolve().parent.parent
qs = os.environ.get("QS_TEST_BIN", "qs")
with tempfile.TemporaryDirectory(prefix="bingux-editor-reload-") as directory:
    fixture = Path(directory)
    for source in (repo / "shell/bingux").iterdir():
        if source.suffix in (".qml", ".js", ".py") or source.name == "qmldir":
            shutil.copy2(source, fixture)
    for folder in ("icons", "preview-assets"):
        shutil.copytree(repo / "shell/bingux" / folder, fixture / folder)
    sidebar = fixture / "TerminalSidebar.qml"
    sidebar.write_text(
        sidebar.read_text().replace(
            'Quickshell.env("HOME") + "/.config/bingux/sidebar.ini"',
            'Quickshell.env("XDG_CONFIG_HOME") + "/bingux/sidebar.ini"',
        )
    )
    shell = fixture / "shell.qml"
    source = (
        shell.read_text().rstrip()[:-1]
        + """
    QtObject {
        id: reloadPlayer
        property string identity: "Reload test"
        property string uniqueId: "reload-test"
        property string trackTitle: "Sample track"
        property string trackArtist: "Sample artist"
        property string trackArtUrl: ""
        property real position: 72
        property real length: 224
        property bool isPlaying: false
        property bool canControl: true
        property bool canPlay: true
        property bool canPause: true
        property bool canSeek: true
        property bool canGoNext: true
        property bool canGoPrevious: true
        property bool positionSupported: true
        property bool lengthSupported: true
    }
    IpcHandler {
        target: "reload-test"
        function media(): void {
            terminalSidebar.selectContent("media");
            terminalSidebar.open();
            terminalSidebar.activePanel.players = [reloadPlayer];
        }
        function open(): void { desktopCustomiser.open(); }
        function cancel(): void { desktopCustomiser.cancel(); }
        function hideSidebar(): void { terminalSidebar.hide(); }
        function detachSidebar(): void { terminalSidebar.popOut(); }
        function status(): string {
            const origin = DesktopEditing.point(clockPill, topBar, 24, 16);
            const destination = desktopCustomiser.preview.paletteRect;
            return JSON.stringify({
                revision: RELOAD_REVISION, pid: Quickshell.processId,
                sidebarOpen: terminalSidebar.opened, detached: terminalSidebar.detached,
                mediaCount: terminalSidebar.contentType === "media" && terminalSidebar.activePanel
                    ? terminalSidebar.activePanel.players.length : 0,
                ready: topBar.visible && BinguxPreferences.loaded &&
                    ControlCentreServices.preferencesReady && dock.appGroupsInitialised,
                visible: desktopCustomiser.visible, drag: desktopCustomiser.draggedId,
                clock: topBar.snapshotLayout()["top-center"].includes("clock"),
                x: origin.x, y: origin.y, dx: destination.x + 20, dy: destination.y + 20
            });
        }
    }
}
"""
    )
    shell.write_text(source.replace("RELOAD_REVISION", "0"))
    environment = os.environ | {
        "BINGUX_TEST_COMPOSITOR_CONFIG": os.environ["XDG_CONFIG_HOME"],
        "XDG_CONFIG_HOME": str(fixture / "config"),
        "XDG_STATE_HOME": str(fixture / "state"),
        "BINGUX_LAYOUT_IMPORT": "0",
    }
    environment.pop("BINGUX_SETTINGS_HELPER", None)
    input_helper = repo / "tests/customise-native-input.py"

    def call(method):
        result = subprocess.run(
            [qs, "-p", str(fixture), "ipc", "call", "reload-test", method],
            env=environment,
            capture_output=True,
            text=True,
            timeout=8,
        )
        return result.stdout.strip()

    def wait_for(predicate):
        deadline = time.monotonic() + 12
        last_response = ""
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise AssertionError(f"Shell exited with {process.returncode}")
            last_response = call("status")
            try:
                state = json.loads(last_response)
                if predicate(state):
                    return state
            except (ValueError, KeyError):
                pass
            time.sleep(0.15)
        raise AssertionError(f"Shell did not reach the expected state: {last_response}")

    stage_compositor_bridge(repo, os.environ["XDG_CONFIG_HOME"])
    subprocess.run(["python3", str(input_helper), "--prepare"], env=environment, check=True)
    with (fixture / "runtime.log").open("w+") as log:
        process = subprocess.Popen(
            [qs, "-p", str(fixture), "--no-color"],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        try:
            initial = wait_for(lambda state: state["ready"])
            call("media")
            wait_for(lambda state: state["sidebarOpen"] and state["mediaCount"] == 1)
            time.sleep(1)
            shell.write_text(source.replace("RELOAD_REVISION", "1"))
            state = wait_for(lambda state: state["revision"] == 1 and state["ready"])
            time.sleep(2)
            state = wait_for(lambda state: state["revision"] == 1 and state["ready"])
            assert state["pid"] == initial["pid"], "A reload before opening the editor restarted Quickshell"
            for revision in range(2, 5):
                call("open")
                state = wait_for(lambda state: state["visible"] and state["clock"])
                # Allow native surfaces to commit their new input regions.
                time.sleep(1.5)
                state = wait_for(lambda state: state["visible"] and state["clock"])
                print("DRAG", revision, state, flush=True)
                subprocess.run(
                    [
                        "python3",
                        str(input_helper),
                        str(state["x"]),
                        str(state["y"]),
                        "--drag-to",
                        str(state["dx"]),
                        str(state["dy"]),
                    ],
                    env=environment,
                    check=True,
                )
                wait_for(lambda state: not state["clock"] and not state["drag"])
                if revision == 3:
                    call("cancel")
                    wait_for(lambda state: not state["visible"] and state["clock"])
                shell.write_text(source.replace("RELOAD_REVISION", str(revision)))
                state = wait_for(lambda state: state["revision"] == revision and state["ready"])
                # Include deferred destruction and the transient reload notice.
                time.sleep(2)
                state = wait_for(lambda state: state["revision"] == revision and state["ready"])
                assert state["pid"] == initial["pid"], "Reload restarted Quickshell"
                assert state["clock"] and not state["visible"], "Reload did not restore the saved layout"
            for revision, action, opened, detached in (
                (5, "hideSidebar", False, False),
                (6, "detachSidebar", True, True),
            ):
                call("media")
                wait_for(lambda state: state["mediaCount"] == 1)
                call(action)
                print("SIDEBAR", revision, action, flush=True)
                time.sleep(1)
                shell.write_text(source.replace("RELOAD_REVISION", str(revision)))
                state = wait_for(lambda state: state["revision"] == revision and state["ready"])
                time.sleep(2)
                state = wait_for(lambda state: state["revision"] == revision and state["ready"])
                assert state["pid"] == initial["pid"], f"{action} reload restarted Quickshell"
                assert state["sidebarOpen"] == opened and state["detached"] == detached, state
            call("open")
            wait_for(lambda state: state["visible"] and state["detached"])
            time.sleep(1)
            shell.write_text(source.replace("RELOAD_REVISION", "7"))
            state = wait_for(lambda state: state["revision"] == 7 and state["ready"])
            time.sleep(2)
            state = wait_for(lambda state: state["revision"] == 7 and state["ready"])
            assert state["pid"] == initial["pid"], "Reloading the detached sidebar editor restarted Quickshell"
            assert not state["visible"] and state["sidebarOpen"] and state["detached"], state
            log.flush()
            log.seek(0)
            output = log.read()
            assert not any(
                error in output
                for error in ("has crashed", "TypeError", "ReferenceError", "Cannot use same item on different windows")
            ), output
            print(
                "PASS: populated media sidebar (open, closed, detached) and three native editor drags survive reload with saved layout and IPC"
            )
        except BaseException:
            log.flush()
            log.seek(0)
            print(log.read())
            raise
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=5)
