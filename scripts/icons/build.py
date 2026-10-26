#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["skia-pathops==0.9.2", "svgelements>=1.9,<2"]
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

Crispness rules, checked here:
- stroke widths are whole numbers, 1 or 2;
- every axis-aligned edge has its outer boundary on the pixel grid
  (centreline at an integer for width 2, at n + 0.5 for width 1);
- filled axis-aligned edges sit on integers;
- ink stays inside the 16 x 16 box.

Usage:
  build.py                    compile sources into packages/bingux-ui/icons
  build.py --output DIR       compile into DIR instead (for comparison copies)
  build.py --check            validate sources, determinism, icons.rs references,
                              and that the output directory matches the sources
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

import pathops
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
FORK_GEOMETRY = HERE / "fork" / "geometry.json"
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
ALLOWED_STROKE_WIDTHS = (1.0, 1.5, 2.0)
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
# `<fork name="Power" weight="regular"/>` pulls the rounded Phosphor geometry that
# ~/dev/chat/packages/icons generated (vendored in fork/geometry.json by
# import-chat.py) onto the 16 grid and makes it crisp, without redesigning it:
#
# 1. scale the 256 unit drawing by 1/16;
# 2. dilate by `dilate` px (regular strokes are 1 px: 0.5 makes them 2 px; bold are
#    1.5 px: 0.25; fill needs none) with round joins, so the fork's corners stay round;
# 3. snap: every long horizontal or vertical edge and every curve extreme is moved to a
#    whole pixel by a monotone warp of the whole drawing, so strokes keep their width and
#    shapes keep their curves. A coordinate half way between two pixels moves away from
#    the middle of the box, so symmetric icons stay symmetric.

_FORK: dict | None = None
FORK_WEIGHT_DILATE = {"regular": 0.5, "bold": 0.25, "fill": 0.0}


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
    if amount <= EPS:
        return path
    grown = pathops.Path(path)
    grown.stroke(2 * amount, pathops.LineCap.ROUND_CAP, pathops.LineJoin.ROUND_JOIN, 4.0)
    grown.convertConicsToQuads(0.02)
    return pathops.simplify(pathops.op(path, pathops.simplify(grown), pathops.PathOp.UNION))


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


def snap_target(value: float) -> float:
    """The pixel line for `value`; half pixels move away from the middle of the box."""
    frac = value - math.floor(value)
    if abs(frac - 0.5) < 0.15:
        return math.floor(value) + 1.0 if value >= GRID / 2 else math.floor(value)
    return float(math.floor(value + 0.5))


def snap_warp(coords: list[float], eps: float = 0.12):
    clusters: list[list[float]] = []
    for v in sorted(coords):
        if clusters and v - clusters[-1][-1] <= eps:
            clusters[-1].append(v)
        else:
            clusters.append([v])
    pairs: list[tuple[float, float]] = []
    for cluster in clusters:
        c = sum(cluster) / len(cluster)
        t = snap_target(c)
        if pairs and (t <= pairs[-1][1] or (t - pairs[-1][1]) < 0.5 * (c - pairs[-1][0])):
            continue
        pairs.append((c, t))
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


def fork_path(attrs: dict[str, str], where: str) -> pathops.Path:
    name = attrs.get("name", "")
    weight = attrs.get("weight", "regular")
    if weight not in FORK_WEIGHT_DILATE:
        raise BuildError(f"{where}: fork weight must be regular, bold or fill")
    # mode="exact" inside a custom icon: the fork's outline as it is, only scaled and moved.
    exact = attrs.get("mode") == "exact"
    dilate = parse_float(attrs, "dilate", 0.0 if exact else FORK_WEIGHT_DILATE[weight])
    path = scaled_path(fork_geometry(name, weight, where), 1 / 16)
    dx, dy, scale = parse_float(attrs, "dx"), parse_float(attrs, "dy"), parse_float(attrs, "scale", 1.0)
    if abs(scale - 1.0) > EPS or abs(dx) > EPS or abs(dy) > EPS:
        path = warped(path, lambda v: 8 + (v - 8) * scale + dx, lambda v: 8 + (v - 8) * scale + dy)
    path = dilated(path, dilate)
    if exact:
        return path
    # Dilation can push the ink out of the box: shrink about the middle until it fits.
    left, top, right, bottom = path.bounds
    reach = max(8 - left, right - 8, 8 - top, bottom - 8)
    if reach > 8 + EPS:
        fit = 8 / reach
        path = warped(path, lambda v: 8 + (v - 8) * fit, lambda v: 8 + (v - 8) * fit)
    if attrs.get("snap", "auto") != "none":
        xs, ys = snap_anchors(path)
        path = warped(path, snap_warp(xs), snap_warp(ys))
    box = pathops.Path()
    box.moveTo(0, 0)
    box.lineTo(GRID, 0)
    box.lineTo(GRID, GRID)
    box.lineTo(0, GRID)
    box.close()
    return pathops.simplify(pathops.op(path, box, pathops.PathOp.INTERSECTION))


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
        errors.append(f"{where}: stroke-width {fmt(stroke_width)} must be 1, 1.5 or 2")
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


def best_offset(path: pathops.Path, reach: float = 0.5, step: float = 0.05) -> tuple[float, float]:
    """The whole-icon shift (at most `reach` px) that puts the most edge on whole pixels.

    Each long straight edge and curve extreme is an anchor; the cost of a shift is the sum
    of each anchor's distance to the nearest pixel line. Only a translation: no shape changes.
    """
    xs, ys = snap_anchors(path)

    def cost(values: list[float], shift: float) -> float:
        return sum(abs((v + shift) - round(v + shift)) for v in values)

    def best(values: list[float]) -> float:
        if not values:
            return 0.0
        steps = [i * step for i in range(-round(reach / step), round(reach / step) + 1)]
        # Ties go to the smallest move.
        return min(steps, key=lambda t: (round(cost(values, t), 6), abs(t)))

    return round(best(xs), 4), round(best(ys), 4)


def compile_exact(src: Path, attrs: dict[str, str]) -> Compiled:
    """`<fork name=".." weight=".." mode="exact"/>`: the fork's own outline, byte for byte.

    The fork draws on a 256 unit grid, so the 16 grid is the same drawing scaled by
    1/16 (`transform`), possibly moved by `dx`/`dy` pixels so that its dominant edges
    land on pixel boundaries. Curves and corners are never touched.
    """
    where = src.name
    weight = attrs.get("weight", "regular")
    if weight not in FORK_WEIGHT_DILATE:
        raise BuildError(f"{where}: fork weight must be regular, bold or fill")
    paths = fork_geometry(attrs.get("name", ""), weight, where)
    dx, dy = parse_float(attrs, "dx"), parse_float(attrs, "dy")
    # `scale` grows the whole outline about the box centre, for glyphs the fork draws with
    # a wide margin. The outline itself is untouched.
    zoom = parse_float(attrs, "scale", 1.0)
    dx, dy = dx + (GRID / 2) * (1 - zoom), dy + (GRID / 2) * (1 - zoom)
    if attrs.get("align", "auto") == "auto":
        unshifted = scaled_path(paths, zoom / 16)
        auto_dx, auto_dy = best_offset(unshifted)
        # The pixel-grid shift never pushes ink out of the box: an icon drawn to the
        # fork's edge keeps the move that stays inside.
        left, top, right, bottom = unshifted.bounds
        auto_dx = min(max(auto_dx, -(left + dx)), GRID - (right + dx))
        auto_dy = min(max(auto_dy, -(top + dy)), GRID - (bottom + dy))
        dx, dy = dx + auto_dx, dy + auto_dy
    shift = f"translate({fmt(dx)} {fmt(dy)}) " if abs(dx) > EPS or abs(dy) > EPS else ""
    body = "".join(f'    <path transform="{shift}scale({fmt(zoom / 16)})" d="{d}"/>\n' for d in paths)
    svg = f'<svg height="16px" viewBox="0 0 16 16" width="16px" xmlns="http://www.w3.org/2000/svg">\n{body}</svg>\n'
    whole = scaled_path(paths, zoom / 16)
    if dx or dy:
        whole = warped(whole, lambda v: v + dx, lambda v: v + dy)
    left, top, right, bottom = whole.bounds
    if left < -EPS or top < -EPS or right > GRID + EPS or bottom > GRID + EPS:
        raise BuildError(f"{where}: ink {fmt(left)},{fmt(top)} to {fmt(right)},{fmt(bottom)} leaves the 16 x 16 box")
    return Compiled(name=src.stem, svg=svg, ink=(left, top, right, bottom), notes=soft_edges(whole))


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

    if len(shapes) == 1 and shapes[0][0] == "fork" and shapes[0][1].get("mode") == "exact":
        return compile_exact(src, shapes[0][1])

    errors: list[str] = []
    painted: list[Painted] = []
    for index, (tag, own, eff) in enumerate(shapes):
        where = f"{src.name}#{index}<{tag}>"
        cut = own.get("data-op") == "cut"
        if own.get("data-op", "union") not in ("union", "cut"):
            errors.append(f"{where}: data-op must be union or cut")
        if tag == "fork":
            try:
                painted.append(Painted(fork_path(own, where), cut, where))
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
                errors.append(f"{where}: stroke-width {fmt(stroke_width)} must be 1, 1.5 or 2")
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
    return Compiled(name=name, svg=svg, ink=(left, top, right, bottom), notes=soft)


def soft_edges(path: pathops.Path) -> list[str]:
    """Long straight edges and curve extremes that are not on a whole pixel (they render soft)."""
    xs, ys = snap_anchors(path)
    notes = []
    for axis, values in (("x", xs), ("y", ys)):
        off = sorted({round(v, 2) for v in values if abs(v - round(v)) > 0.03 and 0.03 < v < GRID - 0.03})
        if off:
            notes.append(f"soft {axis}: " + " ".join(fmt(v) for v in off))
    return notes


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


def check(out_dir: Path) -> None:
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
    if problems:
        raise BuildError("\n".join(problems))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUT, help="output directory")
    parser.add_argument("--check", action="store_true", help="validate instead of writing")
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
        if args.check:
            check(out_dir)
            print(f"icons: ok ({len(referenced_icons())} referenced, all sources valid, output matches)")
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
