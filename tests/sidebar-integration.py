#!/usr/bin/env python3
"""Run with Gnoblin's isolated run-gnome-shell.sh, never on the live session."""

import fcntl
import struct
import termios
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

if sys.argv[1:] == ["--window"]:
    app_file = Path(os.environ["XDG_CONFIG_HOME"]) / "sidebar-app.qml"
    app_file.write_text("""import QtQuick
import QtQuick.Window
import Quickshell
ShellRoot {
    Window {
        visible: true; width: 400; height: 300
        title: "Sidebar work-area test"
        color: "#f4f4f4"
        Text { anchors.centerIn: parent; text: "Maximised application" }
    }
}
""")
    os.execvp("qs", ["qs", "-p", str(app_file)])

root = Path(os.environ["XDG_CONFIG_HOME"]) / "gnoblin"
if not str(root).startswith("/tmp/gnoblin-gs."):
    sys.exit("Use GNOBLIN_TEST_DBUS_CLIENT with Gnoblin scripts/run-gnome-shell.sh.")
(root / "scripts").mkdir(parents=True, exist_ok=True)
fixture = root / "sidebar-fixture"
fixture.mkdir()
source = Path(__file__).resolve().parent.parent / "shell/bingux"
for component in source.iterdir():
    if component.suffix in (".qml", ".js") or component.name == "qmldir":
        shutil.copy2(component, fixture / component.name)
sidebar_file = fixture / "TerminalSidebar.qml"
sidebar_file.write_text(
    sidebar_file.read_text().replace(
        '"file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar.ini"',
        json.dumps("file://" + str(fixture / "sidebar.ini")),
    )
)
monitor_file = fixture / "SidebarMonitor.qml"
monitor_file.write_text(
    monitor_file.read_text().replace(
        '"file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar-system.ini"',
        json.dumps("file://" + str(fixture / "system.ini")),
    )
)
notes_file = fixture / "SidebarNotes.qml"
notes_path = fixture / "notes.ini"
notes_file.write_text(
    notes_file.read_text().replace(
        '"file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar-notes.ini"',
        json.dumps("file://" + str(notes_path)),
    )
)
(fixture / "shell.qml").write_text("""//@ pragma UseQApplication

import QtQuick
import Quickshell
import Quickshell.Wayland
ShellRoot {
    TerminalSidebar {
        id: sidebar
        screen: Quickshell.screens[0]
        settings: QtObject {
            property bool sidebarEnabled: true
            property bool dockEnabled: true
        }
    }
    PanelWindow {
        margins.left: sidebar.leftInset
        margins.right: sidebar.rightInset
        implicitHeight: Theme.barHeight
        exclusiveZone: Theme.barHeight
        anchors { top: true; left: true; right: true }
        color: "#202020"
        WlrLayershell.namespace: "bingux-sidebar-test-bar"
    }
    PanelWindow {
        margins.left: sidebar.leftInset
        margins.right: sidebar.rightInset
        implicitHeight: 80
        exclusiveZone: 80
        anchors { bottom: true; left: true; right: true }
        color: "#202020"
        WlrLayershell.namespace: "bingux-sidebar-test-dock"
    }
}
""")
report = root / "sidebar-report.json"
probe = root / "scripts/sidebar-probe.js"


def snapshot(action=""):
    probe.write_text(
        """
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
function rect(r) { return {x:r.x, y:r.y, width:r.width, height:r.height}; }
export default function () {
    const seat = Clutter.get_default_backend().get_default_seat();
    Main.wm._sidebarPointer ??= seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    Main.wm._sidebarKeyboard ??= seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    ACTION
    const area = global.workspace_manager.get_active_workspace().get_work_area_for_monitor(0);
    GLib.file_set_contents(REPORT, JSON.stringify({area: rect(area),
        windows: global.get_window_actors().map(a => ({title: a.meta_window.get_title(),
            rect: rect(a.meta_window.get_frame_rect()), visible: a.visible,
            pid: a.meta_window.get_pid(), maximized: a.meta_window.is_maximized(), layer: a.meta_window.get_window_type() === Meta.WindowType.DOCK,
            focus: a.meta_window === global.display.focus_window}))}));
}
""".replace("ACTION", action).replace("REPORT", json.dumps(str(report)))
    )
    subprocess.run(["gnoblinctl", "script", "reload"], check=True, capture_output=True)
    return json.loads(report.read_text())


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
        ["qs", "-p", str(fixture), "ipc", "call", "sidebar", method, *args], capture_output=True, text=True, timeout=5
    )
    if result.returncode:
        return None
    return json.loads(result.stdout) if method == "status" else True


def move(x, y):
    snapshot(f"Main.wm._sidebarPointer.notify_absolute_motion(GLib.get_monotonic_time(), {x}, {y});")


def pointer_button(state):
    snapshot(f"Main.wm._sidebarPointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.{state});")


def click():
    pointer_button("PRESSED")
    pointer_button("RELEASED")


def terminal_size(pid):
    fd = os.open(f"/proc/{pid}/fd/0", os.O_RDONLY | os.O_NOCTTY)
    try:
        return struct.unpack("HHHH", fcntl.ioctl(fd, termios.TIOCGWINSZ, bytes(8)))[:2]
    finally:
        os.close(fd)


def work_area():
    return snapshot()["area"]


def key(keyval):
    for state in ["PRESSED", "RELEASED"]:
        snapshot(
            f"Main.wm._sidebarKeyboard.notify_keyval(GLib.get_monotonic_time(), {keyval}, Clutter.KeyState.{state});"
        )


app = subprocess.Popen([sys.executable, __file__, "--window"])
shell = None
log = (root / "sidebar.log").open("w")
try:
    wait_for(lambda: any(w["title"] == "Sidebar work-area test" for w in snapshot()["windows"]), "test window")
    snapshot(
        "global.get_window_actors().find(a => a.meta_window.get_title() === 'Sidebar work-area test').meta_window.maximize();"
    )
    move(640, 400)
    shell = subprocess.Popen(["qs", "-p", str(fixture)], stdout=log, stderr=subprocess.STDOUT)
    wait_for(lambda: ipc("status"), "sidebar IPC")
    wait_for(lambda: work_area()["y"] > 0, "test bar")
    base = work_area()
    assert not ipc("status")["open"]
    assert not ipc("status")["pid"], "helper must start only on first use"
    # The only layer surfaces initially are the bar and the edge sensor.
    assert not any(w["rect"]["height"] == 72 for w in snapshot()["windows"])
    # Click the sensor itself, outside the handle's clamped vertical position.
    move(base["width"] - 1, 5)
    click()
    assert not ipc("status")["pillVisible"], "pill stays out of top bar"
    assert not ipc("status")["open"] and work_area() == base
    assert not ipc("status")["pid"], "click must not launch a terminal"
    move(base["width"] - 6, 400)

    def button():
        return next(
            (
                w["rect"]
                for w in snapshot()["windows"]
                if w["rect"]["width"] == 24
                and w["rect"]["height"] == 72
                and w["rect"]["x"] == base["width"] - 24
                and w["rect"]["y"] <= 400 < w["rect"]["y"] + 72
            ),
            None,
        )

    rect = wait_for(button, "edge hover reveals button")
    wait_for(lambda: ipc("status")["pillHovered"], "outermost edge is hovered")
    move(rect["x"] + rect["width"] // 2, rect["y"] + rect["height"] // 2)
    time.sleep(0.8)
    assert button(), "button must remain visible while hovered"
    for cycle in range(8):
        click()
        move(640, 400)
        wait_for(lambda: not ipc("status")["pillHovered"], "leave after pulse click")
        time.sleep(0.85)
        move(base["width"] - 1, 400)
        wait_for(
            lambda: ipc("status")["pillVisible"] and ipc("status")["pillHovered"], f"edge re-entry {cycle} enters hover"
        )
    click()
    wait_for(lambda: ipc("status")["hinting"], "pill click pulses drag hint")
    assert not ipc("status")["open"] and work_area() == base
    # A first drag must not initialise a shell at a transient narrow width.
    for extent in [149, 150]:
        move(base["width"] - 1, 400)
        time.sleep(0.25)
        pointer_button("PRESSED")
        move(base["width"] - 1 - extent, 400)
        wait_for(lambda: ipc("status")["dragging"], "first-use drag")
        assert not ipc("status")["ready"] and not ipc("status")["pid"], "shell waits for release"
        pointer_button("RELEASED")
        if extent < 150:
            wait_for(lambda: work_area() == base, "aborted first drag closes")
            assert not ipc("status")["ready"], "aborted drag never creates terminal"
    wait_for(lambda: ipc("status")["open"], "valid release opens sidebar")
    shell_pid = wait_for(lambda: ipc("status")["pid"], "terminal shell pid")
    assert ipc("status")["headerHeight"] == base["y"], "sidebar header aligns with the top bar"
    content = ipc("status")
    assert content["terminalTransparent"], "terminal background is transparent"
    move(content["contentX"] + 48, content["contentY"] + 16)
    click()
    wait_for(lambda: ipc("status")["menuOpen"], "content dropdown")
    time.sleep(0.25)
    subprocess.run(["grim", "/tmp/bingux-sidebar-dropdown.png"], check=True)
    key(0xFF54)
    key(0xFF54)
    key(0xFF0D)
    wait_for(lambda: ipc("status")["contentType"] == "notes", "notes view")
    for character in "Sidebar note":
        key(ord(character))
    wait_for(lambda: notes_path.exists() and "Sidebar note" in notes_path.read_text(), "notes saved to disk")
    subprocess.run(["grim", "/tmp/bingux-sidebar-notes.png"], check=True)
    move(content["contentX"] + 32, content["contentY"] + 16)
    click()
    wait_for(lambda: ipc("status")["menuOpen"], "reopen content dropdown")
    time.sleep(0.25)
    move(content["contentX"] + 32, content["contentY"] + content["headerHeight"] + 92)
    click()
    wait_for(lambda: ipc("status")["contentType"] == "monitor", "system view")
    assert ipc("status")["pid"] == shell_pid, "switching content must preserve the shell"
    subprocess.run(["grim", "/tmp/bingux-sidebar-monitor.png"], check=True)
    ipc("select", "terminal")
    assert ipc("status")["pid"] == shell_pid, "returning to terminal keeps its session"

    wait_for(lambda: work_area()["width"] < base["width"], "exclusive work area")
    wait_for(lambda: any(w["focus"] and w["layer"] for w in snapshot()["windows"]), "terminal focus")
    time.sleep(0.3)
    sidebar_rect = next(w["rect"] for w in snapshot()["windows"] if w["focus"] and w["layer"])
    assert sidebar_rect["y"] == 0 and sidebar_rect["height"] == 800, (
        f"Sidebar must cover bar and dock areas: {sidebar_rect}"
    )
    subprocess.run(["grim", "/tmp/bingux-sidebar-right.png"], check=True)
    resize_failures = []
    for edge in ["top", "left", "right"]:
        assert ipc("edge", edge)
        axis, other = ("height", "width") if edge == "top" else ("width", "height")
        ipc("hide")
        wait_for(lambda: work_area() == base, f"{edge} hidden before drag")
        move(640, 400)
        time.sleep(0.3)
        start_x, start_y = (640, 0) if edge == "top" else (0 if edge == "left" else base["width"] - 1, 400)
        move(start_x, start_y)
        time.sleep(0.25)
        # Right starts inside the visible handle; left/top exercise the exact edge.
        if edge == "right":
            start_x -= 10
            move(start_x, start_y)

        def pull(distance):
            move(
                start_x + (distance if edge == "left" else -distance if edge == "right" else 0),
                start_y + (distance if edge == "top" else 0),
            )

        pointer_button("PRESSED")
        previous_distance, previous_corner = 0, 0
        for distance in [25, 80, 140, 220, 160, 60]:
            pull(distance)
            time.sleep(0.04)
            if edge == "right" and distance in [80, 220, 160]:
                subprocess.run(["grim", f"/tmp/bingux-sidebar-drag-{distance}.png"], check=True)
            geometry = ipc("status")
            assert (geometry["cornerSize"] > previous_corner) == (distance > previous_distance), (
                "desktop corner follows the drag size"
            )
            previous_distance, previous_corner = distance, geometry["cornerSize"]
            assert geometry["surfaceWidth"] == 1280 and geometry["surfaceHeight"] == (
                800 - geometry["headerHeight"] if edge == "top" else 800
            ), f"{edge} backing surface must remain stable: {geometry}"
        pull(60)
        wait_for(lambda: ipc("status")["dragging"], f"{edge} follows held pointer")
        wait_for(lambda: 30 < base[axis] - work_area()[axis] < 90, f"{edge} reserves only the dragged distance")
        expected = base["y"] + 60 if edge == "top" else 60 if edge == "left" else base["width"] - 60 - 24
        assert any(
            (w["rect"]["width"], w["rect"]["height"]) == ((72, 24) if edge == "top" else (24, 72))
            and abs(w["rect"]["y" if edge == "top" else "x"] - expected) <= 2
            for w in snapshot()["windows"]
        ), f"{edge} visual handle follows panel"
        if edge != "top":
            for height in [base["y"], 80]:
                chrome = next(w["rect"] for w in snapshot()["windows"] if w["layer"] and w["rect"]["height"] == height)
                assert abs(chrome["width"] - (base["width"] - 60)) <= 2, f"{edge} chrome resizes: {chrome}"
                assert abs(chrome["x"] - (60 if edge == "left" else 0)) <= 2, f"{edge} chrome moves: {chrome}"
        pull(149)
        pointer_button("RELEASED")
        wait_for(lambda: work_area() == base and not ipc("status")["dragging"], f"{edge} release below 150px closes")
        assert not ipc("status")["open"]
        wait_for(
            lambda: next(w["rect"] for w in snapshot()["windows"] if w["title"] == "Sidebar work-area test") == base,
            f"{edge} close restores original application geometry",
        )
        for height in [base["y"], 80]:
            chrome = next(w["rect"] for w in snapshot()["windows"] if w["layer"] and w["rect"]["height"] == height)
            assert chrome["width"] == base["width"] and chrome["x"] == 0, "close restores chrome geometry"
        move(640, 400)
        time.sleep(0.3)
        move(start_x if edge != "right" else base["width"] - 1, start_y)
        time.sleep(0.25)
        move(start_x, start_y)
        pointer_button("PRESSED")
        pull(300)
        wait_for(lambda: ipc("status")["dragging"], f"{edge} second drag")
        pointer_button("RELEASED")
        wait_for(lambda: ipc("status")["open"] and ipc("status")["reveal"] == 1, f"{edge} release keeps panel open")
        wait_for(lambda: abs(base[axis] - work_area()[axis] - 300) <= 2, f"{edge} release preserves dragged size")
        wait_for(
            lambda: work_area()[axis] < base[axis] and work_area()[other] == base[other],
            f"{edge} reserves only its own edge",
        )
        time.sleep(0.8)
        # The open resize edge follows pointer movement without a button held.
        for cross in [200, 500, 350]:
            edge_x = cross if edge == "top" else 320 if edge == "left" else base["width"] - 321
            edge_y = 354 if edge == "top" else cross
            move(edge_x, edge_y)
            time.sleep(0.22)
            assert any(
                (w["rect"]["width"], w["rect"]["height"]) == ((72, 24) if edge == "top" else (24, 72))
                and abs(w["rect"]["x" if edge == "top" else "y"] - (cross - 36)) <= 2
                for w in snapshot()["windows"]
            ), f"{edge} open pill follows unpressed pointer"
        # The threshold applies once per approach, then even small moves track continuously.
        move(600, 600)
        time.sleep(0.8)
        for cross, expected_center in [(410, 350), (460, 460), (465, 465), (470, 470), (463, 463)]:
            move(
                cross if edge == "top" else 300 if edge == "left" else base["width"] - 301,
                base["y"] + 300 if edge == "top" else cross,
            )
            time.sleep(0.22)
            assert any(
                (w["rect"]["width"], w["rect"]["height"]) == ((72, 24) if edge == "top" else (24, 72))
                and abs(w["rect"]["x" if edge == "top" else "y"] - (expected_center - 36)) <= 2
                for w in snapshot()["windows"]
            ), f"{edge} follow threshold"
        grip = next(
            w["rect"]
            for w in snapshot()["windows"]
            if (w["rect"]["width"], w["rect"]["height"]) == ((72, 24) if edge == "top" else (24, 72))
        )
        start_x, start_y = grip["x"] + grip["width"] // 2, grip["y"] + grip["height"] // 2
        for right_corner in [False, True]:
            for bottom_corner in [False, True]:
                move(500, 500)
                wait_for(lambda: not ipc("status")["pillHovered"], f"{edge} pointer leaves pill")
                grip = next(
                    w["rect"]
                    for w in snapshot()["windows"]
                    if (w["rect"]["width"], w["rect"]["height"]) == ((72, 24) if edge == "top" else (24, 72))
                )
                move(
                    grip["x"] + (grip["width"] - 1 if right_corner else 1),
                    grip["y"] + (grip["height"] - 1 if bottom_corner else 1),
                )
                wait_for(lambda: ipc("status")["pillHovered"], f"{edge} hit-area corner is hovered")
        grip = next(
            w["rect"]
            for w in snapshot()["windows"]
            if (w["rect"]["width"], w["rect"]["height"]) == ((72, 24) if edge == "top" else (24, 72))
        )
        start_x, start_y = grip["x"] + grip["width"] // 2, grip["y"] + grip["height"] // 2
        move(start_x, start_y)
        click()
        wait_for(lambda: ipc("status")["hinting"], f"{edge} open pill click pulses")
        assert ipc("status")["open"] and abs(base[axis] - work_area()[axis] - 300) <= 2, "click must preserve open size"
        old_terminal_size = terminal_size(shell_pid)
        pointer_button("PRESSED")
        if edge != "top":
            pull(800)
            wait_for(lambda: base[axis] - work_area()[axis] == 384, f"{edge} width capped at 30 percent")
        for distance in [40, 100, 60, 80]:
            pull(distance)
            time.sleep(0.04)
        wait_for(
            lambda: abs(base[axis] - work_area()[axis] - 380) <= 2,
            f"{edge} persistent grip resizes relative to existing size",
        )
        terminal_axis = 0 if edge == "top" else 1
        wait_for(
            lambda: terminal_size(shell_pid)[terminal_axis] > old_terminal_size[terminal_axis],
            f"{edge} terminal grid resizes before pointer release",
        )
        cross_shift = 70
        move(
            start_x + (cross_shift if edge == "top" else 80 if edge == "left" else -80),
            start_y + (80 if edge == "top" else cross_shift),
        )
        time.sleep(0.22)
        expected_cross = (start_x if edge == "top" else start_y) + cross_shift - 36
        wait_for(
            lambda: any(
                (w["rect"]["width"], w["rect"]["height"]) == ((72, 24) if edge == "top" else (24, 72))
                and abs(w["rect"]["x" if edge == "top" else "y"] - expected_cross) <= 2
                for w in snapshot()["windows"]
            ),
            f"{edge} pill follows cross-axis movement",
        )
        assert abs(base[axis] - work_area()[axis] - 380) <= 2, "cross-axis movement must not change extent"
        if edge != "top":
            drag_x = start_x + (80 if edge == "left" else -80)
            for blocked_y in [10, 790]:
                move(drag_x, blocked_y)
                wait_for(lambda: not ipc("status")["pillVisible"], f"{edge} pill hides over reserved chrome")
                wait_for(lambda: ipc("status")["pillOpacity"] == 0, f"{edge} boundary fade completes")
            move(drag_x, start_y + cross_shift)
            wait_for(lambda: ipc("status")["pillVisible"], f"{edge} pill returns to usable area")

        pointer_button("RELEASED")
        wait_for(
            lambda: not ipc("status")["dragging"] and abs(base[axis] - work_area()[axis] - 380) <= 2,
            f"{edge} resize release keeps new size",
        )
        time.sleep(0.3)
        grip = next(
            w["rect"]
            for w in snapshot()["windows"]
            if (w["rect"]["width"], w["rect"]["height"]) == ((72, 24) if edge == "top" else (24, 72))
        )
        start_x, start_y = grip["x"] + grip["width"] // 2, grip["y"] + grip["height"] // 2
        move(start_x, start_y)
        pointer_button("PRESSED")
        pull(-231)
        pointer_button("RELEASED")
        wait_for(lambda: not ipc("status")["open"] and work_area() == base, f"{edge} resize below 150px closes")
        ipc("open")
        wait_for(lambda: abs(base[axis] - work_area()[axis] - 380) <= 2, f"{edge} reopen remembers resized extent")
        time.sleep(0.4)
        app_rect = next(w["rect"] for w in snapshot()["windows"] if w["title"] == "Sidebar work-area test")
        if app_rect[axis] >= base[axis]:
            resize_failures.append(edge)
        assert ipc("status")["pid"] == shell_pid, "moving restarted the shell"
        if edge == "top":
            subprocess.run(["grim", "/tmp/bingux-sidebar-top.png"], check=True)
    # Actual keyboard input proves the embedded terminal is usable, rather than merely alive.
    marker = Path(f"/tmp/bingux-sidebar-command-{os.getpid()}")
    for character in f"touch {marker}":
        key(ord(character))
    key(0xFF0D)  # Return
    wait_for(lambda: marker.exists(), "terminal command")
    marker.unlink()
    assert ipc("hide")
    wait_for(lambda: work_area() == base, "hide releases space")
    assert ipc("status")["pid"] == shell_pid, "hiding restarted the shell"
    ipc("open")
    wait_for(lambda: ipc("status")["open"], "reopen")
    assert ipc("status")["pid"] == shell_pid, "reopening restarted the shell"
    os.kill(shell_pid, 9)
    wait_for(lambda: not ipc("status")["running"], "shell exit state")
    assert ipc("status")["open"], "shell exit must leave restart controls visible"
    panel_rect = next(w["rect"] for w in snapshot()["windows"] if w["focus"] and w["layer"])
    time.sleep(0.3)
    subprocess.run(["grim", "/tmp/bingux-sidebar-exited.png"], check=True)
    content = ipc("status")
    move(content["contentX"] + 52, content["contentY"] + content["contentHeight"] - 30)
    click()
    wait_for(lambda: ipc("status")["running"] and ipc("status")["pid"] != shell_pid, "New shell button")
    for character in "exit":
        key(ord(character))
    key(0xFF0D)
    wait_for(lambda: not ipc("status")["running"], "normal shell exit")
    ipc("hide")
    wait_for(lambda: work_area() == base, "release space after shell exit")
    ipc("select", "notes")
    ipc("edge", "left")
    ipc("open")
    wait_for(lambda: ipc("status")["open"] and ipc("status")["contentType"] == "notes", "notes before restart")
    time.sleep(0.35)
    restored_area = work_area()
    saved_notes = notes_path.read_text()
    shell.terminate()
    shell.wait(timeout=5)
    shell = subprocess.Popen(["qs", "-p", str(fixture)], stdout=log, stderr=subprocess.STDOUT)
    wait_for(lambda: ipc("status") and ipc("status")["open"], "open state restored after restart")
    assert ipc("status")["edge"] == "left" and ipc("status")["contentType"] == "notes"
    wait_for(lambda: work_area() == restored_area, "restored sidebar size and reservation")
    assert not ipc("status")["pid"], "restoring notes must not start a terminal"
    assert notes_path.read_text() == saved_notes, "notes survive restart unchanged"
    # Saving an edit after reload must retain the earlier document contents.
    content = ipc("status")
    move(content["contentX"] + 32, content["contentY"] + content["headerHeight"] + 24)
    click()
    key(0xFF57)  # End: append without splitting the saved text.
    key(ord("!"))
    wait_for(
        lambda: "!" in notes_path.read_text() and "Sidebar note" in notes_path.read_text(),
        "restored notes remain editable",
    )
    ipc("hide")
    wait_for(lambda: work_area() == base, "hide after restore")
    shell.terminate()
    shell.wait(timeout=5)
    shell = subprocess.Popen(["qs", "-p", str(fixture)], stdout=log, stderr=subprocess.STDOUT)
    wait_for(lambda: ipc("status"), "closed state restart")
    time.sleep(0.25)
    assert not ipc("status")["open"], "closed sidebar stays closed after restart"
    print(
        "PASS: click hints, stable backing surface, persistent resize grip, all edges, chrome/window restoration, terminal input, session persistence and shell restart"
    )
    assert not resize_failures, f"Compositor did not resize the maximised application on: {resize_failures}"
finally:
    for process in (shell, app):
        if process and process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
    log.close()
    client_log = (root / "sidebar.log").read_text()
    print(client_log)
    assert "TypeError:" not in client_log and "Failed to load configuration" not in client_log, "QML runtime error"
