#!/usr/bin/env python3
"""Run with Gnoblin's isolated run-gnome-shell.sh, never on the live session."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

if sys.argv[1:] == ['--window']:
    app_file = Path(os.environ['XDG_CONFIG_HOME']) / 'popup-focus-app.qml'
    app_file.write_text("""import QtQuick
import QtQuick.Window
import Quickshell
ShellRoot {
    Window {
        visible: true; width: 400; height: 300
        title: "Popup focus test"
        color: "#f4f4f4"
        Text { anchors.centerIn: parent; text: "Maximised application" }
    }
}
""")
    os.execvp('qs', ['qs', '-p', str(app_file)])

root = Path(os.environ['XDG_CONFIG_HOME']) / 'gnoblin'
if not str(root).startswith('/tmp/gnoblin-gs.'):
    sys.exit('Use GNOBLIN_TEST_DBUS_CLIENT with Gnoblin scripts/run-gnome-shell.sh.')
(root / 'scripts').mkdir(parents=True, exist_ok=True)
fixture = root / 'popup-focus-fixture'
fixture.mkdir()
source = Path(__file__).resolve().parent.parent / 'shell/bingux'
for name in ('Theme.qml', 'ShellPopup.qml', 'MenuNavigator.qml'):
    shutil.copy2(source / name, fixture / name)
(fixture / 'qmldir').write_text('singleton Theme 1.0 Theme.qml\nShellPopup 1.0 ShellPopup.qml\nMenuNavigator 1.0 MenuNavigator.qml\n')
report = root / 'popup-focus-report.json'
probe = root / 'scripts/popup-focus-probe.js'


def snapshot(action=''):
    probe.write_text('''
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
function rect(r) { return {x:r.x, y:r.y, width:r.width, height:r.height}; }
export default function () {
    const seat = Clutter.get_default_backend().get_default_seat();
    Main.wm._popupPointer ??= seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    Main.wm._popupKeyboard ??= seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    ACTION
    const area = global.workspace_manager.get_active_workspace().get_work_area_for_monitor(0);
    GLib.file_set_contents(REPORT, JSON.stringify({focusTrace: Main.wm._popupFocus ?? [], area: rect(area),
        windows: global.get_window_actors().map(a => ({title: a.meta_window.get_title(),
            rect: rect(a.meta_window.get_frame_rect()), visible: a.visible,
            pid: a.meta_window.get_pid(), maximized: a.meta_window.is_maximized(), layer: a.meta_window.get_window_type() === Meta.WindowType.DOCK,
            focus: a.meta_window === global.display.focus_window}))}));
}
'''.replace('ACTION', action).replace('REPORT', json.dumps(str(report))))
    subprocess.run(['gnoblinctl', 'reload-scripts'], check=True, capture_output=True)
    return json.loads(report.read_text())


def wait_for(operation, description):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        result = operation()
        if result:
            return result
        time.sleep(.1)
    raise AssertionError(f'{description}: {snapshot()}')


def ipc(method, *args):
    result = subprocess.run(['qs', '-p', str(fixture), 'ipc', 'call', 'popup', method, *args],
                            capture_output=True, text=True, timeout=5)
    if result.returncode:
        return None
    return json.loads(result.stdout) if method == 'status' else True


def move(x, y):
    snapshot(f'Main.wm._popupPointer.notify_absolute_motion(GLib.get_monotonic_time(), {x}, {y});')


def pointer_button(state):
    snapshot('Main.wm._popupPointer.notify_button(GLib.get_monotonic_time(), '
             f'1, Clutter.ButtonState.{state});')


def click():
    pointer_button('PRESSED')
    pointer_button('RELEASED')


def key(keyval):
    for state in ['PRESSED', 'RELEASED']:
        snapshot('Main.wm._popupKeyboard.notify_keyval(GLib.get_monotonic_time(), '
                 f'{keyval}, Clutter.KeyState.{state});')



(fixture / 'shell.qml').write_text("""import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
    property int activations: 0
    property int mouseActivations: 0
    ShellPopup {
        id: popup; popupWidth: 240; popupHeight: 140
        onVisibleChanged: if (visible) navigation.focusMenu()
        MenuNavigator {
            id: navigation
            entries: [first, second]
            onEscapeRequested: popup.visible = false
            onActivateRequested: { activations++; popup.visible = false; }
        }
        Item {
            id: first; property bool menuEntry: true; Keys.forwardTo: [navigation]
            width: parent.width; height: 40
            MouseArea {
                id: pointerTarget; anchors.fill: parent; hoverEnabled: true
                onClicked: mouseActivations++
            }
        }
        Item { id: second; property bool menuEntry: true; Keys.forwardTo: [navigation] }
    }
    IpcHandler {
        target: "popup"
        function open(): void { popup.visible = true; }
        function reopen(): void { popup.visible = false; popup.visible = true; }
        function status(): string { return JSON.stringify({open: popup.visible, index: navigation.currentIndex, activations, mouseActivations, hovered: pointerTarget.containsMouse}); }
    }
}
""")
app = subprocess.Popen([sys.executable, __file__, '--window'])
shell = None
try:
    wait_for(lambda: any(w['title'] == 'Popup focus test' for w in snapshot()['windows']), 'application')
    snapshot("global.get_window_actors().find(a => a.meta_window.get_title() === 'Popup focus test').meta_window.activate(global.get_current_time());")
    shell = subprocess.Popen(['qs', '-p', str(fixture)])
    wait_for(lambda: ipc('status'), 'popup IPC')
    snapshot("Main.wm._popupFocus = []; global.display.connect('notify::focus-window', () => Main.wm._popupFocus.push(global.display.focus_window?.get_title() ?? 'null'));")
    ipc('open')
    time.sleep(.4)
    move(900, 600)
    click()
    time.sleep(.4)
    result = snapshot()
    trace = result['focusTrace']
    print('FOCUS_TRACE:', trace)
    assert not trace, 'menu keyboard routing must leave the application focused throughout'
    assert not ipc('status')['open'], 'outside click closes menu'
    ipc('open')
    time.sleep(.3)
    ipc('reopen')
    time.sleep(.2)
    key(0xff54)
    wait_for(lambda: ipc('status')['index'] == 0, 'Down works immediately without a menu click')
    key(0xff54)
    wait_for(lambda: ipc('status')['index'] == 1, 'Down selects next entry')
    key(0xff52)
    wait_for(lambda: ipc('status')['index'] == 0, 'Up selects previous entry')
    key(0xff0d)
    wait_for(lambda: ipc('status')['activations'] == 1 and not ipc('status')['open'], 'Enter activates selected entry')
    ipc('open')
    time.sleep(.3)
    wait_for(lambda: any(w['title'] == 'Popup focus test' and w['focus'] for w in snapshot()['windows']), 'app remains focused while menu receives keys')
    key(0xff1b)
    wait_for(lambda: not ipc('status')['open'], 'Escape closes focused menu')
    wait_for(lambda: any(w['title'] == 'Popup focus test' and w['focus'] for w in snapshot()['windows']), 'Escape restores application focus')
    ipc('open')
    time.sleep(.3)
    for cycle in range(8):
        move(640, 74)
        wait_for(lambda: ipc('status')['hovered'], 'menu hover before leaving')
        move(900, 600)
        wait_for(lambda: not ipc('status')['hovered'], 'pointer leaves menu')
        time.sleep(.15)
        move(640, 74)
        wait_for(lambda: ipc('status')['hovered'], f'menu hover returns on cycle {cycle}')
        click()
        wait_for(lambda: ipc('status')['mouseActivations'] == cycle + 1, 'returned pointer activates menu')
    move(900, 600)
    click()
    wait_for(lambda: not ipc('status')['open'], 'outside dismissal still works after pointer re-entry')
    assert not snapshot()['focusTrace'], 'keyboard and pointer menu interaction never deactivates the app'
    ipc('open')
    time.sleep(.2)
    shell.terminate()
    shell.wait(timeout=5)
    shell = None
    wait_for(lambda: not any(w['layer'] for w in snapshot()['windows']), 'open menu teardown removes surfaces')
    assert not snapshot()['focusTrace'], 'destroying an open menu preserves application focus'
    print('POPUP_FOCUS_PASSED')
finally:
    if shell: shell.terminate(); shell.wait(timeout=5)
    app.terminate(); app.wait(timeout=5)
