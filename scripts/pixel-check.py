#!/usr/bin/env python3
"""Fail when a capture's pixels break a rule a token test cannot see.

Two checks read the rendered PNGs, not the design tokens, so a hand-built box or
a surface that is not the one the token table assumes is caught as it looks:

  CK-20  text-contrast   CL-04 (CL-03, BR-07): the ink of every text node against the
         notice-edge     colour actually painted behind it. Text is judged at 4.5:1,
         floating-dark   large text (24 px, or 18.66 px and bold) at 3:1. A coloured
                         notice's edge may be at most 1.3:1 against its fill, and a
                         toast is darker than the window it floats over.
  CK-07  popup-perimeter BR-03 (SP-06): every popup, dialog, menu, sheet and
                         dropdown card has a visible border or shadow on all four sides
                         and its corners, header strip included, in both schemes and at
                         1x and 2x; the header bar is inside the node that carries the
                         border.

The apps render through `bingux_ui::snapshot` without a compositor. With
BINGUX_UI_PROBE=FILE set every capture also writes where its text nodes, modal
scopes and clipping boxes are (crates/ui/src/inspector/probe_checks) and
this script reads PNG and probe together. It runs every Settings page, subpage and
popup at desktop and 390x800, every taxonomy page (which holds a menu, dialog or
banner of every component) and the Files window and its dialogs, in the dark and
the light scheme; the popups, the taxonomy and Files again at 2x.

    python3 scripts/pixel-check.py                       # both checks, everything
    python3 scripts/pixel-check.py --check perimeter --only taxonomy
    python3 scripts/pixel-check.py --variants dark,light,darkx2,lightx2
    python3 scripts/pixel-check.py --fixtures            # the known-bad and clean fixtures only

How the numbers are taken

  contrast   The background is the colour that covers most of the text's box in the
             PNG (a colour that covers under a third of it is a busy ground, an image
             or a gradient, and is counted, not judged). The ink is the text colour
             the layout resolved, at the node's alpha and its ancestors' opacity,
             composited over that background; the darkest or lightest pixel of the box
             is the ink of a text that declares no colour. A box with nothing drawn in it
             (its brightest and darkest pixels within 1.15:1 of the ground) is covered,
             still fading in or in a frame with its own origin, and is counted as
             `unlocated`, not judged. Text with opacity below 0.99 is
             dimmed on purpose (disabled, or fading in) and is counted, not judged,
             as WCAG exempts inactive components (contrast_matrix.rs does the same).
             Text outside the clip of an ancestor is not on screen and is skipped, as
             is text behind a modal scope (the page under a scrim).
  perimeter  Each side of the card is cut into eight runs, without the corners. On a
             run, three rows of pixels are compared: the ring (the card's own edge
             pixels, where its hairline is), the inside (a few pixels in) and the outside
             (a few pixels out, where its shadow falls). The side is visible where one of
             the three pairs is at least 1.10:1 in WCAG luminance ratio: a hairline
             against the card, a hairline against what is behind, or the card against what
             is behind. Each corner is measured on its arc the same way. A side that
             touches the window edge is not measured (a sheet's bottom).

Findings name the rule IDs of design/rules.json. Exit status is 1 on any finding
that ALLOWED does not cover, an unused ALLOWED entry, a capture run that produced
nothing, and a fixture that is not judged as it should be. Fix the component or
token; an entry in ALLOWED needs its reason.
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import probe_lib as lib  # noqa: E402
from probe_lib import Allow, Finding, Probe  # noqa: E402

# ---- the numbers ----------------------------------------------------------------

NORMAL_TEXT = 4.5
LARGE_TEXT = 3.0
LARGE_PX = 24.0
LARGE_BOLD_PX = 18.66
BOLD = 700
# A text box whose brightest or darkest pixel is within this ratio of its ground has nothing drawn in it.
UNLOCATED = 1.15
# A colour covering less of the text box than this is a busy ground.
GROUND_SHARE = 0.34
NOTICE_EDGE_MAX = 1.3
# A side, a run of it or a corner is visible from this ratio.
EDGE_MIN = 1.10
# A rounded corner's arc is antialiased and one pixel wide: a hairline there reads weaker than on a straight run.
CORNER_MIN = 1.05
RUNS = 8
MIN_TEXT_PX = 4

# Findings that are right by design: rule, capture regex, target regex, reason.
ALLOWED: list[dict] = []


# ---- contrast -------------------------------------------------------------------


def visible_rect(probe: Probe, index: int) -> tuple[float, float, float, float] | None:
    """The part of a node's box that is on screen: its own box clipped by every
    clipping ancestor and the window. `None` when nothing is left."""
    x, y, w, h = probe.box(index)
    left, top, right, bottom = x, y, x + w, y + h
    for up in probe.ancestors(index):
        if probe.flags(up) & lib.CLIP:
            ax, ay, aw, ah = probe.box(up)
            left, top = max(left, ax), max(top, ay)
            right, bottom = min(right, ax + aw), min(bottom, ay + ah)
    left, top = max(left, 0.0), max(top, 0.0)
    right, bottom = min(right, probe.window[0]), min(bottom, probe.window[1])
    if right - left < 1 or bottom - top < 1:
        return None
    return left, top, right - left, bottom - top


def ground_of(crop: np.ndarray) -> tuple[np.ndarray, float]:
    """The colour that covers most of `crop`, and the share of it that colour covers."""
    packed = (crop[..., 0].astype(np.int64) << 16) | (crop[..., 1].astype(np.int64) << 8) | crop[..., 2].astype(np.int64)
    values, counts = np.unique(packed, return_counts=True)
    top = int(values[np.argmax(counts)])
    return np.array([(top >> 16) & 255, (top >> 8) & 255, top & 255], dtype=np.float64), float(counts.max()) / packed.size


def occluders(probe: Probe) -> list[tuple[int, tuple[float, float, float, float]]]:
    """Boxes that are painted over the page: each modal card, and every painted node an overlay is (out of the flow)."""
    found = [(modal["node"], probe.box(modal["node"])) for modal in probe.modals]
    for index in range(len(probe.nodes)):
        flags = probe.flags(index)
        x, y, w, h = probe.box(index)
        if flags & lib.OUT_OF_FLOW and flags & lib.PAINTS and flags & lib.VISIBLE and w >= 40 and h >= 20:
            found.append((index, (x, y, w, h)))
    return found


def text_findings(probe: Probe, pixels: np.ndarray, stats: dict[str, int]) -> list[Finding]:
    found: list[Finding] = []
    scale = probe.scale
    height, width = pixels.shape[:2]
    over = occluders(probe)
    for row in probe.texts:
        index, colour, size, weight, opacity, _lines = row
        flags = probe.flags(index)
        if not flags & lib.VISIBLE or flags & lib.INERT:
            continue
        rect = visible_rect(probe, index)
        if rect is None:
            continue
        x, y, w, h = rect
        if over:
            mine = {index, *probe.ancestors(index)}
            cx, cy = x + w / 2, y + h / 2
            if any(o > index and o not in mine and ox <= cx <= ox + ow and oy <= cy <= oy + oh for o, (ox, oy, ow, oh) in over):
                stats["covered"] = stats.get("covered", 0) + 1
                continue
        x0, y0 = max(0, math.floor(x * scale)), max(0, math.floor(y * scale))
        x1, y1 = min(width, math.ceil((x + w) * scale)), min(height, math.ceil((y + h) * scale))
        if x1 - x0 < MIN_TEXT_PX or y1 - y0 < MIN_TEXT_PX:
            continue
        text = probe.text(index).strip()
        if not text:
            continue
        stats["text"] += 1
        if opacity < 0.99:
            stats["dimmed"] += 1
            continue
        crop = pixels[y0:y1, x0:x1]
        ground, share = ground_of(crop)
        if share < GROUND_SHARE:
            stats["busy"] += 1
            continue
        ground_lum = float(lib.luminance(ground))
        lum = lib.luminance(crop.reshape(-1, 3))
        # The pixel furthest from the ground, as the eye gets it.
        far = float(lum.max()) if abs(float(lum.max()) - ground_lum) >= abs(float(lum.min()) - ground_lum) else float(lum.min())
        seen = lib.contrast(far, ground_lum)
        if seen < UNLOCATED:
            # Nothing is drawn where the tree puts this text: it is covered, still fading in, or in a
            # frame that has its own origin (a phone preview inside a page). Not judged.
            stats["unlocated"] = stats.get("unlocated", 0) + 1
            continue
        if colour[3] == 0:
            # The element declares no colour (it inherits one the headless capture does not resolve): the
            # ink is the pixel furthest from the ground. Antialiasing keeps that a little short of the true
            # colour, so a text that passes this way passes by more.
            ratio = seen
            ink = crop.reshape(-1, 3)[int(np.argmax(np.abs(lum - ground_lum)))]
        else:
            ink = lib.over(colour[:3], colour[3] / 255.0, ground)
            ratio = lib.contrast(lib.luminance(ink), ground_lum)
        large = size >= LARGE_PX or (size >= LARGE_BOLD_PX and weight >= BOLD)
        need = LARGE_TEXT if large else NORMAL_TEXT
        stats["judged"] += 1
        shown = text if len(text) <= 40 else text[:37] + "..."
        target = f"{probe.title(probe.parent(index)) if probe.parent(index) >= 0 else ''} \"{shown}\" ({size:g} px)"
        if ratio < need:
            found.append(
                Finding(
                    "text-contrast",
                    ["CL-04"],
                    probe.name,
                    target,
                    f"ink #{int(ink[0]):02x}{int(ink[1]):02x}{int(ink[2]):02x} on #{int(ground[0]):02x}{int(ground[1]):02x}{int(ground[2]):02x} is {ratio:.2f}:1, needs {need:g}:1",
                    {"ratio": round(ratio, 2), "need": need, "size": size},
                )
            )
            continue
    return found


def notice_findings(probe: Probe, pixels: np.ndarray, stats: dict[str, int]) -> list[Finding]:
    """BR-07 and CL-03: a notice's edge is soft, and a toast is darker than the window."""
    found: list[Finding] = []
    scale = probe.scale
    height, width = pixels.shape[:2]
    window_ground = None
    for index in range(len(probe.nodes)):
        title = probe.title(index)
        if title not in ("Banner", "Toast") or not probe.flags(index) & lib.VISIBLE or probe.flags(index) & lib.INERT:
            continue
        x, y, w, h = probe.box(index)
        x0, y0, x1, y1 = (round(v * scale) for v in (x, y, x + w, y + h))
        if x0 < 0 or y0 < 0 or x1 > width or y1 > height or x1 - x0 < 8 or y1 - y0 < 8:
            continue
        stats["notices"] += 1
        inside = pixels[y0 + 4 * round(scale) : y1 - 4 * round(scale), x0 + 4 * round(scale) : x1 - 4 * round(scale)]
        if inside.size == 0:
            continue
        fill, _ = ground_of(inside)
        # The pill is rounded at its ends: read its top and bottom edge rows along the straight run between them.
        thick = max(1, round(scale))
        inset = max(4, (y1 - y0) // 2 + 2)
        if x1 - x0 <= 2 * inset + 8:
            continue
        ring = np.median(
            np.concatenate([pixels[y0 : y0 + thick, x0 + inset : x1 - inset], pixels[y1 - thick : y1, x0 + inset : x1 - inset]]).reshape(-1, 3),
            axis=0,
        )
        edge = lib.contrast(lib.luminance(ring), lib.luminance(fill))
        if edge > NOTICE_EDGE_MAX:
            found.append(
                Finding(
                    "notice-edge",
                    ["BR-07"],
                    probe.name,
                    f"{title} at {x:.0f},{y:.0f}",
                    f"the edge is {edge:.2f}:1 against the fill; a coloured notice has a soft edge, at most {NOTICE_EDGE_MAX:g}:1",
                    {"edge": round(edge, 2)},
                )
            )
        if title == "Toast":
            # The window behind it: the colour that covers most of a strip above the toast, or below it.
            strip = pixels[max(0, y0 - 16 * round(scale)) : max(1, y0 - 2 * round(scale)), x0:x1]
            if strip.size == 0:
                strip = pixels[min(height - 1, y1 + 2 * round(scale)) : min(height, y1 + 16 * round(scale)), x0:x1]
            if strip.size:
                window_ground, _ = ground_of(strip)
                if lib.luminance(fill) >= lib.luminance(window_ground):
                    found.append(
                        Finding(
                            "floating-dark",
                            ["CL-03"],
                            probe.name,
                            f"Toast at {x:.0f},{y:.0f}",
                            "a toast uses the dark floating token: it must be darker than the window it floats over",
                            {"toast": round(float(lib.luminance(fill)), 3), "window": round(float(lib.luminance(window_ground)), 3)},
                        )
                    )
    return found


# ---- perimeter --------------------------------------------------------------------


def lum_of(block: np.ndarray) -> float:
    """Luminance of the median colour of a block of pixels."""
    return float(lib.luminance(np.median(block.reshape(-1, 3), axis=0)))


def side_runs(pixels: np.ndarray, rect: tuple[int, int, int, int], side: str, radius: int, thick: int) -> list[tuple[str, float]]:
    """(label, separation) for each run of one side of the card, corners left out."""
    x0, y0, x1, y1 = rect
    height, width = pixels.shape[:2]
    inner_at, outer_at = 3 * thick, 3 * thick
    if side in ("top", "bottom"):
        span = (x0 + radius, x1 - radius)
    else:
        span = (y0 + radius, y1 - radius)
    length = span[1] - span[0]
    if length < RUNS * 2:
        return []
    cuts = [span[0] + round(i * length / RUNS) for i in range(RUNS + 1)]
    found = []
    for i in range(RUNS):
        a, b = cuts[i], cuts[i + 1]
        if side == "top":
            ring = pixels[y0 : y0 + thick, a:b]
            inner = pixels[y0 + inner_at : y0 + inner_at + thick, a:b]
            outer = pixels[max(0, y0 - outer_at - thick + 1) : max(1, y0 - outer_at + 1), a:b]
        elif side == "bottom":
            ring = pixels[y1 - thick : y1, a:b]
            inner = pixels[y1 - inner_at - thick : y1 - inner_at, a:b]
            outer = pixels[min(height - 1, y1 + outer_at) : min(height, y1 + outer_at + thick), a:b]
        elif side == "left":
            ring = pixels[a:b, x0 : x0 + thick]
            inner = pixels[a:b, x0 + inner_at : x0 + inner_at + thick]
            outer = pixels[a:b, max(0, x0 - outer_at - thick + 1) : max(1, x0 - outer_at + 1)]
        else:
            ring = pixels[a:b, x1 - thick : x1]
            inner = pixels[a:b, x1 - inner_at - thick : x1 - inner_at]
            outer = pixels[a:b, min(width - 1, x1 + outer_at) : min(width, x1 + outer_at + thick)]
        if min(ring.size, inner.size, outer.size) == 0:
            continue
        lr, li, lo = lum_of(ring), lum_of(inner), lum_of(outer)
        separation = max(lib.contrast(lr, li), lib.contrast(lr, lo), lib.contrast(li, lo))
        found.append((f"{side} run {i + 1}/{RUNS}", separation))
    return found


def corner_runs(pixels: np.ndarray, rect: tuple[int, int, int, int], radius: int, thick: int) -> list[tuple[str, float]]:
    """(label, separation) at each rounded corner's arc.

    The arc is sampled at three angles and three radii either side of the card's own radius, and the best
    separation counts: a hairline that is there shows at one of them, whereas a corner with no border shows
    nothing at any, and a pixel of rounding in where the arc falls does not read as a missing border.
    """
    x0, y0, x1, y1 = rect
    height, width = pixels.shape[:2]
    if radius < 6:
        return []
    found = []
    for name, cx, cy, sx, sy in (
        ("top-left", x0 + radius, y0 + radius, -1, -1),
        ("top-right", x1 - radius, y0 + radius, 1, -1),
        ("bottom-left", x0 + radius, y1 - radius, -1, 1),
        ("bottom-right", x1 - radius, y1 - radius, 1, 1),
    ):
        def at(distance: float, angle: float) -> np.ndarray | None:
            px = round(cx + sx * math.cos(angle) * distance)
            py = round(cy + sy * math.sin(angle) * distance)
            if not (1 <= px < width - 1 and 1 <= py < height - 1):
                return None
            return pixels[py - 1 : py + 2, px - 1 : px + 2]

        best = 0.0
        for angle in (math.radians(35), math.radians(45), math.radians(55)):
            for shift in (-thick, 0, thick):
                ring = at(radius - thick * 0.5 + shift, angle)
                inner = at(radius - 3 * thick - 1, angle)
                outer = at(radius + 3 * thick, angle)
                if ring is None or inner is None or outer is None:
                    continue
                lr, li, lo = lum_of(ring), lum_of(inner), lum_of(outer)
                best = max(best, lib.contrast(lr, li), lib.contrast(lr, lo), lib.contrast(li, lo))
        found.append((f"{name} corner", best))
    return found


def perimeter_findings(probe: Probe, pixels: np.ndarray, stats: dict[str, int]) -> list[Finding]:
    found: list[Finding] = []
    scale = probe.scale
    thick = max(1, round(scale))
    height, width = pixels.shape[:2]
    for modal in probe.modals:
        index = modal["node"]
        x, y, w, h = probe.box(index)
        rect = (round(x * scale), round(y * scale), round((x + w) * scale), round((y + h) * scale))
        x0, y0, x1, y1 = rect
        title = modal["title"]
        target = f"{title} {w:.0f}x{h:.0f} at {x:.0f},{y:.0f}"
        # A modal scope that fills the window (a full-window sheet, a page) has no perimeter to see.
        if w >= probe.window[0] - 2 and h >= probe.window[1] - 2:
            continue
        stats["cards"] += 1
        if modal["headers_outside"]:
            found.append(
                Finding(
                    "popup-perimeter-structure",
                    ["BR-03", "SP-06"],
                    probe.name,
                    target,
                    f"{modal['headers_outside']} header bar(s) on its top edge are not inside it: the border must be on the outermost container, header included",
                    {"headers_outside": modal["headers_outside"]},
                )
            )
        radius = round(max(modal["radii"]) * scale)
        sides = {
            "top": y0 > 1,
            "bottom": y1 < height - 1,
            "left": x0 > 1,
            "right": x1 < width - 1,
        }
        weak: list[tuple[str, float]] = []
        measured = 0
        for side, on_screen in sides.items():
            if not on_screen:
                continue
            for label, separation in side_runs(pixels, rect, side, radius + 2 * thick, thick):
                measured += 1
                if separation < EDGE_MIN:
                    weak.append((label, separation))
        corners_on = {name for name in ("top-left", "top-right", "bottom-left", "bottom-right")}
        if not sides["top"]:
            corners_on -= {"top-left", "top-right"}
        if not sides["bottom"]:
            corners_on -= {"bottom-left", "bottom-right"}
        if not sides["left"]:
            corners_on -= {"top-left", "bottom-left"}
        if not sides["right"]:
            corners_on -= {"top-right", "bottom-right"}
        for label, separation in corner_runs(pixels, rect, radius, thick):
            if label.split(" ")[0] in corners_on:
                measured += 1
                if separation < CORNER_MIN:
                    weak.append((label, separation))
        stats["runs"] += measured
        if weak:
            worst = min(separation for _, separation in weak)
            names = ", ".join(f"{label} {separation:.2f}" for label, separation in weak[:4]) + (" ..." if len(weak) > 4 else "")
            found.append(
                Finding(
                    "popup-perimeter",
                    ["BR-03"],
                    probe.name,
                    target,
                    f"no visible border or shadow ({EDGE_MIN:g}:1 needed) on {len(weak)} of {measured} runs: {names}",
                    {"worst": round(worst, 3), "weak": len(weak), "runs": measured},
                )
            )
    return found


# ---- fixtures -----------------------------------------------------------------------


def fixture_probe(name: str, size: tuple[int, int], texts: list, modals: list, nodes: list, scale: float = 1.0) -> Probe:
    return Probe(
        capture=name,
        scale=scale,
        window=(float(size[0]), float(size[1])),
        titles=["Window", "text", "Dialog", "HeaderBar", "Toast"],
        strings=["Sample text"],
        nodes=nodes,
        texts=texts,
        modals=modals,
        scrollers=[],
        job="fixture",
    )


def card_image(size=(200, 160), card=(40, 30, 160, 130), fill=(34, 34, 38), behind=(20, 20, 22), line=(51, 51, 55), skip_top=False):
    """A card on a dimmed page, with a hairline on all four sides (or on all but the top)."""
    image = np.zeros((size[1], size[0], 3), dtype=np.float64)
    image[:] = behind
    x0, y0, x1, y1 = card
    image[y0:y1, x0:x1] = fill
    for rows, cols in (
        (slice(y0, y1), slice(x0, x0 + 1)),
        (slice(y0, y1), slice(x1 - 1, x1)),
        (slice(y1 - 1, y1), slice(x0, x1)),
    ):
        image[rows, cols] = line
    if not skip_top:
        image[y0 : y0 + 1, x0:x1] = line
    return image


def run_fixtures() -> list[str]:
    """The checks on pictures made here: a known-bad one must be found, a clean one must pass."""
    problems: list[str] = []
    stats: dict[str, int] = {k: 0 for k in ("text", "dimmed", "busy", "judged", "notices", "cards", "runs")}

    def expect(label: str, findings: list[Finding], rule: str | None) -> None:
        got = sorted({f.rule for f in findings})
        if rule is None and got:
            problems.append(f"fixture {label}: clean but found {got}")
        if rule is not None and rule not in got:
            problems.append(f"fixture {label}: known bad but {rule!r} was not found (found {got})")

    window = (200, 160)
    card_nodes = [(-1, 0, 0, 0, 200, 160, 1, -1), (0, 2, 40, 30, 120, 100, 1 | lib.MODAL, -1)]
    modal = {"node": 1, "title": "Dialog", "radii": [0, 0, 0, 0], "borders": 1, "shadows": 0, "headers_inside": 1, "headers_outside": 0}
    clean = fixture_probe("clean-card", window, [], [modal], card_nodes)
    expect("card with a hairline on four sides", perimeter_findings(clean, card_image(), stats), None)
    bad = fixture_probe("no-top-border", window, [], [modal], card_nodes)
    expect("card whose top edge has no hairline", perimeter_findings(bad, card_image(fill=(34, 34, 38), behind=(36, 36, 40), line=(36, 36, 40), skip_top=False), stats), "popup-perimeter")
    bad_header = dict(modal, headers_inside=0, headers_outside=1)
    expect("header outside the bordered box", perimeter_findings(fixture_probe("header-outside", window, [], [bad_header], card_nodes), card_image(), stats), "popup-perimeter-structure")

    text_nodes = [(-1, 0, 0, 0, 200, 160, 1, -1), (0, 1, 10, 10, 90, 20, 1 | lib.TEXT, 0)]

    def text_image(ink, ground):
        image = np.zeros((160, 200, 3), dtype=np.float64)
        image[:] = ground
        image[16:24, 12:40] = ink  # the glyph strokes, solid, under a third of the box
        return image

    # Bg #222222, ink #cccccc: about 11:1. Bg #222222, ink #555555: about 1.9:1.
    good_texts = [[1, [204, 204, 204, 255], 14.0, 400, 1.0, 1]]
    weak_texts = [[1, [85, 85, 85, 255], 14.0, 400, 1.0, 1]]
    expect("light ink on dark", text_findings(fixture_probe("t-good", window, good_texts, [], text_nodes), text_image((204, 204, 204), (34, 34, 34)), stats), None)
    expect("grey ink on dark", text_findings(fixture_probe("t-weak", window, weak_texts, [], text_nodes), text_image((85, 85, 85), (34, 34, 34)), stats), "text-contrast")
    before = stats.get("unlocated", 0)
    expect(
        "text whose box has nothing drawn in it is not judged",
        text_findings(fixture_probe("t-unlocated", window, good_texts, [], text_nodes), text_image((36, 36, 36), (34, 34, 34)), stats),
        None,
    )
    if stats.get("unlocated", 0) != before + 1:
        problems.append("fixture: a text box with nothing drawn in it was not counted as unlocated")
    return problems


# ---- main ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    lib.add_sweep_args(parser, variants="dark,light,darkx2,lightx2")
    parser.add_argument("--check", default="contrast,perimeter", help="contrast, perimeter or both")
    parser.add_argument("--fixtures", action="store_true", help="only check the checks on their own fixtures")
    args = parser.parse_args()

    problems = run_fixtures()
    for problem in problems:
        print(problem)
    if args.fixtures or problems:
        print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
        return 1 if problems else 0
    print("fixtures: all judged as they should be")

    wanted = {name.strip() for name in args.check.split(",")}
    if args.build:
        lint.build()
    workdir = lib.workdir_of(args, "pixel-check-")
    variants = lib.parse_variants(args.variants)
    results = lib.run_sweep(args, workdir, variants)

    stats = {k: 0 for k in ("text", "dimmed", "busy", "judged", "notices", "cards", "runs")}
    findings: list[Finding] = []
    captures = 0
    for result in sorted(results, key=lambda r: r["name"]):
        for probe in result["probes"]:
            captures += 1
            if not Path(probe.capture).exists():
                continue
            pixels = probe.pixels()
            if "contrast" in wanted:
                findings += text_findings(probe, pixels, stats)
                findings += notice_findings(probe, pixels, stats)
            if "perimeter" in wanted:
                findings += perimeter_findings(probe, pixels, stats)
    print(
        f"{captures} captures: {stats['judged']} texts judged ({stats['dimmed']} dimmed, {stats['busy']} on a busy ground and {stats.get('covered', 0)} covered by an overlay not judged), "
        f"{stats['notices']} notices, {stats['cards']} cards over {stats['runs']} runs"
    )
    measured = (stats["judged"] if "contrast" in wanted else 0) + (stats["runs"] if "perimeter" in wanted else 0)
    return lib.finish("pixel-check", findings, Allow(ALLOWED), args, results, measured, workdir, "measurements")


lint = lib.lint

if __name__ == "__main__":
    sys.exit(main())
