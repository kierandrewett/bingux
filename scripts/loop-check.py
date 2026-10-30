#!/usr/bin/env python3
"""Fail when an indeterminate bar or a spinner stands still.

An indeterminate bar once sat still in every window while its animation value
kept changing (its segment was shifted with `offset_x`, which moves a box's
children, and the segment has none). Nothing that looks at the animation can
see that, so this looks at pixels: it renders the taxonomy's Progress bar and
Spinner page twice, with a different settle time, and diffs the two frames.
A loop that runs is in another place the second time.

The page is rendered by `bingux-taxonomy --snapshot` (no compositor), and so
are the states the page's specimens script: an indeterminate bar in a dialog
and in a popup. The changed pixels are grouped into boxes and classified by
shape: a bar is wide and at most 10 px tall, a spinner is at most 64 px and not more than twice as wide as tall or the
other way round (a turning arc changes only part of its ring). It runs in both motion modes, because under BINGUX_REDUCED_MOTION=1 the
bar breathes in place and the spinner turns slowly: still visible activity, and
a static bar would look frozen.

    python3 scripts/loop-check.py
    python3 scripts/loop-check.py --binary /path/to/bingux-taxonomy --settle 700,1200
    python3 scripts/loop-check.py --all          # every taxonomy page and state, loops and idle motion
    python3 scripts/loop-check.py --fixtures     # the classifier on drawn bars, spinners and a clock only

`--all` (CK-19, MO-10, BH-05) is the same two renders of every page, desktop, phone and
every scripted state, not only the Progress page. Each pair of captures is diffed and the changed
boxes are classified: a bar or a spinner that moved is a loop that runs (counted), anything
else that changed between the two settle times is motion nobody asked for on a settled page
(a clock, a number that counts, a skeleton that never lands, an animation that does not end)
and is a finding unless ALLOWED_MOTION names the page and why, as does a page that is declared
to hold loops (`LOOP_PAGES`) and shows fewer than it should. The Progress page and its dialog and
popup states are still held to the counts above.

Exits 1 when a frame pair has no changed bar (or spinner) where there must be
one. The real-window check is the same idea with grim: record the bar's region
of a running window for a few seconds and require frames that differ
(docs/bingux-ui.md, "Progress bars and spinners").

Build first: `cargo build` in apps/taxonomy, with CARGO_TARGET_DIR set
as usual. Needs PIL and numpy.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BINARY = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target")) / "debug/bingux-taxonomy"
PAGE = "controls/feedback"
# A pixel that differs by more than this (summed over channels) between the two frames has changed.
# Low, because a breathing fill under reduced motion changes by a few levels between two phases.
THRESHOLD = 6

# What each rendered state must show moving, as (bars, spinners) at least. The
# page holds each live specimen twice (desktop and phone density): two
# indeterminate bars and, in the same mode, three spinners and a captioned one.
# Only the desktop page is checked: the 390 x 800 phone capture shows nothing
# below the first specimen.
PAGE_MIN = (2, 4)
# Scripted states: a dialog and a popup with an indeterminate bar. (The popup's
# spinner is not required: its changed pixels are two thin arcs, not one box the
# shape test can tell from a bar's edge. The page's spinners are.)
STATES = {
    "indeterminate-in-a-dialog": (1, 0),
    "indeterminate-in-a-popup": (1, 0),
}


def scratch_root() -> str:
    """Where the captures go. Not /tmp: `tmp-purge.timer` empties it every five minutes and a
    `--all` run takes twenty, so its first renders were deleted before the second pair was made
    ("0 captures: nothing was checked")."""
    tmp = os.environ.get("TMPDIR", "")
    return tmp if tmp and not tmp.startswith("/tmp") else "/var/tmp"


def render(binary: Path, out: Path, settle_ms: int, reduced: bool, page: str | None = PAGE) -> None:
    """One render of `page` (every page when `page` is None) at one settle time."""
    out.mkdir(parents=True, exist_ok=True)
    # A capture is software rendering: it must never find the live desktop or write its dconf.
    env = isolated(dict(os.environ, BINGUX_SNAPSHOT_SETTLE_MS=str(settle_ms)))
    if reduced:
        env["BINGUX_REDUCED_MOTION"] = "1"
    else:
        env.pop("BINGUX_REDUCED_MOTION", None)
    subprocess.run([str(binary), "--snapshot", str(out), *([page] if page else [])], env=env, check=True, stdout=subprocess.DEVNULL)


def boxes(mask: np.ndarray) -> list[tuple[int, int]]:
    """The (width, height) of each group of changed pixels, grouped within 2 px of each other."""
    points = {(int(x), int(y)) for y, x in zip(*np.nonzero(mask))}
    seen: set[tuple[int, int]] = set()
    found = []
    for start in sorted(points):
        if start in seen:
            continue
        seen.add(start)
        stack = [start]
        x0 = x1 = start[0]
        y0 = y1 = start[1]
        while stack:
            x, y = stack.pop()
            x0, x1, y0, y1 = min(x0, x), max(x1, x), min(y0, y), max(y1, y)
            for dy in range(-2, 3):
                for dx in range(-2, 3):
                    near = (x + dx, y + dy)
                    if near in points and near not in seen:
                        seen.add(near)
                        stack.append(near)
        found.append((x1 - x0 + 1, y1 - y0 + 1))
    return found


def block_boxes(mask: np.ndarray, block: int = 4) -> list[tuple[int, int, int]]:
    """(width, height, pixels) of each group of changed pixels, grouped within a block of each other.

    The same grouping as `boxes` on a grid of `block` px squares, so that a page that changed
    all over is grouped in time: a group's size is right to within one block.
    """
    height, width = mask.shape
    gh, gw = -(-height // block), -(-width // block)
    padded = np.zeros((gh * block, gw * block), dtype=bool)
    padded[:height, :width] = mask
    grid = padded.reshape(gh, block, gw, block).any(axis=(1, 3))
    counts = padded.reshape(gh, block, gw, block).sum(axis=(1, 3))
    seen = np.zeros_like(grid)
    found = []
    for gy, gx in zip(*np.nonzero(grid)):
        if seen[gy, gx]:
            continue
        stack = [(int(gy), int(gx))]
        seen[gy, gx] = True
        y0 = y1 = int(gy)
        x0 = x1 = int(gx)
        pixels = 0
        while stack:
            y, x = stack.pop()
            y0, y1, x0, x1 = min(y0, y), max(y1, y), min(x0, x), max(x1, x)
            pixels += int(counts[y, x])
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    ny, nx = y + dy, x + dx
                    if 0 <= ny < gh and 0 <= nx < gw and grid[ny, nx] and not seen[ny, nx]:
                        seen[ny, nx] = True
                        stack.append((ny, nx))
        found.append(((x1 - x0 + 1) * block, (y1 - y0 + 1) * block, pixels))
    return found


def classify(width: int, height: int) -> str:
    """`bar`, `spinner` or `other` for one group of changed pixels."""
    if width >= 24 and height <= 10:
        return "bar"
    if 8 <= width <= 64 and 8 <= height <= 64 and 0.5 <= width / height <= 2.0:
        return "spinner"
    return "other"


def difference(first: Path, second: Path) -> np.ndarray:
    a = np.asarray(Image.open(first).convert("RGB"), dtype=np.int16)
    b = np.asarray(Image.open(second).convert("RGB"), dtype=np.int16)
    if a.shape != b.shape:
        # A page's height is measured from where its labels settle, so two renders can differ by a few
        # pixels of height: compare what they share, from the top.
        rows, cols = min(a.shape[0], b.shape[0]), min(a.shape[1], b.shape[1])
        a, b = a[:rows, :cols], b[:rows, :cols]
    return np.abs(a - b).sum(axis=2) > THRESHOLD


def moving(first: Path, second: Path) -> tuple[int, int]:
    """How many bars and spinners differ between two renders of one state."""
    mask = difference(first, second)
    bars = spinners = 0
    for width, height in boxes(mask):
        kind = classify(width, height)
        bars += kind == "bar"
        spinners += kind == "spinner"
    return bars, spinners


# `--all`: pages that hold loops, with the (bars, spinners) their desktop capture must show moving, and the pages
# whose settled capture may change between two settle times for a reason that is not a loop: rule IDs and why.
LOOP_PAGES = {"controls_feedback": PAGE_MIN}
FILES_SPECIMEN = (
    "a Files specimen lists a fake folder and its icons and thumbnails are drawn on workers after the page is up: "
    "what changes between 700 and 1200 ms is a row's icon arriving, not an animation"
)
ALLOWED_MOTION: dict[str, str] = {
    "files_address-bar-and-places": FILES_SPECIMEN,
    "files_file-manager-and-chooser": FILES_SPECIMEN,
    "files_file-views": FILES_SPECIMEN,
    "files_file-preview": FILES_SPECIMEN + "; a film's first frame and a song's poster arrive the same way",
    "data_charts": "a chart draws itself in over DRAW_MS (700 ms), which is still running at the first settle time; the captures the taxonomy takes settle for 900 ms",
    "controls_feedback": "the page's live specimens move by design: skeletons pulse (PULSE_MS), spinners carry captions and a progress bar fills on a timer",
}


def all_pages(args: argparse.Namespace, first_ms: int, second_ms: int) -> int:
    """Two renders of every taxonomy page and state: loops must run, nothing else may."""
    problems: list[str] = []
    seen_allowed: set[str] = set()
    counts = {"captures": 0, "bars": 0, "spinners": 0, "idle": 0}
    for mode, reduced in (("normal", False), ("reduced", True)):
        one, two = args.out / mode / "all-a", args.out / mode / "all-b"
        render(args.binary, one, first_ms, reduced, None)
        render(args.binary, two, second_ms, reduced, None)
        for file in sorted(one.glob("*.png")):
            other = two / file.name
            if not other.exists():
                problems.append(f"{mode}: {file.name} was rendered once, not twice")
                continue
            counts["captures"] += 1
            stem = file.stem.split(".")[0]
            mask = difference(file, other)
            if not mask.any():
                kinds = {"bar": 0, "spinner": 0, "other": 0}
            else:
                kinds = {"bar": 0, "spinner": 0, "other": 0}
                for width, height, _ in block_boxes(mask):
                    kinds[classify(width, height)] += 1
            counts["bars"] += kinds["bar"]
            counts["spinners"] += kinds["spinner"]
            if kinds["other"]:
                if stem in ALLOWED_MOTION:
                    seen_allowed.add(stem)
                else:
                    counts["idle"] += 1
                    problems.append(
                        f"[BH-05] {mode}: {file.name}: {kinds['other']} region(s) changed between settle times {first_ms} and {second_ms} ms "
                        "that are not a bar or a spinner: motion on a page that is meant to be still"
                    )
            if file.name == "controls_feedback.png":
                want = LOOP_PAGES[stem]
                if kinds["bar"] < want[0] or kinds["spinner"] < want[1]:
                    problems.append(f"[MO-10] {mode}: {file.name}: {kinds['bar']} bars and {kinds['spinner']} spinners moved, wanted {want[0]} and {want[1]}")
    for stem in sorted(set(ALLOWED_MOTION) - seen_allowed):
        problems.append(f"unused ALLOWED_MOTION entry: {stem}: {ALLOWED_MOTION[stem]}")
    print(f"loop-check --all: {counts['captures']} captures, {counts['idle']} with motion that is not a loop and not allowed")
    if counts["captures"] == 0:
        problems.append("no captures: nothing was checked")
    for problem in problems:
        print(f"  {problem}")
    return 1 if problems else 0


def fixtures() -> list[str]:
    """The classifier on drawn pictures: a bar and a spinner that move, a clock that ticks, a still page."""
    problems = []
    base = np.zeros((200, 300), dtype=bool)
    bar = base.copy()
    bar[50:54, 20:140] = True
    spin = base.copy()
    spin[100:130, 200:230] = True
    clock = base.copy()
    clock[10:40, 10:290] = True
    if [classify(w, h) for w, h, _ in block_boxes(bar)] != ["bar"]:
        problems.append("fixture: a changed bar 120 x 4 was not classified as a bar")
    if [classify(w, h) for w, h, _ in block_boxes(spin)] != ["spinner"]:
        problems.append("fixture: a changed 30 x 30 box was not classified as a spinner")
    if [classify(w, h) for w, h, _ in block_boxes(clock)] != ["other"]:
        problems.append("fixture: a changed 280 x 30 strip was not classified as other motion")
    if block_boxes(base):
        problems.append("fixture: a still page has changed regions")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--out", type=Path, help="keep the captures here (default: a scratch directory removed at the end)")
    parser.add_argument("--settle", default="700,1200", help="two settle times in ms; the frames are diffed")
    parser.add_argument("--all", action="store_true", help="every taxonomy page and state: loops must run and nothing else may move")
    parser.add_argument("--fixtures", action="store_true", help="only check the classifier on drawn pictures")
    args = parser.parse_args()
    problems = fixtures()
    for problem in problems:
        print(problem)
    print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
    if args.fixtures or problems:
        return 1 if problems else 0
    if not args.binary.exists():
        print(f"loop-check: {args.binary} is not built (cargo build in apps/taxonomy)", file=sys.stderr)
        return 2
    first_ms, second_ms = (int(part) for part in args.settle.split(","))
    scratch = None
    if args.out is None:
        scratch = tempfile.mkdtemp(prefix="loop-check-", dir=scratch_root())
        args.out = Path(scratch)
    try:
        return run(args, first_ms, second_ms)
    finally:
        if scratch:
            shutil.rmtree(scratch, ignore_errors=True)


def run(args: argparse.Namespace, first_ms: int, second_ms: int) -> int:
    if args.all:
        return all_pages(args, first_ms, second_ms)

    failures = []
    for mode, reduced in (("normal", False), ("reduced", True)):
        one, two = args.out / mode / "a", args.out / mode / "b"
        render(args.binary, one, first_ms, reduced)
        render(args.binary, two, second_ms, reduced)
        checks = [("page", sorted(one.glob("controls_feedback.png")), PAGE_MIN)]
        for state, minimum in STATES.items():
            checks.append((state, sorted(one.glob(f"*{state}*.png")), minimum))
        for name, files, (want_bars, want_spinners) in checks:
            if not files:
                failures.append(f"{mode}: no render of {name}")
                continue
            bars, spinners = moving(files[0], two / files[0].name)
            verdict = "ok" if bars >= want_bars and spinners >= want_spinners else "STILL"
            print(
                f"{mode:8} {name:34} moving bars {bars} (want {want_bars}+), "
                f"spinners {spinners} (want {want_spinners}+)  {verdict}"
            )
            if verdict != "ok":
                failures.append(f"{mode}: {name} did not move between settle times {first_ms} and {second_ms} ms")
    if failures:
        print("\nloop-check: something stood still:")
        for failure in failures:
            print(f"  {failure}")
        return 1
    print("\nloop-check: every indeterminate bar and spinner moved.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
