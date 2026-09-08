"""Opt-in real Win+Period verification. Never inserts text or changes the clipboard."""
import json
from pathlib import Path
import subprocess
import time
from gi.repository import Gio, GLib

shell = Path(__file__).resolve().parents[1] / 'shell/bingux'
command = ['qs', 'ipc', '--any-display', '-p', str(shell), 'call', 'emoji']


def status():
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        result = subprocess.run(command + ['status'], capture_output=True, text=True, timeout=3)
        try:
            return json.loads(result.stdout)
        except ValueError:
            time.sleep(.05)
    raise AssertionError('Shell did not finish reloading')


assert not status()['visible'], 'Leave an already-open emoji picker untouched'
assert status()['ready'], 'Win+Period was not registered'
emoji_settings = Gio.Settings.new('org.freedesktop.ibus.panel.emoji')
assert '<Super>period' not in emoji_settings.get_strv('hotkey'), 'IBus is still intercepting Win+Period'
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
destination = 'org.gnome.Mutter.RemoteDesktop'


def call(path, interface, method, args=None):
    return bus.call_sync(destination, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 3000, None)


session = call('/org/gnome/Mutter/RemoteDesktop', destination, 'CreateSession').unpack()[0]


def key(code, pressed):
    symbol = {125: 0xffeb, 52: 0x2e, 1: 0xff1b}[code]
    call(session, destination + '.Session', 'NotifyKeyboardKeysym', GLib.Variant('(ub)', (symbol, pressed)))


def press(hold):
    key(125, True)
    time.sleep(.05)
    key(52, True)
    time.sleep(hold)
    key(52, False)
    key(125, False)
    time.sleep(.12)


def wait_visible(value):
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        if status()['visible'] == value:
            return
        time.sleep(.03)
    raise AssertionError(f'Picker visible did not become {value}')


try:
    call(session, destination + '.Session', 'Start')
    time.sleep(.1)
    for hold in [.06, .7]:
        for attempt in range(5):
            before = status()['instance']
            try:
                press(hold)
                wait_visible(True)
                key(1, True)
                key(1, False)
                wait_visible(False)
                assert status()['instance'] == before, 'Shell reloaded during the test'
                print(f'PASS Win+Period opens, Escape closes; holding {hold}s keeps it open')
                break
            except AssertionError:
                if status()['instance'] == before or attempt == 4:
                    raise
                subprocess.run(command + ['close'], check=True, timeout=3)
                print('Retrying after a concurrent shell reload')
    for panel in ['search', 'controls', 'calendar']:
        subprocess.run(command[:-1] + ['shell', panel], check=True, timeout=3)
        time.sleep(.2)
        press(.06)
        wait_visible(True)
        key(1, True)
        key(1, False)
        wait_visible(False)
        print(f'PASS visual emoji picker opens over {panel}')
finally:
    key(52, False)
    key(125, False)
    call(session, destination + '.Session', 'Stop')
    subprocess.run(command + ['close'], check=True, timeout=3)
