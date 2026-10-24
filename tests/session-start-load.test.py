#!/usr/bin/env python3
"""Keep boot-time helpers out of the way of the compositor's start.

On 5f53de8.iso the live installer failed to start in 3 of 4 E2E boots and in 5
of 5 boots on a busy host. Gnoblin's guardian runs `systemctl --user start
gnoblin-session.target` before it starts the process that sends the compositor
its configuration, and the compositor stops waiting for that configuration
after 10 s. The start job waited for `bingux.target` and so for the wallpaper
export (`bingux-greeter-export.service`, about 4 s of CPU): 14 to 22 s from
spawn to supervisor in the logs. These checks read the unit files; the
timing was measured in a booted ISO (TODO.md, "Black live installer").
"""

import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
USER_UNITS = REPO / "packaging/systemd"
PROFILE_SYSTEM = REPO / "distro/profile/airootfs/etc/systemd/system"


def settings(path):
    """Unit file settings as {(section, key): [values]}; comments and blank lines are skipped."""
    found = {}
    section = None
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith(("#", ";")):
            continue
        header = re.fullmatch(r"\[(\w+)\]", line)
        if header:
            section = header.group(1)
            continue
        key, _, value = line.partition("=")
        found.setdefault((section, key.strip()), []).append(value.strip())
    return found


class StartLoadTest(unittest.TestCase):
    def test_helper_target_does_not_gate_the_session_target(self):
        # A target waits for the units it wants unless DefaultDependencies=no.
        unit = settings(USER_UNITS / "bingux.target")
        self.assertEqual(unit.get(("Unit", "DefaultDependencies")), ["no"])
        self.assertEqual(unit.get(("Install", "WantedBy")), ["gnoblin-session.target"])

    def test_no_helper_unit_orders_the_session_target_after_it(self):
        # Before=gnoblin-session.target (or graphical-session.target) on a helper would
        # bring the gate back.
        for path in sorted(USER_UNITS.glob("*.service")) + sorted(USER_UNITS.glob("*.path")):
            with self.subTest(unit=path.name):
                before = " ".join(settings(path).get(("Unit", "Before"), []))
                self.assertNotIn("session.target", before)

    def test_wallpaper_export_runs_in_idle_time(self):
        unit = settings(USER_UNITS / "bingux-greeter-export.service")
        self.assertEqual(unit.get(("Service", "Type")), ["oneshot"])
        self.assertEqual(unit.get(("Service", "Nice")), ["19"])
        self.assertEqual(unit.get(("Service", "CPUSchedulingPolicy")), ["idle"])
        self.assertEqual(unit.get(("Service", "IOSchedulingClass")), ["idle"])

    def test_keyring_setup_yields_the_cpu_to_the_compositor(self):
        # About 9 s of gpg CPU while the live session starts. Not SCHED_IDLE: the
        # installer needs the keyring and must not wait behind other work.
        unit = settings(PROFILE_SYSTEM / "pacman-init.service.d/10-nice.conf")
        self.assertEqual(unit.get(("Service", "Nice")), ["19"])
        self.assertNotIn(("Service", "CPUSchedulingPolicy"), unit)


if __name__ == "__main__":
    unittest.main()
