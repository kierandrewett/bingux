#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["skia-pathops==0.9.2", "svgelements>=1.9,<2", "numpy>=1.26", "pillow"]
# ///
"""Review sheets for the Bingux icons: what each icon is, where it is used, and how it looks.

  review.py sheets [--out DIR] [--icons DIR]
      One PNG per group of groups.txt (`group-<name>.png`), each icon on a row with its Icon
      variant, file, glyph, policy category, where it is used and its metaphor, drawn at 16 px
      (dark and light), 18 px and 48 px; plus `icons.txt`, the same table as plain text.
  review.py compare BEFORE_DIR AFTER_DIR [--group NAME] [--out FILE]
      The icons of a group from two icon directories, before above after, at 16 and 18 px,
      dark and light (for judging a change to a set such as the sidebar).
  review.py catalogue [--out DIR]
      Every icon of the chat fork (regular, bold and fill), fitted by build.py's rules, named.
  review.py candidates NAME:weight ... [--out FILE]
      Fork glyphs side by side, for choosing one.

Pixels come from raster.py, the exact-area rasteriser `build.py --check` uses (no
gamma, no hinting), magnified with nearest-neighbour so each square is one device pixel.
Freya's own rendering is judged with sheets.py.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from types import SimpleNamespace

import build
import labels as labelsmod
import numpy as np
import raster
from PIL import Image, ImageDraw, ImageFont
from svgelements import Matrix
from svgelements import Path as SvgPath

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
ICONS_RS = REPO / "packages/bingux-ui/src/icons.rs"

DARK = ((18, 18, 20), (240, 240, 240))
LIGHT = ((250, 250, 250), (34, 34, 34))
TEXT = (30, 30, 30)
MUTED = (110, 110, 110)
PAGE = (255, 255, 255)


def font(size: int, mono: bool = False) -> ImageFont.ImageFont:
    for candidate in (
        "/usr/share/fonts/google-noto-vf/NotoSansMono[wght].ttf" if mono else "/usr/share/fonts/google-noto-vf/NotoSans[wght].ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    ):
        if Path(candidate).exists():
            return ImageFont.truetype(candidate, size)
    return ImageFont.load_default()


def outline_of_svg(text: str):
    """The filled outline of one of our icon files (paths with an optional transform)."""
    import xml.etree.ElementTree as ET

    root = ET.fromstring(text)
    acc = build.pathops.Path()
    for node in root.iter():
        if node.tag.removeprefix(build.SVG_NS) != "path":
            continue
        sp = SvgPath(node.attrib["d"])
        if node.attrib.get("transform"):
            sp = sp * Matrix(node.attrib["transform"])
        sp.reify()
        acc.addPath(build.d_to_path(sp.d()))
    acc.convertConicsToQuads(0.02)
    return build.pathops.simplify(acc)


def polys_of_file(path: Path):
    return build.polygons(outline_of_svg(path.read_text()))


def cell(polys, size: int, zoom: int, scheme) -> Image.Image:
    """`size` px raster magnified `zoom` times, ink on background."""
    bg, ink = scheme
    cov = raster.coverage(polys, size)[:, :, None]
    rgb = np.array(bg, dtype=np.float64) * (1 - cov) + np.array(ink, dtype=np.float64) * cov
    image = Image.fromarray(np.round(rgb).astype(np.uint8), "RGB")
    return image.resize((size * zoom, size * zoom), Image.NEAREST)


def variants() -> dict[str, str]:
    """icon file stem -> `Icon` variant, from icons.rs."""
    result: dict[str, str] = {}
    pending = None
    for line in ICONS_RS.read_text().splitlines():
        line = line.strip()
        match = re.match(r"Self::(\w+) =>(.*)", line)
        if match:
            pending = match.group(1)
            line = match.group(2)
        file = re.search(r'icons/([^"]+)-symbolic\.svg', line)
        if file and pending:
            result[file.group(1)] = pending
            pending = None
    return result


def group_order() -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    current = None
    for line in (HERE / "groups.txt").read_text().splitlines():
        line = line.split("#")[0].strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            current = line[1:-1]
            result[current] = []
        elif current:
            result[current].extend(line.split())
    return result


def glyph_text(row) -> str:
    if row.fork is None:
        return row.glyph
    attrs = " ".join(f"{k}={v}" for k, v in row.attrs.items() if k != "symmetric")
    return f"{row.fork} {row.weight}" + (f" {attrs}" if attrs else "")


def wrap(draw: ImageDraw.ImageDraw, text: str, fnt, width: int) -> list[str]:
    lines: list[str] = []
    line = ""
    for word in text.split():
        trial = f"{line} {word}".strip()
        if draw.textlength(trial, font=fnt) <= width or not line:
            line = trial
        else:
            lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines or [""]


def sheets(out: Path, icons: Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    labels = labelsmod.load()
    names = variants()
    plain = ["# file | Icon variant | category | glyph | where it is used | metaphor"]
    seen: set[str] = set()
    name_font, small_font, mono_font = font(15), font(13), font(13, mono=True)
    for group, files in group_order().items():
        rows = []
        for stem in files:
            seen.add(stem)
            rows.append(stem)
        render_group(out / f"group-{group}.png", group, rows, labels, names, icons, name_font, small_font, mono_font)
    rest = sorted(set(labels.rows) - seen)
    if rest:
        render_group(out / "group-other.png", "other", rest, labels, names, icons, name_font, small_font, mono_font)
    for stem, row in sorted(labels.rows.items()):
        plain.append(
            " | ".join(
                [stem, f"Icon::{names[stem]}" if stem in names else "(shell/QML only)", labels.policy.get(stem, "?"), glyph_text(row), row.where, row.metaphor]
            )
        )
    (out / "icons.txt").write_text("\n".join(plain) + "\n")
    print(f"review: wrote {len(seen | set(rest))} icons in {len(group_order())} group sheets and icons.txt to {out}")


def render_group(path, group, stems, labels, names, icons, name_font, small_font, mono_font) -> None:
    zoom16, zoom18, zoom48 = 6, 5, 2
    row_h = 16 * zoom16 + 16
    col_name, col_img = 330, 2 * 16 * zoom16 + 18 * zoom18 + 48 * zoom48 + 4 * 12
    col_text = 760
    width = col_name + col_img + col_text + 24
    header_h = 54
    image = Image.new("RGB", (width, header_h + row_h * max(1, len(stems))), PAGE)
    draw = ImageDraw.Draw(image)
    draw.text((12, 10), f"{group}: {len(stems)} icons", fill=TEXT, font=font(22))
    draw.text(
        (12, 36),
        "dark 16 px | light 16 px | light 18 px | light 48 px (x2)    columns: Icon, file, category | glyph, weight, fit | where it is used, metaphor",
        fill=MUTED,
        font=small_font,
    )
    for index, stem in enumerate(stems):
        top = header_h + index * row_h
        if index % 2:
            draw.rectangle((0, top, width, top + row_h), fill=(245, 246, 248))
        row = labels.rows.get(stem)
        file = icons / f"{stem}-symbolic.svg"
        if not file.exists():
            draw.text((12, top + 8), f"{stem}: file missing", fill=(190, 0, 0), font=name_font)
            continue
        polys = polys_of_file(file)
        x = col_name + 6
        for size, zoom, scheme in ((16, zoom16, DARK), (16, zoom16, LIGHT), (18, zoom18, LIGHT)):
            tile = cell(polys, size, zoom, scheme)
            image.paste(tile, (x, top + (row_h - tile.height) // 2))
            x += tile.width + 12
        tile = cell(polys, 48, zoom48, LIGHT)
        image.paste(tile, (x, top + (row_h - tile.height) // 2))
        variant = names.get(stem)
        draw.text((12, top + 8), f"Icon::{variant}" if variant else "(shell / QML only)", fill=TEXT if variant else MUTED, font=name_font)
        draw.text((12, top + 30), stem, fill=MUTED, font=mono_font)
        draw.text((12, top + 52), f"category {labels.policy.get(stem, '?')}", fill=MUTED, font=small_font)
        if row is not None:
            draw.text((12, top + 72), glyph_text(row)[:48], fill=TEXT, font=small_font)
            tx = col_name + col_img + 12
            ty = top + 8
            for line in wrap(draw, row.where, name_font, col_text - 12)[:2]:
                draw.text((tx, ty), line, fill=TEXT, font=name_font)
                ty += 20
            for line in wrap(draw, row.metaphor, small_font, col_text - 12)[:2]:
                draw.text((tx, ty + 6), line, fill=MUTED, font=small_font)
                ty += 17
    image.save(path)


def compare(before: Path, after: Path, group: str, out: Path) -> None:
    order = group_order()
    if group not in order:
        raise SystemExit(f"review: unknown group {group}; groups: {', '.join(order)}")
    stems = order[group]
    zoom = 6
    cell_w = 16 * zoom + 14
    row_h = 16 * zoom + 8
    blocks = [("before", before), ("after", after)]
    width = 110 + cell_w * len(stems)
    image = Image.new("RGB", (width, 30 + len(blocks) * (2 * row_h + 18 * 6 // 2 + 8)), PAGE)
    draw = ImageDraw.Draw(image)
    small = font(12)
    draw.text((8, 6), f"{group}: before / after at 16 px (x{zoom}), dark then light; below each 18 px light", fill=TEXT, font=font(16))
    y = 30
    for label, directory in blocks:
        draw.text((8, y + 40), label, fill=TEXT, font=font(18))
        for index, stem in enumerate(stems):
            path = directory / f"{stem}-symbolic.svg"
            if not path.exists():
                continue
            polys = polys_of_file(path)
            x = 110 + index * cell_w
            image.paste(cell(polys, 16, zoom, DARK), (x, y))
            image.paste(cell(polys, 16, zoom, LIGHT), (x, y + 16 * zoom + 4))
            image.paste(cell(polys, 18, 3, LIGHT), (x, y + 2 * (16 * zoom + 4)))
            if label == "after":
                draw.text((x, y + 2 * (16 * zoom + 4) + 56), stem.split("-")[-1][:12], fill=MUTED, font=small)
        y += 2 * row_h + 18 * 6 // 2 + 8
    image.save(out)
    print(f"review: wrote {out}")


def fitted_polys(name: str, weight: str, careful: bool = False):
    """The glyph fitted to the size rules: build.py's own search (`careful`) or one scale and a half pixel shift."""
    base = {"name": name, "weight": weight}
    if careful:
        try:
            fit = build.suggest_fit(f"zz-{name}", SimpleNamespace(fork=name, weight=weight, attrs={}), labelsmod.Labels())
        except build.BuildError:
            fit = {}
        return build.polygons(build.fork_outline({**base, **fit}, name).path)
    left, top, right, bottom = build.fork_outline(base, name).path.bounds
    long_side, short = max(right - left, bottom - top), max(min(right - left, bottom - top), 0.01)
    target = 14.0 if long_side / short < build.ELONGATED_ASPECT else 14.75
    zoom = round(min(max(target / max(long_side, 0.01), 0.5), 3.0), 2)
    left, top, right, bottom = build.fork_outline({**base, "scale": repr(zoom)}, name).path.bounds
    dx = -round(((left + right) / 2 - 8) * 2) / 2
    dy = -round(((top + bottom) / 2 - 8) * 2) / 2
    fitted = {**base, "scale": repr(zoom), "dx": repr(dx), "dy": repr(dy)}
    path = build.fork_outline(fitted, name).path
    left, top, right, bottom = path.bounds
    if left < 0 or top < 0 or right > 16 or bottom > 16:
        return build.polygons(build.fork_outline(base, name).path)
    return build.polygons(path)


def candidates(specs: list[str], out: Path) -> None:
    zoom = 6
    cell_w, cell_h = 16 * zoom + 24, 16 * zoom * 2 + 52
    cols = min(len(specs), 10)
    rows = (len(specs) + cols - 1) // cols
    image = Image.new("RGB", (cols * cell_w + 12, rows * cell_h + 12), PAGE)
    draw = ImageDraw.Draw(image)
    fnt = font(12)
    for index, spec in enumerate(specs):
        name, _, weight = spec.partition(":")
        weight = weight or "fill"
        polys = fitted_polys(name, weight, careful=True)
        x, y = 12 + (index % cols) * cell_w, 8 + (index // cols) * cell_h
        image.paste(cell(polys, 16, zoom, DARK), (x, y))
        image.paste(cell(polys, 16, zoom, LIGHT), (x, y + 16 * zoom + 4))
        draw.text((x, y + 2 * (16 * zoom + 4)), name, fill=TEXT, font=fnt)
        draw.text((x, y + 2 * (16 * zoom + 4) + 16), weight, fill=MUTED, font=fnt)
    out.parent.mkdir(parents=True, exist_ok=True)
    image.save(out)
    print(f"review: wrote {out}")


def catalogue(out: Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    geometry = build.json.loads(build.FORK_GEOMETRY.read_text())["icons"]
    names = sorted(geometry)
    per_page = 120
    cols = 10
    zoom = 3
    cell_w, cell_h = 16 * zoom * 3 + 30, 16 * zoom + 34
    fnt = font(11)
    for page, start in enumerate(range(0, len(names), per_page)):
        chunk = names[start : start + per_page]
        rows = (len(chunk) + cols - 1) // cols
        image = Image.new("RGB", (cols * cell_w + 12, rows * cell_h + 28), PAGE)
        draw = ImageDraw.Draw(image)
        draw.text((8, 6), f"chat fork icons {chunk[0]} to {chunk[-1]} (regular, bold, fill; 16 px x{zoom}, fitted by build.py)", fill=TEXT, font=font(13))
        for index, name in enumerate(chunk):
            x, y = 8 + (index % cols) * cell_w, 28 + (index // cols) * cell_h
            for k, weight in enumerate(("regular", "bold", "fill")):
                if weight not in geometry[name]:
                    continue
                polys = fitted_polys(name, weight)
                image.paste(cell(polys, 16, zoom, LIGHT), (x + k * (16 * zoom + 4), y))
            draw.text((x, y + 16 * zoom + 4), name, fill=TEXT, font=fnt)
        image.save(out / f"catalogue-{page + 1:02d}.png")
    (out / "catalogue.txt").write_text("\n".join(f"{name}\t{' '.join(w for w in ('regular', 'bold', 'fill') if w in geometry[name])}" for name in names) + "\n")
    print(f"review: wrote {len(names)} icons on {page + 1} pages to {out}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="mode", required=True)
    s = sub.add_parser("sheets")
    s.add_argument("--out", type=Path, default=Path("/tmp/bingux-icon-review"))
    s.add_argument("--icons", type=Path, default=build.DEFAULT_OUT)
    c = sub.add_parser("compare")
    c.add_argument("before", type=Path)
    c.add_argument("after", type=Path)
    c.add_argument("--group", default="sidebar")
    c.add_argument("--out", type=Path, default=Path("/tmp/bingux-icon-compare.png"))
    k = sub.add_parser("catalogue")
    k.add_argument("--out", type=Path, default=Path("/tmp/bingux-icon-catalogue"))
    d = sub.add_parser("candidates")
    d.add_argument("specs", nargs="+")
    d.add_argument("--out", type=Path, default=Path("/tmp/bingux-icon-candidates.png"))
    args = parser.parse_args()
    if args.mode == "sheets":
        sheets(args.out, args.icons)
    elif args.mode == "compare":
        compare(args.before, args.after, args.group, args.out)
    elif args.mode == "catalogue":
        catalogue(args.out)
    else:
        candidates(args.specs, args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
