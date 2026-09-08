#!/usr/bin/env python3
"""Click and type through the private compositor's real input path."""
import json
import os
from pathlib import Path
import subprocess
import sys

config = Path(os.environ['BINGUX_TEST_COMPOSITOR_CONFIG'])
if not str(config).startswith('/tmp/gnoblin-gs.') or not os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-'):
    raise SystemExit('Only the private Gnoblin test session can receive test input')
prepare = sys.argv[1:] == ['--prepare']
x, y = (300, 300) if prepare else map(float, sys.argv[1:3])
probe = config / 'gnoblin/scripts/bingux-customise-input.js'
probe.parent.mkdir(parents=True, exist_ok=True)
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
    // A second move clears the initial screen-edge barrier in the headless seat.
    const actions = [
        () => pointer.notify_absolute_motion(GLib.get_monotonic_time(), X, Y),
        () => pointer.notify_absolute_motion(GLib.get_monotonic_time(), X, Y),
        () => pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.PRESSED),
        () => pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.RELEASED)
    ];
    for (const character of 'Find') {
        actions.push(() => keyboard.notify_keyval(GLib.get_monotonic_time(), character.charCodeAt(0), Clutter.KeyState.PRESSED));
        actions.push(() => keyboard.notify_keyval(GLib.get_monotonic_time(), character.charCodeAt(0), Clutter.KeyState.RELEASED));
    }
    if (PREPARE) actions.splice(2);
    let timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 80, () => {
        actions.shift()();
        if (actions.length) return GLib.SOURCE_CONTINUE;
        if (!PREPARE && SCREENSHOT) {
            const shot = new Shell.Screenshot();
            shot.screenshot(false, Gio.File.new_for_path(SCREENSHOT).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null)).then(() => {});
        }
        timer = 0;
        return GLib.SOURCE_REMOVE;
    });
    return () => {
        if (timer) GLib.source_remove(timer);
    };
}
'''.replace('X, Y', f'{x}, {y}').replace('PREPARE', 'true' if prepare else 'false').replace('SCREENSHOT', json.dumps(os.environ.get('BINGUX_NATIVE_SCREENSHOT', ''))))
subprocess.run(['gnoblinctl', 'reload-scripts'], env=os.environ | {'XDG_CONFIG_HOME': str(config)}, check=True, capture_output=True)
