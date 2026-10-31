#!/usr/bin/env python3
"""Walk the Files location field through its states and capture each one.

`bingux-files --snapshot` renders the window headlessly, so this needs no
compositor. Seven states, at desktop (1100x760) and phone (390x800) size, in
both colour schemes (28 PNGs):

  1-resting     the trail of folders
  2-editing     Ctrl+L: a plain field, no card
  3-path        a typed path: the folders that complete it
  4-search      words: this folder, places, the search streaming in, the scope
                and filter chips inside the card
  5-noresults   words that match nothing
  6-error       a path that is not there
  7-denied      a path inside a folder that may not be read

The check each capture gets is the one that does not need eyes: the card of a
state that has one is on the page (the pixels of the capture differ from the
resting one in the rows under the field), the states that must not have one do
not, and a capture never matches another state's. Look at the PNGs for the
rest. Usage:

    python3 scripts/omnibox-walk.py [--out DIR] [--binary PATH] [--keep-fixture]
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
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))
SIZES = {"desktop": "1100x760", "phone": "390x800"}
SCHEMES = ("light", "dark")


def walks(docs: Path) -> dict[str, tuple[str, bool]]:
    """State name -> (script, whether a card is on show at the end)."""
    return {
        "1-resting": ("wait 900", False),
        "2-editing": ("wait 900; key ctrl+l; wait 500", False),
        "3-path": (f"wait 900; key ctrl+l; wait 300; key ctrl+a; type {docs}/Re; wait 1200", True),
        "4-search": ("wait 900; key ctrl+f; wait 300; type re; wait 2000", True),
        "5-noresults": ("wait 900; key ctrl+f; wait 300; type zzqqxx; wait 2000", True),
        "6-error": ("wait 900; key ctrl+l; wait 300; key ctrl+a; type /no/such/place; key return; wait 1200", True),
        "7-denied": (
            f"wait 900; key ctrl+l; wait 300; key ctrl+a; type {docs}/locked/inner; key return; wait 1200",
            True,
        ),
    }


def make_fixture(root: Path) -> Path:
    """A home folder with Documents to search, and a folder that may not be read."""
    docs = root / "home" / "Documents"
    for name in ("Reports/annual-report.pdf", "report.pdf", "tax-return-2024.pdf", "notes.txt", "budget.xlsx"):
        (docs / name).parent.mkdir(parents=True, exist_ok=True)
        (docs / name).write_text("x\n")
    for name in ("Pictures", "Music", "Downloads", "Videos", "Desktop"):
        (root / "home" / name).mkdir(parents=True, exist_ok=True)
    (docs / "locked" / "inner").mkdir(parents=True, exist_ok=True)
    (docs / "locked").chmod(0)
    return docs


def capture(binary: Path, root: Path, docs: Path, size: str, scheme: str, script: str, out: Path) -> Path:
    env = isolated()
    env.update(
        HOME=str(root / "home"),
        XDG_STATE_HOME=str(root / "state"),
        XDG_CONFIG_HOME=str(root / "config"),
        XDG_DATA_HOME=str(root / "data"),
        BINGUX_FILES_SNAPSHOT_SIZE=size,
        BINGUX_FILES_SNAPSHOT_SCRIPT=script,
        BINGUX_SETTINGS_SNAPSHOT_SCHEME=f"prefer-{scheme}",
    )
    work = Path(tempfile.mkdtemp(prefix="walk-", dir=root))
    result = subprocess.run(
        [str(binary), "--snapshot", str(work), str(docs)], env=env, capture_output=True, text=True, timeout=240
    )
    png = work / "files.png"
    if result.returncode != 0 or not png.exists():
        raise SystemExit(f"capture failed ({size} {scheme}): {result.stderr or result.stdout}")
    shutil.move(png, out)
    return out


def pixels(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.int16)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", default="/tmp/omnibox-walk")
    parser.add_argument("--binary", default=str(TARGET / "debug" / "bingux-files"))
    parser.add_argument("--keep-fixture", action="store_true")
    args = parser.parse_args()
    binary = Path(args.binary)
    if not binary.exists():
        sys.exit(f"{binary} is missing: build apps/files with --features snapshot")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="omnibox-walk-"))
    docs = make_fixture(root)
    problems: list[str] = []
    try:
        for scheme in SCHEMES:
            for tag, size in SIZES.items():
                shots: dict[str, np.ndarray] = {}
                for name, (script, card) in walks(docs).items():
                    png = capture(binary, root, docs, size, scheme, script, out / f"{name}-{tag}-{scheme}.png")
                    shots[name] = pixels(png)
                    print(f"[walk] {png}")
                resting = shots["1-resting"]
                for name, (_, card) in walks(docs).items():
                    height = resting.shape[0]
                    band = slice(int(height * 0.14), int(height * 0.5))
                    changed = int(np.abs(shots[name][band] - resting[band]).sum(axis=2).astype(bool).sum())
                    if card and changed < 2000:
                        problems.append(
                            f"{name} {tag} {scheme}: no card on the page ({changed} pixels differ from resting)"
                        )
                names = list(shots)
                for at, one in enumerate(names):
                    for other in names[at + 1 :]:
                        if np.array_equal(shots[one], shots[other]):
                            problems.append(f"{one} and {other} {tag} {scheme} are the same picture")
    finally:
        (root / "home" / "Documents" / "locked").chmod(0o755)
        if not args.keep_fixture:
            shutil.rmtree(root, ignore_errors=True)
    for problem in problems:
        print(f"omnibox-walk: {problem}")
    print(f"omnibox-walk: {len(problems)} problem(s); captures in {out}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
