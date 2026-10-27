#!/usr/bin/env python3
"""Unit tests of tests/bsm_security_frames.py on synthetic frames (no VM).

The security scenarios call a frame a leak when a user's marker colour shows in it. These
tests pin what that means: the greeter, the cover and a black frame are never leaks, the
marker is found at full strength and at the opacity a fade would show it, and a black frame
is told apart from the dark blue-grey cover.
"""

import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import bsm_security_frames as frames  # noqa: E402

HEIGHT, WIDTH = 400, 640
COVER = (13, 17, 23)


def solid(colour):
    return np.full((HEIGHT, WIDTH, 3), colour, np.uint8)


def greeter(avatar=(215, 110, 110)):
    """Dark blue gradient, a bright clock, and one saturated avatar disc: the busiest legitimate scene."""
    y = np.linspace(0, 1, HEIGHT)[:, None]
    frame = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    frame[..., 0] = 35 - 20 * y
    frame[..., 1] = 60 - 35 * y
    frame[..., 2] = 110 - 60 * y
    frame[40:90, 200:440] = 255  # clock
    yy, xx = np.mgrid[:HEIGHT, :WIDTH]
    frame[(yy - 300) ** 2 + (xx - 320) ** 2 < 36**2] = avatar
    return np.clip(frame, 0, 255).astype(np.uint8)


def blend(a, b, alpha):
    return np.clip((1 - alpha) * a.astype(np.float32) + alpha * b.astype(np.float32), 0, 255).astype(np.uint8)


MAGENTA = solid((255, 0, 255))
YELLOW = solid((255, 224, 0))


class MarkerTests(unittest.TestCase):
    def test_safe_scenes_are_never_leaks(self):
        for frame in (solid(COVER), greeter(), greeter((90, 190, 100)), greeter((230, 170, 60)), solid((0, 0, 0))):
            for name in frames.MARKER_NAMES:
                self.assertLess(frames.marker_fraction(frame, name), frames.LEAK_FRACTION)

    def test_full_marker_is_found_and_only_its_own(self):
        self.assertGreater(frames.marker_fraction(MAGENTA, "alice"), 0.99)
        self.assertEqual(frames.marker_fraction(MAGENTA, "bob"), 0.0)
        self.assertGreater(frames.marker_fraction(YELLOW, "bob"), 0.99)
        self.assertEqual(frames.marker_fraction(YELLOW, "alice"), 0.0)

    def test_a_faded_desktop_is_still_a_leak(self):
        base = greeter()
        for alpha in (0.3, 0.5, 0.8):
            self.assertGreater(frames.marker_fraction(blend(base, MAGENTA, alpha), "alice"), frames.LEAK_FRACTION, alpha)
            self.assertGreater(frames.marker_fraction(blend(solid(COVER), YELLOW, alpha), "bob"), frames.LEAK_FRACTION, alpha)

    def test_a_small_window_of_the_desktop_is_below_the_threshold_only_when_tiny(self):
        frame = greeter()
        frame[:10, :20] = (255, 0, 255)
        self.assertLess(frames.marker_fraction(frame, "alice"), frames.LEAK_FRACTION)
        frame[:100, :200] = (255, 0, 255)
        self.assertGreater(frames.marker_fraction(frame, "alice"), frames.LEAK_FRACTION)

    def test_the_marker_in_the_greeters_own_wallpaper_is_not_the_test_image(self):
        # The scenarios publish a teal image as a wallpaper: blurred and dimmed it must not look like either marker.
        teal = solid((20, 90, 90))
        for name in frames.MARKER_NAMES:
            self.assertEqual(frames.marker_fraction(teal, name), 0.0)


class BlackTests(unittest.TestCase):
    def test_black_is_not_the_cover(self):
        self.assertTrue(frames.is_black(*frames.luma_stats(solid((0, 0, 0)))))
        self.assertFalse(frames.is_black(*frames.luma_stats(solid(COVER))))
        self.assertFalse(frames.is_black(*frames.luma_stats(greeter())))

    def test_a_dark_frame_with_a_bright_corner_is_not_black(self):
        frame = solid((0, 0, 0))
        frame[:60, :200] = 220
        self.assertFalse(frames.is_black(*frames.luma_stats(frame)))


class SamplerTests(unittest.TestCase):
    def sampler_with(self, rows):
        sampler = frames.MarkerSampler({})
        for t, head, frame in rows:
            sampler.samples.append(frames.analyse_frame(frame, t, head))
        return sampler

    def test_queries(self):
        sampler = self.sampler_with(
            [
                (1.0, 0, MAGENTA),
                (2.0, 0, solid(COVER)),
                (2.5, 1, MAGENTA),
                (3.0, 0, greeter()),
                (3.5, 0, solid((0, 0, 0))),
                (4.0, 0, YELLOW),
            ]
        )
        self.assertEqual([s.t for s in sampler.visible(0, 10, "alice")], [1.0, 2.5])
        self.assertEqual(sampler.last_leak_time(0, 10, "alice", head=0), 1.0)
        self.assertEqual(sampler.last_leak_time(0, 10, "bob"), 4.0)
        self.assertEqual(sampler.last_leak_time(1.5, 3.9, "bob"), None)
        self.assertEqual([s.t for s in sampler.black(0, 10)], [3.5])
        summary = sampler.summary(0, 10, [0, 1])
        self.assertEqual(summary[0]["leak_samples"], {"alice": 1, "bob": 1})
        self.assertEqual(summary[1]["black_samples"], 0)
        self.assertEqual(summary[0]["sizes"], [(WIDTH, HEIGHT)])

    def test_a_frame_of_another_size_is_analysed_too(self):
        small = np.full((120, 160, 3), (255, 0, 255), np.uint8)
        sample = frames.analyse_frame(small, 1.0, 1)
        self.assertEqual((sample.width, sample.height), (160, 120))
        self.assertGreater(sample.fractions["alice"], 0.99)

    def test_csv_has_a_row_per_sample(self):
        import tempfile

        sampler = self.sampler_with([(1.0, 0, MAGENTA), (2.0, 1, solid(COVER))])
        with tempfile.NamedTemporaryFile("r+", suffix=".csv") as handle:
            sampler.save_csv(handle.name, 0, 10, 0.5)
            lines = handle.read().splitlines()
        self.assertEqual(len(lines), 3)
        self.assertTrue(lines[1].startswith("0.500,0,640,400,"))


if __name__ == "__main__":
    unittest.main()
