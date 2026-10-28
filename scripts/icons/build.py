#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["skia-pathops==0.9.2", "svgelements>=1.9,<2", "numpy>=1.26"]
# ///
"""Build the Bingux icon set from hand-drawn sources.

Sources live in scripts/icons/src/<name>.svg on a 16 x 16 grid. Most are one line
that takes the chat icon fork's own drawing (see labels.tsv and map-fork.py):

  <fork name="Power" weight="bold" mode="exact"/>

`mode="exact"` emits the fork's outline untouched (scaled by 1/16 through a transform,
and moved as a whole by at most half a pixel so that straight edges meet pixel
boundaries; `dx`/`dy` add to that, `align="none"` turns it off). Custom icons are
written in a small dialect of plain SVG shapes, which may also include `<fork/>`
pieces (`mode="exact"` keeps a piece untouched, scaled by `scale`, moved by `dx`/`dy`; without
`mode` a piece is dilated and snapped, see "the chat fork's geometry" below):

- Root `<svg viewBox="0 0 16 16">` sets the defaults that children inherit:
  fill="none" stroke="none" stroke-width="2" stroke-linecap="round"
  stroke-linejoin="round". Override them on `<g>` or on a shape.
- Any shape with fill != "none" is filled. Any shape with stroke != "none" is
  stroked (outline of the centreline). Colours are ignored: the output is tinted
  by bingux-ui at draw time.
- `data-op="cut"` subtracts the shape from everything painted before it.
- `data-round="r"` on polyline/polygon rounds each corner with radius r.
- Shapes: rect (rx), circle, ellipse, line, polyline, polygon, path.

Output is SVG, one file per source (a single fill path for custom icons), with the
same header as the Adwaita files it replaces. Output is deterministic: the same sources always give
byte-identical files.

Source rules, checked on every source:
- stroke widths are whole numbers, 1, 2 or 3 (3 only for a cut);
- every axis-aligned edge of a rect, line or polygon has its outer boundary on the pixel grid
  (centreline at an integer for width 2, at n + 0.5 for width 1);
- filled axis-aligned edges sit on integers;
- ink stays inside the 16 x 16 box.

Pixel rules, checked on the rasterised output by --check and --lint (see "pixel rules" below
and docs/bingux-ui.md): ink size by optical class, ink centred, partly covered pixels on
straight edges, left-right symmetry of the icons labels.tsv lists, no hole closed by fitting,
and the fill/outline weight of each icon's category.

Usage:
  build.py                    compile sources into packages/bingux-ui/icons
  build.py --output DIR       compile into DIR instead (for comparison copies)
  build.py --check            validate sources, determinism, icons.rs references, that the
                              output directory matches the sources, and every pixel rule
  build.py --lint [--src DIR] only the pixel rules (DIR: a draft or probe directory)
  build.py --measure          the pixel rules plus a table of every icon's measurements
  build.py --suggest [ICON]   scale, dx, dy for fork rows of labels.tsv that satisfy the rules
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import shutil
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import pathops
import raster
from labels import Labels, load as load_labels
from svgelements import Arc, Close, CubicBezier, Move
from svgelements import Line as SvgLine
from svgelements import Path as SvgPath
from svgelements import QuadraticBezier

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
SRC = HERE / "src"
COPYING_SRC = HERE / "LICENSE"
DEFAULT_OUT = REPO / "packages/bingux-ui/icons"
SHELL_OUT = REPO / "shell/bingux/icons"
SHELL_INDEX = REPO / "shell/bingux/BinguxIconIndex.js"
SHELL_FILL = "#2e3436"
FORK_GEOMETRY = REPO / "packages/bingux-ui/icons/chat/geometry.json"
ICONS_RS = REPO / "packages/bingux-ui/src/icons.rs"

GRID = 16.0
SVG_NS = "{http://www.w3.org/2000/svg}"
INHERITED = ("fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin")
DEFAULTS = {
    "fill": "none",
    "stroke": "none",
    "stroke-width": "2",
    "stroke-linecap": "round",
    "stroke-linejoin": "round",
}
ALLOWED_STROKE_WIDTHS = (1.0, 1.5, 2.0, 3.0)
CAPS = {
    "butt": pathops.LineCap.BUTT_CAP,
    "round": pathops.LineCap.ROUND_CAP,
    "square": pathops.LineCap.SQUARE_CAP,
}
JOINS = {
    "miter": pathops.LineJoin.MITER_JOIN,
    "round": pathops.LineJoin.ROUND_JOIN,
    "bevel": pathops.LineJoin.BEVEL_JOIN,
}
EPS = 1e-6


class BuildError(Exception):
    """A source is invalid. Raised before any output file is touched."""


@dataclass
class Painted:
    path: pathops.Path
    cut: bool
    where: str


@dataclass
class Compiled:
    name: str
    svg: str
    ink: tuple[float, float, float, float]
    path: pathops.Path
    notes: list[str] = field(default_factory=list)


# ---------------------------------------------------------------- geometry text


def fmt(v: float) -> str:
    """Format a coordinate for output: 3 decimals, no trailing zeros, no -0."""
    s = f"{v:.3f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def fmt_src(v: float) -> str:
    """Format a number for generated source-path text (round-trips exactly)."""
    return repr(float(v)) if v != int(v) else str(int(v))


def parse_float(attr: dict[str, str], key: str, default: float = 0.0) -> float:
    raw = attr.get(key)
    if raw is None:
        return default
    try:
        return float(raw.strip().removesuffix("px"))
    except ValueError as exc:
        raise BuildError(f"attribute {key}={raw!r} is not a number") from exc


SMOOTHING = 0.6
"""Default corner smoothing of rounded rects (0 = circular arcs, 1 = most gradual).

Chat's icons use continuous (squircle) corners: curvature eases in and out
instead of jumping from straight to circular. The corner is cubic, arc, cubic,
the construction behind Figma's corner smoothing, with the same handle maths.
"""


def rect_d(x: float, y: float, w: float, h: float, rx: float, smoothing: float = SMOOTHING) -> str:
    rx = min(max(rx, 0.0), w / 2, h / 2)
    if rx <= EPS:
        return f"M{fmt_src(x)} {fmt_src(y)}H{fmt_src(x + w)}V{fmt_src(y + h)}H{fmt_src(x)}Z"
    # The corner reaches p = (1 + s) r along each edge; it cannot pass half an edge.
    smoothing = max(0.0, min(smoothing, min(w, h) / 2 / rx - 1.0))
    f = fmt_src
    if smoothing <= EPS:
        r = f(rx)
        return (
            f"M{f(x + rx)} {f(y)}"
            f"H{f(x + w - rx)}A{r} {r} 0 0 1 {f(x + w)} {f(y + rx)}"
            f"V{f(y + h - rx)}A{r} {r} 0 0 1 {f(x + w - rx)} {f(y + h)}"
            f"H{f(x + rx)}A{r} {r} 0 0 1 {f(x)} {f(y + h - rx)}"
            f"V{f(y + rx)}A{r} {r} 0 0 1 {f(x + rx)} {f(y)}Z"
        )
    p = (1.0 + smoothing) * rx
    arc_measure = 90.0 * (1.0 - smoothing)
    arc_len = math.sin(math.radians(arc_measure / 2)) * rx * math.sqrt(2)
    alpha = (90.0 - arc_measure) / 2
    p3_p4 = rx * math.tan(math.radians(alpha / 2))
    beta = 45.0 * smoothing
    c = p3_p4 * math.cos(math.radians(beta))
    d = c * math.tan(math.radians(beta))
    b = (p - arc_len - c - d) / 3
    a = 2 * b
    # Clockwise corners as (corner point, incoming unit vector u, outgoing unit vector v).
    corners = [
        ((x + w, y), (1, 0), (0, 1)),
        ((x + w, y + h), (0, 1), (-1, 0)),
        ((x, y + h), (-1, 0), (0, -1)),
        ((x, y), (0, -1), (1, 0)),
    ]

    def at(origin, u, v, px, py):
        return f"{f(round(origin[0] + px * u[0] + py * v[0], 6))} {f(round(origin[1] + px * u[1] + py * v[1], 6))}"

    parts = [f"M{f(x + p)} {f(y)}"]
    for corner, u, v in corners:
        start = (corner[0] - p * u[0], corner[1] - p * u[1])
        parts.append(f"L{f(round(start[0], 6))} {f(round(start[1], 6))}")
        parts.append(f"C{at(start, u, v, a, 0)} {at(start, u, v, a + b, 0)} {at(start, u, v, a + b + c, d)}")
        parts.append(f"A{f(rx)} {f(rx)} 0 0 1 {at(start, u, v, a + b + c + arc_len, d + arc_len)}")
        parts.append(f"C{at(start, u, v, p, d + arc_len + c)} {at(start, u, v, p, d + arc_len + b + c)} {at(start, u, v, p, p)}")
    return "".join(parts) + "Z"


def ellipse_d(cx: float, cy: float, rx: float, ry: float) -> str:
    if rx <= EPS or ry <= EPS:
        raise BuildError("circle/ellipse needs a positive radius")
    a, b = fmt_src(rx), fmt_src(ry)
    return (
        f"M{fmt_src(cx - rx)} {fmt_src(cy)}"
        f"A{a} {b} 0 1 0 {fmt_src(cx + rx)} {fmt_src(cy)}"
        f"A{a} {b} 0 1 0 {fmt_src(cx - rx)} {fmt_src(cy)}Z"
    )


def parse_points(raw: str) -> list[tuple[float, float]]:
    nums = [float(n) for n in re.findall(r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?", raw)]
    if len(nums) < 4 or len(nums) % 2:
        raise BuildError(f"points {raw!r} need an even count of at least 2 pairs")
    return list(zip(nums[0::2], nums[1::2]))


def rounded_poly_d(pts: list[tuple[float, float]], radius: float, closed: bool) -> str:
    """Polyline or polygon with each interior corner replaced by a quadratic fillet.

    The cut distance along each edge is min(radius, half of each adjacent edge),
    so fillets never overlap. Endpoints of open polylines stay sharp.
    """
    n = len(pts)

    def towards(a, b, t):
        dx, dy = b[0] - a[0], b[1] - a[1]
        length = math.hypot(dx, dy)
        if length < EPS:
            raise BuildError("zero-length edge in polyline/polygon")
        return (a[0] + dx / length * t, a[1] + dy / length * t), length

    def corner(i):
        p = pts[i]
        if closed:
            prev, nxt = pts[i - 1], pts[(i + 1) % n]
        elif 0 < i < n - 1:
            prev, nxt = pts[i - 1], pts[i + 1]
        else:
            return p, p
        _, dl_prev = towards(p, prev, 0.0)
        _, dl_next = towards(p, nxt, 0.0)
        t = min(radius, dl_prev / 2, dl_next / 2)
        if t <= EPS:
            return p, p
        entry, _ = towards(p, prev, t)
        exit_, _ = towards(p, nxt, t)
        return entry, exit_

    corners = [corner(i) for i in range(n)]
    f = fmt_src
    if closed:
        x0 = corners[0][1]
        parts = [f"M{f(x0[0])} {f(x0[1])}"]
        for i in list(range(1, n)) + [0]:
            e, x = corners[i]
            p = pts[i]
            parts.append(f"L{f(e[0])} {f(e[1])}Q{f(p[0])} {f(p[1])} {f(x[0])} {f(x[1])}")
        return "".join(parts) + "Z"
    parts = [f"M{f(pts[0][0])} {f(pts[0][1])}"]
    for i in range(1, n - 1):
        e, x = corners[i]
        p = pts[i]
        parts.append(f"L{f(e[0])} {f(e[1])}Q{f(p[0])} {f(p[1])} {f(x[0])} {f(x[1])}")
    parts.append(f"L{f(pts[-1][0])} {f(pts[-1][1])}")
    return "".join(parts)


def shape_d(tag: str, a: dict[str, str]) -> str:
    if tag == "rect":
        return rect_d(
            parse_float(a, "x"),
            parse_float(a, "y"),
            parse_float(a, "width"),
            parse_float(a, "height"),
            parse_float(a, "rx", parse_float(a, "ry")),
            parse_float(a, "data-smooth", SMOOTHING),
        )
    if tag == "circle":
        r = parse_float(a, "r")
        return ellipse_d(parse_float(a, "cx"), parse_float(a, "cy"), r, r)
    if tag == "ellipse":
        return ellipse_d(
            parse_float(a, "cx"),
            parse_float(a, "cy"),
            parse_float(a, "rx"),
            parse_float(a, "ry"),
        )
    if tag == "line":
        x1, y1 = parse_float(a, "x1"), parse_float(a, "y1")
        x2, y2 = parse_float(a, "x2"), parse_float(a, "y2")
        return f"M{fmt_src(x1)} {fmt_src(y1)}L{fmt_src(x2)} {fmt_src(y2)}"
    if tag in ("polyline", "polygon"):
        pts = parse_points(a.get("points", ""))
        radius = parse_float(a, "data-round")
        closed = tag == "polygon"
        if radius > 0:
            return rounded_poly_d(pts, radius, closed)
        d = "M" + " L".join(f"{fmt_src(x)} {fmt_src(y)}" for x, y in pts)
        return d + ("Z" if closed else "")
    if tag == "path":
        d = a.get("d", "").strip()
        if not d:
            raise BuildError("path without d")
        return d
    raise BuildError(f"unsupported element <{tag}>")


# ------------------------------------------------------------- pathops building


def d_to_path(d: str) -> pathops.Path:
    """Convert SVG path data into a pathops path, with arcs turned into cubics."""
    out = pathops.Path()
    open_sub = False
    for seg in SvgPath(d):
        if isinstance(seg, Move):
            out.moveTo(seg.end.x, seg.end.y)
            open_sub = True
        elif isinstance(seg, Close):
            if open_sub:
                out.close()
                open_sub = False
        elif isinstance(seg, SvgLine):
            if not open_sub:
                raise BuildError("path segment before moveTo")
            out.lineTo(seg.end.x, seg.end.y)
        elif isinstance(seg, CubicBezier):
            out.cubicTo(seg.control1.x, seg.control1.y, seg.control2.x, seg.control2.y, seg.end.x, seg.end.y)
        elif isinstance(seg, QuadraticBezier):
            out.quadTo(seg.control.x, seg.control.y, seg.end.x, seg.end.y)
        elif isinstance(seg, Arc):
            for cub in seg.as_cubic_curves():
                out.cubicTo(cub.control1.x, cub.control1.y, cub.control2.x, cub.control2.y, cub.end.x, cub.end.y)
        else:
            raise BuildError(f"unsupported path segment {type(seg).__name__}")
    return out


def segments_of(path: pathops.Path) -> list[tuple[str, list[tuple[float, float]]]]:
    """Return (verb, points) with quadratic runs split into single quads.

    pathops closes a quadratic contour with a qCurveTo whose on-curve end is
    None, meaning "back to the contour start". A contour with no moveTo is
    all off-curve: its start is the midpoint of the last and first control
    points. Both cases are resolved to explicit on-curve points here.
    """
    result: list[tuple[str, list[tuple[float, float]]]] = []
    start: tuple[float, float] | None = None
    for verb, pts in path.segments:
        if verb == "moveTo":
            start = tuple(pts[0])
            result.append((verb, [start]))
        elif verb == "closePath":
            start = None
            result.append((verb, []))
        elif verb == "qCurveTo" and pts[-1] is None:
            ctrls = list(pts[:-1])
            if start is None:
                n = len(ctrls)
                result.append(("moveTo", [_mid(ctrls[-1], ctrls[0])]))
                for i, ctrl in enumerate(ctrls):
                    end = _mid(ctrl, ctrls[(i + 1) % n])
                    result.append(("qCurveTo", [ctrl, end]))
            else:
                for ctrl, end in pathops.decompose_quadratic_segment(
                    (*ctrls, start)
                ):
                    result.append(("qCurveTo", [ctrl, end]))
        elif verb == "qCurveTo":
            for ctrl, end in pathops.decompose_quadratic_segment(pts):
                result.append(("qCurveTo", [ctrl, end]))
        else:
            result.append((verb, list(pts)))
    return result


def _mid(a: tuple[float, float], b: tuple[float, float]) -> tuple[float, float]:
    return ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)


def path_d(path: pathops.Path) -> str:
    path.convertConicsToQuads(0.02)
    parts: list[str] = []
    here: tuple[float, float] | None = None
    for verb, pts in segments_of(path):
        if verb == "moveTo":
            parts.append(f"M{fmt(pts[0][0])} {fmt(pts[0][1])}")
            here = pts[0]
        elif verb == "lineTo":
            if here is not None and abs(pts[0][0] - here[0]) < 5e-4 and abs(pts[0][1] - here[1]) < 5e-4:
                continue
            parts.append(f"L{fmt(pts[0][0])} {fmt(pts[0][1])}")
            here = pts[0]
        elif verb == "qCurveTo":
            (cx, cy), (ex, ey) = pts
            parts.append(f"Q{fmt(cx)} {fmt(cy)} {fmt(ex)} {fmt(ey)}")
            here = pts[-1]
        elif verb == "curveTo":
            parts.append("C" + " ".join(f"{fmt(x)} {fmt(y)}" for x, y in pts))
            here = pts[-1]
        elif verb == "closePath":
            parts.append("Z")
        elif verb == "endPath":
            continue
        else:
            raise BuildError(f"unexpected output verb {verb}")
    return "".join(parts)


# ------------------------------------------------------- the chat fork's geometry
#
# `<fork name="Power" weight="bold"/>` pulls the rounded Phosphor geometry that
# ~/dev/chat/packages/icons generated (vendored in packages/bingux-ui/icons/chat/geometry.json by
# import-chat.py) onto the 16 grid. The outline is the fork's own; only these things
# ever happen to it, each one a per-icon attribute written by map-fork.py from labels.tsv:
#
# 1. `mirror="left"|"right"`: keep that half and replace the other with its mirror image
#    (for glyphs the fork draws a little lopsided, such as User);
# 2. `scale` grows or shrinks the whole drawing about the middle of the box, so each icon
#    fills its box by its optical size (see RULES);
# 3. stroke widths are made whole pixels: a bold stroke (1.5 px, times `scale`) is grown
#    or shrunk to the nearest whole pixel by offsetting the outline with round joins
#    (at most 0.25 px a side), a regular one (1 px) likewise. Fill glyphs have no stroke.
#    `stroke="none"` leaves the width alone;
# 4. `dx`/`dy` move the whole drawing, so its straight edges meet pixel boundaries
#    and its ink is centred;
# 5. snap: every long horizontal or vertical edge and every curve extreme is moved to the
#    nearest pixel line by a monotone piecewise linear warp (at most half a pixel; half
#    way cases move away from the middle). `snap="none"` turns it off. A symmetric icon
#    (labels.tsv `#symmetric`) is warped on its left half and mirrored.
#
# Curves and corners are never redrawn. When none of this changes the outline it is written
# as the fork's own path data under a `transform`, byte for byte.

_FORK: dict | None = None
FORK_STROKE = {"regular": 1.0, "bold": 1.5, "fill": 0.0}


def fork_geometry(name: str, weight: str, where: str) -> list[str]:
    global _FORK
    if _FORK is None:
        if not FORK_GEOMETRY.exists():
            raise BuildError(f"{FORK_GEOMETRY} is missing; run scripts/icons/import-chat.py")
        _FORK = json.loads(FORK_GEOMETRY.read_text())["icons"]
    entry = _FORK.get(name)
    if entry is None:
        raise BuildError(f"{where}: fork icon {name} is not vendored; run scripts/icons/import-chat.py")
    if weight not in entry:
        raise BuildError(f"{where}: fork icon {name} has no {weight} weight")
    return entry[weight]


def scaled_path(ds: list[str], scale: float) -> pathops.Path:
    out = pathops.Path()
    for d in ds:
        sp = SvgPath(d) * f"scale({scale})"
        sp.reify()
        out.addPath(d_to_path(sp.d()))
    out.convertConicsToQuads(0.02)
    return pathops.simplify(out)


def dilated(path: pathops.Path, amount: float) -> pathops.Path:
    """Offset the outline by `amount` px with round joins: out when positive, in when negative."""
    if abs(amount) <= EPS:
        return path
    band = pathops.Path(path)
    band.stroke(2 * abs(amount), pathops.LineCap.ROUND_CAP, pathops.LineJoin.ROUND_JOIN, 4.0)
    band.convertConicsToQuads(0.02)
    band = pathops.simplify(band)
    if amount > 0:
        return pathops.simplify(pathops.op(path, band, pathops.PathOp.UNION))
    return pathops.simplify(pathops.op(path, band, pathops.PathOp.DIFFERENCE))


def contours_of(path: pathops.Path) -> list[list[tuple[str, list[tuple[float, float]]]]]:
    """Closed contours as [(verb, points)]; the first item is the moveTo."""
    result: list[list[tuple[str, list[tuple[float, float]]]]] = []
    current: list[tuple[str, list[tuple[float, float]]]] = []
    for verb, pts in segments_of(path):
        if verb == "moveTo":
            current = [("M", [tuple(pts[0])])]
        elif verb == "lineTo":
            current.append(("L", [tuple(pts[0])]))
        elif verb == "qCurveTo":
            current.append(("Q", [tuple(p) for p in pts]))
        elif verb == "curveTo":
            current.append(("C", [tuple(p) for p in pts]))
        elif verb == "closePath":
            result.append(current)
            current = []
    return result


def snap_anchors(path: pathops.Path, min_len: float = 0.9, tol: float = 0.02) -> tuple[list[float], list[float]]:
    """x and y coordinates of long axis-aligned edges and of curve extremes."""
    xs: list[float] = []
    ys: list[float] = []
    for contour in contours_of(path):
        segs = []
        prev = contour[0][1][0]
        for kind, pts in contour[1:]:
            segs.append((prev, kind, pts))
            prev = pts[-1]
        n = len(segs)
        for i, (p0, kind, pts) in enumerate(segs):
            end = pts[-1]
            if kind == "L":
                length = math.hypot(end[0] - p0[0], end[1] - p0[1])
                if length >= min_len:
                    if abs(end[0] - p0[0]) < tol:
                        xs.append(end[0])
                    elif abs(end[1] - p0[1]) < tol:
                        ys.append(end[1])
            q_in = pts[-2] if len(pts) >= 2 else p0
            t_in = (end[0] - q_in[0], end[1] - q_in[1])
            nxt = segs[(i + 1) % n]
            t_out = (nxt[2][0][0] - end[0], nxt[2][0][1] - end[1])
            for t in (t_in, t_out):
                length = math.hypot(*t)
                if length < 1e-9:
                    continue
                if abs(t[1]) / length < tol:
                    ys.append(end[1])
                if abs(t[0]) / length < tol:
                    xs.append(end[0])
    return xs, ys


def warped(path: pathops.Path, wx, wy) -> pathops.Path:
    out = pathops.Path()
    for contour in contours_of(path):
        for kind, pts in contour:
            m = [(wx(x), wy(y)) for x, y in pts]
            if kind == "M":
                out.moveTo(*m[0])
            elif kind == "L":
                out.lineTo(*m[0])
            elif kind == "Q":
                out.quadTo(*m[0], *m[1])
            else:
                out.cubicTo(*m[0], *m[1], *m[2])
        out.close()
    return pathops.simplify(out)


def snap_target(value: float) -> float:
    """The pixel line for `value`; half pixels move away from the middle of the box."""
    frac = value - math.floor(value)
    if abs(frac - 0.5) < 0.15:
        return math.floor(value) + 1.0 if value >= GRID / 2 else math.floor(value)
    return float(math.floor(value + 0.5))


def snap_target_inward(value: float) -> float:
    """The pixel line for `value`; half pixels move towards the middle of the box."""
    frac = value - math.floor(value)
    if abs(frac - 0.5) < 0.15:
        return math.ceil(value) if value < GRID / 2 else math.floor(value)
    return float(math.floor(value + 0.5))


def snap_pairs(coords: list[float], eps: float = 0.3) -> list[tuple[float, float]]:
    """(from, to) for each cluster of anchors: `to` is the pixel line the cluster moves to.

    The outermost edges on each side never move outwards (snapping may not grow the ink past
    the size rule); every other half pixel moves away from the middle (two edges never close a gap).
    """
    clusters: list[list[float]] = []
    for v in sorted(coords):
        if clusters and v - clusters[-1][-1] <= eps:
            clusters[-1].append(v)
        else:
            clusters.append([v])
    pairs: list[tuple[float, float]] = []
    for index, cluster in enumerate(clusters):
        c = sum(cluster) / len(cluster)
        t = snap_target_inward(c) if index in (0, len(clusters) - 1) else snap_target(c)
        # Never fold two edges together or reverse their order.
        if pairs and (t <= pairs[-1][1] or (t - pairs[-1][1]) < 0.5 * (c - pairs[-1][0])):
            continue
        pairs.append((c, t))
    return pairs


def warp_of(pairs: list[tuple[float, float]]):
    """The monotone piecewise linear map that sends each `from` to its `to` and is a shift beyond the ends."""
    if not pairs:
        return lambda v: v

    def warp(v: float) -> float:
        if v <= pairs[0][0]:
            return v + (pairs[0][1] - pairs[0][0])
        if v >= pairs[-1][0]:
            return v + (pairs[-1][1] - pairs[-1][0])
        for (a, ta), (b, tb) in zip(pairs, pairs[1:]):
            if a <= v <= b:
                return ta + (v - a) * (tb - ta) / (b - a)
        return v

    return warp


def snap_warps(path: pathops.Path, symmetric: bool):
    """(wx, wy, moved): warps that put the long straight edges and curve extremes on whole pixels.

    Strokes keep their width (their two edges are anchors on the same side of a pixel after
    `stroke_growth`), curves keep their shape (the warp is piecewise linear and moves a point
    by at most half a pixel). For a symmetric icon x is warped on the left half and mirrored,
    so both halves move alike.
    """
    xs, ys = snap_anchors(path)
    if symmetric:
        folded = [min(v, GRID - v) for v in xs] + [GRID / 2]
        left = warp_of(snap_pairs(folded))
        wx = lambda v: left(v) if v <= GRID / 2 else GRID - left(GRID - v)  # noqa: E731
        x_moved = any(abs(left(v) - v) > 1e-3 for v in folded)
    else:
        pairs = snap_pairs(xs)
        wx = warp_of(pairs)
        x_moved = any(abs(t - f) > 1e-3 for f, t in pairs)
    y_pairs = snap_pairs(ys)
    return wx, warp_of(y_pairs), x_moved or any(abs(t - f) > 1e-3 for f, t in y_pairs)


def box_path(left: float, top: float, right: float, bottom: float) -> pathops.Path:
    box = pathops.Path()
    box.moveTo(left, top)
    box.lineTo(right, top)
    box.lineTo(right, bottom)
    box.lineTo(left, bottom)
    box.close()
    return box


def mirrored(path: pathops.Path, keep: str) -> pathops.Path:
    """The `keep` half of `path` (about x = 8) and its mirror image."""
    half = box_path(-GRID, -GRID, GRID / 2, 2 * GRID) if keep == "left" else box_path(GRID / 2, -GRID, 2 * GRID, 2 * GRID)
    kept = pathops.simplify(pathops.op(path, half, pathops.PathOp.INTERSECTION))
    flipped = warped(kept, lambda v: GRID - v, lambda v: v)
    return pathops.simplify(pathops.op(kept, flipped, pathops.PathOp.UNION))


@dataclass
class ForkOutline:
    path: pathops.Path
    # `(paths, transform)` when the outline is the fork's own, so that it can be written as is.
    verbatim: tuple[list[str], str] | None


def stroke_growth(weight: str, zoom: float, mode: str) -> float:
    """How far to offset the outline (px a side) so the stroke is a whole number of pixels."""
    width = FORK_STROKE[weight] * zoom
    if width <= 0 or mode == "none":
        return 0.0
    # Rounds up from 0.35 over (a 1.4 px bold stroke becomes 2 px, not 1): a stroke that thins
    # out reads weaker than its neighbours, one that thickens a little does not.
    grow = (max(1.0, math.ceil(width - 0.35)) - width) / 2
    return 0.0 if abs(grow) < 1e-3 else grow


def fork_outline(attrs: dict[str, str], where: str) -> ForkOutline:
    name = attrs.get("name", "")
    weight = attrs.get("weight", "regular")
    if weight not in FORK_STROKE:
        raise BuildError(f"{where}: fork weight must be regular, bold or fill")
    mirror = attrs.get("mirror", "")
    if mirror not in ("", "left", "right"):
        raise BuildError(f"{where}: mirror must be left or right")
    stroke = attrs.get("stroke", "auto")
    if stroke not in ("auto", "none"):
        raise BuildError(f"{where}: stroke must be auto or none")
    snap = attrs.get("snap", "auto")
    if snap not in ("auto", "none"):
        raise BuildError(f"{where}: snap must be auto or none")
    symmetric = attrs.get("symmetric", "") == "true"
    ds = fork_geometry(name, weight, where)
    zoom = parse_float(attrs, "scale", 1.0)
    if zoom <= 0:
        raise BuildError(f"{where}: scale must be positive")
    # The drawing is scaled about the middle of the box, then moved by dx, dy.
    dx = parse_float(attrs, "dx") + (GRID / 2) * (1 - zoom)
    dy = parse_float(attrs, "dy") + (GRID / 2) * (1 - zoom)
    grow = stroke_growth(weight, zoom, stroke)
    path = scaled_path(ds, 1 / 16)
    if mirror:
        path = mirrored(path, mirror)
    path = dilated(warped(path, lambda v: v * zoom + dx, lambda v: v * zoom + dy), grow)
    moved = False
    if snap == "auto":
        wx, wy, moved = snap_warps(path, symmetric)
        if moved:
            path = warped(path, wx, wy)
    if not mirror and grow == 0.0 and not moved:
        transform = f"translate({fmt(dx)} {fmt(dy)}) " if abs(dx) > EPS or abs(dy) > EPS else ""
        return ForkOutline(path, (ds, f"{transform}scale({fmt(zoom / 16)})"))
    return ForkOutline(path, None)


# ------------------------------------------------------------------- compiling


def effective(stack: list[dict[str, str]], own: dict[str, str]) -> dict[str, str]:
    merged = dict(DEFAULTS)
    for layer in stack:
        for key in INHERITED:
            if key in layer:
                merged[key] = layer[key]
    for key in INHERITED:
        if key in own:
            merged[key] = own[key]
    return merged


def walk(node: ET.Element, stack: list[dict[str, str]], out: list[tuple[str, dict[str, str], dict[str, str]]]) -> None:
    tag = node.tag.removeprefix(SVG_NS)
    if tag in ("svg", "g"):
        layer = {k: node.attrib[k] for k in INHERITED if k in node.attrib}
        for child in node:
            walk(child, stack + [layer], out)
        return
    if tag in ("title", "desc", "defs", "metadata"):
        return
    own = {k: v for k, v in node.attrib.items()}
    out.append((tag, own, effective(stack, own)))


def is_none(value: str) -> bool:
    return value.strip() in ("none", "")


def lint_edges(
    where: str,
    tag: str,
    a: dict[str, str],
    stroke_width: float | None,
    filled: bool,
) -> list[str]:
    """Check that axis-aligned edges sit on the pixel grid. Returns error strings."""
    errors: list[str] = []
    if stroke_width is not None and stroke_width not in ALLOWED_STROKE_WIDTHS:
        errors.append(f"{where}: stroke-width {fmt(stroke_width)} must be 1, 1.5, 2 or 3")
    if tag == "rect":
        x, y = parse_float(a, "x"), parse_float(a, "y")
        w, h = parse_float(a, "width"), parse_float(a, "height")
        lines = [(True, x), (True, x + w), (False, y), (False, y + h)]
    elif tag == "line":
        x1, y1 = parse_float(a, "x1"), parse_float(a, "y1")
        x2, y2 = parse_float(a, "x2"), parse_float(a, "y2")
        lines = []
        if abs(x1 - x2) < EPS:
            lines.append((True, x1))
        if abs(y1 - y2) < EPS:
            lines.append((False, y1))
    elif tag in ("polyline", "polygon") and parse_float(a, "data-round") == 0:
        pts = parse_points(a.get("points", ""))
        if tag == "polygon":
            pts = pts + [pts[0]]
        lines = []
        for (ax, ay), (bx, by) in zip(pts, pts[1:]):
            if abs(ax - bx) < EPS:
                lines.append((True, ax))
            elif abs(ay - by) < EPS:
                lines.append((False, ay))
    else:
        lines = []
    for vertical, coord in lines:
        if stroke_width == 1.5:
            continue  # the fork's bold weight: 1.5 px strokes cannot all sit on whole pixels
        if stroke_width is not None:
            outer = coord - stroke_width / 2
            if abs(outer - round(outer)) > EPS:
                errors.append(f"{where}: edge at {fmt(coord)} is off the grid for stroke {fmt(stroke_width)}")
        elif filled and abs(coord - round(coord)) > EPS:
            errors.append(f"{where}: filled edge at {fmt(coord)} is not on an integer")
    return errors


def compile_fork(src: Path, attrs: dict[str, str]) -> Compiled:
    """An icon that is one `<fork name=".." weight=".."/>`: the fork's outline, see "the chat fork's geometry"."""
    where = src.name
    outline = fork_outline(attrs, where)
    left, top, right, bottom = outline.path.bounds
    if left < -EPS or top < -EPS or right > GRID + EPS or bottom > GRID + EPS:
        raise BuildError(f"{where}: ink {fmt(left)},{fmt(top)} to {fmt(right)},{fmt(bottom)} leaves the 16 x 16 box")
    if outline.verbatim is not None:
        paths, transform = outline.verbatim
        body = "".join(f'    <path transform="{transform}" d="{d}"/>\n' for d in paths)
    else:
        body = f'    <path d="{path_d(outline.path)}"/>\n'
    svg = f'<svg height="16px" viewBox="0 0 16 16" width="16px" xmlns="http://www.w3.org/2000/svg">\n{body}</svg>\n'
    return Compiled(
        name=src.stem, svg=svg, ink=(left, top, right, bottom), notes=soft_edges(outline.path), path=outline.path
    )


def compile_source(src: Path) -> Compiled:
    name = src.stem
    try:
        root = ET.parse(src).getroot()
    except ET.ParseError as exc:
        raise BuildError(f"{src.name}: cannot parse: {exc}") from exc
    if root.tag.removeprefix(SVG_NS) != "svg" or root.attrib.get("viewBox", "").split() != ["0", "0", "16", "16"]:
        raise BuildError(f"{src.name}: root must be <svg viewBox=\"0 0 16 16\">")

    shapes: list[tuple[str, dict[str, str], dict[str, str]]] = []
    walk(root, [], shapes)
    if not shapes:
        raise BuildError(f"{src.name}: no shapes")

    if len(shapes) == 1 and shapes[0][0] == "fork":
        return compile_fork(src, shapes[0][1])

    errors: list[str] = []
    painted: list[Painted] = []
    for index, (tag, own, eff) in enumerate(shapes):
        where = f"{src.name}#{index}<{tag}>"
        cut = own.get("data-op") == "cut"
        if own.get("data-op", "union") not in ("union", "cut"):
            errors.append(f"{where}: data-op must be union or cut")
        if tag == "fork":
            try:
                painted.append(Painted(fork_outline(own, where).path, cut, where))
            except (BuildError, ValueError, IndexError, KeyError) as exc:
                errors.append(str(exc))
            continue
        fill_on = not is_none(eff["fill"]) and tag != "line"
        stroke_on = not is_none(eff["stroke"])
        if not fill_on and not stroke_on:
            errors.append(f"{where}: shape is neither filled nor stroked")
            continue
        try:
            d = shape_d(tag, own)
            base = d_to_path(d)
        except (BuildError, ValueError, IndexError) as exc:
            errors.append(f"{where}: {exc}")
            continue

        stroke_width = None
        if stroke_on:
            try:
                stroke_width = float(eff["stroke-width"])
            except ValueError:
                errors.append(f"{where}: stroke-width {eff['stroke-width']!r} is not a number")
                continue
            if stroke_width not in ALLOWED_STROKE_WIDTHS:
                errors.append(f"{where}: stroke-width {fmt(stroke_width)} must be 1, 1.5, 2 or 3")
        errors += lint_edges(where, tag, own, stroke_width, fill_on)

        if fill_on:
            fill_path = d_to_path(d)
            fill_path.close()
            if own.get("fill-rule", "nonzero") == "evenodd":
                fill_path.fillType = pathops.FillType.EVEN_ODD
            painted.append(Painted(pathops.simplify(fill_path), cut, where))
        if stroke_on:
            stroke_path = d_to_path(d)
            cap = CAPS.get(eff["stroke-linecap"])
            join = JOINS.get(eff["stroke-linejoin"])
            if cap is None or join is None:
                errors.append(f"{where}: unknown stroke-linecap or stroke-linejoin")
                continue
            stroke_path.stroke(stroke_width, cap, join, 4.0)
            stroke_path.convertConicsToQuads(0.02)
            painted.append(Painted(pathops.simplify(stroke_path), cut, where))

    if errors:
        raise BuildError("\n".join(errors))

    acc = pathops.Path()
    started = False
    for item in painted:
        if item.cut:
            if started:
                acc = pathops.op(acc, item.path, pathops.PathOp.DIFFERENCE)
            continue
        if started:
            acc = pathops.op(acc, item.path, pathops.PathOp.UNION)
        else:
            acc = item.path
            started = True
    acc.convertConicsToQuads(0.02)
    if not started or not list(acc.segments):
        raise BuildError(f"{src.name}: nothing is painted")
    acc = pathops.simplify(acc)
    left, top, right, bottom = acc.bounds
    if left < -EPS or top < -EPS or right > GRID + EPS or bottom > GRID + EPS:
        raise BuildError(f"{src.name}: ink {fmt(left)},{fmt(top)} to {fmt(right)},{fmt(bottom)} leaves the 16 x 16 box")

    soft = soft_edges(acc)
    d = path_d(acc)
    svg = (
        '<svg height="16px" viewBox="0 0 16 16" width="16px" xmlns="http://www.w3.org/2000/svg">\n'
        f'    <path d="{d}"/>\n'
        "</svg>\n"
    )
    return Compiled(name=name, svg=svg, ink=(left, top, right, bottom), notes=soft, path=acc)


def soft_edges(path: pathops.Path) -> list[str]:
    """Long straight edges and curve extremes that are not on a whole pixel (they render soft)."""
    xs, ys = snap_anchors(path)
    notes = []
    for axis, values in (("x", xs), ("y", ys)):
        off = sorted({round(v, 2) for v in values if abs(v - round(v)) > 0.03 and 0.03 < v < GRID - 0.03})
        if off:
            notes.append(f"soft {axis}: " + " ".join(fmt(v) for v in off))
    return notes


# ------------------------------------------------------------------ pixel rules
#
# Every compiled icon is rasterised (raster.py) at 14, 16, 18 and 20 px and judged by its
# pixels, so the rules hold for what is drawn, not for what the source claims.

SIZES = (14, 16, 18, 20)
DESIGN_SIZE = 16

RULES = {
    # class: (smallest, largest) length of the longer side of the ink, px on the 16 grid.
    "compact": (13.0, 15.0),  # round and square glyphs: about 14 px of ink
    "elongated": (14.0, 15.5),  # aspect 1.3 or more: the long side fills the box
    "mark": (8.0, 13.0),  # chevrons, ticks, plus and minus: smaller on purpose (labels.tsv #mark)
}
ELONGATED_ASPECT = 1.3
MARGIN = 0.25  # ink may not come closer to the box edge than this
CENTRE_TOLERANCE = 0.5  # centre of the ink from the centre of the box, each axis
SYMMETRIC_TOLERANCE = 0.05  # the same for a symmetric icon: it is mirrored about x = 8
SYMMETRY_LIMIT = (0.15, 0.03)  # largest and mean coverage difference from the mirror image
# Partly covered pixels on straight edge runs (raster.soft_edge_pixels). 16 px is the size
# icons are designed for; the others are scaled copies and can only be as crisp as 18/16
# of a pixel grid allows, so they are held to a looser bound that still catches a regression.
SOFT_LIMIT = {14: 100, 16: 10, 18: 120, 20: 140}
# What each category of icon is drawn with (labels.tsv #policy lines; docs/bingux-ui.md).
OUTLINE = ("regular", "bold")
POLICY = {
    "object": ("fill",),  # things: devices, places, tools
    "status": ("fill",),  # badges whose meaning is the mass: info, warning, error, secure, media marks
    "detail": OUTLINE,  # things recognised by their inner structure (keys, buttons, frames, globe, envelope)
    "action": OUTLINE,  # verbs: search, copy, share, refresh, delete
    "control": OUTLINE,  # arrows, chevrons, ticks, plus, close, menus: simple strokes, bold (2 px)
    "indicator": OUTLINE,  # levels and states: battery, volume, Bluetooth, network
    "section": OUTLINE,  # the icons of the settings sidebar: one weight, whatever they show
    "drawn": (),  # drawn by hand from strokes, not a fork glyph
}


@dataclass
class Measure:
    name: str
    cls: str
    ink: tuple[float, float, float, float]
    soft: dict[int, int]
    asym: tuple[float, float]
    density: float

    @property
    def width(self) -> float:
        return self.ink[2] - self.ink[0]

    @property
    def height(self) -> float:
        return self.ink[3] - self.ink[1]

    @property
    def centre(self) -> tuple[float, float]:
        return (self.ink[0] + self.ink[2]) / 2 - GRID / 2, (self.ink[1] + self.ink[3]) / 2 - GRID / 2


def polygons(path: pathops.Path) -> list[list[tuple[float, float]]]:
    """The outline as closed polygons, curves flattened finely enough for 20 px rasters."""
    result: list[list[tuple[float, float]]] = []
    for contour in contours_of(path):
        pts = [contour[0][1][0]]
        for kind, p in contour[1:]:
            x0, y0 = pts[-1]
            if kind == "L":
                pts.append(p[0])
            elif kind == "Q":
                (cx, cy), (x1, y1) = p
                for i in range(1, 11):
                    t = i / 10
                    u = 1 - t
                    pts.append((u * u * x0 + 2 * u * t * cx + t * t * x1, u * u * y0 + 2 * u * t * cy + t * t * y1))
            else:
                (ax, ay), (bx, by), (x1, y1) = p
                for i in range(1, 13):
                    t = i / 12
                    u = 1 - t
                    pts.append(
                        (
                            u**3 * x0 + 3 * u * u * t * ax + 3 * u * t * t * bx + t**3 * x1,
                            u**3 * y0 + 3 * u * u * t * ay + 3 * u * t * t * by + t**3 * y1,
                        )
                    )
        result.append(pts)
    return result


def short_name(name: str) -> str:
    return name.removesuffix("-symbolic")


def classify(name: str, width: float, height: float, labels: Labels) -> str:
    if short_name(name) in labels.marks:
        return "mark"
    short = max(min(width, height), 0.01)
    return "elongated" if max(width, height) / short >= ELONGATED_ASPECT else "compact"


def measure_icon(item: Compiled, labels: Labels) -> Measure:
    polys = polygons(item.path)
    covs = {size: raster.coverage(polys, size) for size in SIZES}
    # The ink box is read from a 64 px raster (a quarter of a pixel of 16 px) so that a curve's
    # apex, which only part covers its pixel, still counts as ink.
    ink = raster.ink_extents(raster.coverage(polys, 4 * DESIGN_SIZE))
    width, height = ink[2] - ink[0], ink[3] - ink[1]
    return Measure(
        name=item.name,
        cls=classify(item.name, width, height, labels),
        ink=ink,
        soft={size: raster.soft_edge_pixels(cov) for size, cov in covs.items()},
        asym=raster.asymmetry(covs[DESIGN_SIZE]),
        density=float(covs[DESIGN_SIZE].sum()) / (DESIGN_SIZE * DESIGN_SIZE),
    )


def judge(m: Measure, labels: Labels) -> list[str]:
    """The rule violations of one icon, each phrased with the fix."""
    short = short_name(m.name)

    def exempt(rule: str) -> bool:
        return short in labels.exempt.get(rule, set())

    problems: list[str] = []
    lo, hi = RULES[m.cls]
    long_side = max(m.width, m.height)
    if not exempt("size") and not (lo - 0.05 <= long_side <= hi + 0.05):
        problems.append(
            f"{short}: ink is {m.width:.2f} x {m.height:.2f} px ({m.cls}): the long side must be {lo:g} to {hi:g} px; "
            f"change scale (about {((lo + hi) / 2) / long_side:.2f} times the current one)"
        )
    left, top, right, bottom = m.ink
    if not exempt("size") and min(left, top, GRID - right, GRID - bottom) < MARGIN - 0.05:
        problems.append(f"{short}: ink {left:.2f},{top:.2f} to {right:.2f},{bottom:.2f} is within {MARGIN} px of the box edge")
    cx, cy = m.centre
    tolerance = SYMMETRIC_TOLERANCE if short in labels.symmetric else CENTRE_TOLERANCE
    if not exempt("centre") and (abs(cx) > tolerance + 0.01 or abs(cy) > CENTRE_TOLERANCE + 0.01):
        problems.append(f"{short}: ink centre is off the box centre by {cx:+.2f}, {cy:+.2f} px; move it with dx/dy")
    for size in SIZES:
        if not exempt("soft") and m.soft[size] > SOFT_LIMIT[size]:
            problems.append(f"{short}: {m.soft[size]} soft edge pixels at {size} px (limit {SOFT_LIMIT[size]}); fix scale, dx, dy or the stroke")
    if short in labels.symmetric and (m.asym[0] > SYMMETRY_LIMIT[0] or m.asym[1] > SYMMETRY_LIMIT[1]):
        problems.append(
            f"{short}: not left-right symmetric (worst pixel differs by {m.asym[0]:.2f}, mean {m.asym[1]:.3f}); "
            "mirror one half of the source"
        )
    return problems


def label_problems(compiled: dict[str, Compiled], labels: Labels) -> list[str]:
    """labels.tsv against the sources: every icon is described, every directive names a real icon."""
    problems: list[str] = []
    shorts = {short_name(name) for name in compiled}
    for short in sorted(shorts - set(labels.rows)):
        problems.append(f"{short}: no row in scripts/icons/labels.tsv")
    for short in sorted(set(labels.rows) - shorts):
        problems.append(f"{short}: labels.tsv row has no source")
    for short in sorted(shorts):
        category = labels.policy.get(short)
        if category is None:
            problems.append(f"{short}: no `#policy` category in labels.tsv")
            continue
        if category not in POLICY:
            problems.append(f"{short}: unknown policy category {category}")
            continue
        row = labels.rows.get(short)
        expected = POLICY[category]
        if row is not None and expected and row.fork is not None and row.weight not in expected:
            problems.append(f"{short}: {category} icons use the {' or '.join(expected)} weight, not {row.weight}")
        if row is not None and not expected and row.fork is not None:
            problems.append(f"{short}: category {category} is for drawn icons, but the glyph is {row.fork}")
    named = labels.symmetric | labels.marks | {n for names in labels.exempt.values() for n in names} | set(labels.policy)
    for short in sorted(named - shorts):
        problems.append(f"{short}: named in a labels.tsv directive but has no source")
    return problems


def counter_problems(compiled: dict[str, Compiled], labels: Labels) -> list[str]:
    """A fork glyph keeps every hole and cut-out it has: fitting must not close one up."""
    problems: list[str] = []
    for name, item in compiled.items():
        short = short_name(name)
        row = labels.rows.get(short)
        if row is None or row.fork is None or short in labels.exempt.get("counters", set()):
            continue
        natural = fork_outline({"name": row.fork, "weight": row.weight, "snap": "none", "stroke": "none"}, name).path
        before, after = len(contours_of(natural)), len(contours_of(item.path))
        if after < before:
            problems.append(f"{short}: fitting closed {before - after} of the {row.fork} glyph's {before} holes or parts (scale, dx, dy or snap)")
    return problems


SIDEBAR_DENSITY = (0.10, 0.30)  # ink as a share of the 16 x 16 box: one visual weight across the sidebar


def sidebar_problems(measures: list[Measure], labels: Labels) -> list[str]:
    """The sidebar icons (groups.txt [sidebar]) are one set: outline weight, similar ink."""
    names: list[str] = []
    in_group = False
    for line in (HERE / "groups.txt").read_text().splitlines():
        line = line.split("#")[0].strip()
        if line.startswith("["):
            in_group = line == "[sidebar]"
        elif in_group:
            names += line.split()
    by_name = {short_name(m.name): m for m in measures}
    problems: list[str] = []
    for short in names:
        m, row = by_name.get(short), labels.rows.get(short)
        if m is None:
            continue
        if row is not None and row.fork is not None and row.weight == "fill":
            problems.append(f"{short}: a sidebar icon must be an outline, this one is a fill")
        if not SIDEBAR_DENSITY[0] <= m.density <= SIDEBAR_DENSITY[1]:
            problems.append(
                f"{short}: ink covers {100 * m.density:.0f} % of the box; the sidebar set keeps to "
                f"{100 * SIDEBAR_DENSITY[0]:.0f} to {100 * SIDEBAR_DENSITY[1]:.0f} % so no icon looks heavier than the rest"
            )
    return problems


def lint_pixels(compiled: dict[str, Compiled], labels: Labels, whole_set: bool) -> tuple[list[str], list[Measure]]:
    """Judge every icon by its pixels. `whole_set` also checks labels.tsv covers the sources."""
    measures = [measure_icon(item, labels) for item in compiled.values()]
    problems = (label_problems(compiled, labels) + counter_problems(compiled, labels)) if whole_set else []
    for m in measures:
        problems += judge(m, labels)
    if whole_set:
        problems += sidebar_problems(measures, labels)
    return problems, measures


def soft_table(measures: list[Measure], count: int = 10) -> str:
    """The icons with the most soft edge pixels at 16 px, then the worst at the other sizes."""
    worst = sorted(measures, key=lambda m: (-m.soft[DESIGN_SIZE], m.name))[:count]
    lines = [f"{'icon':44s} " + " ".join(f"{size:>4d}px" for size in SIZES)]
    for m in worst:
        lines.append(f"{short_name(m.name):44s} " + " ".join(f"{m.soft[size]:6d}" for size in SIZES))
    totals = {size: sum(m.soft[size] for m in measures) for size in SIZES}
    lines.append(f"{'all icons (sum)':44s} " + " ".join(f"{totals[size]:6d}" for size in SIZES))
    return "\n".join(lines)


# ------------------------------------------------------------------- fitting
#
# `build.py --suggest` proposes the scale, dx and dy that satisfy RULES for a fork icon:
# the smallest change of scale that brings the long side into range, then the shift (at most
# what keeps the ink centred) that puts the most straight edges on whole pixels.


def anchor_cost(values: np.ndarray, shifts: np.ndarray) -> np.ndarray:
    moved = values[None, :] + shifts[:, None]
    return np.abs(moved - np.round(moved)).sum(axis=1)


def best_shift(values: list[float], low: float, high: float, step: float = 0.05) -> float:
    """The shift in [low, high] that puts the anchors closest to whole pixels; ties go to the smaller move."""
    count = int(math.floor((high - low) / step + 1e-9))
    shifts = np.array([low + i * step for i in range(count + 1)]) if count >= 0 else np.array([(low + high) / 2])
    if not values:
        return float(shifts[np.argmin(np.abs(shifts))])
    cost = anchor_cost(np.array(values), shifts)
    order = np.lexsort((np.abs(shifts), np.round(cost, 4)))
    return float(shifts[order[0]])


def suggest_fit(short: str, row, labels: Labels) -> dict[str, str]:
    """Scale, dx and dy for a fork row, as strings ready for labels.tsv."""
    base = {"name": row.fork, "weight": row.weight}
    for key in ("mirror", "stroke"):
        if key in row.attrs:
            base[key] = row.attrs[key]
    where = f"{short}-symbolic.svg"

    def outline(zoom: float, dx: float = 0.0, dy: float = 0.0) -> pathops.Path:
        attrs = dict(base, scale=repr(zoom), dx=repr(dx), dy=repr(dy), snap="none")
        return fork_outline(attrs, where).path

    natural_path = outline(1.0)
    natural = natural_path.bounds
    nat_w, nat_h = natural[2] - natural[0], natural[3] - natural[1]
    nat_contours = len(contours_of(natural_path))
    cls = classify(short, nat_w, nat_h, labels)
    lo, hi = RULES[cls]
    inner = 0.1
    valid = []
    for step in range(int(0.60 / 0.02), int(1.70 / 0.02) + 1):
        zoom = round(step * 0.02, 3)
        l, t, r, b = outline(zoom).bounds
        if lo + inner <= max(r - l, b - t) <= hi - inner:
            valid.append(zoom)
    if not valid:
        raise BuildError(f"{short}: no scale puts the long side in {lo:g} to {hi:g} px")
    start = min(valid, key=lambda z: (abs(z - 1.0), z))
    symmetric = short in labels.symmetric
    best: tuple[float, int, float, float, float, float] | None = None
    for k in range(0, 41):
        zoom = round(start - 0.08 + k * 0.005, 3)
        if zoom <= 0:
            continue
        path = outline(zoom)
        l, t, r, b = path.bounds
        xs, ys = snap_anchors(path)
        cx, cy = (l + r) / 2 - GRID / 2, (t + b) / 2 - GRID / 2
        tol_x = SYMMETRIC_TOLERANCE / 2 if symmetric else CENTRE_TOLERANCE - 0.02
        low_x = max(-cx - tol_x, MARGIN + 0.05 - l)
        high_x = min(-cx + tol_x, GRID - MARGIN - 0.05 - r)
        low_y = max(-cy - (CENTRE_TOLERANCE - 0.02), MARGIN + 0.05 - t)
        high_y = min(-cy + (CENTRE_TOLERANCE - 0.02), GRID - MARGIN - 0.05 - b)
        if low_x > high_x:
            low_x = high_x = (low_x + high_x) / 2
        if low_y > high_y:
            low_y = high_y = (low_y + high_y) / 2
        sx = round(-cx if symmetric else best_shift(xs, low_x, high_x), 2)
        sy = round(best_shift(ys, low_y, high_y), 2)
        # Judge the real outline, snapping included.
        final = fork_outline(
            dict(base, scale=repr(zoom), dx=repr(sx), dy=repr(sy), symmetric="true" if symmetric else ""), where
        ).path
        l, t, r, b = final.bounds
        long_side = max(r - l, b - t)
        # How far the outline is from satisfying size, margin and centre (0 when it does).
        violation = (
            max(0.0, lo + 0.02 - long_side, long_side - (hi - 0.02))
            + max(0.0, MARGIN + 0.02 - min(l, t, GRID - r, GRID - b))
            + max(0.0, abs((l + r) / 2 - GRID / 2) - ((SYMMETRIC_TOLERANCE if symmetric else CENTRE_TOLERANCE) - 0.01))
            + max(0.0, abs((t + b) / 2 - GRID / 2) - (CENTRE_TOLERANCE - 0.01))
            + 1.0 * max(0, nat_contours - len(contours_of(final)))  # a counter or a cut-out closed up
        )
        soft = raster.soft_edge_pixels(raster.coverage(polygons(final), DESIGN_SIZE))
        candidate = (round(violation, 3), soft, abs(zoom - 1.0), zoom, sx, sy)
        if best is None or candidate < best:
            best = candidate
    if best is None:
        raise BuildError(f"{short}: no scale puts the long side in {lo:g} to {hi:g} px with the ink inside the box")
    violation, _, _, zoom, sx, sy = best
    if violation > 0.05:
        print(f"icons: warning: {short}: best fit still breaks a rule by {violation} px", file=sys.stderr)
    result: dict[str, str] = {}
    if abs(zoom - 1.0) > 1e-9:
        result["scale"] = f"{zoom:g}"
    if abs(sx) > 1e-9:
        result["dx"] = f"{sx:.2f}".rstrip("0").rstrip(".")
    if abs(sy) > 1e-9:
        result["dy"] = f"{sy:.2f}".rstrip("0").rstrip(".")
    return result


def referenced_icons() -> set[str]:
    text = ICONS_RS.read_text()
    return set(re.findall(r'include_bytes!\("\.\./icons/([^"]+)\.svg"\)', text))


def compile_all(only: set[str] | None = None) -> dict[str, Compiled]:
    sources = sorted(SRC.glob("*.svg"))
    if only is not None:
        sources = [src for src in sources if src.stem in only or src.stem.removesuffix("-symbolic") in only]
    if not sources:
        raise BuildError(f"no sources in {SRC}")
    compiled: dict[str, Compiled] = {}
    errors: list[str] = []
    for src in sources:
        try:
            compiled[src.stem] = compile_source(src)
        except BuildError as exc:
            errors.append(str(exc))
    missing = [] if only is not None or SRC != HERE / "src" else sorted(referenced_icons() - set(compiled))
    for name in missing:
        errors.append(f"icons.rs references {name}.svg but scripts/icons/src/{name}.svg does not exist")
    if errors:
        raise BuildError("\n".join(errors))
    return compiled


# -------------------------------------------------------------------- output


def read_copying() -> str:
    return COPYING_SRC.read_text()


def shell_files(compiled: dict[str, Compiled]) -> dict[str, str]:
    """The QML shell's copy: Adwaita's dark fill on the path (the shell recolours it)
    and an index of the names it can serve, in `BinguxIconIndex.js`."""
    files = {f"{name}.svg": item.svg.replace('<path d=', f'<path fill="{SHELL_FILL}" d=') for name, item in compiled.items()}
    names = "".join(f'    "{name}": true,\n' for name in sorted(compiled))
    files["BinguxIconIndex.js"] = (
        "// Generated by scripts/icons/build.py; do not edit.\n"
        ".pragma library\n"
        "// Icons the shell serves from shell/bingux/icons instead of the Adwaita theme.\n"
        f"var names = {{\n{names}}};\n"
    )
    return files


def generated_shell_names() -> set[str]:
    """File names (`x.svg`) a previous run generated, from the names in BinguxIconIndex.js.

    shell/bingux/icons also holds hand-made icons (capture-*, control-centre, ...). Only
    files named in the index are ours to rewrite or remove; nothing else is ever touched.
    """
    if not SHELL_INDEX.exists():
        return set()
    return {f"{name}.svg" for name in re.findall(r'^\s*"([^"]+)": true,$', SHELL_INDEX.read_text(), re.M)}


def shell_collisions(compiled: dict[str, Compiled]) -> list[str]:
    """Hand-made shell icons (files not named in the index) that a source would overwrite."""
    ours = generated_shell_names()
    return [
        f"shell/bingux/icons/{name}.svg is a hand-made shell icon; rename the source or remove the file by hand"
        for name in sorted(compiled)
        if (SHELL_OUT / f"{name}.svg").exists() and f"{name}.svg" not in ours
    ]


def write_shell(compiled: dict[str, Compiled]) -> None:
    collisions = shell_collisions(compiled)
    if collisions:
        raise BuildError("\n".join(collisions))
    previous = generated_shell_names()
    files = shell_files(compiled)
    SHELL_OUT.mkdir(parents=True, exist_ok=True)
    for name, text in files.items():
        target = SHELL_INDEX if name == "BinguxIconIndex.js" else SHELL_OUT / name
        tmp = target.with_name(target.name + ".tmp")
        tmp.write_text(text)
        os.replace(tmp, target)
    keep = {name for name in files if name != "BinguxIconIndex.js"}
    for name in sorted(previous - keep):
        (SHELL_OUT / name).unlink(missing_ok=True)
    (SHELL_OUT / "COPYING").write_text(read_copying())


def write_all(out_dir: Path, compiled: dict[str, Compiled]) -> None:
    """Stage every file, then rename each one into place. Stale .svg files are removed."""
    staging = out_dir.with_name(out_dir.name + ".staging")
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    try:
        for name, item in compiled.items():
            (staging / f"{name}.svg").write_text(item.svg)
        (staging / "COPYING").write_text(read_copying())
        out_dir.mkdir(parents=True, exist_ok=True)
        for child in staging.iterdir():
            os.replace(child, out_dir / child.name)
        keep = {f"{name}.svg" for name in compiled} | {"COPYING"}
        for stale in out_dir.glob("*.svg"):
            if stale.name not in keep:
                stale.unlink()
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def expected_files(compiled: dict[str, Compiled]) -> dict[str, str]:
    files = {f"{name}.svg": item.svg for name, item in compiled.items()}
    files["COPYING"] = read_copying()
    return files


def lint(compiled: dict[str, Compiled]) -> tuple[list[str], list[Measure]]:
    """Pixel rules and labels.tsv for `compiled`. Drafts (--src) are not held to labels.tsv."""
    return lint_pixels(compiled, load_labels(), whole_set=SRC == HERE / "src")


def check(out_dir: Path) -> str:
    first = compile_all()
    second = compile_all()
    if expected_files(first) != expected_files(second):
        raise BuildError("output is not deterministic")
    if not out_dir.exists():
        raise BuildError(f"{out_dir} does not exist; run build.py first")
    expected = expected_files(first)
    problems: list[str] = []
    for name, text in expected.items():
        path = out_dir / name
        if not path.exists():
            problems.append(f"{name} missing from {out_dir}")
        elif path.read_text() != text:
            problems.append(f"{name} is stale; run scripts/icons/build.py")
    for stale in out_dir.glob("*.svg"):
        if stale.name not in expected:
            problems.append(f"{stale.name} has no source")
    if out_dir.resolve() == DEFAULT_OUT.resolve():
        for name, text in shell_files(first).items():
            path = SHELL_INDEX if name == "BinguxIconIndex.js" else SHELL_OUT / name
            if not path.exists() or path.read_text() != text:
                problems.append(f"{path.relative_to(REPO)} is stale; run scripts/icons/build.py")
        problems += shell_collisions(first)
    pixel_problems, measures = lint(first)
    table = soft_table(measures)
    if problems or pixel_problems:
        raise BuildError("\n".join(problems + pixel_problems) + "\n\nsoft edge pixels, worst icons first:\n" + table)
    return table


def measure_table(measures: list[Measure]) -> str:
    lines = [f"{'icon':42s} {'class':9s} {'w':>6s} {'h':>6s} {'cx':>6s} {'cy':>6s} {'soft16':>6s} {'soft18':>6s} {'ink%':>5s}"]
    for m in sorted(measures, key=lambda m: m.name):
        cx, cy = m.centre
        lines.append(
            f"{short_name(m.name):42s} {m.cls:9s} {m.width:6.2f} {m.height:6.2f} {cx:+6.2f} {cy:+6.2f} "
            f"{m.soft[16]:6d} {m.soft[18]:6d} {100 * m.density:5.1f}"
        )
    return "\n".join(lines)


def suggest(names: set[str]) -> str:
    labels = load_labels()
    lines = []
    for short, row in sorted(labels.rows.items()):
        if row.fork is None or (names and short not in names):
            continue
        fit = suggest_fit(short, row, labels)
        lines.append(f"{short}\t" + " ".join(f"{key}={value}" for key, value in fit.items()))
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUT, help="output directory")
    parser.add_argument("--check", action="store_true", help="validate instead of writing")
    parser.add_argument("--lint", action="store_true", help="only judge the sources' pixels (size, centre, crispness, symmetry)")
    parser.add_argument("--measure", action="store_true", help="print the measurements of every icon")
    parser.add_argument(
        "--suggest", nargs="*", metavar="ICON", help="propose scale, dx and dy for fork icons (all, or the ones named)"
    )
    parser.add_argument("--report", action="store_true", help="list edges that are not on a whole pixel")
    parser.add_argument("--src", type=Path, help="compile sources from this directory (drafts)")
    parser.add_argument("--geometry", type=Path, help="fork geometry file to use (drafts)")
    parser.add_argument(
        "--only",
        help="comma-separated icon names to compile (with or without -symbolic); for drafting into --output",
    )
    args = parser.parse_args(argv)
    out_dir = args.output if args.output.is_absolute() else Path.cwd() / args.output
    global SRC, FORK_GEOMETRY
    if args.src:
        SRC = args.src.resolve()
    if args.geometry:
        FORK_GEOMETRY = args.geometry.resolve()
    try:
        if args.suggest is not None:
            print(suggest(set(args.suggest)))
        elif args.lint or args.measure:
            problems, measures = lint(compile_all())
            if args.measure:
                print(measure_table(measures))
            if problems:
                raise BuildError("\n".join(problems) + "\n\nsoft edge pixels, worst icons first:\n" + soft_table(measures))
            print(f"icons: pixel rules ok for {len(measures)} icons")
        elif args.check:
            table = check(out_dir)
            print("soft edge pixels, worst icons first:\n" + table)
            print(f"icons: ok ({len(referenced_icons())} referenced, all sources valid, output matches, pixel rules hold)")
        else:
            only = {n for n in args.only.split(",") if n} if args.only else None
            if only is not None and out_dir.resolve() == DEFAULT_OUT.resolve():
                raise BuildError("--only is for drafts: give --output a scratch directory")
            compiled = compile_all(only)
            write_all(out_dir, compiled)
            if args.report:
                for item in compiled.values():
                    for note in item.notes:
                        print(f"{item.name}: {note}")
            if out_dir.resolve() == DEFAULT_OUT.resolve():
                write_shell(compiled)
            print(f"icons: wrote {len(compiled)} icons to {out_dir}")
    except BuildError as exc:
        print(f"icons: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
