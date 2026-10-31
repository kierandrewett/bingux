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
  review.py page [--out FILE] [--icons DIR]
      One self-contained HTML page with every icon: its pixels at 16 px (dark and light, at real size
      and magnified), 18 px and 48 px, where it is used and its metaphor, and a verdict and note field
      per icon. Feedback is kept in the browser and exported as text or JSON.

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
ICONS_RS = REPO / "crates/ui/src/icons.rs"

DARK = ((18, 18, 20), (240, 240, 240))
LIGHT = ((250, 250, 250), (34, 34, 34))
TEXT = (30, 30, 30)
MUTED = (110, 110, 110)
PAGE = (255, 255, 255)


def font(size: int, mono: bool = False) -> ImageFont.ImageFont:
    for candidate in (
        "/usr/share/fonts/google-noto-vf/NotoSansMono[wght].ttf"
        if mono
        else "/usr/share/fonts/google-noto-vf/NotoSans[wght].ttf",
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
        render_group(
            out / f"group-{group}.png",
            group,
            rows,
            labels,
            names,
            icons,
            name_font,
            small_font,
            mono_font,
        )
    rest = sorted(set(labels.rows) - seen)
    if rest:
        render_group(
            out / "group-other.png",
            "other",
            rest,
            labels,
            names,
            icons,
            name_font,
            small_font,
            mono_font,
        )
    for stem, row in sorted(labels.rows.items()):
        plain.append(
            " | ".join(
                [
                    stem,
                    f"Icon::{names[stem]}" if stem in names else "(shell/QML only)",
                    labels.policy.get(stem, "?"),
                    glyph_text(row),
                    row.where,
                    row.metaphor,
                ]
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
        for size, zoom, scheme in (
            (16, zoom16, DARK),
            (16, zoom16, LIGHT),
            (18, zoom18, LIGHT),
        ):
            tile = cell(polys, size, zoom, scheme)
            image.paste(tile, (x, top + (row_h - tile.height) // 2))
            x += tile.width + 12
        tile = cell(polys, 48, zoom48, LIGHT)
        image.paste(tile, (x, top + (row_h - tile.height) // 2))
        variant = names.get(stem)
        draw.text(
            (12, top + 8),
            f"Icon::{variant}" if variant else "(shell / QML only)",
            fill=TEXT if variant else MUTED,
            font=name_font,
        )
        draw.text((12, top + 30), stem, fill=MUTED, font=mono_font)
        draw.text(
            (12, top + 52),
            f"category {labels.policy.get(stem, '?')}",
            fill=MUTED,
            font=small_font,
        )
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
    draw.text(
        (8, 6),
        f"{group}: before / after at 16 px (x{zoom}), dark then light; below each 18 px light",
        fill=TEXT,
        font=font(16),
    )
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
                draw.text(
                    (x, y + 2 * (16 * zoom + 4) + 56),
                    stem.split("-")[-1][:12],
                    fill=MUTED,
                    font=small,
                )
        y += 2 * row_h + 18 * 6 // 2 + 8
    image.save(out)
    print(f"review: wrote {out}")


def fitted_polys(name: str, weight: str):
    """The glyph as build.py places it: the scale that satisfies the size rule (one factor on both axes) and the half pixel shift."""
    base = {"name": name, "weight": weight}
    try:
        fit = build.suggest_fit(
            f"zz-{name}",
            SimpleNamespace(fork=name, weight=weight, attrs={}),
            labelsmod.Labels(),
        )
    except build.BuildError:
        fit = {}
    return build.polygons(build.fork_outline({**base, **fit}, name).path)


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
        polys = fitted_polys(name, weight)
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
        draw.text(
            (8, 6),
            f"chat fork icons {chunk[0]} to {chunk[-1]} (regular, bold, fill; 16 px x{zoom}, fitted by build.py)",
            fill=TEXT,
            font=font(13),
        )
        for index, name in enumerate(chunk):
            x, y = 8 + (index % cols) * cell_w, 28 + (index // cols) * cell_h
            for k, weight in enumerate(("regular", "bold", "fill")):
                if weight not in geometry[name]:
                    continue
                polys = fitted_polys(name, weight)
                image.paste(cell(polys, 16, zoom, LIGHT), (x + k * (16 * zoom + 4), y))
            draw.text((x, y + 16 * zoom + 4), name, fill=TEXT, font=fnt)
        image.save(out / f"catalogue-{page + 1:02d}.png")
    (out / "catalogue.txt").write_text(
        "\n".join(
            f"{name}\t{' '.join(w for w in ('regular', 'bold', 'fill') if w in geometry[name])}" for name in names
        )
        + "\n"
    )
    print(f"review: wrote {len(names)} icons on {page + 1} pages to {out}")


PAGE_TEMPLATE = r"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Bingux icon review</title>
<style>
:root{--bg:#f4f5f7;--card:#fff;--ink:#1d1d1f;--muted:#6b6b72;--line:#dcdde1;--accent:#2f6fed}
*{box-sizing:border-box}body{margin:0;font:14px/1.4 system-ui,sans-serif;background:var(--bg);color:var(--ink)}
header{position:sticky;top:0;z-index:5;background:#fff;border-bottom:1px solid var(--line);padding:10px 20px;display:flex;gap:16px;align-items:center;flex-wrap:wrap}
header h1{font-size:16px;margin:0}header .count{color:var(--muted)}
header button,header select{font:inherit;padding:5px 10px;border:1px solid var(--line);background:#fff;border-radius:6px;cursor:pointer}
header button.primary{background:var(--accent);border-color:var(--accent);color:#fff}
nav{padding:8px 20px;display:flex;gap:8px;flex-wrap:wrap;background:#fff;border-bottom:1px solid var(--line)}
nav a{color:var(--accent);text-decoration:none;padding:2px 8px;border:1px solid var(--line);border-radius:12px;font-size:13px}
main{max-width:1180px;margin:0 auto;padding:16px 20px 80px}
h2{margin:28px 0 8px;font-size:18px}
.card{display:grid;grid-template-columns:360px minmax(0,1fr);gap:16px;background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px 14px;margin:8px 0}
.card.focus{outline:2px solid var(--accent)}
.card.done{border-left:5px solid #2e9e5b}.card.flag{border-left:5px solid #d9822b}
.tiles{display:flex;gap:10px;align-items:flex-end;flex-wrap:nowrap}
.tile{text-align:center;font-size:11px;color:var(--muted);flex:none}
.tile img{display:block;image-rendering:pixelated;margin:0 auto 3px;border:1px solid var(--line)}
.real{display:flex;gap:6px;margin-top:8px;align-items:center;font-size:11px;color:var(--muted)}
.real span{display:inline-flex;padding:8px;border-radius:6px}
.real .d{background:#121214}.real .l{background:#fafafa;border:1px solid var(--line)}
.real img{image-rendering:pixelated}.inv{filter:invert(1) brightness(1.8)}.miss{color:#b3402f;margin:8px 0}
.meta h3{margin:0;font-size:15px}.meta code{color:var(--muted);font-size:12px}
.meta p{margin:4px 0}.k{color:var(--muted)}
.verdicts{display:flex;gap:6px;flex-wrap:wrap;margin:8px 0}
.verdicts label{border:1px solid var(--line);border-radius:14px;padding:3px 10px;cursor:pointer;user-select:none}
.verdicts input{display:none}
.verdicts input:checked+span{font-weight:600}
.verdicts label:has(input:checked){background:var(--accent);border-color:var(--accent);color:#fff}
.verdicts label.v-good:has(input:checked){background:#2e9e5b;border-color:#2e9e5b}
.verdicts label.v-remove:has(input:checked){background:#b3402f;border-color:#b3402f}
textarea{width:100%;min-height:44px;font:inherit;padding:6px;border:1px solid var(--line);border-radius:6px;resize:vertical}
.hidden{display:none}.help{color:var(--muted);font-size:12px}
</style></head><body>
<header><h1>Bingux icon review</h1><span class="count" id="count"></span>
<select id="filter"><option value="all">All icons</option><option value="todo">Not reviewed</option><option value="flag">Flagged (not Good)</option></select>
<button id="copy" class="primary">Copy feedback as text</button><button id="json">Download JSON</button><button id="clear">Clear all</button>
<span class="help">Keys: J / K move, 1 Good, 2 Tweak, 3 Redraw, 4 Wrong metaphor, 5 Remove, N edit note, Esc leave note</span></header>
<nav id="nav"></nav><main id="main"></main>
<script>
const ITEMS = __ITEMS__;
const VERDICTS = [["good","Good"],["tweak","Tweak"],["redraw","Redraw"],["metaphor","Wrong metaphor"],["remove","Remove"]];
const KEY = "bingux-icon-review-v1";
let store = {};
try { store = JSON.parse(localStorage.getItem(KEY) || "{}"); } catch (e) {}
const save = () => { localStorage.setItem(KEY, JSON.stringify(store)); update(); };
const main = document.getElementById("main"), nav = document.getElementById("nav");
const cards = [];
let current = -1, lastGroup = null;
const png = (s) => "data:image/png;base64," + s;
for (const it of ITEMS) {
  if (it.group !== lastGroup) {
    lastGroup = it.group;
    const h = document.createElement("h2"); h.id = "g-" + it.group; h.textContent = it.group; main.appendChild(h);
    const a = document.createElement("a"); a.href = "#g-" + it.group; a.textContent = it.group; nav.appendChild(a);
  }
  const el = document.createElement("section"); el.className = "card"; el.dataset.id = it.id;
  const tile = (k, label, scale) => `<div class="tile"><img src="${png(it.tiles[k])}" width="${it.sizes[k] * scale}" height="${it.sizes[k] * scale}"><div>${label}</div></div>`;
  const vec = (px, label, dark) => `<div class="tile"><img class="${dark && it.symbolic ? "inv" : ""}" style="background:${dark ? "#121214" : "#fafafa"}" src="data:image/svg+xml;base64,${it.svg}" width="${px}" height="${px}"><div>${label}</div></div>`;
  const vecReal = (px, dark) => `<span class="${dark ? "d" : "l"}"><img class="${dark && it.symbolic ? "inv" : ""}" src="data:image/svg+xml;base64,${it.svg}" width="${px}" height="${px}"></span>`;
  const shown = it.missing
    ? `<p class="miss">No icon file. The shell asks for this name, and the theme has nothing to draw.</p>`
    : it.svg
    ? `<div class="tiles">${vec(32, "16 dark", true)}${vec(32, "16 light", false)}${vec(48, "24 light", false)}${vec(96, "48 light", false)}</div>
    <div class="real">${vecReal(16, true)}${vecReal(16, false)}${vecReal(24, false)}${vecReal(48, false)}actual size</div>`
    : `<div class="tiles">${tile("d16","16 dark",4)}${tile("l16","16 light",4)}${tile("l18","18 light",4)}${tile("l48","48 light",2)}</div>
    <div class="real"><span class="d"><img src="${png(it.tiles.d16)}" width="16" height="16"></span><span class="l"><img src="${png(it.tiles.l16)}" width="16" height="16"></span><span class="l"><img src="${png(it.tiles.l18)}" width="18" height="18"></span>actual size</div>`;
  el.innerHTML = `<div>${shown}</div>
    <div class="meta"><h3>${it.variant ? "Icon::" + it.variant : it.svg ? it.category : "(shell / QML only)"}</h3><code>${it.id}</code>
    <p><span class="k">Used:</span> ${it.where}</p>${it.metaphor ? `<p><span class="k">Metaphor:</span> ${it.metaphor}</p>` : ""}
    <p><span class="k">Category:</span> ${it.category} &nbsp; <span class="k">Glyph:</span> ${it.glyph}</p>
    <div class="verdicts">${VERDICTS.map(([v, l]) => `<label class="v-${v}"><input type="radio" name="v-${it.id}" value="${v}"><span>${l}</span></label>`).join("")}</div>
    <textarea placeholder="What is wrong, or what it should look like"></textarea></div>`;
  el.querySelectorAll("input").forEach((r) => r.addEventListener("change", () => { setVerdict(it.id, r.value); }));
  const ta = el.querySelector("textarea");
  ta.addEventListener("input", () => { (store[it.id] = store[it.id] || {}).note = ta.value; save(); });
  el.addEventListener("click", () => { current = cards.findIndex((c) => c.el === el); markFocus(); });
  main.appendChild(el); cards.push({ it, el, ta });
}
function setVerdict(id, v) { (store[id] = store[id] || {}).verdict = v; save(); }
function update() {
  let done = 0, flagged = 0;
  const f = document.getElementById("filter").value;
  for (const { it, el, ta } of cards) {
    const s = store[it.id] || {};
    el.querySelectorAll("input").forEach((r) => (r.checked = r.value === s.verdict));
    if (document.activeElement !== ta) ta.value = s.note || "";
    const reviewed = !!s.verdict || !!(s.note && s.note.trim());
    const flag = !!s.verdict && s.verdict !== "good";
    if (reviewed) done++; if (flag) flagged++;
    el.classList.toggle("done", reviewed && !flag); el.classList.toggle("flag", flag);
    el.classList.toggle("hidden", (f === "todo" && reviewed) || (f === "flag" && !flag));
  }
  document.getElementById("count").textContent = `${done} of ${ITEMS.length} reviewed, ${flagged} flagged`;
}
function markFocus() { cards.forEach((c, i) => c.el.classList.toggle("focus", i === current)); }
function move(d) {
  let i = current;
  do { i += d; } while (i >= 0 && i < cards.length && cards[i].el.classList.contains("hidden"));
  if (i < 0 || i >= cards.length) return;
  current = i; markFocus(); cards[i].el.scrollIntoView({ block: "center" });
}
document.addEventListener("keydown", (e) => {
  if (e.target.tagName === "TEXTAREA") { if (e.key === "Escape") e.target.blur(); return; }
  if (e.key === "j" || e.key === "ArrowDown") { e.preventDefault(); move(1); }
  else if (e.key === "k" || e.key === "ArrowUp") { e.preventDefault(); move(-1); }
  else if (current >= 0 && /^[1-5]$/.test(e.key)) { setVerdict(cards[current].it.id, VERDICTS[+e.key - 1][0]); }
  else if (current >= 0 && e.key === "n") { e.preventDefault(); cards[current].ta.focus(); }
});
const label = Object.fromEntries(VERDICTS);
function lines() {
  return ITEMS.filter((it) => store[it.id] && (store[it.id].verdict || (store[it.id].note || "").trim()))
    .map((it) => { const s = store[it.id]; return `${it.id} | ${it.variant || "shell"} | ${label[s.verdict] || "no verdict"}${(s.note || "").trim() ? " | " + s.note.trim().replace(/\s+/g, " ") : ""}`; });
}
document.getElementById("copy").onclick = async () => {
  const text = "# icon | Icon variant | verdict | note\n" + lines().join("\n") + "\n";
  try { await navigator.clipboard.writeText(text); } catch (e) { prompt("Copy this feedback", text); }
};
document.getElementById("json").onclick = () => {
  const blob = new Blob([JSON.stringify(store, null, 2)], { type: "application/json" });
  const a = document.createElement("a"); a.href = URL.createObjectURL(blob); a.download = "bingux-icon-feedback.json"; a.click();
};
document.getElementById("clear").onclick = () => { if (confirm("Delete all feedback in this browser?")) { store = {}; save(); } };
document.getElementById("filter").onchange = update;
update();
</script></body></html>
"""


def png_base64(image: Image.Image) -> str:
    import base64
    import io

    buffer = io.BytesIO()
    image.save(buffer, "PNG", optimize=True)
    return base64.b64encode(buffer.getvalue()).decode("ascii")


def extra_items(ours: set[str]) -> list[dict]:
    """Icons that are not in the generated symbolic set: the colour file and folder icons, the application icons,
    and the theme icons the shell asks for by name. They are shown as the browser draws the vector file."""
    import base64
    import html

    def entry(group, ident, path, where, category, symbolic):
        return {
            "id": ident,
            "group": group,
            "variant": None,
            "category": category,
            "glyph": html.escape(str(path.relative_to(REPO)) if path.is_relative_to(REPO) else str(path)),
            "where": html.escape(where),
            "metaphor": "",
            "symbolic": symbolic,
            "svg": base64.b64encode(path.read_bytes()).decode("ascii"),
        }

    items = []
    for path in sorted((HERE / "colour").glob("*.svg")):
        kind = "folder" if path.stem.startswith(("folder", "trash")) else "file type"
        items.append(
            entry(
                "files-colour",
                path.stem,
                path,
                f"Files: {kind} icon, drawn in colour",
                "colour",
                False,
            )
        )
    apps = [
        *sorted((REPO / "apps").glob("*/*.svg")),
        REPO / "distro/packages/bingux-os/bingux-mark.svg",
    ]
    for path in apps:
        if path.exists() and "assets" not in path.parts:
            items.append(
                entry(
                    "bingux-apps",
                    path.stem,
                    path,
                    "Application icon: dock, launcher, window switcher",
                    "app",
                    False,
                )
            )
    users: dict[str, set[str]] = {}
    for qml in sorted((REPO / "shell/bingux").rglob("*.qml")):
        for name in re.findall(r'"([a-z0-9][a-z0-9.-]*)-symbolic"', qml.read_text()):
            users.setdefault(name, set()).add(qml.name)
    theme = Path("/usr/share/icons/Adwaita/symbolic")
    aliases = dict(
        re.findall(
            r'"([^"]+)":\s*"([^"]+)"',
            (REPO / "shell/bingux/AdwaitaIconIndex.js").read_text().split("var paths")[0],
        )
    )
    for name in sorted(users):
        if name in ours:
            continue
        target = aliases.get(f"{name}-symbolic", aliases.get(name, f"{name}-symbolic"))
        found = next(iter(sorted(theme.glob(f"*/{target}.svg"))), None)
        where = "Shell: " + ", ".join(sorted(users[name])[:4]) + (" and more" if len(users[name]) > 4 else "")
        if target != f"{name}-symbolic":
            where += f" (the shell maps it to {target})"
        if found is None:
            print(
                f"review: shell icon {name} has no file in the Adwaita theme; added as a missing card",
                file=sys.stderr,
            )
            items.append(
                {
                    "id": name,
                    "group": "shell-theme",
                    "variant": None,
                    "category": "no file",
                    "glyph": "",
                    "where": html.escape(where),
                    "metaphor": "",
                    "symbolic": True,
                    "svg": None,
                    "missing": True,
                }
            )
            continue
        items.append(entry("shell-theme", name, found, where, "Adwaita theme", True))
    return items


def page(out: Path, icons: Path) -> None:
    """The interactive review page. Tiles are the exact rasteriser's pixels, drawn unscaled and magnified by the browser."""
    import html
    import json

    labels = labelsmod.load()
    names = variants()
    items = []
    seen: set[str] = set()
    groups = list(group_order().items())
    rest = sorted(set(labels.rows) - {stem for _, files in groups for stem in files})
    if rest:
        groups.append(("other", rest))
    for group, files in groups:
        for stem in files:
            if stem in seen:
                continue
            seen.add(stem)
            file = icons / f"{stem}-symbolic.svg"
            if not file.exists():
                print(f"review: {stem}: file missing in {icons}, skipped", file=sys.stderr)
                continue
            polys = polys_of_file(file)
            row = labels.rows.get(stem)
            tiles = {
                "d16": cell(polys, 16, 1, DARK),
                "l16": cell(polys, 16, 1, LIGHT),
                "l18": cell(polys, 18, 1, LIGHT),
                "l48": cell(polys, 48, 1, LIGHT),
            }
            items.append(
                {
                    "id": stem,
                    "group": group,
                    "variant": names.get(stem),
                    "category": labels.policy.get(stem, "?"),
                    "glyph": html.escape(glyph_text(row)) if row else "?",
                    "where": html.escape(row.where) if row else "",
                    "metaphor": html.escape(row.metaphor) if row else "",
                    "sizes": {key: tile.width for key, tile in tiles.items()},
                    "tiles": {key: png_base64(tile) for key, tile in tiles.items()},
                }
            )
    items.extend(extra_items({stem for _, files in groups for stem in files} | set(labels.rows)))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(PAGE_TEMPLATE.replace("__ITEMS__", json.dumps(items).replace("</", "<\\/")))
    print(f"review: wrote {len(items)} icons to {out}")


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
    p = sub.add_parser("page")
    p.add_argument("--out", type=Path, default=Path("/tmp/bingux-icon-review.html"))
    p.add_argument("--icons", type=Path, default=build.DEFAULT_OUT)
    d = sub.add_parser("candidates")
    d.add_argument("specs", nargs="+")
    d.add_argument("--out", type=Path, default=Path("/tmp/bingux-icon-candidates.png"))
    args = parser.parse_args()
    if args.mode == "sheets":
        sheets(args.out, args.icons)
    elif args.mode == "compare":
        compare(args.before, args.after, args.group, args.out)
    elif args.mode == "page":
        page(args.out, args.icons)
    elif args.mode == "catalogue":
        catalogue(args.out)
    else:
        candidates(args.specs, args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
