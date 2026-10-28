"""Exact-area rasteriser and the pixel metrics `build.py --check` judges icons by.

Icons are compiled to vector outlines on a 16 unit grid; Bingux draws them at 12 to 20 px.
This module turns an outline (a list of closed polygons, curves already flattened) into the
per-pixel coverage a box-filter anti-aliaser gives (the signed-area accumulation used by
font rasterisers, exact for straight edges, so a pixel on an axis-aligned edge reads exactly the
fraction of it that is inked) and measures:

- `ink_extents`: the ink bounding box in 16 grid units, read from the coverage (a pixel
  that is 40 % inked at the left edge puts the edge 0.6 px into that pixel);
- `soft_edge_pixels`: partly covered pixels that sit on a straight horizontal or vertical
  edge run. A crisp icon has none: its straight edges meet pixel boundaries;
- `asymmetry`: how far the coverage is from its own left-right mirror image.

No Skia and no cargo: `sheets.py` still renders through Freya for the final look.
"""

from __future__ import annotations

import math

import numpy as np

Point = tuple[float, float]


def coverage(polys: list[list[Point]], size: int, grid: float = 16.0) -> np.ndarray:
    """Coverage in 0..1 of a `size` x `size` raster of the outline `polys` (grid units)."""
    k = size / grid
    acc = np.zeros((size, size + 2), dtype=np.float64)
    for poly in polys:
        n = len(poly)
        for i in range(n):
            x0, y0 = poly[i]
            x1, y1 = poly[(i + 1) % n]
            _line(acc, x0 * k, y0 * k, x1 * k, y1 * k, size)
    cov = np.abs(np.cumsum(acc, axis=1))[:, :size]
    return np.clip(cov, 0.0, 1.0)


def _line(acc: np.ndarray, x0: float, y0: float, x1: float, y1: float, size: int) -> None:
    if y0 == y1:
        return
    direction = 1.0
    if y0 > y1:
        x0, y0, x1, y1 = x1, y1, x0, y0
        direction = -1.0
    dxdy = (x1 - x0) / (y1 - y0)
    x = x0
    if y0 < 0:
        x -= y0 * dxdy
        y0 = 0.0
    top = int(y0)
    for y in range(top, min(size, int(math.ceil(y1)))):
        dy = min(y + 1, y1) - max(y, y0)
        if dy <= 0:
            continue
        xnext = x + dxdy * dy
        d = dy * direction
        a, b = (x, xnext) if x < xnext else (xnext, x)
        a = min(max(a, 0.0), float(size))
        b = min(max(b, 0.0), float(size))
        a_floor = math.floor(a)
        ai = int(a_floor)
        bi = int(math.ceil(b))
        row = acc[y]
        if bi <= ai + 1:
            xmf = 0.5 * (a + b) - a_floor
            row[ai] += d - d * xmf
            row[ai + 1] += d * xmf
        else:
            s = 1.0 / (b - a)
            af = a - a_floor
            a0 = 0.5 * s * (1.0 - af) * (1.0 - af)
            bf = b - bi + 1.0
            am = 0.5 * s * bf * bf
            row[ai] += d * a0
            if bi == ai + 2:
                row[ai + 1] += d * (1.0 - a0 - am)
            else:
                a1 = s * (1.5 - af)
                row[ai + 1] += d * (a1 - a0)
                for xi in range(ai + 2, bi - 1):
                    row[xi] += d * s
                a2 = a1 + (bi - ai - 3) * s
                row[bi - 1] += d * (1.0 - a2 - am)
            row[bi] += d * am
        x = xnext


def ink_extents(cov: np.ndarray, grid: float = 16.0, floor: float = 0.04) -> tuple[float, float, float, float]:
    """(left, top, right, bottom) of the ink in grid units, from the coverage."""
    size = cov.shape[0]
    cols = cov.max(axis=0)
    rows = cov.max(axis=1)
    xs = np.nonzero(cols > floor)[0]
    ys = np.nonzero(rows > floor)[0]
    if len(xs) == 0 or len(ys) == 0:
        return (0.0, 0.0, 0.0, 0.0)
    k = grid / size
    left = xs[0] + (1.0 - cols[xs[0]])
    right = xs[-1] + cols[xs[-1]]
    top = ys[0] + (1.0 - rows[ys[0]])
    bottom = ys[-1] + rows[ys[-1]]
    return (left * k, top * k, right * k, bottom * k)


def soft_edge_pixels(
    cov: np.ndarray, partial: tuple[float, float] = (0.15, 0.85), like: float = 0.05, run: int = 3
) -> int:
    """Partly covered pixels on a straight horizontal or vertical edge.

    A straight edge that is not on a pixel boundary reads as a row (or column) of at least `run`
    neighbouring pixels with the same partial coverage. Those pixels are counted. A diagonal or
    curved edge changes coverage from pixel to pixel, the rounded end of a stroke is shorter than
    `run`, and neither is counted.
    """
    lo, hi = partial
    part = (cov > lo) & (cov < hi)
    counted = np.zeros(cov.shape, dtype=bool)
    for grid, flip in ((cov, False), (cov.T, True)):
        p = part.T if flip else part
        mark = np.zeros_like(p)
        for y in range(grid.shape[0]):
            x = 0
            while x < grid.shape[1]:
                if not p[y, x]:
                    x += 1
                    continue
                end = x + 1
                while end < grid.shape[1] and p[y, end] and abs(grid[y, end] - grid[y, end - 1]) <= like:
                    end += 1
                if end - x >= run:
                    mark[y, x:end] = True
                x = end
        counted |= mark.T if flip else mark
    return int(np.count_nonzero(counted))


def asymmetry(cov: np.ndarray) -> tuple[float, float]:
    """(largest, mean) absolute difference between the coverage and its left-right mirror.

    The mean is over the inked pixels, so a large icon is not judged more leniently than a small one.
    """
    diff = np.abs(cov - cov[:, ::-1])
    inked = max(1, int(np.count_nonzero((cov > 0.02) | (cov[:, ::-1] > 0.02))))
    return float(diff.max()), float(diff.sum() / inked)
