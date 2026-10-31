"""The desktop boots: Gnoblin, then the Bingux shell drawing its bar, wallpaper and dock."""

import re

import pytest

from bxdesktop.imaging import OcrUnavailable

pytestmark = pytest.mark.needs_gnoblin

WIDTH, HEIGHT = 1280, 720


def test_gnoblin_answers_on_its_control_socket_with_one_virtual_monitor(desktop):
    assert "pong" in desktop.gnoblinctl("ping")
    monitors = desktop.gnoblinctl("monitor", "list")
    assert "Meta-0" in monitors and f"{WIDTH}" in monitors and f"{HEIGHT}" in monitors


def test_the_shell_maps_its_bar_wallpaper_and_dock_where_a_desktop_has_them(desktop):
    layers = {layer["namespace"]: layer for layer in desktop.layers() if layer.get("mapped")}

    bar = layers["bingux-top-bar"]["geometry"]
    assert (bar["x"], bar["y"], bar["width"]) == (0, 0, WIDTH)
    assert 24 <= bar["height"] <= 48

    wallpaper = layers["bingux-wallpaper"]
    assert wallpaper["layer"] == "background"
    assert (wallpaper["geometry"]["width"], wallpaper["geometry"]["height"]) == (WIDTH, HEIGHT)

    dock = layers["bingux-dock"]
    assert dock["layer"] == "top" and "bottom" in dock["anchor"]
    assert dock["geometry"]["y"] + dock["geometry"]["height"] == HEIGHT  # flush with the bottom edge
    assert dock["exclusive_zone"] == dock["geometry"]["height"]  # windows are kept clear of it


def test_the_screen_is_drawn_not_black(desktop_session):
    picture = desktop_session.boot["screenshot"]
    assert picture.size == (WIDTH, HEIGHT)
    assert not picture.is_blank()
    assert picture.distinct_colours() > 1000  # the wallpaper alone is a painting


def test_the_bar_has_its_search_clock_and_indicator_regions_drawn(desktop_session):
    picture = desktop_session.boot["screenshot"]
    bar = next(layer for layer in desktop_session.boot["layers"] if layer["namespace"] == "bingux-top-bar")["geometry"]
    height = bar["height"]
    search, clock, indicators = (0, 0, 60, height), (500, 0, 780, height), (1000, 0, WIDTH, height)
    for name, region in (("search button", search), ("clock", clock), ("indicators", indicators)):
        assert not picture.is_blank(region), f"the bar's {name} region is flat"
    # The wallpaper below the bar is a different surface from the bar itself.
    assert picture.changed_fraction(picture, (0, 0, WIDTH, height)) == 0.0
    assert picture.distinct_colours((0, height + 2, WIDTH, HEIGHT)) > 10 * picture.distinct_colours(
        (0, 0, WIDTH, height)
    )


def test_the_clock_shows_a_time(desktop_session):
    picture = desktop_session.boot["screenshot"]
    try:
        text = picture.text((500, 0, 780, 32), scale=4)
    except OcrUnavailable as error:
        pytest.skip(str(error))
    assert re.search(r"\d{1,2}:\d{2}", text), f"no time in the bar's clock region: {text!r}"


def test_a_new_profile_shows_the_introduction_once(desktop_session):
    introduction = desktop_session.boot["introduction"]
    assert introduction == {"open": True, "ready": True}
    namespaces = {layer["namespace"] for layer in desktop_session.boot["layers"]}
    assert "bingux-introduction" in namespaces


def test_boot_wrote_no_error_lines_to_any_log(desktop_session):
    desktop_session.boot["log"].assert_clean()
