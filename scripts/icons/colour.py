#!/usr/bin/env python3
"""Build the colour file icons from scripts/icons/colour/*.svg.

The monochrome set (build.py) is one fill path tinted at draw time. The file and folder
icons of the Files app are a second, separate set, drawn in the language of the Bingux Files
app icon (packages/bingux-files-app/bingux-files.svg): rounded shapes with one radius ratio,
a folder with a tab and a lighter front panel, a page with a rounded folded corner, light
falling from above (a pale sheen at the top, a soft shade at the bottom), a darker back layer,
a thin highlight on the top edge. Their colours bingux-ui takes from the theme at draw time
(`files/colour.rs`). They never go through build.py's fit and weight policy; this script is
their compiler and their check.

A source is a plain SVG, one per icon (`file-pdf.svg`, `folder-music.svg`), with two parts:

  - shapes on a 16 x 16 design grid: the pixel art for the grids 16, 20, 24 and 32;
  - one `<g data-grid="128">` with the same glyph drawn smoothly on a 128 x 128 grid (round
    caps and joins, curves): the vector for 48 px and up. It is copied, not snapped.

Colours are roles, not values:

  fill / stroke = "ink"    the glyph colour, `currentColor` in the output
                 "body"    the family colour: no fill attribute, the viewer's `fill` is used
                 "shade"   black at 22 %, over the body: the fold, the back of a folder
                 "light"   white at 28 %, over the body: an edge catching the light
                 "lit"     white fading out downwards (the sheen at the top of a surface)
                 "deep"    black fading in downwards (the shade at the foot of a surface)
                 "paper"   the white sheet of a folder or a bin, near-white at the top, grey at the foot
                 "none"
  stroke = "ink" or "none"  (strokes take no other colour)

The root says what the compiler adds around the shapes:

  data-chrome="page"      the page: rounded outline, sheen, shade, rounded folded corner
  data-chrome="folder"    the folder: back with its tab, the paper standing in it (from 32), the
                          front panel with its sheen, lip and highlight
  data-chrome="none"      nothing: the source draws everything (the bin)
  data-label="XLS"        from 24 px the glyph is replaced by this label, set in a pixel font at
                          24 and 32 (3 x 5, 5 x 7) and in rounded strokes at 128; 2 to 4 letters

A pixel shape may carry `data-at="small"` (grids 16 and 20), `data-at="mid"` (24 and 32), or a
list of grids (`data-at="20"`), to draw it only there. A rect may have `rx`: its radius, scaled
with the grid and not snapped.

Each source is compiled for five grids: 16, 20, 24, 32 (pixel) and 128 (vector). An icon is
drawn at the grid whose whole multiple is nearest, up to 40 px, and from the 128 grid above it
(`files/colour.rs::colour_grid`), so the shapes sit on the pixel lattice at every size the
Files app uses. Pixel grids snap coordinates: rectangles to whole pixels, strokes to the pixel
centre (odd widths) or the line between pixels (even widths), polygon and path points to whole
pixels; the corner radii of the chrome are tables per grid (RADIUS), the straight edges stay on
the lattice. Nothing is ever warped or thickened.

Output: packages/bingux-ui/icons/colour/<name>-<grid>.svg.

Usage:
  colour.py            compile
  colour.py --check    sources valid, output current, the list in files/colour.rs matches,
                       every pixel shape on the lattice, every icon inked inside its box
  colour.py --sheet DIR   contact sheets (PNG, via rsvg-convert) with the palette of both schemes
  colour.py --sheet DIR --tones FILE
                       the same in the tones the app derives: `BINGUX_COLOUR_TONES=FILE cargo test
                       --features testing --lib the_tones_can_be_written` in packages/bingux-ui writes FILE
"""

from __future__ import annotations

import argparse
import json
import math
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
SRC = HERE / "colour"
OUT = REPO / "packages/bingux-ui/icons/colour"
LIST_RS = REPO / "packages/bingux-ui/src/files/colour.rs"

# The rules of design/rules.json this check holds (read by scripts/rules-check.py).
RULE_IDS = {"*": ["IC-11"]}

GRIDS = (16, 20, 24, 32, 128)
PIXEL_GRIDS = (16, 20, 24, 32)
VECTOR_GRID = 128
NS = "{http://www.w3.org/2000/svg}"
SHADE = ('#000', '.22')
LIGHT = ('#fff', '.28')

# The overlays. The three gradients run top to bottom over the body colour, so the light falls
# from above in every icon: `lit` is white at LIT_TOP at the top edge and gone half way down,
# `deep` is black from half way down to DEEP_BOTTOM at the foot. files/colour.rs holds the
# same numbers (it keeps the tones readable under them) and a test reads them back from here.
LIT_TOP = 0.32
DEEP_BOTTOM = 0.10
GRADIENTS = {
    "lit": [(0, "#fff", LIT_TOP), (0.55, "#fff", 0)],
    "deep": [(0.45, "#000", 0), (1, "#000", DEEP_BOTTOM)],
    "paper": [(0, "#fff", 1), (1, "#ebeae8", 1)],
}

# The rim of a page: black at RIM over the body (and over its sheen and shade), RIM_WIDTH
# pixels wide on the inside of the silhouette. It holds the page's edge on the ground, so a
# light body (a yellow) can keep its hue without the page being darkened to meet contrast.
# files/colour.rs holds the same number (the edge contrast is computed through it) and a test
# reads it back from the output.
RIM = 0.26
RIM_WIDTH = {16: 1, 20: 1, 24: 1, 32: 1, 128: 3}

# The page of each grid: left, right, top, bottom and the size of the folded corner.
PAGE = {
    16: (2, 14, 1, 15, 4),
    20: (3, 17, 1, 19, 5),
    24: (3, 21, 1, 23, 6),
    32: (4, 28, 2, 30, 8),
    128: (16, 112, 8, 120, 32),
}
# The folder of each grid: left, right, top of the tab, the x where the tab ends (where its top
# edge turns down), the x where the slope meets the body, the top of the body behind, the top
# and the bottom of the front panel, the lip below the panel.
FOLDER = {
    16: (1, 15, 2, 7, 8, 3, 5, 14, 0),
    20: (1, 19, 3, 9, 10, 4, 6, 17, 0),
    24: (2, 22, 3, 10, 12, 5, 8, 21, 0),
    32: (3, 29, 4, 14, 17, 7, 11, 28, 1),
    128: (12, 116, 16, 57, 66, 27, 44, 114, 2),
}
# Corner radii, per grid, of the page and the folder (about one eighth of the width: the app
# icon's 12 on its 104), of the cut that makes the page's folded corner and of the folded
# flap itself. Whole pixel grids keep their straight edges on the lattice; only the corners turn.
RADIUS = {
    16: (2.0, 0.6, 0.0),
    20: (2.5, 0.8, 0.0),
    24: (3.0, 1.0, 0.8),
    32: (4.0, 1.4, 1.2),
    128: (14.0, 6.0, 5.0),
}
# Where a label sits: the font, its cell size and the top row, per grid with a label.
LABEL = {24: (3, 5, 14, 1), 32: (5, 7, 17, 2)}
# The label of the vector grid: letter height, stroke, the widest a letter, the gap, the top row.
VLABEL = (30, 7, 19, 10, 64)

FONT3 = {
    "T": ("111", "010", "010", "010", "010"),
    "X": ("101", "101", "010", "101", "101"),
    "D": ("110", "101", "101", "101", "110"),
    "O": ("111", "101", "101", "101", "111"),
    "C": ("011", "100", "100", "100", "011"),
    "L": ("100", "100", "100", "100", "111"),
    "S": ("011", "100", "010", "001", "110"),
    "P": ("110", "101", "110", "100", "100"),
    "F": ("111", "100", "110", "100", "100"),
    "Z": ("111", "001", "010", "100", "111"),
    "I": ("111", "010", "010", "010", "111"),
    "M": ("101", "111", "111", "101", "101"),
    "B": ("110", "101", "110", "101", "110"),
    "E": ("111", "100", "110", "100", "111"),
}
FONT5 = {
    "T": ("11111", "00100", "00100", "00100", "00100", "00100", "00100"),
    "X": ("10001", "10001", "01010", "00100", "01010", "10001", "10001"),
    "D": ("11110", "10001", "10001", "10001", "10001", "10001", "11110"),
    "O": ("01110", "10001", "10001", "10001", "10001", "10001", "01110"),
    "C": ("01110", "10001", "10000", "10000", "10000", "10001", "01110"),
    "L": ("10000", "10000", "10000", "10000", "10000", "10000", "11111"),
    "S": ("01111", "10000", "10000", "01110", "00001", "00001", "11110"),
    "P": ("11110", "10001", "10001", "11110", "10000", "10000", "10000"),
    "F": ("11111", "10000", "10000", "11110", "10000", "10000", "10000"),
    "Z": ("11111", "00001", "00010", "00100", "01000", "10000", "11111"),
    "I": ("11111", "00100", "00100", "00100", "00100", "00100", "11111"),
    "M": ("10001", "11011", "10101", "10001", "10001", "10001", "10001"),
    "B": ("11110", "10001", "10001", "11110", "10001", "10001", "11110"),
    "E": ("11111", "10000", "10000", "11110", "10000", "10000", "11111"),
}
FONTS = {3: FONT3, 5: FONT5}


class ColourError(Exception):
    pass


def half_up(v: float) -> int:
    return math.floor(v + 0.5)


def num(v: float) -> str:
    s = f"{v:.3f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


# ---------------------------------------------------------------- sources


def source_names() -> list[str]:
    return sorted(p.stem for p in SRC.glob("*.svg"))


def applies(node: ET.Element, grid: int) -> bool:
    at = node.get("data-at")
    if at is None:
        return True
    if at == "small":
        return grid <= 20
    if at == "mid":
        return grid in (24, 32)
    return str(grid) in at.split()


def role_style(attr: dict[str, str], where: str) -> dict[str, str]:
    """Output paint attributes from the roles of a source element."""
    out: dict[str, str] = {}
    fill = attr.get("fill", "none")
    stroke = attr.get("stroke", "none")
    opacity = attr.get("fill-opacity")
    if fill == "ink":
        out["fill"] = "currentColor"
        if opacity:
            out["fill-opacity"] = opacity
    elif fill == "body":
        if opacity:
            raise ColourError(f"{where}: body fill takes no opacity")
    elif fill == "shade":
        out["fill"], out["fill-opacity"] = SHADE
    elif fill == "light":
        out["fill"], out["fill-opacity"] = LIGHT
    elif fill in GRADIENTS:
        out["fill"] = f"url(#{fill})"
    elif fill == "none":
        out["fill"] = "none"
    else:
        raise ColourError(f"{where}: fill must be ink, body, shade, light, lit, deep, paper or none, not {fill!r}")
    if stroke == "ink":
        out["stroke"] = "currentColor"
    elif stroke != "none":
        raise ColourError(f"{where}: stroke must be ink or none, not {stroke!r}")
    if "fill-rule" in attr:
        out["fill-rule"] = attr["fill-rule"]
    return out


def attrs_text(attrs: dict[str, str]) -> str:
    return "".join(f' {k}="{v}"' for k, v in attrs.items())


# ---------------------------------------------------------------- snapping


def stroke_width(sw: float, k: float) -> int:
    return max(1, half_up(sw * k))


def snap_stroke(v: float, k: float, width: int) -> float:
    """A stroke centre: the middle of a pixel for an odd width, a pixel edge for an even one."""
    if width % 2:
        return math.floor(v * k) + 0.5
    return float(half_up(v * k))


PATH_ARITY = {"M": 2, "L": 2, "H": 1, "V": 1, "A": 7, "C": 6, "Q": 4, "Z": 0}


def parse_path(d: str, where: str) -> list[tuple[str, list[float]]]:
    tokens = re.findall(r"[MLHVACQZ]|-?\d*\.?\d+", d)
    out: list[tuple[str, list[float]]] = []
    i = 0
    while i < len(tokens):
        cmd = tokens[i]
        if cmd not in PATH_ARITY:
            raise ColourError(f"{where}: path command {cmd!r} (use absolute M L H V A C Q Z)")
        n = PATH_ARITY[cmd]
        i += 1
        while True:
            args = [float(t) for t in tokens[i : i + n]]
            if len(args) != n:
                raise ColourError(f"{where}: path {cmd} needs {n} numbers")
            out.append((cmd, args))
            i += n
            if n == 0 or i >= len(tokens) or tokens[i] in PATH_ARITY:
                break
            if cmd == "M":
                cmd = "L"
    return out


def snap_path(d: str, k: float, snap, where: str) -> str:
    parts: list[str] = []
    for cmd, a in parse_path(d, where):
        if cmd in "ML":
            parts.append(f"{cmd}{num(snap(a[0]))} {num(snap(a[1]))}")
        elif cmd == "H":
            parts.append(f"H{num(snap(a[0]))}")
        elif cmd == "V":
            parts.append(f"V{num(snap(a[0]))}")
        elif cmd == "A":
            parts.append(
                f"A{num(a[0] * k)} {num(a[1] * k)} {num(a[2])} {int(a[3])} {int(a[4])} {num(snap(a[5]))} {num(snap(a[6]))}"
            )
        elif cmd == "C":
            parts.append("C" + " ".join(num(snap(v)) for v in a))
        elif cmd == "Q":
            parts.append("Q" + " ".join(num(snap(v)) for v in a))
        else:
            parts.append("Z")
    return "".join(parts)


def float_list(raw: str) -> list[float]:
    return [float(v) for v in re.findall(r"-?\d*\.?\d+", raw)]


def shape(node: ET.Element, grid: int, where: str) -> str:
    """One source shape as an output element, snapped to `grid`."""
    k = grid / 16
    tag = node.tag.removeprefix(NS)
    attr = dict(node.attrib)
    style = role_style(attr, where)
    snap = lambda v: float(half_up(v * k))  # noqa: E731
    stroked = style.get("stroke") == "currentColor"
    extra = ""
    if stroked:
        width = stroke_width(float(attr.get("stroke-width", "1")), k)
        extra = f' stroke-width="{width}" stroke-linejoin="{attr.get("stroke-linejoin", "miter")}" stroke-linecap="{attr.get("stroke-linecap", "butt")}"'
        snap = lambda v: snap_stroke(v, k, width)  # noqa: E731
    paint = attrs_text(style) + extra
    f = lambda key, default=0.0: float(attr.get(key, default))  # noqa: E731
    if tag == "rect":
        x0, y0 = half_up(f("x") * k), half_up(f("y") * k)
        # A thin bar keeps its thickness; anything else keeps its neighbours (no gap, no overlap).
        if f("width") < 2:
            x1 = x0 + max(1, half_up(f("width") * k))
        else:
            x1 = half_up((f("x") + f("width")) * k)
        if f("height") < 2:
            y1 = y0 + max(1, half_up(f("height") * k))
        else:
            y1 = half_up((f("y") + f("height")) * k)
        x1, y1 = max(x1, x0 + 1), max(y1, y0 + 1)
        if stroked:
            # The centreline of an outline: inset by half the stroke so the outer edge is the lattice.
            w = stroke_width(float(attr.get("stroke-width", "1")), k)
            x0, y0, x1, y1 = (v + (w / 2 if i < 2 else -w / 2) for i, v in enumerate((x0, y0, x1, y1)))
        if f("rx"):
            return rect_rounded(x0, y0, x1, y1, min(f("rx") * k, (x1 - x0) / 2, (y1 - y0) / 2), paint)
        return f'<rect x="{num(x0)}" y="{num(y0)}" width="{num(x1 - x0)}" height="{num(y1 - y0)}"{paint}/>'
    if tag == "circle":
        cx, cy, r = f("cx") * k, f("cy") * k, f("r") * k
        # Centre on the lattice or half-way, so the extremes of the circle meet a pixel edge or its middle.
        cx, cy, r = (half_up(v * 2) / 2 for v in (cx, cy, r))
        return f'<circle cx="{num(cx)}" cy="{num(cy)}" r="{num(r)}"{paint}/>'
    if tag in ("polygon", "polyline"):
        pts = float_list(attr["points"])
        pairs = [(snap(pts[i]), snap(pts[i + 1])) for i in range(0, len(pts), 2)]
        text = " ".join(f"{num(x)},{num(y)}" for x, y in pairs)
        return f'<{tag} points="{text}"{paint}/>'
    if tag == "line":
        x1, y1, x2, y2 = (snap(f(key)) for key in ("x1", "y1", "x2", "y2"))
        return f'<line x1="{num(x1)}" y1="{num(y1)}" x2="{num(x2)}" y2="{num(y2)}"{paint}/>'
    if tag == "path":
        return f'<path d="{snap_path(attr["d"], k, snap, where)}"{paint}/>'
    raise ColourError(f"{where}: <{tag}> is not supported (rect, circle, polygon, polyline, line, path)")


def rounded(points: list[tuple[float, float, float]]) -> str:
    """A closed path through `points` (x, y, radius), each corner turned by its radius (cubic
    arcs, at most half of either edge so neighbours never overlap)."""
    n = len(points)
    parts: list[str] = []
    for i, (x, y, r) in enumerate(points):
        px, py, _ = points[i - 1]
        nx, ny, _ = points[(i + 1) % n]
        d1, d2 = math.hypot(px - x, py - y), math.hypot(nx - x, ny - y)
        t = min(r, d1 / 2, d2 / 2)
        ax, ay = x + (px - x) / d1 * t, y + (py - y) / d1 * t
        bx, by = x + (nx - x) / d2 * t, y + (ny - y) / d2 * t
        parts.append(("M" if i == 0 else "L") + f"{num(ax)} {num(ay)}")
        if t > 0:
            k = 0.5523
            c1 = (ax + (x - ax) * k, ay + (y - ay) * k)
            c2 = (bx + (x - bx) * k, by + (y - by) * k)
            parts.append(f"C{num(c1[0])} {num(c1[1])} {num(c2[0])} {num(c2[1])} {num(bx)} {num(by)}")
    return "".join(parts) + "Z"


def box(x0: float, y0: float, x1: float, y1: float, r: float) -> str:
    return rounded([(x0, y0, r), (x1, y0, r), (x1, y1, r), (x0, y1, r)])


def layer(d: str, paint: str = "") -> str:
    """One chrome path; no paint is the body (the viewer's fill)."""
    return f'<path d="{d}"{paint}/>'


SHADE_PAINT = f' fill="{SHADE[0]}" fill-opacity="{SHADE[1]}"'


def page_chrome(grid: int) -> list[str]:
    x0, x1, y0, y1, fold = PAGE[grid]
    r, cut, flap = RADIUS[grid]
    xf, yf = x1 - fold, y0 + fold
    outline = rounded([(x0, y0, r), (xf, y0, cut), (x1, yf, cut), (x1, y1, r), (x0, y1, r)])
    corner = rounded([(xf, y0, cut), (xf, yf, flap), (x1, yf, cut)])
    # The rim: the body, darkened, along the whole silhouette and inside it. It is a stroke of
    # twice the rim's width on the outline, clipped to the outline, so it follows the corners
    # and the folded corner's cut and never leaves the page.
    clip = f'<clipPath id="page"><path d="{outline}"/></clipPath>'
    rim = f'<path d="{outline}" fill="none" stroke="#000" stroke-opacity="{RIM}" stroke-width="{2 * RIM_WIDTH[grid]}" clip-path="url(#page)"/>'
    return [
        layer(outline),
        layer(outline, ' fill="url(#lit)"'),
        layer(outline, ' fill="url(#deep)"'),
        layer(corner, SHADE_PAINT),
        clip,
        rim,
    ]


def folder_chrome(grid: int) -> list[str]:
    x0, x1, tab_top, tab_x, slope_x, body_top, face_top, face_bottom, lip = FOLDER[grid]
    r = RADIUS[grid][0]
    rt = min(r, 6.0 * grid / 32)
    behind = face_bottom - r
    back = rounded([(x0, tab_top, r), (tab_x, tab_top, rt), (slope_x, body_top, rt), (x1, body_top, r), (x1, behind, 0), (x0, behind, 0)])
    out = [layer(back), layer(back, SHADE_PAINT)]
    if grid >= 32:
        # The paper standing in the folder, and a line of its text above the front panel.
        inset = (x1 - x0) * 14 / 104
        px0, px1 = x0 + inset, x1 - inset
        py0 = body_top + (2 if grid == VECTOR_GRID else 1)
        pr = 6 if grid == VECTOR_GRID else 1.5
        if grid == VECTOR_GRID:
            out.append(layer(box(px0, py0 + 3, px1, face_top + 12, pr), SHADE_PAINT.replace(".22", ".35")))
        out.append(layer(box(px0, py0, px1, face_top + (12 if grid == VECTOR_GRID else 4), pr), ' fill="url(#paper)"'))
        bar_h = 5 if grid == VECTOR_GRID else 1
        bar_y = py0 + (9 if grid == VECTOR_GRID else 1)
        bar_x = px0 + (8 if grid == VECTOR_GRID else 2)
        bar_w = 40 if grid == VECTOR_GRID else 8
        out.append(layer(box(bar_x, bar_y, bar_x + bar_w, bar_y + bar_h, bar_h / 2 if grid == VECTOR_GRID else 0.5), SHADE_PAINT))
    face = box(x0, face_top, x1, face_bottom, r)
    if lip:
        under = box(x0, face_top + lip, x1, face_bottom + lip, r)
        out += [layer(under), layer(under, SHADE_PAINT.replace(".22", ".36"))]
    # The front panel is the accent lifted (a flat light) with the sheen on top: pale, like the app icon's.
    out += [layer(face), layer(face, f' fill="{LIGHT[0]}" fill-opacity="{LIGHT[1]}"'), layer(face, ' fill="url(#lit)"')]
    if grid >= 24:
        # The edge of the panel catching the light, short of the rounded corners.
        hw = 1.5 if grid == VECTOR_GRID else 1
        hx = x0 + r * 0.45
        out.append(layer(box(hx, face_top + (0.5 if grid == VECTOR_GRID else 0), x1 - hx + 0, face_top + (0.5 if grid == VECTOR_GRID else 0) + hw, hw / 2 if grid == VECTOR_GRID else 0), ' fill="#fff" fill-opacity=".6"'))
    return out


# The strokes of the vector label font: letters on a 10 x 14 cell, absolute M L H V Q C only.
STROKE_FONT = {
    "T": "M0 0H10M5 0V14",
    "X": "M0 0L10 14M10 0L0 14",
    "D": "M0 0V14H4Q10 14 10 7Q10 0 4 0Z",
    "O": "M3 0H7Q10 0 10 3V11Q10 14 7 14H3Q0 14 0 11V3Q0 0 3 0Z",
    "C": "M10 3Q10 0 7 0H3Q0 0 0 3V11Q0 14 3 14H7Q10 14 10 11",
    "L": "M0 0V14H10",
    "S": "M10 3Q10 0 7 0H3Q0 0 0 3Q0 6.5 5 7Q10 7.5 10 11Q10 14 7 14H3Q0 14 0 11",
    "P": "M0 14V0H6Q10 0 10 3.5Q10 7 6 7H0",
    "F": "M10 0H0V14M0 7H7",
    "Z": "M0 0H10L0 14H10",
    "I": "M2 0H8M5 0V14M2 14H8",
    "M": "M0 14V0L5 8L10 0V14",
    "B": "M0 0H6Q9.5 0 9.5 3.5Q9.5 7 6 7H0M6 7Q10 7 10 10.5Q10 14 6 14H0V0",
    "E": "M10 0H0V14H10M0 7H7",
}


def vector_label(text: str, where: str) -> str:
    height, stroke, widest, gap, top = VLABEL
    for ch in text:
        if ch not in STROKE_FONT:
            raise ColourError(f"{where}: no letter {ch!r} in the stroke font")
    x0, x1 = PAGE[VECTOR_GRID][0], PAGE[VECTOR_GRID][1]
    inner = x1 - x0 - 2 * 16 - stroke
    width = min(widest, (inner - gap * (len(text) - 1)) / len(text))
    total = len(text) * width + (len(text) - 1) * gap
    left = (x0 + x1) / 2 - total / 2
    d: list[str] = []
    for i, ch in enumerate(text):
        ox = left + i * (width + gap)
        for cmd, args in parse_path(STROKE_FONT[ch], where):
            if cmd == "H":
                d.append(f"H{num(ox + args[0] * width / 10)}")
            elif cmd == "V":
                d.append(f"V{num(top + args[0] * height / 14)}")
            elif cmd == "Z":
                d.append("Z")
            else:
                pts = " ".join(f"{num(ox + args[j] * width / 10)} {num(top + args[j + 1] * height / 14)}" for j in range(0, len(args), 2))
                d.append(f"{cmd}{pts}")
    return f'<path d="{"".join(d)}" fill="none" stroke="currentColor" stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round"/>'


def label_shapes(text: str, grid: int, where: str) -> str:
    cell_w, cell_h, top, gap = LABEL[grid]
    font = FONTS[cell_w]
    for ch in text:
        if ch not in font:
            raise ColourError(f"{where}: no letter {ch!r} in the label font")
    x0, x1 = PAGE[grid][0], PAGE[grid][1]
    width = len(text) * cell_w + (len(text) - 1) * gap
    left = x0 + (x1 - x0 - width) // 2
    rects: list[str] = []
    for i, ch in enumerate(text):
        origin = left + i * (cell_w + gap)
        for row, bits in enumerate(font[ch]):
            col = 0
            while col < cell_w:
                if bits[col] == "1":
                    end = col
                    while end < cell_w and bits[end] == "1":
                        end += 1
                    rects.append(f"M{origin + col} {top + row}h{end - col}v1h-{end - col}z")
                    col = end
                else:
                    col += 1
    return f'<path d="{"".join(rects)}" fill="currentColor"/>'


VECTOR_GEOMETRY = {
    "rect": ("x", "y", "width", "height", "rx", "ry"),
    "circle": ("cx", "cy", "r"),
    "ellipse": ("cx", "cy", "rx", "ry"),
    "path": ("d",),
    "polygon": ("points",),
    "polyline": ("points",),
    "line": ("x1", "y1", "x2", "y2"),
}


def vector_shape(node: ET.Element, where: str) -> str:
    """One shape of the vector group, copied: the geometry is the author's, the paint a role."""
    tag = node.tag.removeprefix(NS)
    if tag not in VECTOR_GEOMETRY:
        raise ColourError(f"{where}: <{tag}> is not supported in the vector group")
    attr = dict(node.attrib)
    style = role_style(attr, where)
    geometry = "".join(f' {key}="{attr[key]}"' for key in VECTOR_GEOMETRY[tag] if key in attr)
    extra = "".join(
        f' {key}="{attr[key]}"'
        for key in ("stroke-width", "stroke-linecap", "stroke-linejoin", "opacity", "stroke-opacity", "transform")
        if key in attr
    )
    return f"<{tag}{geometry}{attrs_text(style)}{extra}/>"


def rect_rounded(x0: float, y0: float, x1: float, y1: float, r: float, paint: str) -> str:
    return f'<path d="{box(x0, y0, x1, y1, r)}"{paint}/>'


def defs_for(body: str) -> str:
    used = [name for name in GRADIENTS if f"url(#{name})" in body]
    if not used:
        return ""
    out = ["\n    <defs>"]
    for name in used:
        stops = "".join(
            f'<stop offset="{num(offset)}" stop-color="{colour}"' + (f' stop-opacity="{num(alpha)}"' if alpha != 1 else "") + "/>"
            for offset, colour, alpha in GRADIENTS[name]
        )
        out.append(f'<linearGradient id="{name}" x1="0" x2="0" y1="0" y2="1">{stops}</linearGradient>')
    out.append("</defs>")
    return "".join(out)


def compile_one(name: str, grid: int) -> str:
    path = SRC / f"{name}.svg"
    try:
        root = ET.parse(path).getroot()
    except ET.ParseError as exc:
        raise ColourError(f"{path.name}: {exc}") from exc
    where = path.name
    if root.tag != NS + "svg" or root.get("viewBox") != "0 0 16 16":
        raise ColourError(f'{where}: the root must be <svg viewBox="0 0 16 16">')
    chrome = root.get("data-chrome")
    if chrome not in ("page", "folder", "none"):
        raise ColourError(f'{where}: data-chrome must be "page", "folder" or "none"')
    label = root.get("data-label")
    if label and not 2 <= len(label) <= 4:
        raise ColourError(f"{where}: a label has two to four letters")
    vector = grid == VECTOR_GRID
    body: list[str] = []
    if chrome == "page":
        body += page_chrome(grid)
    elif chrome == "folder":
        body += folder_chrome(grid)
    if label and grid >= 24:
        body.append(vector_label(label, where) if vector else label_shapes(label, grid, where))
    for node in root:
        if node.tag == NS + "g":
            if node.get("data-grid") != str(VECTOR_GRID):
                raise ColourError(f'{where}: a group must be <g data-grid="{VECTOR_GRID}">')
            if vector:
                body += [vector_shape(child, where) for child in node]
            continue
        if vector or (label and grid >= 24) or not applies(node, grid):
            continue
        body.append(shape(node, grid, where))
    text = "".join(f"\n    {part}" for part in body)
    return (
        f'<svg height="{grid}px" viewBox="0 0 {grid} {grid}" width="{grid}px" xmlns="http://www.w3.org/2000/svg">'
        + defs_for(text)
        + text
        + "\n</svg>\n"
    )


def expected() -> dict[str, str]:
    files: dict[str, str] = {}
    errors: list[str] = []
    for name in source_names():
        for grid in GRIDS:
            try:
                files[f"{name}-{grid}.svg"] = compile_one(name, grid)
            except ColourError as exc:
                errors.append(str(exc))
    if errors:
        raise ColourError("\n".join(dict.fromkeys(errors)))
    return files


# ---------------------------------------------------------------- checks


def listed_in_rust() -> list[str]:
    text = LIST_RS.read_text()
    block = re.search(r"colour_icons! \{(.*?)\n\}", text, re.S)
    if not block:
        raise ColourError(f"{LIST_RS.relative_to(REPO)} has no `colour_icons! {{ ... }}` list")
    return re.findall(r'"([a-z0-9-]+)"', block.group(1))


NUM = r"-?\d+(?:\.\d+)?"


def lattice_problems(name: str, grid: int, svg: str) -> list[str]:
    """Rectangles on whole pixels, strokes on the right half pixel, inked inside the box. The
    vector grid is drawn as the author wrote it: only its box is held."""
    problems: list[str] = []
    where = f"{name}-{grid}"
    if grid == VECTOR_GRID:
        absolute = [m for m in re.findall(r'(?:points|d)="([^"]*)"', svg) if not re.search("[a-z]", m)]
        if any(v < -0.001 or v > grid + 0.001 for chunk in absolute for v in (float(x) for x in re.findall(NUM, chunk))):
            problems.append(f"{where}: a coordinate is outside the {grid} px box")
        if not re.search(r"<(?:path|rect|polygon|polyline|circle)", svg):
            problems.append(f"{where}: draws nothing")
        return problems
    for m in re.finditer(r"<rect ([^>]*)/>", svg):
        a = dict(re.findall(r'([a-z-]+)="([^"]*)"', m.group(1)))
        values = [float(a[k]) for k in ("x", "y", "width", "height")]
        if "stroke-width" in a:
            w = float(a["stroke-width"])
            if any((v - w / 2) % 1 for v in values[:2]):
                problems.append(f"{where}: an outline rect is off the lattice: {m.group(0)}")
        elif any(v % 1 for v in values):
            problems.append(f"{where}: a rect is off whole pixels: {m.group(0)}")
    for m in re.finditer(r'<(?:polyline|line|path)[^>]*stroke-width="(\d+)"[^>]*>', svg):
        width = int(m.group(1))
        coords = [float(v) for v in re.findall(NUM, re.sub(r'stroke-width="\d+"', "", m.group(0)))]
        if width % 2 and any((v - 0.5) % 1 for v in coords if True) and 'points="' in m.group(0):
            problems.append(f"{where}: a stroke of width {width} is not centred on pixels: {m.group(0)}")
    # Absolute coordinates only: a label is drawn with relative moves, from a start inside the page.
    absolute = [m for m in re.findall(r'(?:points|d)="([^"]*)"', svg) if not re.search("[a-z]", m)]
    for v in (float(x) for chunk in absolute for x in re.findall(NUM, chunk)):
        if v < -0.001 or v > grid + 0.001:
            problems.append(f"{where}: a coordinate ({num(v)}) is outside the {grid} px box")
            break
    if not re.search(r"<(?:path|rect|polygon|polyline|circle)", svg):
        problems.append(f"{where}: draws nothing")
    allowed_paint = {"none", "currentColor", "#000", "#fff"} | {f"url(#{name})" for name in GRADIENTS}
    for fill in re.findall(r'(?:fill|stroke)="([^"]+)"', svg):
        if fill not in allowed_paint:
            problems.append(f"{where}: paint {fill!r} is not a role colour")
    return problems


def check() -> str:
    first, second = expected(), expected()
    if first != second:
        raise ColourError("colour output is not deterministic")
    problems: list[str] = []
    names = source_names()
    listed = listed_in_rust()
    for name in names:
        if name not in listed:
            problems.append(f"{name}.svg is not listed in files/colour.rs")
    for name in listed:
        if name not in names:
            problems.append(f"files/colour.rs lists {name} but scripts/icons/colour/{name}.svg does not exist")
    if len(set(listed)) != len(listed):
        problems.append("files/colour.rs lists an icon twice")
    for fname, text in first.items():
        path = OUT / fname
        if not path.exists():
            problems.append(f"{fname} missing from {OUT.relative_to(REPO)}; run scripts/icons/colour.py")
        elif path.read_text() != text:
            problems.append(f"{fname} is stale; run scripts/icons/colour.py")
    if OUT.exists():
        for stale in OUT.glob("*.svg"):
            if stale.name not in first:
                problems.append(f"{stale.name} has no source")
    for fname, text in first.items():
        stem, grid = fname.removesuffix(".svg").rsplit("-", 1)
        problems += lattice_problems(stem, int(grid), text)
    if problems:
        raise ColourError("\n".join(problems))
    return f"colour icons: ok ({len(names)} icons x {len(GRIDS)} grids, output matches, pixel shapes on the lattice)"


def write() -> str:
    files = expected()
    OUT.mkdir(parents=True, exist_ok=True)
    for fname, text in files.items():
        (OUT / fname).write_text(text)
    for stale in OUT.glob("*.svg"):
        if stale.name not in files:
            stale.unlink()
    return f"colour icons: wrote {len(files)} files to {OUT.relative_to(REPO)}"


# ---------------------------------------------------------------- contact sheet

# The palette the sheet draws with: only to look at the shapes; the app takes its own from
# the theme (files/colour.rs).
SHEET_PAGE = {"dark": "#222226", "light": "#fafafb"}
SHEET_BODY = {  # family hue (degrees), chroma; lightness per scheme below
    "generic": (260, 0.03), "text": (260, 0.05), "code": (195, 0.10), "config": (80, 0.12), "markup": (235, 0.11),
    "spreadsheet": (148, 0.14), "presentation": (50, 0.16), "pdf": (25, 0.18), "document": (258, 0.16), "ebook": (45, 0.09),
    "image": (335, 0.15), "audio": (5, 0.15), "video": (298, 0.15), "archive": (90, 0.11), "diskimage": (240, 0.04),
    "executable": (165, 0.11), "font": (278, 0.14), "model3d": (125, 0.13), "database": (205, 0.07), "certificate": (100, 0.14),
    "torrent": (135, 0.16), "unknown": (260, 0.02), "folder": (255, 0.15), "trash": (255, 0.15),
}
SHEET_L = {"dark": (0.74, 0.2), "light": (0.52, 0.99)}


def oklch_hex(l: float, c: float, h: float) -> str:
    a, b = c * math.cos(math.radians(h)), c * math.sin(math.radians(h))
    l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3
    m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3
    s_ = (l - 0.0894841775 * a - 1.2914855480 * b) ** 3
    rgb = (4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_, -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_, -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_)
    def enc(v: float) -> int:
        v = min(1.0, max(0.0, v))
        v = 12.92 * v if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055
        return round(v * 255)
    return "#%02x%02x%02x" % tuple(enc(v) for v in rgb)


def family_of(name: str) -> str:
    table = {"file-page": "generic", "file-text": "text", "file-code": "code", "file-config": "config", "file-markup": "markup",
             "file-spreadsheet": "spreadsheet", "file-presentation": "presentation", "file-pdf": "pdf", "file-document": "document",
             "file-ebook": "ebook", "file-image": "image", "file-audio": "audio", "file-video": "video", "file-archive": "archive",
             "file-disk-image": "diskimage", "file-executable": "executable", "file-font": "font", "file-model": "model3d",
             "file-database": "database", "file-certificate": "certificate", "file-torrent": "torrent", "file-unknown": "unknown"}
    return table.get(name, "folder" if name.startswith(("folder", "trash")) else "generic")


def pick_grid(size: int) -> int:
    """The grid files/colour.rs::colour_grid draws a `size` px icon from."""
    if size > 40:
        return VECTOR_GRID
    return min(PIXEL_GRIDS, key=lambda g: (abs(size / g - round(size / g)) / (size / g), -g))


def unique_ids(svg: str, tag: str) -> str:
    """The ids of one icon made its own, so many icons share one sheet document."""
    for name in (*GRADIENTS, "page"):
        svg = svg.replace(f'id="{name}"', f'id="{name}-{tag}"').replace(f"url(#{name})", f"url(#{name}-{tag})")
    return svg


def sheet(dir_: Path, tones: dict | None = None, sizes: tuple[int, ...] = (16, 20, 24, 32, 64, 128)) -> None:
    """Contact sheets: `tones` is the JSON `BINGUX_COLOUR_TONES` writes (the colours the app
    draws), else a stand-in palette to look at the shapes only."""
    dir_.mkdir(parents=True, exist_ok=True)
    cell = 150
    cells = 0
    for part, names in enumerate([source_names()[i : i + 12] for i in range(0, len(source_names()), 12)]):
      for scheme in ("dark", "light"):
          body_l, ink_l = SHEET_L[scheme]
          for scale in (1, 2):
              rows = [(s, s + 14) for s in sizes]
              width = len(names) * cell + 20
              height = sum(h for _, h in rows) + 20
              parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"><rect width="100%" height="100%" fill="{SHEET_PAGE[scheme]}"/>']
              y = 10
              for size, rh in rows:
                  for i, name in enumerate(names):
                      if tones:
                          body, ink = tones[scheme][name]
                      else:
                          hue, chroma = SHEET_BODY[family_of(name)]
                          body = oklch_hex(body_l, chroma, hue)
                          ink = oklch_hex(ink_l, min(chroma, 0.05), hue)
                      grid = pick_grid(size)
                      inner = (OUT / f"{name}-{grid}.svg").read_text().split(">", 1)[1].rsplit("</svg>", 1)[0]
                      cells += 1
                      parts.append(f'<g transform="translate({10 + i * cell} {y}) scale({size / grid})" fill="{body}" color="{ink}">{unique_ids(inner, str(cells))}</g>')
                  y += rh
              parts.append("</svg>")
              svg_path = dir_ / f"sheet-{scheme}-{scale}x-{part}.svg"
              svg_path.write_text("".join(parts))
              subprocess.run(["rsvg-convert", "-z", str(scale), "-o", str(svg_path.with_suffix(".png")), str(svg_path)], check=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--sheet", type=Path)
    parser.add_argument("--tones", type=Path, help="with --sheet: the JSON BINGUX_COLOUR_TONES=FILE writes")
    args = parser.parse_args(argv)
    try:
        if args.check:
            print(check())
        elif args.sheet:
            print(write())
            sheet(args.sheet, json.loads(args.tones.read_text()) if args.tones else None)
        else:
            print(write())
    except ColourError as exc:
        print(f"colour icons: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
