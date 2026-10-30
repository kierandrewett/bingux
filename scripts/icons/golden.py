#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["svgelements>=1.9,<2", "numpy>=1.26", "pillow>=10"]
# ///
"""The icon set against its golden: every icon at every size Bingux draws, compared with what was approved.

CK-01 of design/rules.md (IC-01, IC-03, IC-05, IC-06, AL-15). `build.py --check` holds the
rules an icon must meet on its own (size, centre, crisp edges, symmetry, fill or outline). It
cannot say that a change to the pipeline made every icon a little rounder, as happened once
("why do all the icons look like this now.. they are all super rounded"), because each icon
still met every rule. This holds the icons to what they looked like when they were approved.

The whole compiled set (packages/bingux-ui/icons/*-symbolic.svg, and the colour icons in
icons/colour) is drawn with the exact-area rasteriser of raster.py (no Skia, no cargo, the same
result on every machine) at 16, 18, 20, 24 and 32 px, each at 1x and at 2x, and measured:

  ink       the box of the ink in pixels, read from coverage (a pixel 40% inked puts the edge 0.6 px in)
  coverage  the share of the box that is ink: it falls when the strokes get thinner and rises when a
            corner is rounded off less, so a pipeline that rounds or thickens everything moves it
  soft      partly covered pixels on straight edges: how crisp the icon is at that size
  stroke    twice the area over the outline's length, in grid units: the weight of the line
  dots      each dot of the icon (a part 4.5 grid units across or less): its width, height and
            how much of its box it fills (a circle fills 0.785, a rounded square 0.93)
  hash      of the whole raster, so any change at all is seen

plus, per icon, a 16 px raster of its own (`pixels`, a hex digit a pixel) to say *where* a change is.
`golden.json` is that record, one line an icon so a change reads in a diff.

    scripts/icons/golden.py                 # compare with golden.json (exit 1 on a difference)
    scripts/icons/golden.py --update        # approve the set as it is now: rewrite golden.json
    scripts/icons/golden.py --sheet out.png [--size 16 --scale 2]   # the whole set on one picture
    scripts/icons/golden.py --fixtures      # the checks on known-bad and clean drawings only
    scripts/icons/golden.py --explain NAME  # every number of one icon beside its golden

A difference is a finding when an ink edge moves by more than 0.15 px, coverage by more than 2%,
the stroke weight by more than 0.06 grid units, a dot changes shape, or the raster at 16 px
differs by more than 2.5% of its ink (or one pixel by 5 levels of 15); an icon that is new or
gone is one too, so `golden.json` is part of any change to an icon. A change that only alters
pixels inside those tolerances is counted and listed with `--verbose`, and does not fail.

The absolute rules, held whatever the golden says: every dot of a drawn icon is round (a Wi-Fi dot
is a circle, not a square: IC-05), a Wi-Fi icon has at most two arcs above its dot (IC-03), an
icon listed `#symmetric` in labels.tsv is its own mirror image to within the pixel (AL-15),
and no icon is empty or leaves its box.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

import numpy as np
from svgelements import SVG, Arc, Close, Line, Move, Path as SvgPath, Shape

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import raster  # noqa: E402
from labels import load as load_labels  # noqa: E402

REPO = HERE.parents[1]
ICONS = REPO / "packages/bingux-ui/icons"
COLOUR = ICONS / "colour"
GOLDEN = HERE / "golden.json"

SIZES = (16, 18, 20, 24, 32)
SCALES = (1, 2)
FINE = 64  # the raster the ink box, the dots and the arcs are read from: a quarter of a grid unit at 16
EDGE_PX = 0.15
COVERAGE_REL = 0.02
STROKE_GRID = 0.06
DOT_GRID = 0.12
DOT_FILL = 0.05
PIXELS_MEAN = 0.025
PIXELS_MAX = 5 / 15
DOT_MAX_GRID = 4.5
ROUND_FILL = (0.70, 0.88)  # a circle's share of its box is 0.785
ROUND_ASPECT = 0.12


# ---- drawing -----------------------------------------------------------------------


def flatten(path: SvgPath) -> list[list[tuple[float, float]]]:
    polys: list[list[tuple[float, float]]] = []
    cur: list[tuple[float, float]] = []
    for seg in path.segments(transformed=True):
        if isinstance(seg, Move):
            if len(cur) > 2:
                polys.append(cur)
            cur = [(float(seg.end.x), float(seg.end.y))]
        elif isinstance(seg, Close):
            if len(cur) > 2:
                polys.append(cur)
            cur = []
        elif isinstance(seg, Line):
            cur.append((float(seg.end.x), float(seg.end.y)))
        else:
            steps = 32 if isinstance(seg, Arc) else 16
            pts = seg.npoint(np.linspace(0, 1, steps + 1))[1:]
            cur.extend((float(x), float(y)) for x, y in pts)
    if len(cur) > 2:
        polys.append(cur)
    return polys


class Drawing:
    """One icon as shapes: polygons, a fill rule and an opacity each, on a `grid` unit box."""

    def __init__(self, file: Path):
        svg = SVG.parse(str(file))
        self.grid = float(svg.viewbox.width) if svg.viewbox else 16.0
        self.shapes: list[tuple[list[list[tuple[float, float]]], str, float]] = []
        for element in svg.elements():
            if not isinstance(element, Shape) or isinstance(element, SVG):
                continue
            fill = element.fill
            if fill is None or fill.value is None:
                continue
            path = element if isinstance(element, SvgPath) else SvgPath(element)
            polys = flatten(path)
            if not polys:
                continue
            rule = str(element.values.get("fill-rule", element.values.get("fill_rule", "nonzero")))
            alpha = float(element.values.get("fill-opacity", element.values.get("fill_opacity", 1.0)) or 1.0)
            self.shapes.append((polys, rule, alpha * (fill.opacity if fill.opacity is not None else 1.0)))
        self.polys = [poly for polys, _, _ in self.shapes for poly in polys]

    def coverage(self, size: int) -> np.ndarray:
        """Coverage in 0..1 at `size` px, the shapes laid over each other."""
        out = np.zeros((size, size))
        for polys, rule, alpha in self.shapes:
            if "evenodd" in rule:
                total = sum(raster.coverage([poly], size, self.grid) for poly in polys)
                cov = 1.0 - np.abs(1.0 - np.mod(total, 2.0))
            else:
                cov = raster.coverage(polys, size, self.grid)
            out = 1.0 - (1.0 - out) * (1.0 - cov * alpha)
        return out

    def perimeter(self) -> float:
        total = 0.0
        for poly in self.polys:
            for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
                total += math.hypot(x1 - x0, y1 - y0)
        return total


def components(mask: np.ndarray) -> list[tuple[int, int, int, int, int]]:
    """(left, top, right, bottom, area) of each 4-connected run of True, in pixels; right and bottom are exclusive."""
    seen = np.zeros(mask.shape, dtype=bool)
    found = []
    height, width = mask.shape
    for y in range(height):
        for x in range(width):
            if not mask[y, x] or seen[y, x]:
                continue
            stack = [(y, x)]
            seen[y, x] = True
            x0 = x1 = x
            y0 = y1 = y
            area = 0
            while stack:
                cy, cx = stack.pop()
                area += 1
                x0, x1, y0, y1 = min(x0, cx), max(x1, cx), min(y0, cy), max(y1, cy)
                for ny, nx in ((cy - 1, cx), (cy + 1, cx), (cy, cx - 1), (cy, cx + 1)):
                    if 0 <= ny < height and 0 <= nx < width and mask[ny, nx] and not seen[ny, nx]:
                        seen[ny, nx] = True
                        stack.append((ny, nx))
            found.append((x0, y0, x1 + 1, y1 + 1, area))
    return found


# ---- measuring -----------------------------------------------------------------------


def measure(drawing: Drawing, file: Path) -> dict:
    """Every number of one icon: the golden record and what the rules read."""
    record: dict = {"grid": drawing.grid}
    fine = drawing.coverage(FINE)
    k = drawing.grid / FINE
    mask = fine > 0.5
    dots = []
    for left, top, right, bottom, area in components(mask):
        w, h = (right - left) * k, (bottom - top) * k
        if max(w, h) <= DOT_MAX_GRID and area >= 6:
            dots.append([round(w, 2), round(h, 2), round(area * k * k / (w * h), 2) if w * h else 0.0])
    record["dots"] = sorted(dots)
    record["components"] = len(components(mask))
    area = float(drawing.coverage(FINE).sum()) * k * k
    perimeter = drawing.perimeter()
    record["stroke"] = round(2 * area / perimeter, 3) if perimeter else 0.0
    # The number of separate inked runs along the vertical through the middle: a Wi-Fi icon is its arcs and a dot.
    column = mask[:, FINE // 2 - 1 : FINE // 2 + 1].any(axis=1)
    record["runs"] = int(np.count_nonzero(column[1:] & ~column[:-1]) + int(column[0]))
    asym = raster.asymmetry(drawing.coverage(drawing.grid and int(drawing.grid)))
    record["asym"] = [round(asym[0], 3), round(asym[1], 4)]
    native = int(drawing.grid)
    renditions = {}
    for scale in SCALES:
        sizes = SIZES if drawing.grid == 16 else (native,)
        for size in sizes:
            px = size * scale
            cov = drawing.coverage(px)
            ink = raster.ink_extents(cov, grid=float(px))
            gray = np.round(cov * 255).astype(np.uint8)
            soft = raster.soft_edge_pixels(cov)
            renditions[f"{size}@{scale}"] = [
                round(ink[0], 2),
                round(ink[1], 2),
                round(ink[2], 2),
                round(ink[3], 2),
                round(float(cov.sum()) / (px * px), 4),
                soft,
                hashlib.sha1(gray.tobytes()).hexdigest()[:10],
            ]
    record["r"] = renditions
    cov16 = drawing.coverage(native)
    record["pixels"] = "".join("0123456789abcdef"[int(round(v * 15))] for v in cov16.reshape(-1))
    return record


def icon_files() -> dict[str, Path]:
    files = {path.name: path for path in sorted(ICONS.glob("*-symbolic.svg"))}
    for path in sorted(COLOUR.glob("*.svg")):
        # The 128 grid of a colour icon is a vector drawn at any scale: nothing of the pixel checks applies.
        if not path.name.endswith("-128.svg"):
            files[f"colour/{path.name}"] = path
    return files


def measure_all(only: set[str] | None = None) -> dict[str, dict]:
    out = {}
    for name, path in icon_files().items():
        if only and name not in only and name.removesuffix("-symbolic.svg") not in only:
            continue
        out[name] = measure(Drawing(path), path)
    return out


# ---- the absolute rules -----------------------------------------------------------------


def absolute_problems(measures: dict[str, dict], labels) -> list[str]:
    problems = []
    for name, record in measures.items():
        short = name.removesuffix("-symbolic.svg")
        if record["components"] == 0 or not record["r"]:
            problems.append(f"[IC-01] {name}: empty drawing")
            continue
        ink = record["r"][f"{int(record['grid'])}@1"][:4]
        if ink[0] < -0.05 or ink[1] < -0.05 or ink[2] > record["grid"] + 0.05 or ink[3] > record["grid"] + 0.05:
            problems.append(f"[IC-01] {name}: the ink leaves its box ({ink})")
        row = labels.rows.get(short)
        glyph = row.glyph if row else ""
        if name.endswith("-symbolic.svg") and (labels.policy.get(short) == "drawn" or " dot" in glyph):
            for w, h, fill in record["dots"]:
                aspect = abs(w / h - 1.0) if h else 1.0
                if not ROUND_FILL[0] <= fill <= ROUND_FILL[1] or aspect > ROUND_ASPECT:
                    problems.append(
                        f"[IC-05] {name}: a dot {w:.2f} x {h:.2f} grid units fills {fill:.2f} of its box; "
                        f"a dot is a circle ({ROUND_FILL[0]} to {ROUND_FILL[1]}, near square)"
                    )
        if short.startswith("network-wireless") and short in ("network-wireless",) or short.startswith("network-wireless-signal-"):
            if record["runs"] > 3:
                problems.append(f"[IC-03] {name}: {record['runs']} marks along its middle; a Wi-Fi icon has two arcs and a dot at most")
        if short in labels.symmetric and (record["asym"][0] > 0.15 or record["asym"][1] > 0.03):
            problems.append(f"[AL-15] {name}: not its own mirror image (worst pixel {record['asym'][0]:.2f}, mean {record['asym'][1]:.3f})")
    return problems


# ---- the golden ------------------------------------------------------------------------------


def compare(now: dict, then: dict, name: str) -> tuple[list[str], int]:
    """(findings, count of renditions whose pixels differ inside the tolerances)."""
    out: list[str] = []
    changed = 0
    if abs(now["stroke"] - then["stroke"]) > STROKE_GRID:
        out.append(f"[IC-05] {name}: stroke weight {then['stroke']:.2f} -> {now['stroke']:.2f} grid units")
    if [d[:2] for d in now["dots"]] != [d[:2] for d in then["dots"]] and (
        len(now["dots"]) != len(then["dots"])
        or any(abs(a[i] - b[i]) > (DOT_GRID if i < 2 else DOT_FILL) for a, b in zip(now["dots"], then["dots"]) for i in range(3))
    ):
        out.append(f"[IC-05] {name}: dots {then['dots']} -> {now['dots']} (width, height, fill of its box)")
    elif any(abs(a[2] - b[2]) > DOT_FILL for a, b in zip(now["dots"], then["dots"])):
        out.append(f"[IC-05] {name}: a dot is rounder or squarer: {then['dots']} -> {now['dots']}")
    for key, rendition in sorted(now["r"].items()):
        before = then["r"].get(key)
        if before is None:
            out.append(f"[IC-05] {name}: no golden at {key}")
            continue
        scale = int(key.split("@")[1])
        names = ("left", "top", "right", "bottom")
        for i, edge in enumerate(names):
            if abs(rendition[i] - before[i]) > EDGE_PX * scale:
                out.append(f"[IC-01] {name} at {key}: the ink's {edge} edge moved {before[i]:.2f} -> {rendition[i]:.2f} px")
        if abs(rendition[4] - before[4]) > COVERAGE_REL * max(before[4], 1e-9):
            out.append(f"[IC-05] {name} at {key}: coverage {before[4]:.3f} -> {rendition[4]:.3f} ({(rendition[4] / max(before[4], 1e-9) - 1) * 100:+.1f}%)")
        if abs(rendition[5] - before[5]) > 4 + 0.2 * before[5]:
            out.append(f"[IC-01] {name} at {key}: {before[5]} -> {rendition[5]} soft edge pixels")
        if rendition[6] != before[6]:
            changed += 1
    a, b = now["pixels"], then["pixels"]
    levels = [abs(int(x, 16) - int(y, 16)) for x, y in zip(a, b)]
    inked = max(1, sum(1 for x, y in zip(a, b) if x != "0" or y != "0"))
    if levels and (sum(levels) / 15 / inked > PIXELS_MEAN or max(levels) / 15 > PIXELS_MAX):
        spots = [(i % int(math.sqrt(len(a))), i // int(math.sqrt(len(a)))) for i, v in enumerate(levels) if v / 15 > PIXELS_MAX][:3]
        out.append(
            f"[IC-05] {name}: the 16 px picture differs: {sum(levels) / 15 / inked:.1%} of its ink, worst pixel {max(levels)}/15"
            + (f" at {spots}" if spots else "")
        )
    return out, changed


def load_golden() -> dict:
    return json.loads(GOLDEN.read_text()) if GOLDEN.exists() else {"icons": {}}


def write_golden(measures: dict[str, dict]) -> None:
    lines = ["{", '"version": 1,', f'"sizes": {json.dumps(list(SIZES))},', f'"scales": {json.dumps(list(SCALES))},', '"icons": {']
    names = sorted(measures)
    for i, name in enumerate(names):
        lines.append(f"{json.dumps(name)}: {json.dumps(measures[name], separators=(',', ':'), sort_keys=True)}" + ("," if i + 1 < len(names) else ""))
    lines += ["}", "}"]
    GOLDEN.write_text("\n".join(lines) + "\n")


# ---- the sheet ----------------------------------------------------------------------------------


def sheet(path: Path, size: int, scale: int) -> None:
    from PIL import Image, ImageDraw

    px = size * scale
    cell = px + 12 * scale
    names = [name for name in icon_files() if not name.startswith("colour/")]
    columns = 24
    rows = math.ceil(len(names) / columns)
    image = Image.new("RGB", (columns * cell, rows * cell), (30, 30, 34))
    draw = ImageDraw.Draw(image)
    for i, name in enumerate(names):
        cov = Drawing(ICONS / name).coverage(px)
        tile = (cov[..., None] * np.array([235, 235, 240]) + (1 - cov[..., None]) * np.array([30, 30, 34])).astype(np.uint8)
        x, y = (i % columns) * cell + 6 * scale, (i // columns) * cell + 6 * scale
        image.paste(Image.fromarray(tile), (x, y))
    draw.text((4, 2), f"{len(names)} icons at {size} px x{scale}", fill=(150, 150, 160))
    image.save(path)


# ---- fixtures -----------------------------------------------------------------------------------


def svg_text(body: str) -> str:
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16px" height="16px">{body}</svg>'


def drawn(tmp: Path, name: str, body: str) -> dict:
    file = tmp / name
    file.write_text(svg_text(body))
    return measure(Drawing(file), file)


def run_fixtures() -> list[str]:
    import tempfile

    problems: list[str] = []
    with tempfile.TemporaryDirectory(prefix="icon-golden-") as tmpdir:
        tmp = Path(tmpdir)
        circle = "M10 8A2 2 0 1 1 6 8A2 2 0 1 1 10 8Z"
        square = "M6 6H10V10H6Z"
        round_dot = drawn(tmp, "a.svg", f'<path d="{circle}"/>')
        square_dot = drawn(tmp, "b.svg", f'<path d="{square}"/>')
        thick = drawn(tmp, "c.svg", '<path d="M3 3H13V6H3Z"/>')
        thin = drawn(tmp, "d.svg", '<path d="M3 3H13V4H3Z"/>')
        left_wing = drawn(tmp, "e.svg", '<path d="M2 4H8V12H2Z"/>')
        # The round dot is a circle and the square one is not.
        if not ROUND_FILL[0] <= round_dot["dots"][0][2] <= ROUND_FILL[1]:
            problems.append(f"fixture: a circle should fill about 0.785 of its box, got {round_dot['dots']}")
        if ROUND_FILL[0] <= square_dot["dots"][0][2] <= ROUND_FILL[1]:
            problems.append(f"fixture: a square should not read as round, got {square_dot['dots']}")
        # The same icon is the same.
        findings, changed = compare(round_dot, round_dot, "same")
        if findings or changed:
            problems.append(f"fixture: an icon against itself found {findings} {changed}")
        # Thicker strokes: found.
        findings, _ = compare(thick, thin, "weight")
        if not any("stroke weight" in f for f in findings):
            problems.append(f"fixture: a stroke three times as heavy was not found ({findings})")
        # A circle for a square: found.
        findings, _ = compare(round_dot, square_dot, "round")
        if not findings:
            problems.append("fixture: a circle in the place of a square was not found")
        # An icon that is not its own mirror image is found by the mirror measure.
        if left_wing["asym"][0] < 0.5:
            problems.append(f"fixture: a one-sided shape should be far from its mirror, got {left_wing['asym']}")
        if round_dot["asym"][0] > 0.05:
            problems.append(f"fixture: a centred circle should be its own mirror image, got {round_dot['asym']}")
    return problems


# ---- main ------------------------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--update", action="store_true", help="approve the set as it is: rewrite golden.json")
    parser.add_argument("--sheet", type=Path, help="write the whole set as one picture")
    parser.add_argument("--size", type=int, default=16)
    parser.add_argument("--scale", type=int, default=2)
    parser.add_argument("--fixtures", action="store_true", help="only check the checks on their own drawings")
    parser.add_argument("--explain", help="every number of one icon beside its golden")
    parser.add_argument("--verbose", action="store_true", help="also list icons whose pixels changed within the tolerances")
    args = parser.parse_args()

    problems = run_fixtures()
    for problem in problems:
        print(problem)
    print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
    if problems:
        return 1
    if args.fixtures:
        return 0
    if args.sheet:
        sheet(args.sheet, args.size, args.scale)
        print(f"wrote {args.sheet}")
        return 0
    only = {args.explain} if args.explain else None
    measures = measure_all(only)
    if args.explain:
        golden = load_golden()["icons"]
        for name, record in measures.items():
            print(name)
            for key in ("grid", "stroke", "dots", "components", "runs", "asym"):
                print(f"  {key:11} now {record[key]}   golden {golden.get(name, {}).get(key)}")
            for key, value in sorted(record["r"].items()):
                print(f"  {key:6} now {value}   golden {golden.get(name, {}).get('r', {}).get(key)}")
        return 0
    labels = load_labels()
    failures = absolute_problems(measures, labels)
    if args.update:
        if failures:
            print("not updating: the set breaks an absolute rule\n  " + "\n  ".join(failures))
            return 1
        write_golden(measures)
        print(f"golden.json: {len(measures)} icons approved")
        return 0
    golden = load_golden()["icons"]
    drift = 0
    for name in sorted(set(measures) | set(golden)):
        if name not in golden:
            failures.append(f"[IC-05] {name}: new icon, not in golden.json: look at it, then `scripts/icons/golden.py --update`")
            continue
        if name not in measures:
            failures.append(f"[IC-05] {name}: in golden.json but not in the set any more: `scripts/icons/golden.py --update`")
            continue
        found, changed = compare(measures[name], golden[name], name)
        failures += found
        drift += bool(changed and not found)
        if changed and not found and args.verbose:
            print(f"  pixels differ inside the tolerances: {name} ({changed} renditions)")
    for failure in failures:
        print(failure)
    renditions = sum(len(record["r"]) for record in measures.values())
    print(
        f"icons: {len(measures)} icons, {renditions} renditions at {', '.join(map(str, SIZES))} px x{', x'.join(map(str, SCALES))}: "
        f"{len(failures)} findings, {drift} icons with pixels that differ inside the tolerances"
    )
    if failures:
        print("If the change is meant: look at the sheet (`golden.py --sheet /tmp/icons.png`), then `scripts/icons/golden.py --update`.")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
