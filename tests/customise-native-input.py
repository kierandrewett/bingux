#!/usr/bin/env python3
"""Click, type and drag through the private compositor's real input path."""
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
click_only = sys.argv[3:4] in (['--click-only'], ['--right-click'])
button = 3 if sys.argv[3:4] == ['--right-click'] else 1
probe = config / 'gnoblin/scripts/bingux-customise-input.js'
probe.parent.mkdir(parents=True, exist_ok=True)
completion = probe.with_suffix('.done')
completion.unlink(missing_ok=True)
probe.write_text('''
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Shell from 'gi://Shell';
export default function () {
    const seat = global.stage.context.get_backend().get_default_seat();
    global.__customisePointer ??= seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    global.__customiseKeyboard ??= seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    const pointer = global.__customisePointer, keyboard = global.__customiseKeyboard;
    const origin = __ORIGIN_POINT__;
    // A second move clears the initial screen-edge barrier in the headless seat.
    const actions = [
        () => pointer.notify_absolute_motion(GLib.get_monotonic_time(), origin[0], origin[1]),
        () => pointer.notify_absolute_motion(GLib.get_monotonic_time(), origin[0], origin[1]),
        () => pointer.notify_button(GLib.get_monotonic_time(), __BUTTON__, Clutter.ButtonState.PRESSED)
    ];
    const destination = __DRAG_DESTINATION__;
    if (destination) {
        actions.push(() => pointer.notify_absolute_motion(GLib.get_monotonic_time(), origin[0] + Math.sign(destination[0] - origin[0]) * 12, origin[1] + Math.sign(destination[1] - origin[1]) * 12));
        for (let step = 1; step <= 8; step++) {
            const fraction = step / 8;
            actions.push(() => pointer.notify_absolute_motion(GLib.get_monotonic_time(), origin[0] + (destination[0] - origin[0]) * fraction, origin[1] + (destination[1] - origin[1]) * fraction));
            if (step === 4 && __DRAG_CAPTURE__) actions.push(() => {
                const shot = new Shell.Screenshot();
                return shot.screenshot(true, Gio.File.new_for_path(__DRAG_CAPTURE__).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null));
            });
        }
    }
    actions.push(() => pointer.notify_button(GLib.get_monotonic_time(), __BUTTON__, Clutter.ButtonState.RELEASED));
    for (const character of destination || __CLICK_ONLY__ ? '' : 'Find') {
        actions.push(() => keyboard.notify_keyval(GLib.get_monotonic_time(), character.charCodeAt(0), Clutter.KeyState.PRESSED));
        actions.push(() => keyboard.notify_keyval(GLib.get_monotonic_time(), character.charCodeAt(0), Clutter.KeyState.RELEASED));
    }
    if (PREPARE) actions.splice(2);
    let pending = false;
    let timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 80, () => {
        if (pending) return GLib.SOURCE_CONTINUE;
        const result = actions.length ? actions.shift()() : null;
        if (result && typeof result.then === 'function') {
            pending = true;
            result.finally(() => { pending = false; });
        }
        if (actions.length || pending) return GLib.SOURCE_CONTINUE;
        if (!PREPARE && SCREENSHOT) {
            const shot = new Shell.Screenshot();
            shot.screenshot(false, Gio.File.new_for_path(SCREENSHOT).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null)).then(() => {});
        }
        timer = 0;
        GLib.file_set_contents(__COMPLETION__, 'done');
        return GLib.SOURCE_REMOVE;
    });
    return () => {
        if (timer) GLib.source_remove(timer);
    };
}
'''.replace('__DRAG_CAPTURE__', json.dumps(os.environ.get('BINGUX_NATIVE_DRAG_CAPTURE', ''))).replace('__COMPLETION__', json.dumps(str(completion))).replace('__BUTTON__', str(button)).replace('__CLICK_ONLY__', json.dumps(click_only)).replace('__DRAG_DESTINATION__', json.dumps(destination)).replace('__ORIGIN_POINT__', json.dumps([x, y])).replace('PREPARE', 'true' if prepare else 'false').replace('SCREENSHOT', json.dumps(os.environ.get('BINGUX_NATIVE_SCREENSHOT', ''))))
subprocess.run(['gnoblinctl', 'reload-scripts'], env=os.environ | {'XDG_CONFIG_HOME': str(config)}, check=True, capture_output=True)
deadline = time.monotonic() + 8
while not completion.exists():
    if time.monotonic() >= deadline:
        raise SystemExit('The private compositor did not complete the input gesture')
    time.sleep(.02)
