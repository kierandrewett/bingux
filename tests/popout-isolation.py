#!/usr/bin/env python3
"""Real Super/Alt+Tab and fullscreen tests, inside a private Gnoblin session.

Run through gnoblin/scripts/run-gnome-shell.sh with this file as
GNOBLIN_TEST_DBUS_CLIENT and QUICKSHELL_BIN set to the matching Qt wrapper.
"""
import ast
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

assert os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-')
assert os.environ.get('GNOBLIN_COMPOSITOR_SOCKET', '').startswith('/tmp/')
repo = Path(__file__).resolve().parents[1]
gnoblin = repo.parent / 'gnoblin'
config = Path(os.environ['XDG_CONFIG_HOME'])
scripts = config / 'gnoblin/scripts'
scripts.mkdir(parents=True, exist_ok=True)
shutil.copy2(gnoblin / 'src/scripts/compositor-bridge.js', scripts)
shutil.copytree(gnoblin / 'src/scripts/lib', scripts / 'lib', dirs_exist_ok=True)
qs = os.environ.get('QUICKSHELL_BIN', 'qs')
qml = config / 'quickshell/bingux'
shutil.copytree(repo / 'shell/bingux', qml, dirs_exist_ok=True)
ctl = [str(repo / 'packages/binguxctl/binguxctl.py'), '--quickshell', qs, '--path', str(qml)]
# Some development sessions predate the native command-shortcut registrar.
compat = os.environ.get('GNOBLIN_SHORTCUT_COMPAT')
if compat:
    shutil.copy2(compat, scripts / 'config-shortcuts.js')
(config / 'gnoblin/gnoblin.toml').write_text('[shell]\nwindow-switcher = false\n[[shortcuts]]\nname = "search"\nbinding = "Super"\ncapture-input = true\ncommand = ' + json.dumps(ctl + ['search', 'toggle']) + '\n')
(scripts / 'popout-test.js').write_text('''
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
export default function enable(api) {
 const keyboard = global.stage.context.get_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const settings = new Gio.Settings({schema_id: 'org.gnome.desktop.wm.keybindings'});
 for (const key of ['switch-applications', 'switch-applications-backward', 'switch-windows', 'switch-windows-backward']) settings.set_strv(key, []);
 const windows = () => global.display.list_all_windows();
 const impl = Gio.DBusExportedObject.wrapJSObject(`<node><interface name="org.gnoblin.PopoutTest">
 <method name="Key"><arg type="u" direction="in"/><arg type="b" direction="in"/></method>
 <method name="State"><arg type="s" direction="out"/></method>
 <method name="Focus"><arg type="s" direction="in"/></method>
 <method name="Fullscreen"><arg type="s" direction="in"/></method>
 </interface></node>`, {
 Key(code, down) { keyboard.notify_key(GLib.get_monotonic_time(), code, down ? Clutter.KeyState.PRESSED : Clutter.KeyState.RELEASED); },
 Focus(title) { Main.activateWindow(windows().find(w => w.title === title), global.get_current_time()); },
 Fullscreen(title) { windows().find(w => w.title === title).make_fullscreen(); },
 State() { return JSON.stringify({focus: global.display.focus_window?.title,
  windows: windows().map(w => ({title: w.title, fullscreen: w.is_fullscreen()})),
  order: global.window_group.get_children().filter(a => a.meta_window && a.visible).map(a => Meta.gnoblin_layer_namespace(a.meta_window) || a.meta_window.title)}); }
 });
 impl.export(Gio.DBus.session, '/org/gnoblin/PopoutTest');
 const name = Gio.bus_own_name(Gio.BusType.SESSION, 'org.gnoblin.PopoutTest', Gio.BusNameOwnerFlags.NONE, null, null, null);
 api._disposers.push(() => { impl.unexport(); Gio.bus_unown_name(name); keyboard.run_dispose(); });
}
''')

def run(command, **kwargs):
    return subprocess.run(command, check=True, text=True, capture_output=True, timeout=5, **kwargs).stdout

def native(method, *args):
    return ast.literal_eval(run(['gdbus', 'call', '--session', '--dest', 'org.gnoblin.PopoutTest', '--object-path', '/org/gnoblin/PopoutTest', '--method', 'org.gnoblin.PopoutTest.' + method, *map(str, args)]))[0:]

def state():
    return json.loads(native('State')[0])

def ipc(target, action='status'):
    return json.loads(run(ctl + [target, action]) or '{}')

def wait(predicate, message, timeout=5):
    end = time.monotonic() + timeout
    last = None
    while time.monotonic() < end:
        try:
            last = predicate()
            if last: return last
        except (subprocess.SubprocessError, json.JSONDecodeError):
            pass
        time.sleep(.04)
    raise AssertionError((message, state(), last))

def key(code, down): native('Key', code, 'true' if down else 'false')
def tap(code): key(code, True); key(code, False)

def companions_above_fullscreen():
    order = state()['order']
    return all(name in order for name in ['Popout Fullscreen', 'bingux-top-bar', 'bingux-dock', 'bingux-search']) and all(order.index('Popout Fullscreen') < order.index(name) < order.index('bingux-search') for name in ['bingux-top-bar', 'bingux-dock'])

run([str(gnoblin / 'src/tools/gnoblinctl'), 'reload-config'])
run([str(gnoblin / 'src/tools/gnoblinctl'), 'reload-scripts'])
apps_qml = config / 'popout-apps.qml'
apps_qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
ShellRoot {
 id: fixture
 property string received: ""
 IpcHandler { target: "fixture"; function received(): string { return fixture.received; } }
 Window { visible: true; width: 640; height: 400; title: "Popout Other"; color: "#246824"
  Item { focus: true; Keys.onPressed: event => { fixture.received = "Popout Other:" + event.key; } }
 }
 Window { visible: true; width: 640; height: 400; title: "Popout Fullscreen"; color: "#243868"
  Item { focus: true; Keys.onPressed: event => { fixture.received = "Popout Fullscreen:" + event.key; } }
 }
}''')
processes = []
logs = []
main = None
try:
    for name, path in [('desktop', qml), ('search', qml / 'SearchShell.qml'), ('switcher', qml / 'SwitcherShell.qml'), ('apps', apps_qml)]:
        log = open('/tmp/bingux-popout-' + name + '.log', 'w')
        logs.append(log)
        process = subprocess.Popen([qs, '--path', str(path), '--no-color'], stdout=log, stderr=log)
        processes.append(process)
        if name == 'desktop': main = process
    print('Process IDs:', [p.pid for p in processes], flush=True)
    wait(lambda: ipc('switcher').get('ready'), 'switcher ready')
    wait(lambda: len([w for w in state()['windows'] if w['title'].startswith('Popout ')]) == 2, 'fixture windows')
    wait(lambda: 'bingux-dock' in state()['order'] and 'bingux-top-bar' in state()['order'], 'desktop chrome mapped')
    native('Focus', 'Popout Other')
    native('Focus', 'Popout Fullscreen')
    native('Fullscreen', 'Popout Fullscreen')
    wait(lambda: any(w['title'] == 'Popout Fullscreen' and w['fullscreen'] for w in state()['windows']), 'fullscreen')
    time.sleep(.5)
    # Stop only the main UI process. Both popouts must still handle real keys.
    main.send_signal(signal.SIGSTOP)
    for attempt in range(3):
        tap(125)
        wait(lambda: ipc('search').get('acceptingKeyboard'), 'Super opens search with frozen desktop')
        wait(companions_above_fullscreen, 'both panels above fullscreen and below search')
        if attempt == 0:
            run(['grim', '/tmp/bingux-popout-fullscreen.png'])
        tap(1)
        wait(lambda: not ipc('search').get('visible'), 'Escape closes search')
        order = state()['order']
        assert order.index('bingux-top-bar') < order.index('Popout Fullscreen'), ('restore panel order', order)
    key(56, True); tap(15)
    wait(lambda: ipc('switcher').get('shown'), 'Alt+Tab opens with frozen desktop')
    assert ipc('switcher')['selected'] == 'Popout Other'
    key(56, False)
    wait(lambda: state()['focus'] == 'Popout Other', 'release switches focus')
    key(56, True); tap(15)
    wait(lambda: ipc('switcher').get('shown'), 'second switcher open')
    tap(1); key(56, False)
    assert state()['focus'] == 'Popout Other', 'Escape keeps focus'
    # Resume and terminate the main process: popouts keep their processes and shortcuts.
    main.send_signal(signal.SIGCONT)
    main.terminate(); main.wait(timeout=5)
    tap(125)
    wait(lambda: ipc('search').get('acceptingKeyboard'), 'Super survives desktop exit')
    tap(1)
    wait(lambda: not ipc('search').get('visible'), 'close after desktop exit')
    key(56, True); tap(15)
    wait(lambda: ipc('switcher').get('shown'), 'Alt+Tab survives desktop exit')
    key(56, False)
    wait(lambda: state()['focus'] == 'Popout Fullscreen', 'focus after desktop exit')
    # The visual switcher is also optional. Exercise a stalled connection,
    # then verify that queued UI messages cannot undo the fallback selection.
    search_process, switcher_process = processes[1:3]
    switcher_process.send_signal(signal.SIGSTOP)
    previous = state()['focus']
    key(56, True); tap(15); key(56, False)
    wait(lambda: state()['focus'] != previous, 'fallback with switcher SIGSTOP', timeout=1)
    selected = state()['focus']
    switcher_process.send_signal(signal.SIGCONT)
    time.sleep(.3)
    assert state()['focus'] == selected, 'resumed UI applied stale activation'
    # Alt+Tab must also escape a search surface that has stopped responding.
    tap(125)
    wait(lambda: ipc('search').get('acceptingKeyboard'), 'open search before freezing it')
    search_process.send_signal(signal.SIGSTOP)
    switcher_process.send_signal(signal.SIGSTOP)
    key(56, True); tap(15); key(56, False)
    wait(lambda: state()['focus'] in ('Popout Other', 'Popout Fullscreen'), 'escape frozen search', timeout=1)
    focused = state()['focus']
    tap(30)  # The application must receive A while search remains frozen.
    received = run([qs, 'ipc', '--path', str(apps_qml), 'call', 'fixture', 'received']).strip()
    assert received == focused + ':65', ('keyboard delivery after fallback', focused, received)
    search_process.send_signal(signal.SIGCONT)
    switcher_process.send_signal(signal.SIGCONT)
    wait(lambda: not ipc('search').get('visible'), 'search closes after recovery')
    # No Bingux UI remains, and no client owns the registered accelerators.
    for process in (search_process, switcher_process):
        process.terminate(); process.wait(timeout=5)
    previous = state()['focus']
    key(56, True); tap(15); key(56, False)
    wait(lambda: state()['focus'] != previous, 'fallback after all UI processes exit', timeout=1)
    previous = state()['focus']
    key(56, True); tap(15); tap(1); key(56, False)
    time.sleep(.15)
    assert state()['focus'] == previous, 'fallback Escape changed focus'
    print('PASS: Super and Alt+Tab during desktop SIGSTOP and exit; fullscreen panel reveal/restoration; fallback with frozen and dead UI; stale-message rejection; Escape and modifier release')
finally:
    for process in processes:
        if process.poll() is None: process.send_signal(signal.SIGCONT)
    for code in (1, 15, 56, 125):
        try: key(code, False)
        except Exception: pass
    for process in reversed(processes):
        if process.poll() is None:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
    for log in logs: log.close()
