#!/usr/bin/env python3
"""Click, type and drag through a persistent private compositor input service."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

config = Path(os.environ['BINGUX_TEST_COMPOSITOR_CONFIG'])
if not str(config).startswith('/tmp/gnoblin-gs.') or not os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-'):
    raise SystemExit('Only the private Gnoblin test session can receive test input')
prepare = sys.argv[1:] == ['--prepare']
x, y = (300, 300) if prepare else map(float, sys.argv[1:3])
destination = list(map(float, sys.argv[4:6])) if sys.argv[3:4] == ['--drag-to'] else None
probe = config / 'gnoblin/scripts/bingux-customise-input.js'
completion = probe.with_suffix('.done')
completion.unlink(missing_ok=True)
if prepare:
    probe.parent.mkdir(parents=True, exist_ok=True)
    probe.write_text('''
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Shell from 'gi://Shell';
export default function (api) {
    const seat = global.stage.context.get_backend().get_default_seat();
    const pointer = seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    const keyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    let timer = 0;
    const screenshot = (path, cursor) => new Shell.Screenshot().screenshot(cursor,
        Gio.File.new_for_path(path).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null));
    const service = Gio.DBusExportedObject.wrapJSObject(
        '<node><interface name="org.gnoblin.CustomiseInput"><method name="Run"><arg type="s" direction="in"/></method></interface></node>', {
        Run(request) {
            if (timer) throw new Error('An input gesture is already running');
            const {origin, destination, button, shift, prepare, hoverOnly, clickOnly, capture, captureOnly, dragCapture, windowTitle, windowSize, resizeTo} = JSON.parse(request);
            if (windowTitle) {
                const matches = global.get_window_actors().filter(actor => actor.meta_window.title === windowTitle);
                if (matches.length !== 1) throw new Error('Expected one test window: ' + windowTitle);
                // Qt's content excludes its title bar. The compositor frame
                // excludes buffer shadows and includes that title bar.
                const rect = matches[0].meta_window.get_frame_rect();
                origin[0] += rect.x + (rect.width - windowSize[0]) / 2;
                origin[1] += rect.y + rect.height - windowSize[1];
                if (resizeTo) matches[0].meta_window.move_resize_frame(true, rect.x, rect.y,
                    resizeTo[0] + rect.width - windowSize[0], resizeTo[1] + rect.height - windowSize[1]);
            }
            // A second move clears the initial screen-edge barrier in the headless seat.
            const actions = captureOnly || resizeTo ? [] : [
                () => pointer.notify_absolute_motion(GLib.get_monotonic_time(), origin[0], origin[1]),
                () => pointer.notify_absolute_motion(GLib.get_monotonic_time(), origin[0], origin[1])
            ];
            if (!prepare && !hoverOnly && !captureOnly && !resizeTo) {
                if (shift) actions.push(() => keyboard.notify_keyval(GLib.get_monotonic_time(), Clutter.KEY_Shift_L, Clutter.KeyState.PRESSED));
                actions.push(() => pointer.notify_button(GLib.get_monotonic_time(), button, Clutter.ButtonState.PRESSED));
                if (destination) {
                    actions.push(() => pointer.notify_absolute_motion(GLib.get_monotonic_time(), origin[0] + Math.sign(destination[0] - origin[0]) * 12, origin[1] + Math.sign(destination[1] - origin[1]) * 12));
                    for (let step = 1; step <= 8; step++) {
                        const fraction = step / 8;
                        actions.push(() => pointer.notify_absolute_motion(GLib.get_monotonic_time(), origin[0] + (destination[0] - origin[0]) * fraction, origin[1] + (destination[1] - origin[1]) * fraction));
                        if (step === 4 && dragCapture) actions.push(() => screenshot(dragCapture, true));
                    }
                }
                actions.push(() => pointer.notify_button(GLib.get_monotonic_time(), button, Clutter.ButtonState.RELEASED));
                if (shift) actions.push(() => keyboard.notify_keyval(GLib.get_monotonic_time(), Clutter.KEY_Shift_L, Clutter.KeyState.RELEASED));
                for (const character of destination || clickOnly ? '' : 'Find') {
                    actions.push(() => keyboard.notify_keyval(GLib.get_monotonic_time(), character.charCodeAt(0), Clutter.KeyState.PRESSED));
                    actions.push(() => keyboard.notify_keyval(GLib.get_monotonic_time(), character.charCodeAt(0), Clutter.KeyState.RELEASED));
                }
            }
            if (!prepare && capture) actions.push(() => screenshot(capture, false));
            let pending = false;
            let failure = '';
            timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 80, () => {
                if (pending) return GLib.SOURCE_CONTINUE;
                try {
                    const result = actions.length ? actions.shift()() : null;
                    if (result && typeof result.then === 'function') {
                        pending = true;
                        result.catch(error => { failure = String(error); actions.length = 0; })
                            .finally(() => { pending = false; });
                    }
                } catch (error) { failure = String(error); actions.length = 0; }
                if (actions.length || pending) return GLib.SOURCE_CONTINUE;
                timer = 0;
                GLib.file_set_contents(__COMPLETION__, failure || 'done');
                return GLib.SOURCE_REMOVE;
            });
        }
    });
    service.export(Gio.DBus.session, '/org/gnoblin/CustomiseInput');
    const name = Gio.bus_own_name(Gio.BusType.SESSION, 'org.gnoblin.CustomiseInput', Gio.BusNameOwnerFlags.NONE, null, null, null);
    api._disposers.push(() => {
        if (timer) GLib.source_remove(timer);
        service.unexport();
        Gio.bus_unown_name(name);
        pointer.run_dispose();
        keyboard.run_dispose();
    });
}
'''.replace('__COMPLETION__', json.dumps(str(completion))))
    subprocess.run(['gnoblinctl', 'reload-scripts'], env=os.environ | {'XDG_CONFIG_HOME': str(config)}, check=True, capture_output=True)

window_title = sys.argv[sys.argv.index('--window-title') + 1] if '--window-title' in sys.argv else ''
window_size = list(map(float, sys.argv[sys.argv.index('--window-size') + 1:sys.argv.index('--window-size') + 3])) if window_title else []
request = json.dumps({'origin': [x, y], 'destination': destination, 'prepare': prepare, 'windowTitle': window_title, 'windowSize': window_size,
    'resizeTo': list(map(float, sys.argv[4:6])) if sys.argv[3:4] == ['--resize-to'] else None,
    'hoverOnly': sys.argv[3:4] == ['--hover-only'],
    'shift': sys.argv[3:4] == ['--shift-right-click'],
    'button': 3 if sys.argv[3:4] in (['--right-click'], ['--shift-right-click']) else 1,
    'clickOnly': sys.argv[3:4] in (['--click-only'], ['--right-click'], ['--shift-right-click']),
    'capture': os.environ.get('BINGUX_NATIVE_SCREENSHOT', ''),
    'captureOnly': sys.argv[3:4] == ['--capture-only'],
    'dragCapture': os.environ.get('BINGUX_NATIVE_DRAG_CAPTURE', '')})
subprocess.run(['gdbus', 'call', '--session', '--dest', 'org.gnoblin.CustomiseInput',
    '--object-path', '/org/gnoblin/CustomiseInput', '--method', 'org.gnoblin.CustomiseInput.Run', request],
    check=True, capture_output=True, text=True, timeout=5)
deadline = time.monotonic() + 8
while not completion.exists():
    if time.monotonic() >= deadline:
        raise SystemExit('The private compositor did not complete the input gesture')
    time.sleep(.02)
if completion.read_text() != 'done':
    raise SystemExit(completion.read_text())
