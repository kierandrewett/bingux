#!/usr/bin/env python3
"""Keep the security-critical Bingux-to-Gnoblin lock contract explicit."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


class LockContractTest(unittest.TestCase):
    def test_control_centre_starts_bingux_lock_not_the_gnome_lock_path(self):
        control_centre = (ROOT / "shell/bingux/ControlCentre.qml").read_text()

        self.assertIn('Quickshell.execDetached(["bingux-lock"])', control_centre)
        self.assertNotIn('loginctl", "lock-session', control_centre)

    def test_uses_session_lock_and_keeps_authority_in_the_compositor(self):
        shell = (ROOT / "shell/bingux/LockShell.qml").read_text()
        self.assertIn("WlSessionLock", shell)
        self.assertIn("WlSessionLockSurface", shell)
        self.assertIn("if (secure)", shell)
        self.assertIn("secureSeen", shell)
        self.assertIn("lockAuthentication.unlockRequested", shell)
        self.assertIn("import Bingux.Wayland", shell)
        self.assertIn("onUnlockSubmitted: unlockRoundtrip.synchronize()", shell)
        self.assertIn("onCompleted:", shell)
        self.assertNotIn("ReportPresented", shell)
        self.assertNotIn("ReportEnded", shell)
        self.assertNotIn("PanelWindow", shell)

    def test_uses_a_dedicated_pam_service_without_a_broker_token(self):
        authentication = (ROOT / "shell/bingux/LockAuthentication.qml").read_text()
        self.assertIn('config: "bingux-lock"', authentication)
        self.assertIn("PamResult.Success", authentication)
        self.assertIn("responseRequired", authentication)
        self.assertIn("responseVisible", authentication)
        self.assertIn("sessionLock.secure && !pam.active", authentication)
        self.assertIn("PamResult.Success && root.sessionLock.secure", authentication)
        self.assertIn("root.sessionLock.locked = false;\n                root.unlockSubmitted();", authentication)
        self.assertIn("authentication.sessionLock.secure", (ROOT / "shell/bingux/LockScreen.qml").read_text())
        self.assertFalse((ROOT / "shell/bingux/LockBroker.qml").exists())
        self.assertFalse((ROOT / "shell/bingux/lock-broker.py").exists())
        self.assertFalse((ROOT / "shell/bingux/lock-start-gate.py").exists())
        self.assertFalse((ROOT / "tests/lock-start-gate.test.py").exists())

    def test_unlock_completion_syncs_the_qt_wayland_connection(self):
        plugin = (ROOT / "packages/bingux-wayland-sync/plugin.cpp").read_text()
        self.assertIn("wl_display_sync(display)", plugin)
        self.assertIn("wl_display_flush(display)", plugin)
        self.assertIn("including unlock_and_destroy", plugin)
        self.assertIn("emit self->completed()", plugin)
        self.assertIn("EAGAIN", plugin)

    def test_theme_is_client_only_and_rejects_nonlocal_values(self):
        theme = (ROOT / "shell/bingux/LockTheme.qml").read_text()
        screen = (ROOT / "shell/bingux/LockScreen.qml").read_text()
        self.assertIn("lock-theme.json", theme)
        self.assertIn("isColour", theme)
        self.assertIn("isLocalAbsolutePath", theme)
        self.assertIn("LockTheme.backgroundImage", screen)
        self.assertIn("SystemClock", screen)
        self.assertNotIn("IdleTimeoutSeconds", theme)

    def test_hypridle_example_is_explicitly_opt_in(self):
        example = (ROOT / "docs/hypridle-bingux.conf.example").read_text()
        docs = (ROOT / "docs/locking.md").read_text()
        self.assertIn("ext-idle-notify-v1", docs)
        self.assertIn("ext-session-lock-v1", docs)
        self.assertIn("on-timeout = bingux-lock", example)
        self.assertNotIn("pgrep -x bingux-lock", example)
        self.assertNotIn("before_sleep_cmd =", example)
        self.assertNotIn("inhibit_sleep =", example)
        self.assertIn("hyprland-lock-notify-v1", docs)
        self.assertIn("rejects that request with `finished`", docs)
        self.assertIn("not installed, enabled", docs)

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
