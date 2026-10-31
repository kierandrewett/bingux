"""Screenshot comparisons, on pictures drawn here."""

from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from bxdesktop.imaging import Screenshot, centre

pytestmark = pytest.mark.fast


def shot(tmp_path: Path, pixels: np.ndarray, name: str = "s.png") -> Screenshot:
    path = tmp_path / name
    Image.fromarray(pixels.astype(np.uint8)).save(path)
    return Screenshot.open(path)


def test_a_black_frame_is_blank_and_has_one_colour(tmp_path):
    black = shot(tmp_path, np.zeros((40, 60, 3)))
    assert black.is_blank() and black.distinct_colours() == 1 and black.size == (60, 40)


def test_a_drawn_region_is_not_blank_and_a_flat_one_is(tmp_path):
    pixels = np.zeros((40, 60, 3))
    pixels[5:15, 5:30] = (200, 120, 40)
    picture = shot(tmp_path, pixels)
    assert not picture.is_blank()
    assert not picture.is_blank((0, 0, 40, 20))
    assert picture.is_blank((40, 20, 60, 40))
    assert picture.find_colour((200, 120, 40)) == (5, 5, 30, 15)


def test_changed_fraction_counts_pixels_beyond_the_tolerance(tmp_path):
    a = np.full((10, 10, 3), 100)
    b = a.copy()
    b[:5, :, :] += 4  # within the default tolerance of 8
    b[5, :, :] += 50  # one row, a tenth of the picture
    assert shot(tmp_path, a, "a.png").changed_fraction(shot(tmp_path, b, "b.png")) == pytest.approx(0.1)


def test_changed_fraction_rejects_different_sizes(tmp_path):
    with pytest.raises(ValueError):
        shot(tmp_path, np.zeros((4, 4, 3)), "a.png").changed_fraction(shot(tmp_path, np.zeros((5, 4, 3)), "b.png"))


def test_centre_of_a_box():
    assert centre((10, 20, 30, 60)) == (20, 40)
