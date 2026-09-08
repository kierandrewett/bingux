"""Opt-in native GTK receiver: actual picker search, Enter and Unicode insertion."""
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time
import gi
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, Gio, GLib

shell = Path(__file__).resolve().parents[1] / 'shell/bingux'
command = ['qs', 'ipc', '--any-display', '-p', str(shell), 'call', 'emoji']


def status():
    return json.loads(subprocess.check_output(command + ['status'], text=True, timeout=3))


assert not status()['visible'], 'Leave an existing picker untouched'
window = Gtk.Window(title='Bingux native emoji receiver')
entry = Gtk.Entry()
entry.set_text('Before ')
window.add(entry)
window.set_default_size(440, 100)
window.show_all()
entry.grab_focus()
entry.set_position(-1)
outcome = []


def exercise():
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    destination = 'org.gnome.Mutter.RemoteDesktop'
    session = None
    bridge = socket.socket(socket.AF_UNIX)
    bridge.settimeout(3)
    def call(path, method, args=None):
        interface = destination + '.Session' if path != '/org/gnome/Mutter/RemoteDesktop' else destination
        return bus.call_sync(destination, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 3000, None)
    def key(symbol, down):
        call(session, 'NotifyKeyboardKeysym', GLib.Variant('(ub)', (symbol, down)))
    try:
        bridge.connect(os.environ.get('GNOBLIN_COMPOSITOR_SOCKET', os.environ['XDG_RUNTIME_DIR'] + '/gnoblin/compositor-v1.sock'))
        stream = bridge.makefile('r')
        bridge.sendall(b'{"op":"windows"}\n')
        while True:
            record = json.loads(stream.readline())
            matches = [item for item in record.get('windows', []) if item['title'] == 'Bingux native emoji receiver']
            if matches:
                target = matches[0]['id']
                break
        bridge.sendall((json.dumps({'op': 'activate', 'window': target}) + '\n').encode())
        time.sleep(.2)
        subprocess.run(command + ['open'], check=True, timeout=3)
        time.sleep(.25)
        initial = status()
        assert initial['visible'], 'Picker did not open'
        session = call('/org/gnome/Mutter/RemoteDesktop', 'CreateSession').unpack()[0]
        call(session, 'Start')
        for character in 'grinning face':
            key(ord(character), True)
            key(ord(character), False)
            time.sleep(.02)
        current = status()
        assert current['instance'] == initial['instance'] and current['visible'], 'Picker reloaded or closed'
        assert current['query'] == 'grinning face', current
        assert current['selected'] == '😀', current
        key(0xff0d, True)
        key(0xff0d, False)
        time.sleep(.6)
        def verify():
            text = entry.get_text()
            outcome.append(text == 'Before 😀')
            print('PASS native GTK received emoji through Enter' if outcome[-1] else f'FAIL native GTK text: {text!r}', flush=True)
            Gtk.main_quit()
            return False
        GLib.idle_add(verify)
    except Exception as error:
        outcome.append(False)
        print(f'FAIL {error}', flush=True)
        GLib.idle_add(Gtk.main_quit)
    finally:
        if session:
            call(session, 'Stop')
        bridge.close()
        subprocess.run(command + ['close'], timeout=3)


GLib.timeout_add(300, lambda: (threading.Thread(target=exercise, daemon=True).start(), False)[1])
Gtk.main()
window.destroy()
raise SystemExit(0 if outcome == [True] else 1)
