#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow"]
# ///
"""Crispness contact sheets, rasterised by Freya/Skia as Bingux draws icons.

  sheets.py [--out DIR] [--icons DIR] [--adwaita DIR] [--zoom N] [--group NAME]

Renders every icon in --icons (default crates/ui/icons) at 12, 14, 16, 18, 20 and
48 px, white on black and dark on light, with the Adwaita original beneath each one, plus a
top-bar mock (topbar.txt) into `OUT/all/`, then the sets in groups.txt (top bar, battery,
sidebar, controls, buttons, status, devices, applications), one folder each, so icons are
judged next to the icons they are used with. Writes `sheet-<size>-<scheme>.png` at 1:1 and `...@Nx.png`
(nearest-neighbour zoom, default 4) into --out (default /tmp/bingux-icon-sheets). Set
CARGO_TARGET_DIR to share a build directory. Needs the Adwaita symbolic theme for the
reference rows (--adwaita, default /usr/share/icons/Adwaita/symbolic); without it the
sheets show ours only.
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


def copy_untinted(source: Path, dest: Path) -> None:
    """Adwaita files carry a fixed dark fill; drop it so Freya's tint applies like ours."""
    dest.write_text(re.sub(r'\sfill="#[0-9a-fA-F]{3,8}"', "", source.read_text()))


def flatten_reference(adwaita: Path, icons: Path, dest: Path, extra: list[str]) -> None:
    index: dict[str, Path] = {}
    for path in adwaita.rglob("*-symbolic.svg"):
        index.setdefault(path.name, path)
    wanted = {path.name for path in icons.glob("*.svg")} | {f"{name}-symbolic.svg" for name in extra}
    for name in sorted(wanted):
        if name in index:
            copy_untinted(index[name], dest / name)
    for line in (HERE / "topbar.txt").read_text().splitlines():
        parts = line.split("#")[0].split()
        if len(parts) == 2 and parts[1] in index:
            copy_untinted(index[parts[1]], dest / parts[1])


def groups() -> dict[str, list[str]]:
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


def run_example(out: Path, ours: Path, reference: Path | None, topbar: Path | None) -> None:
    command = [
        "cargo", "run", "-q", "--manifest-path", str(REPO / "crates/ui/Cargo.toml"),
        "--features", "testing", "--example", "icon_sheet", "--", str(out), str(ours),
    ]
    env = dict(os.environ)
    if reference is not None:
        command.append(str(reference))
        if topbar is not None:
            env["ICON_SHEET_TOPBAR"] = str(topbar)
    subprocess.run(command, check=True, env=env)


def zoom_all(out: Path, zoom: int) -> None:
    for png in sorted(out.glob("*.png")):
        if "@" in png.name:
            continue
        image = Image.open(png)
        image.resize((image.width * zoom, image.height * zoom), Image.NEAREST).save(png.with_name(f"{png.stem}@{zoom}x.png"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, default=Path("/tmp/bingux-icon-sheets"))
    parser.add_argument("--icons", type=Path, default=REPO / "crates/ui/icons")
    parser.add_argument("--adwaita", type=Path, default=Path("/usr/share/icons/Adwaita/symbolic"))
    parser.add_argument("--zoom", type=int, default=4)
    parser.add_argument("--group", help="only this group from groups.txt (default: all and every group)")
    args = parser.parse_args()
    have_reference = args.adwaita.is_dir()
    sets = groups()
    if args.group and args.group not in sets:
        parser.error(f"unknown group {args.group}; groups: {', '.join(sets)}")
    with tempfile.TemporaryDirectory() as tmp:
        reference = Path(tmp) / "reference"
        reference.mkdir()
        if have_reference:
            flatten_reference(args.adwaita, args.icons, reference, [n for names in sets.values() for n in names])
        if not args.group:
            out = args.out / "all"
            out.mkdir(parents=True, exist_ok=True)
            run_example(out, args.icons, reference if have_reference else None, HERE / "topbar.txt")
            zoom_all(out, args.zoom)
        for name, icons in sets.items():
            if args.group and name != args.group:
                continue
            ours = Path(tmp) / f"ours-{name}"
            theirs = Path(tmp) / f"ref-{name}"
            ours.mkdir()
            theirs.mkdir()
            for index, icon in enumerate(icons):
                file = f"{icon}-symbolic.svg"
                prefixed = f"{index:03d}-{file}"
                if not (args.icons / file).is_file():
                    print(f"sheets: group {name}: {file} is not in {args.icons}", file=sys.stderr)
                    continue
                shutil.copyfile(args.icons / file, ours / prefixed)
                if (reference / file).is_file():
                    shutil.copyfile(reference / file, theirs / prefixed)
            out = args.out / name
            out.mkdir(parents=True, exist_ok=True)
            run_example(out, ours, theirs if have_reference else None, None)
            zoom_all(out, args.zoom)
    print(f"sheets: wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
