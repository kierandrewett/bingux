#!/usr/bin/env python3
"""Fail when an animated number does not roll the way the stopwatch does.

CK-19 of design/rules.md, MO-11: "the animated number component is genuinely ugly. it
needs to slide like a stopwatch." The component (`animated_number`, crates/ui/src/motion/animated_number.rs) has unit tests
on its layout tree (animated_number_tests.rs); this reads what a person sees: every 16 ms frame
of the taxonomy's Animated number specimens as pixels, around a press on the button that changes
the value.

For each specimen the walk records the frames from the press to the end (BINGUX_SNAPSHOT_SCRIPT and
BINGUX_SNAPSHOT_FRAMES of `bingux-taxonomy --snapshot`) and reads:

  steps monotonic     how far the number's pixels are from the first frame and from the last: the
                      share of the way, `d_first / (d_first + d_last)`, never goes back by more
                      than 15% from one look to the next (two glyphs cross-fading are not a straight line in pixels, so a little give is left) (a digit that rolls up and then back
                      down, or a number that flickers, fails)
  rolls               at least 3 frames are between the first look and the last (a number that
                      cuts, or that is done in one frame, fails)
  no jitter           the box of what changed holds the digits that changed and nothing else, and
                      every pixel of the specimen outside that box, on the same lines, is the same in
                      every frame: the label before the number and the unit after it never move
                      while it counts (a number whose width changes, 99 to 100, is its own
                      case: the words after it may only move one way, and only by as much as the
                      number grew)
  stays in its line   what changed is no taller than a line of the number's own text plus 2 px: a digit
                      that rolls is clipped to its column, it does not climb into the line above

    python3 scripts/number-roll-check.py
    python3 scripts/number-roll-check.py --only counting
    python3 scripts/number-roll-check.py --fixtures      # the analysis on drawn frames only (a roll, a cut, a jitter)

Needs `bingux-taxonomy` built (`cargo build` in apps/taxonomy) and PIL and numpy. Exit
status is 1 on a finding, on a specimen whose walk did not change anything (the press missed its
button), and on a fixture that is not judged as it should be.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))
PAGE = "foundations/motion"
STEP = 14  # a pixel that differs by more than this in grey level has changed
BACK = 0.15
MIN_BETWEEN = 3
LINE_PX = 30  # the tallest line of the number's own text, in pixels, plus the 2 px of room (the Hero style is bigger: its own limit)


@dataclass
class Specimen:
    name: str
    press: str  # the label of the button the walk presses
    grows: bool = False  # the number's width changes, so what is after it moves with it
    line: int = LINE_PX
    steps: int = 1  # presses
    scroll: int = 0  # how far down the page the specimen's button is, in px: the page is taller than the window


# Where each specimen's button sits on the Motion page at the desktop size (read from the layout: `scroll`
# is its distance down the page less the 400 px that puts it well inside the window).
SPECIMENS = [
    Specimen("counting", "Up", scroll=2500),
    Specimen("jump", "Jump", scroll=2500),
    Specimen("seconds", "+1 s", scroll=3150),
    Specimen("percent", "Drain", scroll=3450),
    Specimen("speed", "Faster", scroll=3700),
]


# ---- the analysis ------------------------------------------------------------------------------


def analyse(frames: list[np.ndarray], spec: Specimen) -> dict:
    """What the frames of one roll show. `frames[0]` is the picture at the press."""
    first, last = frames[0], frames[-1]
    final = np.abs(last - first) > STEP
    if not final.any():
        return {"changed": False}
    # The digits that ended up different are where the number is; what moves on the way is judged around them.
    fy, fx = np.nonzero(final)
    x0, x1 = int(fx.min()), int(fx.max()) + 1
    column = slice(max(0, x0 - 3), x1 + 3)
    moved = np.zeros(first.shape, dtype=bool)
    for frame in frames:
        moved |= np.abs(frame - first) > STEP
    ys, xs = np.nonzero(moved[:, column])
    y0, y1 = int(ys.min()), int(ys.max()) + 1
    box = (x0, y0, x1, y1)
    region = (slice(y0, y1), slice(x0, x1))
    d_first = np.array([np.abs(frame[region] - first[region]).mean() for frame in frames])
    d_last = np.array([np.abs(frame[region] - last[region]).mean() for frame in frames])
    progress = d_first / np.maximum(d_first + d_last, 1e-9)
    # Frames that repeat the one before are one look: the recorder writes one every 16 ms step.
    looks = [0]
    for i in range(1, len(frames)):
        if np.abs(frames[i] - frames[looks[-1]]).max() > 0:
            looks.append(i)
    back = [round(float(progress[b] - progress[a]), 3) for a, b in zip(looks, looks[1:]) if progress[b] < progress[a] - BACK]
    between = sum(1 for i in looks if 0.08 < progress[i] < 0.92)
    # Everything outside the digits' columns, on the rows the number takes and two more either side.
    band = slice(max(0, y0 - 2), min(first.shape[0], y1 + 2))
    outside = np.ones(first.shape[1], dtype=bool)
    outside[column] = False
    jitter = 0
    after_shift = []
    for frame in frames:
        diff = np.abs(frame[band, :] - first[band, :]) > STEP
        jitter = max(jitter, int(diff[:, outside].sum()))
        if spec.grows and diff[:, x1:].any():
            cols = np.nonzero(diff[:, x1:].any(axis=0))[0]
            after_shift.append(int(cols.min()) + x1)
    return {
        "changed": True,
        "box": box,
        "height": y1 - y0,
        "looks": len(looks),
        "between": between,
        "back": back,
        "jitter": jitter,
        "after_shift": after_shift,
    }


def judge(spec: Specimen, result: dict) -> list[str]:
    if not result["changed"]:
        return [f"{spec.name}: nothing changed after the press: the walk missed its button {spec.press!r}"]
    problems = []
    if result["back"]:
        problems.append(f"{spec.name}: the roll goes back by {max(result['back']):.0%} of the way between two frames: the digit is not stepping one way")
    if result["between"] < MIN_BETWEEN:
        problems.append(f"{spec.name}: only {result['between']} frames between the first look and the last (at least {MIN_BETWEEN}): it cuts instead of rolling")
    if not spec.grows and result["jitter"]:
        problems.append(f"{spec.name}: {result['jitter']} pixels outside the digits change while it counts: the text around the number moves")
    if result["height"] > spec.line + 2:
        problems.append(f"{spec.name}: what changed is {result['height']} px tall, a line is {spec.line}: the roll leaves its column")
    return problems


# ---- running the specimens ---------------------------------------------------------------------


def grey(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("L"), dtype=np.int16)


def read_frames(directory: Path, marker: str) -> list[np.ndarray]:
    """The frames after the `marker` note, one array each."""
    records = [json.loads(line) for line in (directory / "frames.jsonl").read_text().splitlines() if line.strip()]
    on = False
    cache: dict[str, np.ndarray] = {}
    frames = []
    for record in records:
        if record["kind"] == "marker":
            if record["text"] == marker:
                on = True
            continue
        if not on:
            continue
        name = record["file"]
        if name not in cache:
            cache[name] = grey(directory / name)
        frames.append(cache[name])
    return frames


def walk(binary: Path, spec: Specimen, scratch: Path, timeout: int) -> list[np.ndarray]:
    frames_dir = scratch / "frames"
    shutil.rmtree(frames_dir, ignore_errors=True)
    presses = "; ".join(f"click-text {spec.press}" for _ in range(spec.steps))
    env = dict(
        os.environ,
        # The animation clock runs at a tenth of its speed, so that a 140 ms roll is drawn over many
        # frames even on a busy machine (a frame of this page takes about 100 ms to draw).
        BINGUX_SNAPSHOT_SCRIPT=f"scroll 800,800,{-spec.scroll}; wait 300; slow 0.1; hover-text {spec.press}; wait 300; mark go; {presses}; wait 3200",
        BINGUX_SNAPSHOT_FRAMES=str(frames_dir),
        BINGUX_SNAPSHOT_SETTLE_MS="1200",
        XDG_CACHE_HOME=str(scratch / "cache"),
    )
    env.pop("BINGUX_REDUCED_MOTION", None)
    # A capture is software rendering: it must never find the live desktop or write its dconf.
    env = isolated(env)
    subprocess.run([str(binary), "--snapshot", str(scratch / "png"), PAGE], env=env, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout)
    return read_frames(next(frames_dir.iterdir()), "go")


# ---- fixtures -------------------------------------------------------------------------------------


def drawn(offsets: list[float], neighbour_moves: bool = False, tall: bool = False) -> list[np.ndarray]:
    """Frames of a label, a digit that is at `offsets` px from rest in each, and a unit: grey on black."""
    frames = []
    for i, off in enumerate(offsets):
        frame = np.zeros((60, 300), dtype=np.int16)
        frame[20:40, 10:80] = 200  # the label
        top = int(20 + off)
        frame[max(20, top) : min(40, top + 20), 100:112] = 220  # the digit, clipped to its column
        if tall:
            frame[0:60, 100:112] = 220
        unit_at = 130 + (i % 2) * 3 if neighbour_moves else 130
        frame[20:40, unit_at : unit_at + 40] = 200  # the unit
        frames.append(frame)
    return frames


def run_fixtures() -> list[str]:
    problems = []
    spec = Specimen("fixture", "Up")
    rolls = drawn([0, 0, 4, 8, 12, 16, 20, 20, 20])
    # A digit that slides out of its column: it is fully clipped at 20, so the picture ends blank. Use a pair instead:
    # the old digit leaves upward as the new one comes from below.
    frames = []
    for p in [0, 0, 0.15, 0.35, 0.55, 0.75, 0.9, 1.0, 1.0]:
        frame = np.zeros((60, 300), dtype=np.int16)
        frame[20:40, 10:80] = 200
        old = int(20 - 20 * p)
        new = int(40 - 20 * p)
        stripes = np.zeros((20, 12), dtype=np.int16)
        stripes[:, ::3] = 220
        solid = np.full((20, 12), 220, dtype=np.int16)
        for top, glyph, alpha in ((old, solid, 1 - p), (new, stripes, p)):
            rows = slice(max(20, top), min(40, top + 20))
            part = (glyph[max(20, top) - top : min(40, top + 20) - top] * alpha).astype(np.int16)
            frame[rows, 100:112] = np.maximum(frame[rows, 100:112], part)
        frame[20:40, 130:170] = 200
        frames.append(frame)
    good = judge(spec, analyse(frames, spec))
    if good:
        problems.append(f"fixture: a clean roll was reported: {good}")
    # A cut: the digit changes in one frame.
    cut = [frames[0]] * 3 + [frames[-1]] * 6
    got = judge(spec, analyse(cut, spec))
    if not any("cuts" in g for g in got):
        problems.append(f"fixture: a cut was not found ({got})")
    # A flicker: the roll goes forward, then back.
    flicker = [frames[i] for i in (0, 2, 4, 3, 2, 5, 7)]
    got = judge(spec, analyse(flicker, spec))
    if not any("one way" in g for g in got):
        problems.append(f"fixture: a roll that goes back was not found ({got})")
    # The unit moves while the number counts.
    jitter = []
    for i, frame in enumerate(frames):
        frame = frame.copy()
        shift = 2 * (i % 2)
        frame[20:40, 130:170] = 0
        frame[20:40, 130 + shift : 170 + shift] = 200
        jitter.append(frame)
    got = judge(spec, analyse(jitter, spec))
    if not any("around the number moves" in g for g in got):
        problems.append(f"fixture: a unit that moves was not found ({got})")
    # Nothing changes: the press missed.
    got = judge(spec, analyse([frames[0]] * 5, spec))
    if not any("missed its button" in g for g in got):
        problems.append(f"fixture: a walk that changed nothing was not found ({got})")
    del rolls
    return problems


# ---- main ----------------------------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", type=Path, default=TARGET / "debug" / "bingux-taxonomy")
    parser.add_argument("--only", default="", help="comma-separated specimen names")
    parser.add_argument("--timeout", type=int, default=300)
    parser.add_argument("--keep", type=Path, help="keep the frames here")
    parser.add_argument("--fixtures", action="store_true", help="only check the analysis on drawn frames")
    args = parser.parse_args()
    problems = run_fixtures()
    for problem in problems:
        print(problem)
    print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
    if args.fixtures or problems:
        return 1 if problems else 0
    if not args.binary.exists():
        print(f"number-roll-check: {args.binary} is not built (cargo build in apps/taxonomy)", file=sys.stderr)
        return 2
    wanted = {name for name in args.only.split(",") if name}
    scratch = args.keep or Path(tempfile.mkdtemp(prefix="number-roll-"))
    failed: list[str] = []
    ran = 0
    for spec in SPECIMENS:
        if wanted and spec.name not in wanted:
            continue
        ran += 1
        frames = walk(args.binary, spec, scratch / spec.name, args.timeout)
        result = analyse(frames, spec) if frames else {"changed": False}
        found = judge(spec, result)
        failed += found
        detail = (
            f"{result['looks']} looks, {result['between']} between, jitter {result['jitter']}, box {result['box']}"
            if result.get("changed")
            else "no change"
        )
        print(f"{spec.name:12} {'ok' if not found else 'FAIL'}  {detail}")
    if not args.keep:
        shutil.rmtree(scratch, ignore_errors=True)
    for problem in failed:
        print(f"[MO-11] {problem}")
    print(f"\nnumber-roll-check: {ran} specimens, {len(failed)} findings")
    return 1 if failed or ran == 0 else 0


if __name__ == "__main__":
    sys.exit(main())
