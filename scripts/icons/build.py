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
and moved as a whole by at most half a pixel, the shift that leaves the fewest soft
edges; `dx`/`dy` add to that, `align="none"` turns it off). The outline is only ever
translated and scaled by one factor on both axes: never warped, thickened or snapped.
Custom icons are written in a small dialect of plain SVG shapes, which may also include
`<fork/>` pieces (placed the same way, see "the chat fork's geometry" below):

- Root `<svg viewBox="0 0 16 16">` sets the defaults that children inherit:
  fill="none" stroke="none" stroke-width="2" stroke-linecap="round"
  stroke-linejoin="round". Override them on `<g>` or on a shape.
- Any shape with fill != "none" is filled. Any shape with stroke != "none" is
  stroked (outline of the centreline). Colours are ignored: the output is tinted
  by bingux-ui at draw time.
- `data-op="cut"` subtracts the shape from everything painted before it.
- `data-dim="yes"` paints the shape dimmed: the icon file gets a second path with
  `fill-opacity="0.4"` (`DIM_ALPHA`) for everything dim, and nothing in it overlaps the solid
  path. It is how a level icon keeps the parts that are "off" (a Wi-Fi arc without signal, the
  empty end of a battery) in view. The icon is still tinted with one colour at draw time; the
  dimming is only the path's own alpha. The pixel rules judge the whole silhouette, dim part
  included, and a cut removes ink from both layers.
- `data-round="r"` on polyline/polygon rounds each corner with radius r.
- Shapes: rect (rx), circle, ellipse, line, polyline, polygon, path.

Output is SVG, one file per source (one fill path for custom icons, plus one with `fill-opacity` when the
source has `data-dim` shapes), with the
same header as the Adwaita files it replaces. Output is deterministic: the same sources always give
byte-identical files.

Source rules, checked on every source:
- stroke widths are whole numbers, 1, 2 or 3 (3 only for a cut);
- every axis-aligned edge of a rect, line or polygon has its outer boundary on the pixel grid
  (centreline at an integer for width 2, at n + 0.5 for width 1);
- filled axis-aligned edges sit on integers;
- ink stays inside the 16 x 16 box.

Pixel rules, checked on the rasterised output by --check and --lint (see "pixel rules" below
and docs/bingux-ui.md): ink size, ink near the centre, partly covered pixels on straight
edges (against the icon's own best placement), left-right symmetry of the icons labels.tsv
lists, no hole closed by mirroring, and the fill/outline weight of each icon's category.

Usage:
  build.py                    compile sources into crates/ui/icons
  build.py --output DIR       compile into DIR instead (for comparison copies)
  build.py --check            validate sources, determinism, icons.rs references, that the
                              output directory matches the sources, and every pixel rule
  build.py --lint [--src DIR] only the pixel rules (DIR: a draft or probe directory)
  build.py --measure          the pixel rules plus a table of every icon's measurements
  build.py --suggest [ICON]   the scale for fork rows of labels.tsv that satisfy the size rule
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
DEFAULT_OUT = REPO / "crates/ui/icons"
SHELL_OUT = REPO / "shell/bingux/icons"
SHELL_INDEX = REPO / "shell/bingux/BinguxIconIndex.js"
SHELL_FILL = "#2e3436"
FORK_GEOMETRY = REPO / "crates/ui/icons/chat/geometry.json"
ICONS_RS = REPO / "crates/ui/src/icons.rs"

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
# The rules of design/rules.json this check holds (read by scripts/rules-check.py).
RULE_IDS = {"*": ["IC-01", "IC-04", "IC-06", "AL-15"]}
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
# The alpha of everything a source marks `data-dim`: low enough to read as "off", high enough to be
# seen on the black top bar and on a light page (a missing Wi-Fi arc, the empty end of a battery).
DIM_ALPHA = 0.4


class BuildError(Exception):
    """A source is invalid. Raised before any output file is touched."""


@dataclass
class Painted:
    path: pathops.Path
    op: str  # "union" or "cut"
    where: str
    dim: bool = False


@dataclass
class Compiled:
    name: str
    svg: str
    ink: tuple[float, float, float, float]
    path: pathops.Path  # the whole silhouette: the solid part and the dim part
    notes: list[str] = field(default_factory=list)
    dim: pathops.Path | None = None  # the part drawn at DIM_ALPHA, never overlapping the solid part


# ---------------------------------------------------------------- geometry text


def fmt(v: float) -> str:
    """Format a coordinate for output: 3 decimals, no trailing zeros, no -0."""
    s = f"{v:.3f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def fmt_scale(v: float) -> str:
    """A scale factor: the fork's 1/16 needs 4 decimals (0.0625), `fmt` would round it to 0.062."""
    return f"{v:.6f}".rstrip("0").rstrip(".")


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
# ~/dev/chat/packages/icons generated (vendored in crates/ui/icons/chat/geometry.json by
# import-chat.py) onto the 16 grid. The outline is the fork's own, and it is only ever
# moved or uniformly scaled, never warped, thickened or redrawn. Three per-icon attributes
# (written by map-fork.py from labels.tsv) say how:
#
# 1. `mirror="left"|"right"`: keep that half and replace the other with its mirror image
#    (for glyphs the fork draws a little lopsided, such as User);
# 2. `scale` grows or shrinks the whole drawing about the middle of the box, by one factor
#    on both axes (the fork leaves 1 to 2 px of margin around some glyphs);
# 3. `dx`/`dy` move the whole drawing; on top of them `align="auto"` (the default for an
#    icon that is a single `<fork/>`) adds the shift of at most half a pixel that puts the
#    most straight edges on whole pixels (a `mirror` icon is never moved sideways, so it stays
#    centred on x = 8). `align="none"` turns the automatic shift off.
#
# Strokes keep the fork's own width (1 px `regular`, 1.5 px `bold`), curves and corners are
# never touched: an edge the shift cannot put on a pixel line renders softly, which is
# accepted (see "pixel rules"). When there is no `mirror`, the outline is written as the
# fork's own path data under a `transform`, byte for byte.

_FORK: dict | None = None
FORK_WEIGHTS = ("regular", "bold", "fill")


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


def affine(path: pathops.Path, sx: float, sy: float, tx: float, ty: float) -> pathops.Path:
    """`path` scaled by (sx, sy) and moved by (tx, ty). The two scales may differ only in sign
    (a reflection): stretching one axis would distort the glyph."""
    if abs(abs(sx) - abs(sy)) > EPS:
        raise BuildError(f"affine: non-uniform scale {sx} x {sy}")
    out = pathops.Path()
    for contour in contours_of(path):
        for kind, pts in contour:
            m = [(x * sx + tx, y * sy + ty) for x, y in pts]
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


def soft_placements(
    polys: list[list[tuple[float, float]]], ink: tuple[float, float, float, float], sideways: bool = True
) -> list[tuple[int, float, float]]:
    """(soft pixels at 16 px, dx, dy) for each translation in SOFT_SHIFTS that keeps the ink in the box.

    Only a translation: no shape changes. `sideways=False` keeps dx at 0.
    """
    found = []
    for sx in SOFT_SHIFTS if sideways else (0.0,):
        for sy in SOFT_SHIFTS:
            if ink[0] + sx < -EPS or ink[1] + sy < -EPS or ink[2] + sx > GRID + EPS or ink[3] + sy > GRID + EPS:
                continue
            moved = [[(x + sx, y + sy) for x, y in poly] for poly in polys]
            found.append((raster.soft_edge_pixels(raster.coverage(moved, DESIGN_SIZE)), sx, sy))
    return found


def best_offset(path: pathops.Path, sideways: bool = True) -> tuple[float, float]:
    """The whole-icon shift of at most half a pixel that leaves the fewest soft edge pixels at
    16 px (ties go to the smallest move)."""
    found = soft_placements(polygons(path), path.bounds, sideways)
    if not found:
        return 0.0, 0.0
    _, sx, sy = min(found, key=lambda f: (f[0], abs(f[1]) + abs(f[2]), f[1], f[2]))
    return sx, sy


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
    flipped = affine(kept, -1.0, 1.0, GRID, 0.0)
    return pathops.simplify(pathops.op(kept, flipped, pathops.PathOp.UNION))


@dataclass
class ForkOutline:
    path: pathops.Path
    # `(paths, transform)` when the outline is the fork's own, so that it can be written as is.
    verbatim: tuple[list[str], str] | None


def fork_outline(attrs: dict[str, str], where: str) -> ForkOutline:
    name = attrs.get("name", "")
    weight = attrs.get("weight", "regular")
    if weight not in FORK_WEIGHTS:
        raise BuildError(f"{where}: fork weight must be regular, bold or fill")
    mirror = attrs.get("mirror", "")
    if mirror not in ("", "left", "right"):
        raise BuildError(f"{where}: mirror must be left or right")
    align = attrs.get("align", "auto")
    if align not in ("auto", "none"):
        raise BuildError(f"{where}: align must be auto or none")
    ds = fork_geometry(name, weight, where)
    zoom = parse_float(attrs, "scale", 1.0)
    if zoom <= 0:
        raise BuildError(f"{where}: scale must be positive")
    natural = scaled_path(ds, 1 / 16)
    if mirror:
        natural = mirrored(natural, mirror)
    # The drawing is scaled about the middle of the box, then moved by dx, dy.
    dx = parse_float(attrs, "dx") + (GRID / 2) * (1 - zoom)
    dy = parse_float(attrs, "dy") + (GRID / 2) * (1 - zoom)
    if align == "auto":
        left, top, right, bottom = affine(natural, zoom, zoom, dx, dy).bounds
        shift_x, shift_y = best_offset(affine(natural, zoom, zoom, dx, dy), sideways=not mirror)
        # The shift never pushes ink out of the box.
        dx += min(max(shift_x, -left), GRID - right)
        dy += min(max(shift_y, -top), GRID - bottom)
    path = affine(natural, zoom, zoom, dx, dy)
    if not mirror:
        transform = f"translate({fmt(dx)} {fmt(dy)}) " if abs(dx) > EPS or abs(dy) > EPS else ""
        return ForkOutline(path, (ds, f"{transform}scale({fmt_scale(zoom / 16)})"))
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
        op = own.get("data-op", "union")
        if op not in ("union", "cut"):
            errors.append(f"{where}: data-op must be union or cut")
        dim = "data-dim" in own
        if dim and own["data-dim"] != "yes":
            errors.append(f"{where}: data-dim must be yes (the alpha is DIM_ALPHA, one for every icon)")
        if dim and op != "union":
            errors.append(f"{where}: a {op} cannot be dim")
        if tag == "fork":
            try:
                painted.append(Painted(fork_outline(own, where).path, op, where, dim))
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
            painted.append(Painted(pathops.simplify(fill_path), op, where, dim))
        if stroke_on:
            stroke_path = d_to_path(d)
            cap = CAPS.get(eff["stroke-linecap"])
            join = JOINS.get(eff["stroke-linejoin"])
            if cap is None or join is None:
                errors.append(f"{where}: unknown stroke-linecap or stroke-linejoin")
                continue
            stroke_path.stroke(stroke_width, cap, join, 4.0)
            stroke_path.convertConicsToQuads(0.02)
            painted.append(Painted(pathops.simplify(stroke_path), op, where, dim))

    if errors:
        raise BuildError("\n".join(errors))

    solid, faint = pathops.Path(), pathops.Path()
    has_solid = has_dim = False
    for item in painted:
        if item.op == "cut":
            if has_solid:
                solid = pathops.op(solid, item.path, pathops.PathOp.DIFFERENCE)
            if has_dim:
                faint = pathops.op(faint, item.path, pathops.PathOp.DIFFERENCE)
        elif item.dim:
            faint = pathops.op(faint, item.path, pathops.PathOp.UNION) if has_dim else item.path
            has_dim = True
        else:
            solid = pathops.op(solid, item.path, pathops.PathOp.UNION) if has_solid else item.path
            has_solid = True
    if not has_solid and not has_dim:
        raise BuildError(f"{src.name}: nothing is painted")
    solid.convertConicsToQuads(0.02)
    solid = pathops.simplify(solid) if has_solid else None
    # The solid part wins where the two overlap: the dim part is only what the solid part does not cover.
    if has_dim and solid is not None:
        faint = pathops.op(faint, solid, pathops.PathOp.DIFFERENCE)
    faint = pathops.simplify(faint) if has_dim else None
    if faint is not None and not list(faint.segments):
        faint = None  # all of it is under the solid part, or a cut took it
    if solid is not None and not list(solid.segments):
        solid = None
    if solid is None and faint is None:
        raise BuildError(f"{src.name}: nothing is left of the icon after its cuts")
    if solid is None:
        acc = faint
    elif faint is None:
        acc = solid
    else:
        acc = pathops.simplify(pathops.op(solid, faint, pathops.PathOp.UNION))
    left, top, right, bottom = acc.bounds
    if left < -EPS or top < -EPS or right > GRID + EPS or bottom > GRID + EPS:
        raise BuildError(f"{src.name}: ink {fmt(left)},{fmt(top)} to {fmt(right)},{fmt(bottom)} leaves the 16 x 16 box")

    soft = soft_edges(acc)
    body = f'    <path d="{path_d(solid)}"/>\n' if solid is not None else ""
    if faint is not None:
        body += f'    <path d="{path_d(faint)}" fill-opacity="{fmt_scale(DIM_ALPHA)}"/>\n'
    svg = f'<svg height="16px" viewBox="0 0 16 16" width="16px" xmlns="http://www.w3.org/2000/svg">\n{body}</svg>\n'
    return Compiled(name=name, svg=svg, ink=(left, top, right, bottom), notes=soft, path=acc, dim=faint)


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
    # A fork glyph keeps the size its designer drew (10 px for a nine-dot grid, 16 for a
    # battery); the rule catches a glyph that is lost or leaves the box, not a 1 px difference.
    "icon": (10.0, 16.0),
    "mark": (8.0, 13.0),  # chevrons, ticks, plus and minus: smaller on purpose (labels.tsv #mark)
}
# The fork centres glyphs by their visual mass, not their box (the shackle of a lock, the
# point of a play triangle, the strap of headphones), so the ink box of a glyph may sit up to
# this far from the middle of the box; a symmetric one must be centred sideways.
CENTRE_TOLERANCE = 1.5
SYMMETRIC_TOLERANCE = 0.05  # the same for a symmetric icon: it is mirrored about x = 8
SYMMETRY_LIMIT = (0.15, 0.03)  # largest and mean coverage difference from the mirror image
# Crispness. `soft_edge_pixels` counts partly covered pixels on straight edge runs. A bold stroke
# (1.5 px) always leaves one such row per edge whatever its position, so there is no absolute
# limit; an icon is judged against its own best placement: the fewest soft pixels any
# translation of at most half a pixel gives (the only fitting freedom there is). It may have
# SOFT_SLACK more than that, in a factor and in pixels (curves and the 0.05 px search step).
SOFT_SLACK = (1.35, 6)
SOFT_SHIFTS = (-0.5, -0.25, 0.0, 0.25, 0.5)
# At 14, 18 and 20 px the icons are scaled copies and can only be as crisp as that grid
# allows, so they are held to a bound that still catches a regression.
SOFT_LIMIT = {14: 100, 18: 120, 20: 140}
# What each category of icon is drawn with (labels.tsv #policy lines; docs/bingux-ui.md). The
# 1 px `regular` weight is not used for a named icon: it draws a thin wire that reads rounder
# and weaker than its neighbours at 16 px.
OUTLINE = ("bold",)
POLICY = {
    "object": ("fill",),  # things: devices, places, tools
    "status": ("fill",),  # badges whose meaning is the mass: info, warning, error, secure, media marks
    "detail": ("bold", "fill"),  # things recognised by their inner structure (keys, frames, globe, envelope)
    "action": ("bold", "fill"),  # verbs: search, copy, share, refresh, delete
    "control": OUTLINE,  # arrows, chevrons, ticks, plus, close, menus: simple strokes
    "indicator": ("bold", "fill"),  # levels and states: battery, volume, Bluetooth, network
    "section": ("bold", "fill"),  # the icons of the settings sidebar
    "drawn": (),  # drawn by hand from strokes, not a fork glyph
}


@dataclass
class Measure:
    name: str
    cls: str
    ink: tuple[float, float, float, float]
    soft: dict[int, int]
    soft_best: int  # fewest soft pixels at 16 px over the translations of SOFT_SHIFTS
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


def classify(name: str, labels: Labels) -> str:
    return "mark" if short_name(name) in labels.marks else "icon"


def measure_icon(item: Compiled, labels: Labels) -> Measure:
    polys = polygons(item.path)
    covs = {size: raster.coverage(polys, size) for size in SIZES}
    # The ink box is read from a 64 px raster (a quarter of a pixel of 16 px) so that a curve's
    # apex, which only part covers its pixel, still counts as ink.
    ink = raster.ink_extents(raster.coverage(polys, 4 * DESIGN_SIZE))
    soft = {size: raster.soft_edge_pixels(cov) for size, cov in covs.items()}
    return Measure(
        name=item.name,
        cls=classify(item.name, labels),
        ink=ink,
        soft=soft,
        soft_best=min([soft[DESIGN_SIZE]] + [count for count, _, _ in soft_placements(polys, item.ink)]),
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
    cx, cy = m.centre
    tolerance = SYMMETRIC_TOLERANCE if short in labels.symmetric else CENTRE_TOLERANCE
    if not exempt("centre") and (abs(cx) > tolerance + 0.01 or abs(cy) > CENTRE_TOLERANCE + 0.01):
        problems.append(f"{short}: ink centre is off the box centre by {cx:+.2f}, {cy:+.2f} px; move it with dx/dy")
    for size, limit in SOFT_LIMIT.items():
        if not exempt("soft") and m.soft[size] > limit:
            problems.append(f"{short}: {m.soft[size]} soft edge pixels at {size} px (limit {limit}); the glyph is placed badly")
    allowed = SOFT_SLACK[0] * m.soft_best + SOFT_SLACK[1]
    if not exempt("soft") and m.soft[DESIGN_SIZE] > allowed:
        problems.append(
            f"{short}: {m.soft[DESIGN_SIZE]} soft edge pixels at 16 px, {m.soft_best} at its best placement "
            f"(allowed {allowed:.0f}); the glyph is placed badly: fix dx, dy or align"
        )
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
    named = (
        labels.symmetric
        | labels.marks
        | labels.mirror_rtl
        | {n for names in labels.exempt.values() for n in names}
        | set(labels.policy)
    )
    for short in sorted(named - shorts):
        problems.append(f"{short}: named in a labels.tsv directive but has no source")
    for short in sorted(labels.mirror_rtl & labels.symmetric):
        problems.append(f"{short}: is left-right symmetric, so mirroring it in a right to left interface changes nothing; drop it from `#mirror_rtl`")
    return problems


def counter_problems(compiled: dict[str, Compiled], labels: Labels) -> list[str]:
    """A fork glyph keeps every hole and cut-out it has: fitting must not close one up."""
    problems: list[str] = []
    for name, item in compiled.items():
        short = short_name(name)
        row = labels.rows.get(short)
        if row is None or row.fork is None or short in labels.exempt.get("counters", set()):
            continue
        natural = fork_outline({"name": row.fork, "weight": row.weight, "align": "none"}, name).path
        before, after = len(contours_of(natural)), len(contours_of(item.path))
        if after < before:
            problems.append(f"{short}: fitting closed {before - after} of the {row.fork} glyph's {before} holes or parts (mirror or scale)")
    return problems


def contour_aspects(path: pathops.Path) -> list[float]:
    """Width over height of each contour's ink box, sorted: scale and shift leave it unchanged."""
    aspects = []
    for poly in polygons(path):
        xs, ys = [x for x, _ in poly], [y for _, y in poly]
        aspects.append((max(xs) - min(xs)) / max(max(ys) - min(ys), EPS))
    return sorted(aspects)


def distortion_problems(compiled: dict[str, Compiled], labels: Labels) -> list[str]:
    """A fork glyph is only moved and scaled by one factor: every contour keeps its proportions,
    so a dot stays a circle and a cell keeps its shape. (A `mirror` icon is rebuilt from one half.)"""
    problems: list[str] = []
    for name, item in compiled.items():
        short = short_name(name)
        row = labels.rows.get(short)
        if row is None or row.fork is None or "mirror" in row.attrs:
            continue
        natural = contour_aspects(fork_outline({"name": row.fork, "weight": row.weight, "align": "none"}, name).path)
        placed = contour_aspects(item.path)
        if len(natural) != len(placed) or any(abs(a - b) > 0.02 * max(a, b) for a, b in zip(natural, placed)):
            problems.append(f"{short}: a contour of the {row.fork} glyph changed proportions: the glyph is warped")
    return problems


SIDEBAR_DENSITY = (0.10, 0.55)  # ink as a share of the 16 x 16 box: the set mixes fills (40 to 50 %) and outlines (20 to 40 %) but has no solid slab


def sidebar_problems(measures: list[Measure], labels: Labels) -> list[str]:
    """The sidebar icons (groups.txt [sidebar]) are one set: none is a heavier slab than the rest."""
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
        if not SIDEBAR_DENSITY[0] <= m.density <= SIDEBAR_DENSITY[1]:
            problems.append(
                f"{short}: ink covers {100 * m.density:.0f} % of the box; the sidebar set keeps to "
                f"{100 * SIDEBAR_DENSITY[0]:.0f} to {100 * SIDEBAR_DENSITY[1]:.0f} % so no icon is a slab beside the rest"
            )
    return problems


def lint_pixels(compiled: dict[str, Compiled], labels: Labels, whole_set: bool) -> tuple[list[str], list[Measure]]:
    """Judge every icon by its pixels. `whole_set` also checks labels.tsv covers the sources."""
    measures = [measure_icon(item, labels) for item in compiled.values()]
    problems = (
        label_problems(compiled, labels) + counter_problems(compiled, labels) + distortion_problems(compiled, labels)
    ) if whole_set else []
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
# `build.py --suggest` proposes the `scale` of a fork icon: the factor closest to 1 that brings
# the long side of the ink into the range of its class in RULES. Nothing else is fitted: the
# shift that puts edges on pixel lines is automatic (see "the chat fork's geometry").


def suggest_fit(short: str, row, labels: Labels) -> dict[str, str]:
    """The scale for a fork row, as a string ready for labels.tsv (empty when 1 already fits)."""
    base = {"name": row.fork, "weight": row.weight}
    if "mirror" in row.attrs:
        base["mirror"] = row.attrs["mirror"]
    where = f"{short}-symbolic.svg"

    def long_side(zoom: float) -> float:
        l, t, r, b = fork_outline(dict(base, scale=repr(zoom), align="none"), where).path.bounds
        return max(r - l, b - t)

    cls = classify(short, labels)
    lo, hi = RULES[cls]
    valid = [
        zoom
        for zoom in (round(step * 0.01, 2) for step in range(60, 171))
        if lo - 0.05 <= long_side(zoom) <= hi + 0.05
    ]
    if not valid:
        raise BuildError(f"{short}: no scale puts the long side in {lo:g} to {hi:g} px")
    zoom = min(valid, key=lambda z: (abs(z - 1.0), z))
    return {} if abs(zoom - 1.0) < 1e-9 else {"scale": f"{zoom:g}"}


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
        "--suggest", nargs="*", metavar="ICON", help="propose the scale for fork icons (all, or the ones named)"
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
