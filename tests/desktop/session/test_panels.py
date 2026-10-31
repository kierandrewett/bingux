"""The shell's popups open and close through its public IPC, and draw something when open.

An empty notification centre draws only its two labels, about a fifth of a percent of the screen, so
"drew something" means more than a tenth of a percent of the pixels changed. (In the first run the
notification centre had no panel background at all, only "No notifications" and "Clear all" over the
wallpaper; see docs/desktop-testing.md, observations.)
"""

import pytest

pytestmark = pytest.mark.needs_gnoblin

PANELS = ["search", "calendar", "controls", "notifications"]


def is_open(desktop, name: str) -> bool:
    return desktop.shell_json("shell", "panel", name, "status")["open"]


@pytest.mark.parametrize("name", PANELS)
def test_a_panel_opens_draws_and_closes(desktop, name):
    closed = desktop.settled()
    assert not is_open(desktop, name)

    assert desktop.shell_json("shell", "panel", name, "open") == {"ok": True, "open": True}
    desktop.wait_until(lambda: is_open(desktop, name), 10, f"{name} to report open")
    opened = desktop.wait_until(
        lambda: (shot := desktop.capture()).changed_fraction(closed) > 0.001 and shot, 15, f"{name} to be drawn"
    )
    desktop.screenshot(f"{name}-open")
    assert opened.changed_fraction(closed) > 0.001

    desktop.shell_ipc("shell", "panel", name, "close")
    desktop.wait_until(lambda: not is_open(desktop, name), 10, f"{name} to close")
    desktop.wait_until(
        lambda: desktop.capture().changed_fraction(closed, tolerance=12) < 0.01, 15, f"{name} to be gone"
    )


def test_an_unknown_panel_is_refused(desktop):
    reply = desktop.shell_json("shell", "panel", "no-such-panel", "open")
    assert reply["ok"] is False and "Unknown panel" in reply["error"]
