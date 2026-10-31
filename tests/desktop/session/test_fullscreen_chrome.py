"""The top bar and dock revealed over a fullscreen window go back under it when the user returns to the window.

Super opens search and raises the bar and dock (Overlay layer) above a fullscreen window; Super again, or a click
outside the card, hides only search and keeps them raised so they can be clicked. Nothing used to end that reveal
except Escape, the window switcher or reopening search, so a click into the fullscreen window left both panels
drawn over it (docs/window-switcher.md).

The client is foot with mouse tracking on, so a press that reaches the window shows up in a file.
"""

import re
import time

import pytest

pytestmark = pytest.mark.needs_gnoblin

BAR = (100, 2, 1180, 30)
CLICK_TARGET = (640, 400)
# foot writes a mouse press as `ESC [ < button ; column ; row M` (and the release as `m`) once an application turns
# tracking on.
PRESS = re.compile(r"\x1b\[<0;\d+;\d+M")
TRACK_AND_LOG = "printf '\\033[?1000h\\033[?1006h'; stty raw -echo; cat >> /tmp/{name}.clicks"


def launch_foot(desktop, name: str, *, fullscreen: bool):
    argv = ["foot", *(["--fullscreen"] if fullscreen else []), "-T", name, "sh", "-c", TRACK_AND_LOG.format(name=name)]
    client = desktop.launch(argv, title=name)
    if fullscreen:
        desktop.wait_until(lambda: window(desktop, name)["fullscreen"], 10, f"{name} to be fullscreen")
    return client


def window(desktop, title: str) -> dict:
    return next(w for w in desktop.windows() if w["title"] == title)


def clicks(desktop, name: str) -> str:
    path = desktop.root / "tmp" / f"{name}.clicks"
    return path.read_text(errors="replace") if path.exists() else ""


def layer_of(desktop, namespace: str) -> str | None:
    found = desktop.layer(namespace)
    return found["layer"] if found else None


def is_open(desktop, panel: str) -> bool:
    return desktop.shell_json("shell", "panel", panel, "status")["open"]


@pytest.fixture(autouse=True)
def end_the_reveal(desktop):
    """Leave no raised panels behind: the desktop fixture closes search in a way that keeps them."""
    yield
    if not (desktop.layer("bingux-search-chrome") or layer_of(desktop, "bingux-top-bar") == "overlay"):
        return
    if not is_open(desktop, "search"):
        desktop.key("Super_L")
        desktop.wait_until(lambda: is_open(desktop, "search"), 10, "search to open")
    desktop.wait_until(
        lambda: (desktop.key("Escape"), layer_of(desktop, "bingux-top-bar") == "top")[1], 10, "Escape to end the reveal"
    )


def reveal_chrome_then_hide_search(desktop, *, over_fullscreen: bool = True):
    """Super opens search with the panels raised; Super again hides search and keeps the panels."""
    desktop.key("Super_L")
    desktop.wait_until(lambda: is_open(desktop, "search"), 10, "search to open")
    desktop.wait_until(lambda: layer_of(desktop, "bingux-top-bar") == "overlay", 10, "the top bar to be raised")
    desktop.key("Super_L")
    desktop.wait_until(lambda: not is_open(desktop, "search"), 10, "search to hide")
    if over_fullscreen:  # the surface that takes the dismissing press exists only over a fullscreen window
        desktop.wait_until(lambda: desktop.layer("bingux-search-chrome"), 10, "the dismissal surface")
    assert layer_of(desktop, "bingux-top-bar") == "overlay", "hiding search must keep the top bar raised"
    assert layer_of(desktop, "bingux-dock") == "overlay", "hiding search must keep the dock raised"


def test_a_click_into_the_fullscreen_window_drops_the_panels_under_it(desktop):
    launch_foot(desktop, "fs-click", fullscreen=True)
    desktop.wait_until(lambda: layer_of(desktop, "bingux-top-bar") == "top", 10, "the bar to be a normal panel")
    covered = desktop.settled()
    assert desktop.capture().changed_fraction(covered, BAR) < 0.001, "the top bar is drawn over a fullscreen window"

    reveal_chrome_then_hide_search(desktop)
    desktop.wait_until(lambda: desktop.capture().changed_fraction(covered, BAR) > 0.3, 10, "the bar to be drawn")
    desktop.screenshot("revealed-over-fullscreen")

    desktop.move(*CLICK_TARGET)
    pressed = time.monotonic()
    desktop.click(settle=0)
    desktop.wait_until(
        lambda: layer_of(desktop, "bingux-top-bar") == "top", 5, "the bar to drop under the window", 0.02
    )
    dropped = time.monotonic() - pressed
    desktop.wait_until(
        lambda: desktop.capture().changed_fraction(covered, BAR) < 0.001, 5, "the bar to be covered by the window"
    )
    assert layer_of(desktop, "bingux-dock") == "top"
    assert not desktop.layer("bingux-search-chrome"), "the dismissal surface outlived the reveal"
    assert dropped < 1.0, f"the panels took {dropped:.2f}s to leave after the click"
    desktop.screenshot("dropped-under-fullscreen")

    # The press that ends the reveal is not delivered; the next one is.
    assert not PRESS.search(clicks(desktop, "fs-click"))
    desktop.wait_until(
        lambda: (desktop.click(*CLICK_TARGET, settle=0.1), PRESS.search(clicks(desktop, "fs-click")))[1],
        10,
        "a later click to reach the window",
        0.1,
    )


def test_the_panels_stay_clickable_while_they_are_revealed(desktop):
    launch_foot(desktop, "fs-panels", fullscreen=True)
    desktop.wait_until(lambda: layer_of(desktop, "bingux-top-bar") == "top", 10, "the bar to be a normal panel")
    reveal_chrome_then_hide_search(desktop)

    desktop.click(24, 16)  # the search button at the left end of the bar
    desktop.wait_until(lambda: is_open(desktop, "search"), 10, "the bar's search button to open search")
    assert not PRESS.search(clicks(desktop, "fs-panels"))


def test_a_window_that_is_not_fullscreen_gets_its_first_click(desktop):
    launch_foot(desktop, "plain", fullscreen=False)
    reveal_chrome_then_hide_search(desktop, over_fullscreen=False)
    desktop.click(*CLICK_TARGET)
    desktop.wait_until(lambda: PRESS.search(clicks(desktop, "plain")), 5, "the first click to reach the window")


def test_activating_another_window_drops_the_panels(desktop):
    launch_foot(desktop, "fs-behind", fullscreen=True)
    launch_foot(desktop, "other", fullscreen=False)
    reveal_chrome_then_hide_search(desktop, over_fullscreen=False)

    windows = desktop.shell_json("actions", "window", "list", "")["windows"]
    target = next(w for w in windows if w["title"] == "fs-behind")
    desktop.shell_ipc("actions", "window", "activate", target["id"])
    desktop.wait_until(lambda: layer_of(desktop, "bingux-top-bar") == "top", 5, "the bar to drop under the window")
    assert layer_of(desktop, "bingux-dock") == "top"
