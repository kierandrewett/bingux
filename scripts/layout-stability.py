#!/usr/bin/env python3
"""Fail when a Bingux Settings page moves while its data loads.

The loading contract (docs/settings.md, "Loading") is that a page draws its
final layout on the first frame, from a placeholder model behind a veil, and
only values change when the data lands. This script holds that to account.

Each page opens through `bingux-settings --snapshot` with no saved values
(an empty XDG_CACHE_HOME, so every page starts from nothing, as on a first
run) and with the result of every background job held back for
`--delay-ms` (BINGUX_UI_RESULT_DELAY_MS, a testing-build knob), so frames of the
loading state are on record before the data lands. Every 16 ms frame of the run is
saved, and consecutive frames are compared by their edges (horizontal rules,
vertical rules, text bands) with the matcher in scripts/frame-walk.py. An edge
that moves or changes extent between the loading frames and the loaded frames
is a shift, and the script exits 1 if there is any.

It runs each page at the desktop size and at the 390x800 phone size, and a few
subpages that load their own data. Network, modem and display data are fakes
(BINGUX_SETTINGS_FAKE_*), so the only variable is when the data arrives.

    python3 scripts/layout-stability.py                       # everything
    python3 scripts/layout-stability.py --pages mouse,sound   # some pages
    python3 scripts/layout-stability.py --sizes 390x800       # one size

A page whose backend is missing on the machine (`bingux-config` for Appearance,
Desktop, Search and Users; Gnoblin for Displays) loads into its error state,
and the error replaces the skeleton: that is reported as a shift. Run it where
those backends exist for a verdict on those pages.

Build first: `cargo build --features snapshot` in packages/bingux-settings,
with CARGO_TARGET_DIR set as usual. Output: OUT/report.md (default
/tmp/LayoutStability), with before/after crops of each shift in OUT/shifts.
Needs PIL and numpy.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("frame_walk", HERE / "frame-walk.py")
frame_walk = importlib.util.module_from_spec(spec)
sys.modules["frame_walk"] = frame_walk
spec.loader.exec_module(frame_walk)

DESKTOP = "default"
PHONE = "390x800"

# Subpages whose own data loads after they open.
SUBPAGES = [
    "network/wifi",
    "network/wifi/saved",
    "network/hotspot",
    "network/mobile",
    "applications/default",
    "datetime/timezone",
    "background/font",
    "keyboard/shortcuts",
]

# Data that comes from the machine is replaced by fixed fakes, so a run says
# the same thing on every computer.
FAKES = {
    "BINGUX_SETTINGS_FAKE_NETWORK": "online",
    "BINGUX_SETTINGS_FAKE_MODEM": "ok",
    "BINGUX_SETTINGS_FAKE_DISPLAYS": "1",
}
# Subpages that need more than the default fakes.
EXTRA_FAKES = {"network/mobile": {"BINGUX_SETTINGS_FAKE_NETWORK": "modem"}}

# Pages whose shape follows what the machine reports, with no fake to pin it:
# the number of paired devices or of Gnoblin shortcuts is not known before the
# read, so a cold start cannot place those rows. Reported, never failed. The
# saved values (docs/settings.md, "Cached data") hide this on every later run.
DATA_SHAPED = {
    "bluetooth": "the device lists follow the paired devices",
    "keyboard/shortcuts": "needs a running Gnoblin to read the shortcuts",
    "sound": "the Balance, Fade and Subwoofer rows follow the output's channels",
}


MIN_MOVE = 4  # px; a horizontal rule that moves less is anti-aliasing, not a shift
MIN_RULE = 150  # px; row separators and card borders are long, glyph edges are not


def significant(group: dict) -> list[dict]:
    """The pairs of a shifting group that are real layout moves.

    Row separators and card borders are horizontal rules. One that moves by
    MIN_MOVE px or more between two frames means a row or group changed height
    or appeared. Text bands and vertical rules are not used: the veil dims the
    loading frames, which changes how wide a text band is and where a scrollbar
    thumb ends, without moving anything.
    """
    pairs = group["first_pairs"] + group["last_pairs"]
    return [
        pair
        for pair in pairs
        if pair["kind"] == frame_walk.H
        and pair["change"] == "moved"
        and abs(pair["dc"]) >= MIN_MOVE
        and pair["b"][4] - pair["b"][2] >= MIN_RULE
    ]


def targets(pages: list[str]) -> list[str]:
    names = pages or frame_walk.panels() + SUBPAGES
    return names


def run_one(binary: Path, address: str, size: str, out: Path, delay_ms: int, timeout: int) -> str:
    slug = address.replace("/", "_")
    frames_root = out / "frames" / size
    shutil.rmtree(frames_root / slug, ignore_errors=True)
    env = {**os.environ, **FAKES, **EXTRA_FAKES.get(address, {})}
    env["BINGUX_SETTINGS_SNAPSHOT_FRAMES"] = str(frames_root)
    env["BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS"] = str(delay_ms + 2500)
    env["BINGUX_UI_RESULT_DELAY_MS"] = str(delay_ms)
    if size != DESKTOP:
        env["BINGUX_SETTINGS_SNAPSHOT_SIZE"] = size
    log = out / "logs" / size / f"{slug}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="layout-stability-cache-") as cache, log.open("w") as sink:
        env["XDG_CACHE_HOME"] = cache
        try:
            proc = subprocess.run(
                [str(binary), "--snapshot", str(out / "png" / size), address],
                env=env,
                stdout=sink,
                stderr=subprocess.STDOUT,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return f"timed out after {timeout}s"
    return "ok" if proc.returncode == 0 else f"exit {proc.returncode}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", type=Path, default=frame_walk.DEFAULT_BINARY)
    parser.add_argument("--out", type=Path, default=Path("/tmp/LayoutStability"))
    parser.add_argument("--pages", default="", help="comma list of panel names or addresses; default all")
    parser.add_argument("--sizes", default=f"{DESKTOP},{PHONE}", help="comma list; 'default' is the desktop size")
    parser.add_argument("--delay-ms", type=int, default=1500)
    parser.add_argument("--timeout", type=int, default=120, help="seconds per run")
    parser.add_argument("--skip-run", action="store_true", help="analyse frames already in OUT/frames")
    args = parser.parse_args()

    if not args.skip_run and not args.binary.exists():
        sys.exit(f"no binary at {args.binary}; run `cargo build --features snapshot` in packages/bingux-settings first")
    pages = targets([name for name in args.pages.split(",") if name])
    sizes = [size for size in args.sizes.split(",") if size]
    out = args.out
    shifts = out / "shifts"
    shutil.rmtree(shifts, ignore_errors=True)
    shifts.mkdir(parents=True, exist_ok=True)

    rows: list[dict] = []
    for size in sizes:
        for address in pages:
            slug = address.replace("/", "_")
            status = "skipped"
            if not args.skip_run:
                status = run_one(args.binary, address, size, out, args.delay_ms, args.timeout)
            frames = out / "frames" / size / slug
            if not (frames / "frames.jsonl").exists():
                rows.append({"page": address, "size": size, "run": f"{status}; no frames", "groups": [], "frames": 0})
                print(f"{size:8} {address:24} {status}; no frames recorded", flush=True)
                continue
            result = frame_walk.analyse(f"{size}-{slug}", frames, shifts)
            result["groups"] = [group for group in result["groups"] if significant(group)]
            result.update(page=address, size=size, run=status)
            rows.append(result)
            print(f"{size:8} {address:24} {result['frames']:4} frames, {len(result['groups'])} shifts", flush=True)

    lines = ["# Bingux Settings layout stability", "", "| Size | Page | Frames | Shifts | Run |", "|---|---|---|---|---|"]
    failed = 0
    for row in rows:
        count = len(row["groups"])
        note = DATA_SHAPED.get(row["page"])
        broken = (count > 0 and note is None) or "no frames" in row["run"] or row["run"] not in ("ok", "skipped")
        if note and count:
            lines.append(f"| {row['size']} | {row['page']} | {row['frames']} | {count} (exempt: {note}) | {row['run']} |")
            continue
        failed += broken
        lines.append(f"| {row['size']} | {row['page']} | {row['frames']} | {count} | {row['run']} |")
    lines.append("")
    for row in rows:
        for group in row["groups"]:
            lines.append(f"## {row['size']} {row['page']}: frames {group['frame_from']}-{group['frame_to']}")
            lines.append("")
            for pair in significant(group)[:10]:
                lines.append(f"- {frame_walk.describe(pair)}")
            lines += ["", f"![shift](shifts/{group['crop_first']})", ""]
    (out / "report.md").write_text("\n".join(lines))
    (out / "shifts.json").write_text(json.dumps([{k: v for k, v in row.items()} for row in rows], indent=2, default=str))
    print(f"report: {out / 'report.md'}", flush=True)
    if failed:
        print(f"{failed} of {len(rows)} runs shifted or failed", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
