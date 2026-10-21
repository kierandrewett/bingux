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


def context_list(table, widget_id):
    match = re.search(r"id: \"%s\"[^}]*?contexts: \[([^\]]*)\]" % re.escape(widget_id), table)
    if not match:
        raise AssertionError(widget_id + " has no contexts list")
    return re.findall(r"\"([a-z]+)\"", match.group(1))


class SessionContextTest(unittest.TestCase):
    def test_every_widget_declares_its_contexts(self):
        layout = (SHELL / "DesktopLayout.js").read_text()
        table = layout[layout.index("const widgets = [") : layout.index("];", layout.index("const widgets = ["))]
        entries = re.findall(r"\{\s*id: \"([a-z-]+)\"[^}]*\}", table)
        self.assertGreater(len(entries), 10)
        for entry in re.finditer(r"\{\s*id: \"([a-z-]+)\"(?P<body>[^}]*)\}", table):
            self.assertIn("contexts:", entry.group("body"), entry.group(1) + " has no contexts")
        self.assertIn("function allowed(id, mode)", layout)
        # Clock, metrics, keyboard and the control centre are shared with the
        # pre-login contexts; StatusBar renders them for every context.
        pre_login = ["desktop", "locked", "greeter", "kiosk"]
        for shared in ("clock", "controls", "keyboard", "metrics"):
            self.assertEqual(context_list(table, shared), pre_login, shared)
        for desktop_only in ("search", "tray", "capture", "overflow", "workspaces"):
            self.assertEqual(context_list(table, desktop_only), ["desktop"], desktop_only)
        self.assertEqual(context_list(table, "notifications"), ["desktop", "locked"])
        self.assertIn('"kiosk"', layout[layout.index("const sessionContexts") :].split("\n", 1)[0])

    def test_capability_table_matches_the_design(self):
        services = (SHELL / "SessionServices.qml").read_text()
        self.assertIn('preLoginGrants: ["volume", "power", "network", "doNotDisturb"]', services)
        self.assertIn("locked: preLoginGrants", services)
        self.assertIn("greeter: preLoginGrants", services)
        self.assertIn("kiosk: preLoginGrants", services)

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
