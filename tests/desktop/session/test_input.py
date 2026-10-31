"""Keyboard and pointer input reach the clients through Gnoblin, as they would from a real seat."""

import pytest

pytestmark = pytest.mark.needs_gnoblin


def window(desktop, window_id):
    return next(w for w in desktop.windows() if w["id"] == window_id)


def test_typed_text_reaches_the_focused_client(desktop):
    client = desktop.launch(["foot", "sh", "-c", "cat > /tmp/typed.txt"], title="foot")
    desktop.wait_focused(client)
    desktop.type("hello bingux 42")
    desktop.key("Return")
    desktop.key("Control_L", "d")  # end of input: cat exits and the file is complete
    desktop.wait_until(lambda: client.process.poll() is not None, 10, "the client to exit")
    assert desktop.run(["cat", "/tmp/typed.txt"]).stdout == "hello bingux 42\n"


def test_a_click_moves_keyboard_focus_to_the_window_under_the_pointer(desktop):
    first = desktop.launch(["foot", "sleep", "600"], title="foot")
    second = desktop.launch(["foot", "sleep", "601"], title="foot")
    # Two windows side by side, so a point inside each is unambiguous.
    desktop.gnoblinctl("window", "move", str(first.window["id"]), "20", "60")
    desktop.gnoblinctl("window", "resize", str(first.window["id"]), "500", "400")
    desktop.gnoblinctl("window", "move", str(second.window["id"]), "700", "60")
    desktop.gnoblinctl("window", "resize", str(second.window["id"]), "500", "400")
    desktop.wait_until(lambda: window(desktop, second.window["id"])["focused"], 10, "the second window to be focused")

    desktop.click(250, 250)  # inside the first window
    desktop.wait_until(lambda: window(desktop, first.window["id"])["focused"], 10, "a click to focus the first window")
    assert not window(desktop, second.window["id"])["focused"]

    desktop.click(900, 250)  # inside the second
    desktop.wait_until(
        lambda: window(desktop, second.window["id"])["focused"], 10, "a click to focus the second window"
    )


def test_an_accelerator_reaches_the_shell(desktop):
    """Super+Space is the shell's input-source switcher; Super alone opens search."""
    assert desktop.shell_json("shell", "panel", "search", "status") == {"open": False}
    desktop.key("Super_L")
    desktop.wait_until(lambda: desktop.shell_json("shell", "panel", "search", "status")["open"], 10, "search to open")
    desktop.key("Escape")
    desktop.wait_until(
        lambda: not desktop.shell_json("shell", "panel", "search", "status")["open"], 10, "search to close"
    )
