"""Bingux Settings opens in the real shell under Gnoblin, shows a page, and navigates to a subpage and back."""

import pytest

from bxdesktop.imaging import centre, ocr_available

pytestmark = pytest.mark.needs_gnoblin

# The machine's network, modems, displays and input devices are replaced by fixed data,
# as the snapshot probes in scripts/ do, so the page says the same thing everywhere.
FAKES = {
    "BINGUX_SETTINGS_FAKE_NETWORK": "online",
    "BINGUX_SETTINGS_FAKE_MODEM": "ok",
    "BINGUX_SETTINGS_FAKE_DISPLAYS": "1",
    "BINGUX_SETTINGS_FAKE_BLUETOOTH": "1",
    "BINGUX_SETTINGS_FAKE_INPUT": "1",
}


@pytest.fixture
def settings(desktop, stack):
    binary = stack.binary("bingux-settings")
    if binary is None:
        pytest.skip("bingux-settings is not built: run `make apps`, or set BINGUX_SETTINGS_BIN")
    if not ocr_available():
        pytest.skip("tesseract is not installed: these checks read the page text")
    client = desktop.launch([str(binary), "network"], env=FAKES, title="Settings", timeout=60)
    desktop.wait_until(lambda: desktop.capture().find_text("Connections"), 30, "the Network page")
    return client


# The page area to the right of the sidebar, below the bar.
CONTENT = (290, 40, 1000, 610)


def test_settings_opens_on_the_requested_page_in_a_framed_window(desktop, settings):
    assert settings.window["app_id"] == "bingux-settings"
    picture = desktop.screenshot("network-page")
    assert picture.find_text("Network", (300, 80, 700, 130)), "the page title is not Network"
    assert picture.find_text("Connections")
    assert picture.find_text("Wired connection 1")
    assert desktop.shell_json("actions", "dock", "list", "", "0")["apps"][0]["id"] == "bingux-settings"


def test_a_row_opens_a_subpage_and_back_returns_to_the_page(desktop, settings):
    page = desktop.screenshot("before")
    sidebar = (0, 40, 280, 610)

    row = page.find_text("Connected to Home")
    assert row, "the Wi-Fi row is not on the page"
    # Only the chevron at the end of the row opens the subpage; the rest of the row switches Wi-Fi.
    chevron = (943, centre(row)[1] - 4)
    desktop.hover(*chevron, region=(290, row[1] - 30, 1000, row[3] + 30))  # the row lights up when Settings takes input
    desktop.click(*chevron, settle=2.0)

    sub = desktop.screenshot("subpage")
    assert sub.find_text("Available networks"), "the Wi-Fi subpage did not open"
    assert sub.changed_fraction(page, CONTENT) > 0.03
    assert sub.changed_fraction(page, sidebar) < 0.02, "the sidebar changed while a subpage opened"

    desktop.hover(307, 56, region=(290, 40, 330, 75))
    desktop.click(307, 56, settle=2.0)  # the back arrow in the header
    back = desktop.screenshot("back")
    assert back.find_text("Connections") and not back.find_text("Available networks")
    assert back.changed_fraction(page, (0, 40, 1000, 610)) < 0.03, "Back did not restore the page"
