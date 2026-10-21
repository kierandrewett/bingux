#!/usr/bin/env python3
"""Keep the lock screen and greeter restrictions explicit in the sources.

Every widget declares the session contexts it may appear in, every action
path that could launch, show content or reach a session service asks
SessionServices first, and extensions never load outside the desktop.
"""

import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT / "shell/bingux"


class SessionContextTest(unittest.TestCase):
    def test_every_widget_declares_its_contexts(self):
        layout = (SHELL / "DesktopLayout.js").read_text()
        table = layout[layout.index("const widgets = [") : layout.index("];", layout.index("const widgets = ["))]
        entries = re.findall(r"\{\s*id: \"([a-z-]+)\"[^}]*\}", table)
        self.assertGreater(len(entries), 10)
        for entry in re.finditer(r"\{\s*id: \"([a-z-]+)\"(?P<body>[^}]*)\}", table):
            self.assertIn("contexts:", entry.group("body"), entry.group(1) + " has no contexts")
        self.assertIn("function allowed(id, mode)", layout)
        # The clock lives in SessionScreen, so the bar never duplicates it.
        self.assertRegex(table, r'id: "clock"[^}]*contexts: \["desktop"\]')
        self.assertRegex(table, r'id: "controls"[^}]*contexts: \[\s*"desktop",\s*"locked",\s*"greeter",?\s*\]')
        self.assertRegex(table, r'id: "keyboard"[^}]*contexts: \[\s*"desktop",\s*"locked",\s*"greeter",?\s*\]')
        for desktop_only in ("search", "tray", "metrics", "capture", "overflow", "workspaces"):
            self.assertRegex(table, r'id: "%s"[^}]*contexts: \["desktop"\]' % desktop_only)

    def test_capability_table_matches_the_design(self):
        services = (SHELL / "SessionServices.qml").read_text()
        self.assertIn('locked: ["volume", "power"]', services)
        self.assertIn('greeter: ["power"]', services)

    def test_action_paths_consult_the_gate(self):
        expected = {
            "ShellCommands.qml": 'SessionServices.allows("launch")',
            "ApplicationLauncher.qml": 'SessionServices.allows("launch")',
            "Dock.qml": 'SessionServices.allows("launch")',
            "SearchSocket.qml": 'SessionServices.allows("search")',
            "Tray.qml": 'SessionServices.allows("trayMenus")',
            "ControlCentre.qml": 'SessionServices.allows("settings")',
            "SessionPower.qml": 'SessionServices.allows("power")',
            "NotificationState.qml": 'SessionServices.allows("notificationContent")',
            "SessionBar.qml": 'SessionServices.allows("power")',
        }
        for name, needle in expected.items():
            self.assertIn(needle, (SHELL / name).read_text(), name)
        self.assertIn('SessionServices.allows("lock")', (SHELL / "ControlCentre.qml").read_text())
        self.assertIn("DesktopLayout.allowed(name, SessionContext.mode)", (SHELL / "shell.qml").read_text())

    def test_shells_enter_their_context_before_loading(self):
        self.assertIn('SessionContext.enter("locked")', (SHELL / "LockShell.qml").read_text())
        self.assertIn('SessionContext.enter("greeter")', (SHELL / "GreeterShell.qml").read_text())
        registry = (SHELL / "ExtensionRegistry.qml").read_text()
        self.assertIn("!SessionContext.restricted", registry)

    def test_extension_manifest_validates_contexts(self):
        with tempfile.TemporaryDirectory() as home:
            extension = Path(home) / ".local/share/bingux/extensions/demo"
            extension.mkdir(parents=True)
            (extension / "Widget.qml").write_text("import QtQuick\nItem {}\n")
            config = Path(home) / ".config/bingux"
            config.mkdir(parents=True)
            (config / "extensions.json").write_text(json.dumps({"enabled": ["demo"]}))

            def listing(contexts):
                manifest = {
                    "id": "demo",
                    "name": "Demo",
                    "apiVersion": 1,
                    "widgets": [{"id": "w", "name": "W", "component": "Widget.qml", "contexts": contexts}],
                }
                (extension / "extension.json").write_text(json.dumps(manifest))
                result = subprocess.run(
                    [sys.executable, str(SHELL / "extensions.py"), "list"],
                    env={
                        "HOME": home,
                        "PATH": "/usr/bin:/bin",
                        "XDG_DATA_HOME": str(Path(home) / ".local/share"),
                        "XDG_CONFIG_HOME": str(config.parent),
                    },
                    capture_output=True,
                    text=True,
                    check=True,
                )
                return json.loads(result.stdout)

            good = listing(["desktop", "locked"])
            self.assertEqual(good["widgets"][0]["contexts"], ["desktop", "locked"])
            bad = listing(["desktop", "kiosk"])
            self.assertEqual(bad["widgets"], [])
            self.assertTrue(any("contexts" in error["message"] for error in bad["errors"]), bad["errors"])


if __name__ == "__main__":
    unittest.main()
