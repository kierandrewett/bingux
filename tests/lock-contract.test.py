#!/usr/bin/env python3
"""Keep the security-critical Bingux-to-Gnoblin lock contract explicit."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


class LockContractTest(unittest.TestCase):
    def test_uses_session_lock_and_reports_only_secure_boundaries(self):
        shell = (ROOT / "shell/bingux/LockShell.qml").read_text()
        self.assertIn("WlSessionLock", shell)
        self.assertIn("WlSessionLockSurface", shell)
        self.assertIn("if (secure)", shell)
        self.assertIn("LockBroker.reportPresented()", shell)
        self.assertNotIn("ReportEnded", shell)
        self.assertNotIn("PanelWindow", shell)

    def test_requires_broker_token_and_uses_dedicated_pam_service(self):
        broker = (ROOT / "shell/bingux/LockBroker.qml").read_text()
        authentication = (ROOT / "shell/bingux/LockAuthentication.qml").read_text()
        self.assertIn('Quickshell.env("GNOBLIN_LOCK_TOKEN")', broker)
        helper = (ROOT / "shell/bingux/lock-broker.py").read_text()
        self.assertIn('"org.gnoblin.Lock"', helper)
        self.assertIn('"/org/gnoblin/Lock"', helper)
        self.assertIn("ReportPresented", helper)
        self.assertNotIn("token\n", broker)
        self.assertIn('config: "bingux-lock"', authentication)
        self.assertIn("PamResult.Success", authentication)
        self.assertIn("responseRequired", authentication)
        self.assertIn("responseVisible", authentication)

    def test_pam_and_unit_are_packaged_but_not_enabled(self):
        pam = (ROOT / "packaging/pam/bingux-lock").read_text()
        unit = (ROOT / "packaging/systemd/bingux-lock.service").read_text()
        target = (ROOT / "packaging/systemd/bingux.target").read_text()
        self.assertIn("auth include system-auth", pam)
        self.assertIn("ExecStart=/usr/bin/bingux-lock", unit)
        self.assertNotIn("[Install]", unit)
        self.assertNotIn("bingux-lock.service", target)


if __name__ == "__main__":
    unittest.main()
