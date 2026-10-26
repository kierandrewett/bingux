"""Frame analysis for the bsm transition tests.

Pure numpy functions over RGB frames (H x W x 3, uint8). No VM, no QEMU: they
are unit-tested on synthetic transitions by tests/bsm-frames.test.py and used by
tests/bsm-qemu.py on the frames the VNC recorder delivers.

Three measurements:

- `fade_alpha(frame, under, over)`: how much of `over` is composited on `under`
  (0 to 1). New login: `under` is the greeter, `over` the ready desktop.
- `slide_offset(frame, scene, below)`: how far, in pixels, `scene` has moved up
  and off the screen, with `below` visible in the revealed strip at the bottom.
  Unlock: `scene` is the locked greeter, `below` the desktop.
- `coverage(frame)`: fraction of pixels that are not the uniform cover colour.

All of them work on a decimated luma plane (`luma_plane`; every function also
takes a plane directly) so a sample costs about a millisecond, which lets the
tests sample frames far faster than the video's 10 fps slots.
"""

from __future__ import annotations

import numpy as np

DECIMATE = 4


def luma_plane(frame):
    """Decimated float luma of an RGB frame; a 2-D array is taken to be a plane already."""
    if frame.ndim == 2:
        return frame
    small = frame[::DECIMATE, ::DECIMATE].astype(np.float32)
    return small[..., 0] * 0.299 + small[..., 1] * 0.587 + small[..., 2] * 0.114


def discriminating_mask(under_luma, over_luma, threshold=24.0):
    """Pixels where the two scenes differ enough for a blend ratio to be measurable."""
    return np.abs(over_luma - under_luma) >= threshold


def fade_alpha(frame, under, over, *, under_luma=None, over_luma=None, mask=None):
    """Estimated opacity of `over` on `under` in `frame`, or None if the scenes barely differ.

    Over the pixels where `under` and `over` differ, a frame blended with alpha a
    has luma under + a * (over - under), so a is the mean of
    (frame - under) / (over - under) there. The median resists a moving progress
    bar or a cursor.
    """
    u = luma_plane(under) if under_luma is None else under_luma
    o = luma_plane(over) if over_luma is None else over_luma
    m = discriminating_mask(u, o) if mask is None else mask
    if int(m.sum()) < 200:
        return None
    f = luma_plane(frame)
    ratio = (f[m] - u[m]) / (o[m] - u[m])
    return float(np.clip(np.median(ratio), -0.5, 1.5))


def blend_residual(frame, under, over, alpha):
    """Mean absolute luma error between `frame` and the blend of `under` and `over` at `alpha`.

    Small when the frame really is the greeter with the desktop fading in over it;
    large for a black flash or a cut through some other scene.
    """
    u = luma_plane(under)
    o = luma_plane(over)
    f = luma_plane(frame)
    return float(np.abs(f - ((1.0 - alpha) * u + alpha * o)).mean())


def slide_offset(frame, scene, below, *, step=2, scene_luma=None, below_luma=None):
    """Pixels the `scene` has slid up, estimated on the decimated plane, or None when nothing matches.

    A frame with the scene translated up by y has the scene's rows y..H at rows
    0..H-y and the `below` scene's rows H-y..H underneath. The best y is the one
    with the smallest error over the whole frame. `step` is in decimated rows.
    Returns (offset_in_full_resolution_pixels, error).
    """
    s = luma_plane(scene) if scene_luma is None else scene_luma
    b = luma_plane(below) if below_luma is None else below_luma
    f = luma_plane(frame)
    height = f.shape[0]
    best = None
    for y in range(0, height + 1, step):
        top = f[: height - y]
        err_top = np.abs(top - s[y:]).mean() if height - y > 0 else 0.0
        bottom = f[height - y :]
        err_bottom = np.abs(bottom - b[height - y :]).mean() if y > 0 else 0.0
        weight_top = (height - y) / height
        error = err_top * weight_top + err_bottom * (1.0 - weight_top)
        if best is None or error < best[1]:
            best = (y, float(error))
    if best is None:
        return None
    return best[0] * DECIMATE, best[1]


def coverage(frame, tolerance=6.0):
    """Fraction of the frame that differs from its own most common colour by more than `tolerance`.

    0.0 for a uniform cover (any colour), larger for real content.
    """
    plane = luma_plane(frame)
    values, counts = np.unique(np.round(plane).astype(np.int16), return_counts=True)
    mode = float(values[counts.argmax()])
    return float((np.abs(plane - mode) > tolerance).mean())


def similarity(frame, reference):
    """1 minus the mean absolute luma difference over 255, in 0 to 1."""
    return float(1.0 - np.abs(luma_plane(frame) - luma_plane(reference)).mean() / 255.0)


def is_monotone(values, *, decreasing=False, slack=0.02):
    """True when the series never moves against its direction by more than `slack` (of its range)."""
    if len(values) < 2:
        return True
    series = np.array(values, dtype=np.float64)
    span = float(series.max() - series.min()) or 1.0
    deltas = np.diff(series)
    if decreasing:
        deltas = -deltas
    return bool((deltas >= -slack * span).all())
