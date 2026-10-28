#!/usr/bin/env python3
"""Render a contact sheet of the keyboard focus ring on every control.

Runs packages/bingux-ui/examples/focus_ring_sheet.rs once per column. The
example presses Tab through a page that holds a specimen of each control (switch,
button, suggested and destructive buttons, text field, invalid text field,
dropdown, segmented, settings row, slider, checkbox, radio, file list row,
sidebar item) and saves the frame after each press, so every control is shown
with the real ring. This script crops each frame to its focused control and sets
the crops in a grid: a row per control, a column per variant.

The default columns are the four the ring is held to: light and dark, at the
desktop width (800) and at the phone width (390).

    scripts/focus-ring-sheet.py /tmp/rings.png
    scripts/focus-ring-sheet.py /tmp/rings.png --build --scale 2
    scripts/focus-ring-sheet.py /tmp/rings.png --column dark:BINGUX_SETTINGS_SNAPSHOT_SCHEME=prefer-dark:800

A `--column NAME:KEY=VALUE,KEY=VALUE:WIDTH` is one column with those environment
variables and window width; give it more than once. Needs ImageMagick (`magick`).
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))
SCHEME = "BINGUX_SETTINGS_SNAPSHOT_SCHEME"
DEFAULT_COLUMNS = [
    ("light desktop", {SCHEME: "prefer-light"}, "800"),
    ("dark desktop", {SCHEME: "prefer-dark"}, "800"),
    ("light phone", {SCHEME: "prefer-light"}, "390"),
    ("dark phone", {SCHEME: "prefer-dark"}, "390"),
]
# Pixels around the focused control that the crop keeps, so the whole ring shows.
MARGIN = 8


def parse_column(text: str) -> tuple[str, dict[str, str], str]:
    name, env, width = text.rsplit(":", 2) if text.count(":") >= 2 else (text, "", "800")
    pairs = dict(item.split("=", 1) for item in env.split(",") if item)
    return name, pairs, width


def build() -> None:
    subprocess.run(
        [
            "cargo",
            "build",
            "--example",
            "focus_ring_sheet",
            "--features",
            "testing",
            "--manifest-path",
            str(ROOT / "packages/bingux-ui/Cargo.toml"),
        ],
        check=True,
        env=dict(os.environ, CARGO_TARGET_DIR=str(TARGET)),
    )


def render(name: str, env: dict[str, str], width: str, scale: str, workdir: Path) -> list[tuple[str, Path]]:
    """Run the example for one column and return (control, cropped PNG) pairs."""
    binary = TARGET / "debug" / "examples" / "focus_ring_sheet"
    if not binary.exists():
        sys.exit(f"{binary} is missing: build with --build")
    out = workdir / name.replace(" ", "-")
    out.mkdir(parents=True)
    run_env = dict(os.environ, BINGUX_SNAPSHOT_SCALE=scale, **env)
    subprocess.run([str(binary), str(out), width], check=True, env=run_env)
    pad = MARGIN * float(scale)
    cells = []
    for line in (out / "crops.tsv").read_text().splitlines():
        file, x, y, w, h = line.split("\t")
        x, y, w, h = (float(value) for value in (x, y, w, h))
        left, top = max(0, int(x - pad)), max(0, int(y - pad))
        crop = out / f"crop-{file}"
        subprocess.run(
            ["magick", str(out / file), "-crop", f"{int(w + 2 * pad)}x{int(h + 2 * pad)}+{left}+{top}", "+repage", str(crop)],
            check=True,
        )
        cells.append((file.split("-", 1)[1].removesuffix(".png"), crop))
    return cells


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("output", help="the contact sheet PNG to write")
    parser.add_argument("--column", action="append", help="NAME:KEY=VALUE,..:WIDTH, repeatable")
    parser.add_argument("--scale", default="2", help="render scale (default 2)")
    parser.add_argument("--build", action="store_true", help="build the example first")
    parser.add_argument("--keep", help="keep the frames and crops in this directory")
    args = parser.parse_args()

    if shutil.which("magick") is None:
        sys.exit("ImageMagick (magick) is required")
    if args.build:
        build()
    columns = [parse_column(text) for text in args.column] if args.column else DEFAULT_COLUMNS
    workdir = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix="focus-ring-sheet-"))
    workdir.mkdir(parents=True, exist_ok=True)

    per_column = [render(name, env, width, args.scale, workdir) for name, env, width in columns]
    controls: list[str] = []
    for cells in per_column:
        for control, _ in cells:
            if control not in controls:
                controls.append(control)

    tiles: list[str] = []
    for control in controls:
        for (name, _, _), cells in zip(columns, per_column, strict=True):
            crop = next((path for found, path in cells if found == control), None)
            tiles += ["-label", f"{control} / {name}", str(crop) if crop else "null:"]
    subprocess.run(
        [
            "magick",
            "montage",
            "-background",
            "#101010",
            "-fill",
            "#bbbbbb",
            "-pointsize",
            str(int(11 * float(args.scale))),
            "-tile",
            f"{len(columns)}x",
            "-geometry",
            "+6+6",
            *tiles,
            args.output,
        ],
        check=True,
    )
    if not args.keep:
        shutil.rmtree(workdir, ignore_errors=True)
    print(f"{args.output}: {len(controls)} controls x {len(columns)} columns")
    return 0


if __name__ == "__main__":
    sys.exit(main())
