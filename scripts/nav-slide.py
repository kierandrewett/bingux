#!/usr/bin/env python3
"""Fail when a page transition does not slide.

A subpage slides in on a push and slides out on Back. Kieran saw the first Back
after opening a subpage cut straight to the list, and the later ones animate.
This records every frame of a walk and counts the frames that are between the
page before and the page after: a slide has many, a cut has none.

For each subpage the walk opens it from its parent, waits, presses Back,
waits, then does the same a second time:

    push 1, pop 1 (the first Back), push 2, pop 2

A transition fails when it has fewer than --min-frames (1) in-between frames, and
the first Back fails when it has fewer than half of the second's. The slide
is timed by the clock, so a busy machine draws fewer frames of it (about 18 at
16 ms, a handful at 120 ms) but never none: a transition with no in-between
frame is a cut.

    python3 scripts/nav-slide.py
    python3 scripts/nav-slide.py --subpages "Wi-Fi hotspot,Proxy"

Build first: `cargo build --features snapshot` in packages/bingux-settings.
Needs PIL and numpy.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target")) / "debug"
FAKES = {
    "BINGUX_SETTINGS_FAKE_NETWORK": "online",
    "BINGUX_SETTINGS_FAKE_MODEM": "ok",
    "BINGUX_SETTINGS_FAKE_DISPLAYS": "1",
    "BINGUX_SETTINGS_FAKE_BLUETOOTH": "1",
    "BINGUX_SETTINGS_FAKE_INPUT": "1",
}
# The Back button of the header. The window is made tall enough that every row
# of the Network page is on screen: `click-text` clicks where a label is drawn.
BACK = "307,24"
SIZE = "1000x1600"
# Rows on the Network page that open a subpage.
SUBPAGES = ["Wi-Fi hotspot", "Proxy"]
SEGMENTS = ["push 1", "pop 1", "push 2", "pop 2"]
# Mean grey-level difference from both the first and the last frame of a
# transition for a frame to count as in between.
BETWEEN = 1.5


def script(label: str) -> str:
    steps = ["mark idle start", "wait 2000"]
    for n in (1, 2):
        steps += [f"mark push {n}", f"click-text {label}", "wait 2500"]
        steps += [f"mark pop {n}", f"click {BACK}", "wait 2500"]
    steps += ["mark end"]
    return "; ".join(steps)


def run(binary: Path, label: str, out: Path, timeout: int) -> Path:
    slug = label.lower().replace(" ", "-")
    frames = out / "frames"
    shutil.rmtree(frames / "network", ignore_errors=True)
    env = {
        **os.environ,
        **FAKES,
        "BINGUX_SETTINGS_SNAPSHOT_FRAMES": str(frames),
        "BINGUX_SETTINGS_SNAPSHOT_SIZE": SIZE,
        "BINGUX_SETTINGS_SNAPSHOT_SCRIPT": script(label),
    }
    with tempfile.TemporaryDirectory(prefix="nav-slide-cache-") as cache:
        env["XDG_CACHE_HOME"] = cache
        subprocess.run(
            [str(binary), "--snapshot", str(out / "png"), "network"],
            env=env, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT, timeout=timeout, check=True,
        )
    kept = out / slug
    shutil.rmtree(kept, ignore_errors=True)
    shutil.move(str(frames / "network"), str(kept))
    return kept


def grey(path: Path) -> np.ndarray:
    image = Image.open(path).convert("L")
    image = image.resize((image.width // 4, image.height // 4))
    return np.asarray(image, dtype=np.int16)


def segments(directory: Path) -> dict[str, list[np.ndarray]]:
    """The frames of each transition: from its marker to the next one."""
    records = [json.loads(line) for line in (directory / "frames.jsonl").read_text().splitlines() if line.strip()]
    found: dict[str, list[np.ndarray]] = {}
    current: str | None = None
    cache: dict[str, np.ndarray] = {}
    for record in records:
        if record["kind"] == "marker":
            # The runner also marks every click; only our own markers count.
            if record["text"] in SEGMENTS:
                current = record["text"]
                found.setdefault(current, [])
            elif record["text"] in ("idle start", "end"):
                current = None
            continue
        if current is None:
            continue
        name = record["file"]
        if name not in cache:
            cache[name] = grey(directory / name)
        found[current].append(cache[name])
    return found


def between(frames: list[np.ndarray]) -> int:
    if len(frames) < 3:
        return 0
    first, last = frames[0], frames[-1]
    # The transition ends where the picture stops changing, so ignore the wait
    # that follows it: use the last distinct frame as `last`.
    last = frames[-1]
    count = 0
    for frame in frames[1:-1]:
        if np.abs(frame - first).mean() > BETWEEN and np.abs(frame - last).mean() > BETWEEN:
            count += 1
    return count


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", type=Path, default=TARGET / "bingux-settings")
    parser.add_argument("--subpages", default=",".join(SUBPAGES), help="labels of rows on the Network page")
    parser.add_argument("--min-frames", type=int, default=1)
    parser.add_argument("--out", type=Path, default=Path("/tmp/NavSlide"))
    parser.add_argument("--timeout", type=int, default=240)
    args = parser.parse_args()
    if not args.binary.exists():
        sys.exit(f"no binary at {args.binary}; run `cargo build --features snapshot` in packages/bingux-settings")
    failed = 0
    print(f"{'subpage':16} " + " ".join(f"{name:>8}" for name in SEGMENTS))
    for label in [name for name in args.subpages.split(",") if name]:
        directory = run(args.binary, label, args.out, args.timeout)
        counts = {name: between(frames) for name, frames in segments(directory).items()}
        problems = [name for name in SEGMENTS if counts.get(name, 0) < args.min_frames]
        first, second = counts.get("pop 1", 0), counts.get("pop 2", 0)
        if second and first * 2 < second and "pop 1" not in problems:
            problems.append("pop 1")
        failed += bool(problems)
        row = " ".join(f"{counts.get(name, 0):8}" for name in SEGMENTS)
        print(f"{label:16} {row}" + (f"   FAIL: {', '.join(problems)} did not slide" if problems else ""))
    print(f"frames kept in {args.out}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
