#!/usr/bin/env python3
"""Keep the shell's non-text and tone colours at their WCAG ratios.

Reads the colours from shell/bingux/Theme.qml, so a token change that breaks a
ratio fails here: the off-state switch edge against the popup surface (3:1,
WCAG 1.4.11), the thumb against the off track, white ink on the saving pill,
and danger ink on its tinted button and row fills (4.5:1).
"""

from pathlib import Path
import re
import unittest

THEME = (Path(__file__).resolve().parent.parent / "shell/bingux/Theme.qml").read_text()


def hex_colour(name):
    value = re.search(rf'property color {name}: "#([0-9a-fA-F]+)"', THEME).group(1)
    if len(value) == 8:
        value = value[2:]
    return tuple(int(value[i : i + 2], 16) / 255 for i in (0, 2, 4))


def luminance(rgb):
    def lin(c):
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = (lin(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def ratio(a, b):
    la, lb = sorted((luminance(a), luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def over(fg, alpha, bg):
    return tuple(f * alpha + b * (1 - alpha) for f, b in zip(fg, bg))


# popupSurface: rgba(0.188, 0.188, 0.188, 0.8) over the 0.106 grey backdrop (Theme.qml).
POPUP = over((0.188,) * 3, 0.8, (0.106,) * 3)


class ShellContrast(unittest.TestCase):
    def test_off_switch_edge_reads_against_the_popup(self):
        self.assertGreaterEqual(ratio(hex_colour("controlOffOutline"), POPUP), 3.0)

    def test_off_switch_thumb_reads_against_the_track(self):
        self.assertGreaterEqual(ratio(hex_colour("text"), hex_colour("controlOffTrack")), 3.0)

    def test_saving_pill_ink(self):
        self.assertGreaterEqual(ratio((1, 1, 1), hex_colour("savingIndicator")), 4.5)

    def test_danger_button_ink_on_its_tint(self):
        danger = hex_colour("danger")
        for alpha in (0.2, 0.26, 0.3):
            self.assertGreaterEqual(ratio(hex_colour("dangerInk"), over(danger, alpha, POPUP)), 4.5, alpha)

    def test_destructive_row_ink_on_its_tint(self):
        danger = hex_colour("danger")
        self.assertGreaterEqual(ratio(danger, over(danger, 0.1, POPUP)), 4.5)


if __name__ == "__main__":
    unittest.main()
