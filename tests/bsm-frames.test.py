#!/usr/bin/env python3
"""Unit tests of tests/bsm_frames.py on synthetic transitions (no VM).

Asserts that the measurements the QEMU test relies on recover known opacity and
translation, and that they reject the wrong scene: a black flash is not a fade,
a fade is not a slide.
"""

import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from bsm_frames import blend_residual, coverage, fade_alpha, is_monotone, similarity, slide_offset  # noqa: E402

HEIGHT, WIDTH = 400, 640


def scene(seed, bar=False):
    """A busy synthetic scene: gradient plus blocks, so every row is distinctive."""
    rng = np.random.default_rng(seed)
    y = np.linspace(0, 1, HEIGHT)[:, None, None]
    x = np.linspace(0, 1, WIDTH)[None, :, None]
    frame = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    frame[..., 0] = 40 + 120 * y[..., 0] * (seed % 2 + 0.5)
    frame[..., 1] = 60 + 100 * x[..., 0]
    frame[..., 2] = 90 + 60 * (x[..., 0] * y[..., 0])
    for _ in range(40):
        r0, c0 = rng.integers(0, HEIGHT - 40), rng.integers(0, WIDTH - 60)
        frame[r0 : r0 + 40, c0 : c0 + 60] = rng.integers(0, 255, 3)
    if bar:
        frame[:30] = 230
    return np.clip(frame, 0, 255).astype(np.uint8)


GREETER = scene(1)
DESKTOP = scene(2, bar=True)


def blend(a, b, alpha):
    return np.clip((1 - alpha) * a.astype(np.float32) + alpha * b.astype(np.float32), 0, 255).astype(np.uint8)


def slide(top_scene, below_scene, offset):
    frame = below_scene.copy()
    if offset < HEIGHT:
        frame[: HEIGHT - offset] = top_scene[offset:]
    return frame


class FadeTests(unittest.TestCase):
    def test_recovers_alpha(self):
        for alpha in (0.0, 0.25, 0.5, 0.75, 1.0):
            got = fade_alpha(blend(GREETER, DESKTOP, alpha), GREETER, DESKTOP)
            self.assertIsNotNone(got)
            self.assertAlmostEqual(got, alpha, delta=0.03)

    def test_greeter_stays_visible_under_the_rising_desktop(self):
        alpha = 0.4
        frame = blend(GREETER, DESKTOP, alpha)
        self.assertLess(blend_residual(frame, GREETER, DESKTOP, alpha), 1.5)
        # The desktop fading from black (the wrong direction) does not fit.
        wrong = blend(np.zeros_like(GREETER), DESKTOP, alpha)
        self.assertGreater(blend_residual(wrong, GREETER, DESKTOP, alpha), 10.0)

    def test_black_flash_is_not_a_fade(self):
        black = np.zeros_like(GREETER)
        self.assertGreater(blend_residual(black, GREETER, DESKTOP, 0.5), 20.0)
        self.assertLess(coverage(black), 0.01)

    def test_progress_bar_noise_does_not_move_the_estimate(self):
        frame = blend(GREETER, DESKTOP, 0.3)
        frame[HEIGHT - 20 :, : WIDTH // 2] = 255
        self.assertAlmostEqual(fade_alpha(frame, GREETER, DESKTOP), 0.3, delta=0.04)

    def test_identical_scenes_have_no_estimate(self):
        self.assertIsNone(fade_alpha(GREETER, GREETER, GREETER))


class SlideTests(unittest.TestCase):
    def test_recovers_offset(self):
        for offset in (0, 40, 120, 240, 360, 400):
            got, error = slide_offset(slide(GREETER, DESKTOP, offset), GREETER, DESKTOP)
            self.assertAlmostEqual(got, offset, delta=8)
            self.assertLess(error, 3.0)

    def test_offset_series_is_monotone_for_an_upward_slide(self):
        offsets = [slide_offset(slide(GREETER, DESKTOP, o), GREETER, DESKTOP)[0] for o in range(0, 401, 20)]
        self.assertTrue(is_monotone(offsets))
        self.assertFalse(is_monotone(offsets[::-1]))
        self.assertTrue(is_monotone(offsets[::-1], decreasing=True))

    def test_revealed_strip_must_be_the_desktop(self):
        other = scene(9)
        frame = slide(GREETER, other, 200)
        _, error = slide_offset(frame, GREETER, DESKTOP)
        self.assertGreater(error, 5.0)


class MiscTests(unittest.TestCase):
    def test_coverage_of_a_uniform_cover_is_zero(self):
        cover = np.full((HEIGHT, WIDTH, 3), 12, np.uint8)
        self.assertEqual(coverage(cover), 0.0)
        self.assertGreater(coverage(GREETER), 0.3)

    def test_similarity(self):
        self.assertAlmostEqual(similarity(GREETER, GREETER), 1.0)
        self.assertLess(similarity(GREETER, DESKTOP), 0.95)


if __name__ == "__main__":
    unittest.main()
