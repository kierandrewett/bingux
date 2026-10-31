#!/usr/bin/env python3
"""scripts/frame-walk.py: what the layout-stability matcher reports and what it does not.

Synthetic grey frames (a page, cards, a skeleton bar, rules, text-like ink), so each case is a
picture the matcher must or must not report: a row that arrives on bare page is a pop-in, text that
lands on a card or a skeleton bar is a value; a block of rules that jumps farther than the matcher
window is a shift; the two edges of a slider track are not rules; a scrollbar thumb is not content.
"""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

import numpy as np

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
spec = importlib.util.spec_from_file_location("frame_walk", SCRIPTS / "frame-walk.py")
frame_walk = importlib.util.module_from_spec(spec)
sys.modules["frame_walk"] = frame_walk
spec.loader.exec_module(frame_walk)

PAGE, CARD, BAR, INK, EDGE = 250, 255, 238, 20, 215
W, H = 700, 800


def page() -> np.ndarray:
    return np.full((H, W), PAGE, dtype=np.uint8)


def card(gray: np.ndarray, top: int, bottom: int) -> None:
    gray[top:bottom, 40:660] = CARD
    gray[top, 40:660] = EDGE
    gray[bottom - 1, 40:660] = EDGE


def text(gray: np.ndarray, top: int, left: int = 60, width: int = 120) -> None:
    gray[top : top + 12, left : left + width] = INK


def features(gray: np.ndarray) -> np.ndarray:
    return frame_walk.features(gray, PAGE)


class Matcher(unittest.TestCase):
    def test_text_on_bare_page_is_a_row_that_arrived(self):
        before, after = page(), page()
        text(after, 300)
        found = frame_walk.appeared(features(before), features(after))
        self.assertEqual(len(found), 1)
        self.assertFalse(frame_walk.placeholder_under(before, found[0], PAGE))

    def test_text_on_a_card_with_a_skeleton_bar_is_a_value_landing(self):
        before, after = page(), page()
        for gray in (before, after):
            card(gray, 280, 360)
        before[298:312, 60:180] = BAR
        text(after, 300)
        found = frame_walk.appeared(features(before), features(after))
        self.assertEqual(len(found), 1)
        self.assertTrue(frame_walk.placeholder_under(before, found[0], PAGE))

    def test_a_scrollbar_thumb_is_not_content(self):
        before, after = page(), page()
        after[100:300, 690:695] = 120
        self.assertEqual(frame_walk.appeared(features(before), features(after)), [])

    def test_a_block_of_rules_that_jumps_past_the_window_is_a_shift(self):
        before, after = page(), page()
        for top in (100, 160, 220):
            before[top, 40:660] = EDGE
            after[top + 300, 40:660] = EDGE
        moved = frame_walk.relocated(features(before), features(after))
        self.assertEqual([pair["dc"] for pair in moved], [300, 300, 300])

    def test_one_rule_gone_and_another_come_is_not_a_block(self):
        before, after = page(), page()
        before[100, 40:660] = EDGE
        after[500, 40:660] = EDGE
        self.assertEqual(frame_walk.relocated(features(before), features(after)), [])

    def test_the_two_edges_of_a_slider_track_are_not_rules(self):
        gray = page()
        gray[200:206, 100:600] = 120
        rules = [tuple(row) for row in features(gray).tolist() if row[0] == frame_walk.H]
        self.assertEqual(len(rules), 2)
        self.assertTrue(all(frame_walk.track_edge(rule, rules) for rule in rules))

    def test_a_card_edge_is_a_rule(self):
        gray = page()
        card(gray, 280, 360)
        rules = [tuple(row) for row in features(gray).tolist() if row[0] == frame_walk.H]
        self.assertEqual(len(rules), 4)
        self.assertFalse(any(frame_walk.track_edge(rule, rules) for rule in rules))


if __name__ == "__main__":
    unittest.main()
