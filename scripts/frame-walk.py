#!/usr/bin/env python3
"""Walk every Bingux Settings page, record each frame, and report layout shifts.

Each page runs once through `bingux-settings --snapshot`, with
BINGUX_SETTINGS_SNAPSHOT_FRAMES (every frame of the run) and
BINGUX_SETTINGS_SNAPSHOT_SCRIPT (the walk: subpage and back, one popup, then
scroll down and back). Consecutive frames are compared by their edges:
horizontal rules, vertical rules and text bands. An edge that moves or changes
extent between two frames is a shift when both frames are idle, meaning no
pointer or key input and no navigation in the last 300 ms and nothing happened
between the two frames.

Frames before the walk's `idle start` marker are the load phase (skeleton to
data, no input yet). Shifts there are reported as load shifts.

Output: OUT/report.md (default /tmp/FrameWalk/report.md), OUT/shifts.json,
OUT/shifts/*.png (before/after crops with moved edges boxed in red), and
OUT/logs/<page>.log (the binary's stderr, including click-text misses).

Needs PIL and numpy. Build first: `cargo build --features snapshot` in
apps/settings, with CARGO_TARGET_DIR set as usual.

Known limits: text bands are matched by top edge and x extent, so a font size
change that only alters height is not seen. Edges that appear and never move
are not reported. A page that changes content with no input (a live clock, a
spinner) shows up as shifts; check the crop before filing it.
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
SETTINGS = REPO / "apps" / "settings"
DEFAULT_BINARY = (
    Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target")) / "debug" / "bingux-settings"
)

IDLE_MS = 300  # a frame is idle this long after the last input or navigation
EDGE_DELTA = 25  # luminance step between neighbours that makes an edge pixel
INK_DELTA = 40  # luminance distance from the page background that makes ink
MIN_RUN = 30  # shortest horizontal or vertical rule, in px
MAX_MOVE = 160  # largest displacement matched between two frames, in px
PAD = 24  # padding around a crop, in px
MAX_GROUPS_IN_REPORT = 25  # per page; the rest are counted in shifts.json

H, V, T = 0, 1, 2  # horizontal rule, vertical rule, text band
KIND_NAME = {H: "horizontal rule", V: "vertical rule", T: "text band"}
NO_MATCH = np.iinfo(np.int64).max

# Per page: (subpage to open and return from, label of the control that opens
# the page's popup). Labels are the trigger rows and buttons in src/pages/*.rs.
WALK = {
    "network": ("network/wifi/saved", None),
    "datetime": ("datetime/timezone", "Set date and time"),
    "keyboard": ("keyboard/shortcuts", "Add an input source"),
    "background": ("background/font", None),
    "applications": ("applications/default", None),
    "users": (None, "Change password"),
    "about": (None, "Rename"),
}


def panels() -> list[str]:
    """The sidebar panel names, read from PageId::panel_name."""
    source = (SETTINGS / "src" / "pages" / "mod.rs").read_text()
    match = re.search(r"fn panel_name\(self\).*?\n    \}\n", source, re.S)
    if match is None:
        sys.exit("cannot find PageId::panel_name in src/pages/mod.rs")
    return re.findall(r'=> "([a-z]+)"', match.group(0))


def walk_script(panel: str) -> str:
    """The BINGUX_SETTINGS_SNAPSHOT_SCRIPT for one page."""
    subpage, popup = WALK.get(panel, (None, None))
    steps = ["mark idle start", "wait 1500"]
    if subpage:
        steps += ["mark nav", f"address {subpage}", "wait 1000"]
        steps += ["mark nav", f"address {panel}", "wait 1000"]
    if popup:
        steps += [
            f"click-text {popup}",
            "wait 1000",
            "mark popup",
            "key escape",
            "wait 1000",
        ]
    steps += ["scroll 620,400,-600", "wait 800", "scroll 620,400,600", "wait 800"]
    steps += ["mark end"]
    return "; ".join(steps)


def run_page(binary: Path, panel: str, out: Path, timeout: int) -> str:
    """Run the walk for one page. Returns a short status for the report."""
    frames_root = out / "frames"
    shutil.rmtree(frames_root / panel, ignore_errors=True)
    env = isolated()  # Settings writes through gsettings: no write may reach the person's dconf
    env["BINGUX_SETTINGS_SNAPSHOT_FRAMES"] = str(frames_root)
    env["BINGUX_SETTINGS_SNAPSHOT_SCRIPT"] = walk_script(panel)
    log = out / "logs" / f"{panel}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as sink:
        try:
            proc = subprocess.run(
                [str(binary), "--snapshot", str(out / "png"), panel],
                env=env,
                stdout=sink,
                stderr=subprocess.STDOUT,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return f"timed out after {timeout}s"
    return "ok" if proc.returncode == 0 else f"exit {proc.returncode}"


def runs(mask: np.ndarray):
    """Runs of True along the last axis: (row, start, end-exclusive) arrays."""
    padded = np.pad(mask.astype(np.int8), ((0, 0), (1, 1)))
    delta = np.diff(padded, axis=1)
    rows, starts = np.nonzero(delta == 1)
    _, ends = np.nonzero(delta == -1)
    return rows, starts, ends


def features(gray: np.ndarray, background: int) -> np.ndarray:
    """Horizontal rules, vertical rules and text bands of one frame.

    Rows are (kind, y0, x0, y1, x1). A horizontal rule has y0 == y1, a vertical
    rule has x0 == x1, and a text band spans its ink rows and columns.
    """
    g = gray.astype(np.int16)
    out: list[tuple[int, int, int, int, int]] = []

    rows, starts, ends = runs(np.abs(np.diff(g, axis=0)) > EDGE_DELTA)
    for y, x0, x1 in zip(rows, starts, ends):
        if x1 - x0 >= MIN_RUN:
            out.append((H, int(y), int(x0), int(y), int(x1)))

    cols, starts, ends = runs((np.abs(np.diff(g, axis=1)) > EDGE_DELTA).T)
    for x, y0, y1 in zip(cols, starts, ends):
        if y1 - y0 >= MIN_RUN:
            out.append((V, int(y0), int(x), int(y1), int(x)))

    ink = np.abs(g - background) > INK_DELTA
    _, band_starts, band_ends = runs(ink.any(axis=1)[None, :])
    for y0, y1 in zip(band_starts, band_ends):
        if y1 - y0 < 4:
            continue
        xs = np.nonzero(ink[y0:y1].any(axis=0))[0]
        out.append((T, int(y0), int(xs[0]), int(y1), int(xs[-1]) + 1))

    return np.array(out, dtype=np.int64).reshape(-1, 5)


def axes(feats: np.ndarray):
    """(kind, coord, lo, hi) per feature. coord is the position across the edge."""
    kind = feats[:, 0]
    vertical = kind == V
    coord = np.where(vertical, feats[:, 2], feats[:, 1])
    lo = np.where(vertical, feats[:, 1], feats[:, 2])
    hi = np.where(vertical, feats[:, 3], feats[:, 4])
    return kind, coord, lo, hi


def diff_features(before: np.ndarray, after: np.ndarray) -> list[dict]:
    """Edges that moved or changed extent between two frames.

    Identical edges are dropped first. The rest are matched greedily by the
    cheapest displacement within MAX_MOVE. A match at the same position with a
    different extent is a resize; one at a different position is a move.
    """
    before_list = [tuple(row) for row in before.tolist()]
    after_list = [tuple(row) for row in after.tolist()]
    after_set, before_set = set(after_list), set(before_list)
    a_rest = [row for row in before_list if row not in after_set]
    b_rest = [row for row in after_list if row not in before_set]
    if not a_rest or not b_rest:
        return []
    a = np.array(a_rest, dtype=np.int64)
    b = np.array(b_rest, dtype=np.int64)
    ka, ca, la, ha = axes(a)
    kb, cb, lb, hb = axes(b)
    dc = np.abs(ca[:, None] - cb[None, :])
    dl = np.abs(la[:, None] - lb[None, :])
    dh = np.abs(ha[:, None] - hb[None, :])
    ok = (ka[:, None] == kb[None, :]) & (dc <= MAX_MOVE) & (dl <= MAX_MOVE) & (dh <= MAX_MOVE)
    cost = np.where(ok, 2 * dc + dl + dh, NO_MATCH)
    used_a: set[int] = set()
    used_b: set[int] = set()
    pairs: list[dict] = []
    for flat in np.argsort(cost, axis=None):
        i, j = divmod(int(flat), cost.shape[1])
        if cost[i, j] == NO_MATCH:
            break
        if i in used_a or j in used_b:
            continue
        used_a.add(i)
        used_b.add(j)
        step_c = int(cb[j] - ca[i])
        step_l = int(lb[j] - la[i])
        step_h = int(hb[j] - ha[i])
        if step_c != 0:
            change = "moved"
        elif step_l or step_h:
            change = "resized"
        else:
            continue
        pairs.append(
            {
                "kind": int(ka[i]),
                "change": change,
                "dc": step_c,
                "dlo": step_l,
                "dhi": step_h,
                "a": [int(v) for v in a[i]],
                "b": [int(v) for v in b[j]],
            }
        )
    return pairs


def appeared(before: np.ndarray, after: np.ndarray) -> list[tuple[int, int, int, int, int]]:
    """Text bands of `after` with no text band of `before` at the same place: content that was not there.

    `diff_features` pairs edges that moved or resized and drops the rest, so a group of rows that is added
    below the ones already drawn (a list whose data landed after its first frame) moves nothing and is
    never reported. This is that gap: a band counts as new when no band of `before` starts within
    MAX_MOVE px of it vertically and overlaps it horizontally.
    """
    new_bands = [tuple(int(v) for v in row) for row in after.tolist() if row[0] == T]
    old_bands = [tuple(int(v) for v in row) for row in before.tolist() if row[0] == T]
    found = []
    for band in new_bands:
        _, y0, x0, _, x1 = band
        if not any(abs(y0 - o[1]) <= MAX_MOVE and min(x1, o[4]) - max(x0, o[2]) > 0 for o in old_bands):
            found.append(band)
    return found


def describe(pair: dict) -> str:
    kind = pair["kind"]
    name = KIND_NAME[kind]
    _, y0, x0, y1, x1 = pair["b"]
    where = f"at x {x0}, y {y0}-{y1}" if kind == V else f"at y {y0}, x {x0}-{x1}"
    if pair["change"] == "moved":
        axis = "horizontally" if kind == V else "vertically"
        a_coord = pair["a"][2] if kind == V else pair["a"][1]
        b_coord = pair["b"][2] if kind == V else pair["b"][1]
        return f"{name} {where} moved {pair['dc']:+d} px {axis} (was {a_coord}, now {b_coord})"
    return f"{name} {where} resized: start {pair['dlo']:+d} px, end {pair['dhi']:+d} px"


def translated(pairs: list[dict]) -> str | None:
    """A note when most moved edges moved by the same amount (whole content shift)."""
    moved = [pair["dc"] for pair in pairs if pair["change"] == "moved"]
    if len(moved) < 20:
        return None
    value, count = collections.Counter(moved).most_common(1)[0]
    if count >= 0.6 * len(moved):
        return f"whole content moved {value:+d} px ({count} of {len(moved)} edges)"
    return None


def box(feat) -> tuple[int, int, int, int]:
    _, y0, x0, y1, x1 = feat
    if y1 - y0 < 2:
        y0, y1 = y0 - 1, y1 + 1
    if x1 - x0 < 2:
        x0, x1 = x0 - 1, x1 + 1
    return x0, y0, x1, y1


def write_pair(before: dict, after: dict, pairs: list[dict], path: Path) -> None:
    """Side-by-side crop of the moved edges, before and after, boxed in red."""
    boxes_before = [box(pair["a"]) for pair in pairs]
    boxes_after = [box(pair["b"]) for pair in pairs]
    everything = boxes_before + boxes_after
    height, width = before["gray"].shape
    x0 = max(0, min(b[0] for b in everything) - PAD)
    y0 = max(0, min(b[1] for b in everything) - PAD)
    x1 = min(width, max(b[2] for b in everything) + PAD)
    y1 = min(height, max(b[3] for b in everything) + PAD)
    crop_before = Image.fromarray(before["rgb"][y0:y1, x0:x1])
    crop_after = Image.fromarray(after["rgb"][y0:y1, x0:x1])
    w, h = crop_before.size
    gap, top = 12, 18
    canvas = Image.new("RGB", (w * 2 + gap, h + top), (255, 255, 255))
    canvas.paste(crop_before, (0, top))
    canvas.paste(crop_after, (w + gap, top))
    draw = ImageDraw.Draw(canvas)
    for bx0, by0, bx1, by1 in boxes_before:
        draw.rectangle(
            [bx0 - x0, by0 - y0 + top, bx1 - x0, by1 - y0 + top],
            outline=(220, 0, 0),
            width=2,
        )
    for bx0, by0, bx1, by1 in boxes_after:
        offset = w + gap
        draw.rectangle(
            [bx0 - x0 + offset, by0 - y0 + top, bx1 - x0 + offset, by1 - y0 + top],
            outline=(220, 0, 0),
            width=2,
        )
    draw.text((2, 2), f"before: frame {before['index']}, t={before['t']} ms", fill=(0, 0, 0))
    draw.text(
        (w + gap + 2, 2),
        f"after: frame {after['index']}, t={after['t']} ms",
        fill=(0, 0, 0),
    )
    canvas.save(path)


def load_records(frames: Path) -> list[dict]:
    text = (frames / "frames.jsonl").read_text()
    return [json.loads(line) for line in text.splitlines() if line.strip()]


def analyse(panel: str, frames: Path, shifts: Path) -> dict:
    """Find shifts on one page. Consecutive shifting frame pairs are grouped."""
    groups: list[dict] = []
    frame_count = 0
    idle_pairs = 0
    popins: list[dict] = []
    prev: dict | None = None
    phase = "load"
    marker = ""
    last_nav: int | None = None
    for rec in load_records(frames):
        if rec["kind"] == "marker":
            marker = rec["text"]
            if marker == "idle start":
                phase = "walk"
            if marker == "nav":
                last_nav = rec["t_ms"]
            continue
        frame_count += 1
        if prev is not None and rec["file"] == prev["file"]:
            continue  # byte-identical to the previous frame
        t = rec["t_ms"]
        since = rec["since_input_ms"]
        input_t = t - since if since is not None else None
        events = [e for e in (input_t, last_nav) if e is not None]
        event_t = max(events) if events else None
        rgb = np.asarray(Image.open(frames / rec["file"]).convert("RGB"))
        gray = (rgb @ np.array([0.299, 0.587, 0.114])).astype(np.uint8)
        background = int(np.bincount(gray.ravel(), minlength=256).argmax())
        current = {
            "index": rec["frame"],
            "t": t,
            "since": None if event_t is None else t - event_t,
            "file": rec["file"],
            "phase": phase,
            "marker": marker,
            "rgb": rgb,
            "gray": gray,
            "feats": features(gray, background),
        }
        # Compare only when the pair is idle: the newest input or navigation
        # is at or before the previous frame (nothing happened between them),
        # and the current frame is IDLE_MS past it.
        idle = event_t is None or (event_t <= prev["t"] and t - event_t >= IDLE_MS) if prev else False
        if prev is not None and idle:
            idle_pairs += 1
            pairs = diff_features(prev["feats"], current["feats"])
            if phase == "load":
                new_bands = appeared(prev["feats"], current["feats"])
                if new_bands:
                    popins.append(
                        {
                            "frame": current["index"],
                            "t": t,
                            "bands": len(new_bands),
                            "y0": min(b[1] for b in new_bands),
                            "y1": max(b[3] for b in new_bands),
                            "file": current["file"],
                        }
                    )
            if pairs:
                record(groups, panel, frames, shifts, prev, current, pairs)
        prev = current
    return {
        "frames": frame_count,
        "idle_pairs": idle_pairs,
        "groups": groups,
        "popins": popins,
    }


def record(groups, panel, frames, shifts, before, after, pairs) -> None:
    """Add one shifting frame pair, extending the group it continues.

    A group keeps every pair of its frames in `pairs`: the largest move of a
    multi-frame shift is usually in a middle pair, not the first or the last.
    `crop_worst` is the crop of the pair with the largest move."""
    crop_name = f"{panel}-f{after['index']:06d}.png"
    write_pair(before, after, pairs, shifts / crop_name)
    biggest = max(abs(pair["dc"]) for pair in pairs)
    last = groups[-1] if groups else None
    if last and last["frame_to"] == before["index"] and last["phase"] == after["phase"]:
        worst = crop_name if biggest > last["max_dc"] else last["crop_worst"]
        for stale in {last["crop_last"], last["crop_worst"]} - {last["crop_first"], worst, crop_name}:
            (shifts / stale).unlink(missing_ok=True)
        last["crop_worst"] = worst
        last.update(
            frame_to=after["index"],
            t_to=after["t"],
            since_to=after["since"],
            crop_last=crop_name,
            last_pairs=pairs,
            count=last["count"] + 1,
        )
        last["pairs"] = last["pairs"] + pairs
        last["max_dc"] = max(last["max_dc"], biggest)
        return
    first_full = f"{panel}-f{before['index']:06d}-before.png"
    last_full = f"{panel}-f{after['index']:06d}-after.png"
    shutil.copy(frames / before["file"], shifts / first_full)
    shutil.copy(frames / after["file"], shifts / last_full)
    groups.append(
        {
            "panel": panel,
            "phase": after["phase"],
            "marker": after["marker"],
            "frame_from": before["index"],
            "frame_to": after["index"],
            "t_from": before["t"],
            "t_to": after["t"],
            "since_to": after["since"],
            "count": 1,
            "max_dc": biggest,
            "crop_first": crop_name,
            "crop_last": crop_name,
            "crop_worst": crop_name,
            "full_before": first_full,
            "full_after": last_full,
            "first_pairs": pairs,
            "last_pairs": pairs,
            "pairs": list(pairs),
        }
    )


def notes(out: Path, panel: str) -> list[str]:
    log = out / "logs" / f"{panel}.log"
    if not log.exists():
        return []
    return [line for line in log.read_text().splitlines() if line.startswith("[snapshot]")]


def render(results: dict, binary: Path) -> str:
    lines = [
        "# Bingux Settings frame walk",
        "",
        f"Generated {time.strftime('%Y-%m-%d %H:%M:%S')} with `{binary}`.",
        "",
        "Each page runs the walk in `scripts/frame-walk.py` (subpage and back, one "
        "popup, scroll down and back), recording every frame. A shift is an edge "
        "(horizontal rule, vertical rule or text band) that moves or changes extent "
        "between two consecutive frames, where both frames are idle: no input or "
        f"navigation in the {IDLE_MS} ms before the later frame, and none between "
        "the two. Load-phase shifts happen before the walk's first marker (skeleton "
        "to data). Crops show the moved edges in red.",
        "",
        "Edges match by shape, so text that changes width shows as a resize. Live "
        "clocks and spinners can show up here too; check the crop before filing one.",
        "",
        "## Summary",
        "",
        "| Page | Frames | Idle pairs | Walk shifts | Load shifts | Run |",
        "|---|---|---|---|---|---|",
    ]
    for panel, result in results.items():
        walk = sum(1 for g in result["groups"] if g["phase"] == "walk")
        load = sum(1 for g in result["groups"] if g["phase"] == "load")
        lines.append(f"| {panel} | {result['frames']} | {result['idle_pairs']} | {walk} | {load} | {result['run']} |")
    lines.append("")
    for panel, result in results.items():
        if result["log"]:
            lines += [f"### {panel}: binary notes", ""]
            lines += [f"- `{note}`" for note in result["log"]]
            lines.append("")
    for panel, result in results.items():
        groups = result["groups"]
        if not groups:
            continue
        lines += [f"## {panel}", ""]
        for index, group in enumerate(groups[:MAX_GROUPS_IN_REPORT], 1):
            span = (
                f"frames {group['frame_from']}-{group['frame_to']}"
                if group["frame_from"] != group["frame_to"]
                else f"frame {group['frame_to']}"
            )
            since = "no input yet" if group["since_to"] is None else f"{group['since_to']} ms after last input"
            lines += [
                f"### {index}. {panel}: {span} ({group['phase']}, t={group['t_from']}-{group['t_to']} ms, {since})",
                "",
                f"Marker: `{group['marker']}`. Frame pairs: {group['count']}. Largest step: {group['max_dc']} px.",
                "",
            ]
            tag = translated(group["first_pairs"])
            if tag:
                lines.append(f"- {tag}")
            for pair in group["first_pairs"][:8]:
                lines.append(f"- {describe(pair)}")
            if len(group["first_pairs"]) > 8:
                lines.append(f"- ...and {len(group['first_pairs']) - 8} more edges")
            if group["count"] > 1:
                lines += ["", "Last pair in the group:", ""]
                for pair in group["last_pairs"][:8]:
                    lines.append(f"- {describe(pair)}")
            lines += [
                "",
                f"![before/after](shifts/{group['crop_first']})",
                "",
                f"Full frames: [before](shifts/{group['full_before']}), [after](shifts/{group['full_after']}).",
                "",
            ]
        if len(groups) > MAX_GROUPS_IN_REPORT:
            lines += [
                f"{len(groups) - MAX_GROUPS_IN_REPORT} more groups in shifts.json.",
                "",
            ]
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--out", type=Path, default=Path("/tmp/FrameWalk"))
    parser.add_argument("--pages", default="", help="comma list of panel names; default all")
    parser.add_argument("--timeout", type=int, default=300, help="seconds per page")
    parser.add_argument("--skip-run", action="store_true", help="analyse frames already in OUT/frames")
    parser.add_argument("--keep-frames", action="store_true", help="keep OUT/frames after analysis")
    args = parser.parse_args()

    if not args.skip_run and not args.binary.exists():
        sys.exit(f"no binary at {args.binary}; run `cargo build --features snapshot` in apps/settings first")
    pages = [name for name in args.pages.split(",") if name] or panels()
    out = args.out
    shifts = out / "shifts"
    shutil.rmtree(shifts, ignore_errors=True)
    shifts.mkdir(parents=True, exist_ok=True)

    results: dict[str, dict] = {}
    for panel in pages:
        run = "skipped"
        if not args.skip_run:
            run = run_page(args.binary, panel, out, args.timeout)
            print(f"{panel}: {run}", flush=True)
        frames = out / "frames" / panel
        if not frames.is_dir():
            # The binary ran but wrote no frames: it is not the build with the
            # frame knobs, or it exited before the walk started. Say so.
            run = f"{run}; no frames recorded"
            results[panel] = {
                "run": run,
                "frames": 0,
                "idle_pairs": 0,
                "groups": [],
                "log": notes(out, panel),
            }
            continue
        result = analyse(panel, frames, shifts)
        result["run"] = run
        result["log"] = notes(out, panel)
        results[panel] = result
        print(
            f"{panel}: {result['frames']} frames, {len(result['groups'])} shift groups",
            flush=True,
        )
        if not args.keep_frames:
            shutil.rmtree(frames, ignore_errors=True)

    (out / "shifts.json").write_text(json.dumps(results, indent=2))
    (out / "report.md").write_text(render(results, args.binary))
    print(f"report: {out / 'report.md'}", flush=True)


if __name__ == "__main__":
    main()
