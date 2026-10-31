"""A notification sent over D-Bus is drawn by the shell's own notification server."""

import pytest

from bxdesktop.imaging import ocr_available

pytestmark = pytest.mark.needs_gnoblin

TOAST_AREA = (860, 36, 1280, 220)
NOTIFICATIONS = "org.freedesktop.Notifications"


def notify(desktop, summary: str, body: str, timeout_ms: int = 0) -> int:
    reply = desktop.run(
        ["gdbus", "call", "--session", "--dest", NOTIFICATIONS, "--object-path", "/org/freedesktop/Notifications",
         "--method", f"{NOTIFICATIONS}.Notify", "harness", "0", "", summary, body, "[]", "{}", str(timeout_ms)]
    ).stdout  # fmt: skip
    return int(reply.strip().strip("(),").split()[-1].rstrip(","))


def close(desktop, notification_id: int) -> None:
    desktop.run(
        ["gdbus", "call", "--session", "--dest", NOTIFICATIONS, "--object-path", "/org/freedesktop/Notifications",
         "--method", f"{NOTIFICATIONS}.CloseNotification", str(notification_id)]
    )  # fmt: skip


def test_the_shell_is_the_notification_server(desktop):
    reply = desktop.run(
        ["gdbus", "call", "--session", "--dest", NOTIFICATIONS, "--object-path", "/org/freedesktop/Notifications",
         "--method", f"{NOTIFICATIONS}.GetServerInformation"]
    ).stdout  # fmt: skip
    assert "quickshell" in reply


@pytest.mark.skipif(not ocr_available(), reason="tesseract is not installed: the toast's text is read from the screen")
def test_a_notification_appears_as_a_toast_and_closes(desktop):
    empty = desktop.settled(TOAST_AREA)
    identifier = notify(desktop, "Build finished", "All 42 checks passed")
    assert identifier > 0

    def toast_text():
        shot = desktop.capture()
        return shot if shot.find_text("Build finished", TOAST_AREA) and shot.find_text("passed", TOAST_AREA) else None

    shown = desktop.wait_until(toast_text, 20, "the toast to be drawn")
    desktop.screenshot("toast")
    assert shown.changed_fraction(empty, TOAST_AREA) > 0.1

    close(desktop, identifier)
    desktop.wait_until(
        lambda: desktop.capture().changed_fraction(empty, TOAST_AREA, tolerance=12) < 0.02, 20, "the toast to go away"
    )
