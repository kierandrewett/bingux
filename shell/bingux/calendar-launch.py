#!/usr/bin/env python3
"""Open a Calendar event in its month, using the running app's date locale."""
import argparse
import datetime
import locale
import os
from pathlib import Path
import subprocess
import time
from gi.repository import Gio, GLib


def calendar_locale(bus):
    env = os.environ
    try:
        reply = bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
                              'org.freedesktop.DBus', 'GetConnectionUnixProcessID',
                              GLib.Variant('(s)', ('org.gnome.Calendar',)),
                              GLib.VariantType.new('(u)'), Gio.DBusCallFlags.NONE, 1000, None)
        pid = reply.unpack()[0]
        env = dict(part.split('=', 1) for part in Path(f'/proc/{pid}/environ').read_text().split('\0') if '=' in part)
    except (GLib.Error, OSError, ValueError):
        pass
    return env.get('LC_ALL') or env.get('LC_TIME') or env.get('LANG') or 'C'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--date', required=True)
    parser.add_argument('--uuid')
    args = parser.parse_args()
    date = datetime.date.fromisoformat(args.date)
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    try:
        locale.setlocale(locale.LC_TIME, calendar_locale(bus))
    except locale.Error:
        locale.setlocale(locale.LC_TIME, '')
    # Calendar's CLI uses EDS's locale date parser, not ISO 8601.
    date_format = locale.nl_langinfo(locale.D_FMT).replace('%y', '%Y')
    subprocess.Popen(['gnome-calendar', '--date', date.strftime(date_format)],
                     start_new_session=True, stdin=subprocess.DEVNULL,
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if not args.uuid:
        return
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        reply = bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
                              'org.freedesktop.DBus', 'NameHasOwner',
                              GLib.Variant('(s)', ('org.gnome.Calendar',)),
                              GLib.VariantType.new('(b)'), Gio.DBusCallFlags.NONE, 1000, None)
        if reply.unpack()[0]:
            # Let the date command reach the application before the event action.
            time.sleep(0.2)
            subprocess.Popen(['gnome-calendar', '--uuid', args.uuid],
                             start_new_session=True, stdin=subprocess.DEVNULL,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return
        time.sleep(0.1)
    raise SystemExit('Calendar did not start')


if __name__ == '__main__':
    main()
