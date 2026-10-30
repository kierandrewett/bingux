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
the first Back fails when it has fewer than half of the second's.

Symmetry (CK-09, MO-01, MO-02): the way along a slide is read from the pixels as
p(t) = d_first / (d_first + d_last), the distance of each frame from the first and from the
last picture of the transition, against the time since the transition began as a share of
how long it took. A close that is the open played backwards has p_pop(t) = 1 - p_push(1 - t),
whatever the picture, so the two are compared at 12 points: they must agree within
--curve-tolerance (0.12) and take as
long to within --time-tolerance (40%). This is a note, and fails only with --strict-symmetry: the
progress is read from every pixel, so whatever else changes while the page slides (the subpage's data
landing, a hover, the list behind it reloading) bends the curve; on the Network page the pop measured
about 18 frames against the push's 11 with the Rust curve the same both ways. The exact check is
`nav/slide_users_tests.rs` (with `shell::navigation::tests` and `shell::sheet_motion::tests`). Time is counted in frames: the harness's animation clock
advances 16 ms for each frame it draws, however long the machine takes to draw it. The slide
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

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target")) / "debug"
# The rules of design/rules.json this check holds, and the words that tell the kinds of finding apart
# in its output (read by scripts/rules-check.py).
RULE_IDS = {"slide": ["MO-01"], "symmetry": ["MO-02"]}
RULE_MESSAGES = {"slide": r"did not slide"}
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
    env = isolated({
        **os.environ,
        **FAKES,
        "BINGUX_SETTINGS_SNAPSHOT_FRAMES": str(frames),
        "BINGUX_SETTINGS_SNAPSHOT_SIZE": SIZE,
        "BINGUX_SETTINGS_SNAPSHOT_SCRIPT": script(label),
    })  # software rendering never finds the live desktop; no write reaches the person's dconf
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


def segments(directory: Path, timed: bool = False) -> dict[str, list]:
    """The frames of each transition: from its marker to the next one.

    Each is an array, or with `timed` a (t_ms, array) pair.
    """
    records = [json.loads(line) for line in (directory / "frames.jsonl").read_text().splitlines() if line.strip()]
    found: dict[str, list] = {}
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
        found[current].append((record["t_ms"], cache[name]) if timed else cache[name])
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


def curve(frames: list[tuple[int, np.ndarray]]) -> tuple[list[tuple[float, float]], float]:
    """((t, p) points, duration in ms) of one transition, t and p both 0 to 1.

    The transition runs from its first frame to the last frame that still differs from the picture it
    ends on: the wait that follows it is left out.
    """
    # The animation clock of the headless harness advances one 16 ms step for each frame it draws, however
    # long the machine takes to draw it, so the frame count is the time (the wall clock of a busy machine
    # is not): `times` are frame numbers, and a duration is in frames of 16 ms.
    times = list(range(len(frames)))
    arrays = [a for _, a in frames]
    end = arrays[-1]
    last_moving = max((i for i, a in enumerate(arrays) if np.abs(a - end).mean() > BETWEEN), default=0)
    first = arrays[0]
    stop = min(len(arrays) - 1, last_moving + 1)
    t0, t1 = times[0], times[stop]
    if t1 <= t0:
        return [], 0.0
    last = arrays[stop]
    points = []
    for i in range(stop + 1):
        d_first = float(np.abs(arrays[i] - first).mean())
        d_last = float(np.abs(arrays[i] - last).mean())
        points.append(((times[i] - t0) / (t1 - t0), d_first / max(d_first + d_last, 1e-9)))
    return points, float(t1 - t0)


def sample(points: list[tuple[float, float]], at: float) -> float:
    xs = [x for x, _ in points]
    ys = [y for _, y in points]
    return float(np.interp(at, xs, ys))


def symmetry_problems(timed: dict[str, list], curve_tolerance: float, time_tolerance: float) -> list[str]:
    """The pairs (push 1, pop 1) and (push 2, pop 2): the pop is the push backwards and mirrored."""
    problems = []
    for n in (1, 2):
        push, pop = timed.get(f"push {n}", []), timed.get(f"pop {n}", [])
        if len(push) < 4 or len(pop) < 4:
            continue
        (p_in, d_in), (p_out, d_out) = curve(push), curve(pop)
        if not p_in or not p_out:
            continue
        worst = max(abs(sample(p_out, t) - (1.0 - sample(p_in, 1.0 - t))) for t in np.linspace(0.05, 0.95, 12))
        if worst > curve_tolerance:
            problems.append(f"pop {n} is not push {n} backwards and mirrored: the ways differ by {worst:.2f} (allowed {curve_tolerance:.2f}) [MO-02]")
        if d_in and abs(d_out - d_in) / d_in > time_tolerance:
            problems.append(f"pop {n} takes {d_out:.0f} frames and push {n} {d_in:.0f}: in and out are not the same time [MO-02]")
    return problems


def fixtures() -> list[str]:
    """The comparison on drawn slides: one that is its own mirror, one that closes faster, one that eases differently."""
    problems = []

    def frames(ease, ms: int, forward: bool) -> list[tuple[int, np.ndarray]]:
        # A bar that slides across a 100 px wide picture: the page, at position ease(t).
        out = []
        for i in range(0, ms + 1, 16):
            t = ease(i / ms)
            x = int(round(80 * t))
            image = np.zeros((10, 100), dtype=np.int16)
            image[:, x : x + 20] = 200
            out.append((i, image if forward else image))
        return out

    def symmetric(t: float) -> float:
        k = 2.2
        return t**k / (t**k + (1 - t) ** k)

    def cubic_out(t: float) -> float:
        return 1 - (1 - t) ** 3

    push = frames(symmetric, 320, True)
    # A pop is the push backwards: the same bar, from the far end to the near.
    pop = [(t, image) for t, image in reversed(frames(symmetric, 320, True))]
    pop = [(i * 16, image) for i, (_, image) in enumerate(pop)]
    for name, timed, want_bad in (
        ("a mirrored close", {"push 1": push, "pop 1": pop}, False),
        ("a close in half the time", {"push 1": push, "pop 1": [(i * 16, image) for i, (_, image) in enumerate(pop[::2])]}, True),
        (
            "a close on another curve",
            {"push 1": push, "pop 1": [(i * 16, image) for i, (_, image) in enumerate(reversed(frames(cubic_out, 320, True)))]},
            True,
        ),
    ):
        got = symmetry_problems(timed, 0.12, 0.4)
        if bool(got) != want_bad:
            problems.append(f"fixture: {name}: expected {'a finding' if want_bad else 'none'}, got {got}")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", type=Path, default=TARGET / "bingux-settings")
    parser.add_argument("--subpages", default=",".join(SUBPAGES), help="labels of rows on the Network page")
    parser.add_argument("--min-frames", type=int, default=1)
    parser.add_argument("--out", type=Path, default=Path("/tmp/NavSlide"))
    parser.add_argument("--timeout", type=int, default=240)
    parser.add_argument("--curve-tolerance", type=float, default=0.12, help="largest difference between a close and the open mirrored")
    parser.add_argument("--time-tolerance", type=float, default=0.4, help="largest difference in how long a close and an open take, as a share")
    parser.add_argument("--strict-symmetry", action="store_true", help="fail on a pop that is not its push mirrored (default: print it as a note)")
    parser.add_argument("--fixtures", action="store_true", help="only check the comparison on drawn slides")
    args = parser.parse_args()
    problems = fixtures()
    for problem in problems:
        print(problem)
    print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
    if args.fixtures or problems:
        return 1 if problems else 0
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
        asymmetry = symmetry_problems(segments(directory, timed=True), args.curve_tolerance, args.time_tolerance)
        failed += bool(problems or (asymmetry and args.strict_symmetry))
        row = " ".join(f"{counts.get(name, 0):8}" for name in SEGMENTS)
        print(f"{label:16} {row}" + (f"   FAIL: {', '.join(problems)} did not slide" if problems else ""))
        for problem in asymmetry:
            print(f"{'':16} {'FAIL' if args.strict_symmetry else 'note'}: {problem}")
    print(f"frames kept in {args.out}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
