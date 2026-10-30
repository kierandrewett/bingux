#!/usr/bin/env python3
"""calendar-launch.py starts the user's default calendar, not always GNOME Calendar."""

import importlib.util
from pathlib import Path
import sys
import unittest

SHELL = Path(__file__).resolve().parents[1] / "shell/bingux"
sys.path.insert(0, str(SHELL))
spec = importlib.util.spec_from_file_location("calendar_launch", SHELL / "calendar-launch.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class App:
    def __init__(self, desktop_id, filename):
        self.desktop_id, self.filename = desktop_id, filename

    def get_id(self):
        return self.desktop_id

    def get_filename(self):
        return self.filename


THUNDERBIRD = App(
    "net.thunderbird.Thunderbird.desktop",
    "/usr/share/applications/net.thunderbird.Thunderbird.desktop",
)
GNOME = App("org.gnome.Calendar.desktop", "/usr/share/applications/org.gnome.Calendar.desktop")


class Bus:
    class Reply:
        def unpack(self):
            return (True,)

    def call_sync(self, *args):
        return self.Reply()


class DefaultCalendarTest(unittest.TestCase):
    def run_main(self, app, *argv):
        launched = []
        module.spawn_detached = launched.append
        module.default_calendar = lambda: app
        module.Gio.bus_get_sync = lambda *args: Bus()
        module.calendar_locale = lambda bus: "C"
        module.time.sleep = lambda seconds: None
        sys.argv = ["calendar-launch.py", *argv]
        module.main()
        return launched

    def test_other_default_is_started_without_date_options(self):
        expected = [["gio", "launch", THUNDERBIRD.filename]]
        self.assertEqual(self.run_main(THUNDERBIRD, "--open"), expected)
        self.assertEqual(
            self.run_main(THUNDERBIRD, "--date", "2026-10-13", "--uuid", "a:b"),
            expected,
        )

    def test_gnome_calendar_keeps_date_and_event_links(self):
        launched = self.run_main(GNOME, "--date", "2026-10-13", "--uuid", "source:event")
        self.assertEqual(launched[0][:2], ["gnome-calendar", "--date"])
        self.assertEqual(launched[1], ["gnome-calendar", "--uuid", "source:event"])

    def test_no_default_falls_back_to_gnome_calendar(self):
        self.assertEqual(self.run_main(None, "--open"), [["gnome-calendar"]])

    def test_date_is_required_without_open(self):
        with self.assertRaises(SystemExit):
            self.run_main(GNOME)


if __name__ == "__main__":
    unittest.main()
