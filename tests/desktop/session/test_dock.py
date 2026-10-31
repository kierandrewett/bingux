"""A client launched in the session appears in the shell's dock, and leaves it when it closes."""

import pytest

pytestmark = pytest.mark.needs_gnoblin

WIDTH, HEIGHT = 1280, 720


def dock_apps(desktop):
    return desktop.shell_json("actions", "dock", "list", "", "0")["apps"]


def dock_region(desktop):
    layer = desktop.layer("bingux-dock")["geometry"]
    return (0, layer["y"], WIDTH, layer["y"] + layer["height"])


def test_an_empty_desktop_has_an_empty_dock(desktop):
    assert dock_apps(desktop) == []


def test_a_launched_client_gets_a_dock_icon_and_its_window_is_counted(desktop):
    empty = desktop.screenshot("dock-empty")
    client = desktop.launch(["foot", "sleep", "600"], title="foot")
    apps = desktop.wait_until(lambda: dock_apps(desktop), 15, "foot to appear in the dock")
    assert [(app["id"], app["windows"]) for app in apps] == [("foot", 1)]
    assert client.window["app_id"] == "foot"

    desktop.wait_until(
        lambda: desktop.capture().changed_fraction(empty, dock_region(desktop)) > 0.01, 10, "the dock icon to be drawn"
    )
    drawn = desktop.screenshot("dock-icon")
    middle = (WIDTH // 2 - 80, dock_region(desktop)[1], WIDTH // 2 + 80, HEIGHT)
    assert drawn.changed_fraction(empty, middle) > 0.05, "no icon in the middle of the dock"
    # Away from the icon (and from the window's shadow) the dock is as it was.
    for beside in ((0, dock_region(desktop)[1], 200, HEIGHT), (WIDTH - 200, dock_region(desktop)[1], WIDTH, HEIGHT)):
        assert drawn.changed_fraction(empty, beside) < 0.001, "something besides the app icon changed in the dock"


def test_a_second_window_of_the_same_app_shares_the_icon(desktop):
    desktop.launch(["foot", "sleep", "600"], title="foot")
    desktop.launch(["foot", "sleep", "601"], title="foot")
    apps = desktop.wait_until(lambda: [a for a in dock_apps(desktop) if a["windows"] == 2], 15, "one icon, two windows")
    assert len(apps) == 1 and apps[0]["id"] == "foot"


def test_the_icon_goes_when_the_last_window_closes(desktop):
    client = desktop.launch(["foot", "sleep", "600"], title="foot")
    desktop.wait_until(lambda: dock_apps(desktop), 15, "foot to appear in the dock")
    desktop.gnoblinctl("window", "close", str(client.window["id"]))
    desktop.wait_until(lambda: dock_apps(desktop) == [], 15, "the dock to empty")
    assert desktop.windows() == []
