#!/usr/bin/env python3
"""Opt-in real shortcut latency: key injection to published visibility, not first paint.

Requires the installed binguxctl and an idle desktop. Opens selectors only.
"""
import json, os, socket, subprocess, time, statistics, shutil
ctl = shutil.which('binguxctl')
assert ctl, 'Install binguxctl first'
for target, key in [('emoji', 'visible'), ('capture', 'opened')]:
    state = json.loads(subprocess.check_output([ctl, target, 'status'], text=True))
    assert not state[key], 'Leave an open selector untouched'
    if target == 'capture':
        assert state['state'] in ('idle', 'saved', 'error'), 'Leave an active capture untouched'
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
dest = 'org.gnome.Mutter.RemoteDesktop'

def call(path, method, args=None):
    return bus.call_sync(dest, path, dest + ('.Session' if path != '/org/gnome/Mutter/RemoteDesktop' else ''), method, args, None, Gio.DBusCallFlags.NONE, 3000, None)
session = call('/org/gnome/Mutter/RemoteDesktop', 'CreateSession').unpack()[0]

def key(symbol, down):
    call(session, 'NotifyKeyboardKeysym', GLib.Variant('(ub)', (symbol, down)))
try:
    call(session, 'Start')
    for target, close, modifier, symbol in [('emoji', 'close', 65515, 46), ('capture', 'cancel', 65513, ord('s'))]:
        values = []
        for _ in range(6):
            subprocess.run([ctl, target, close], check=True, stdout=subprocess.DEVNULL)
            s = socket.socket(socket.AF_UNIX)
            s.connect(os.environ['XDG_RUNTIME_DIR'] + '/gnoblin/compositor-v1.sock')
            s.settimeout(4)
            f = s.makefile('r')
            s.sendall(b'{"op":"ui-session","action":"watch"}\n')

            def wait(visible):
                while True:
                    x = json.loads(f.readline())
                    if x.get('event') == 'ui-state' and x.get('name') == target and ((x.get('state') or {}).get('visible') == visible):
                        return
            wait(False)
            key(modifier, True)
            t = time.monotonic()
            key(symbol, True)
            wait(True)
            values.append((time.monotonic() - t) * 1000)
            key(symbol, False)
            key(modifier, False)
            time.sleep(0.1)
            f.close()
            s.close()
        subprocess.run([ctl, target, close], check=True, stdout=subprocess.DEVNULL)
        print(target, 'key-to-visible-state ms', list(map(lambda x: round(x, 1), values)), 'median', round(statistics.median(values), 1), flush=True)
finally:
    for symbol in [ord('s'), 65513, 46, 65515]:
        key(symbol, False)
    call(session, 'Stop')
