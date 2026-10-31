"""The map from every existing test script to its fate (tests/desktop/legacy.toml).

python3 -m bxdesktop.legacy        # the table of areas, files and fates
python3 -m bxdesktop.legacy --list # every file with its area
"""

from __future__ import annotations

import fnmatch
import subprocess
import sys
import tomllib
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
FATES = {"keep", "suite", "fold", "port", "hardware"}


def load() -> list[dict]:
    entries = tomllib.loads((REPO / "tests" / "desktop" / "legacy.toml").read_text())["entry"]
    for entry in entries:
        if entry["fate"] not in FATES:
            raise ValueError(f"{entry['area']}: fate {entry['fate']!r} is not one of {sorted(FATES)}")
    return entries


def tracked_files() -> list[str]:
    done = subprocess.run(["git", "ls-files", "tests", "scripts"], cwd=REPO, capture_output=True, text=True, check=True)
    return [line for line in done.stdout.splitlines() if line]


def classify(path: str, entries: list[dict]) -> dict | None:
    """The first entry whose pattern matches."""
    for entry in entries:
        if any(fnmatch.fnmatch(path, pattern) for pattern in entry["patterns"]):
            return entry
    return None


def main(argv: list[str]) -> int:
    entries = load()
    files = tracked_files()
    placed = {path: classify(path, entries) for path in files}
    if "--list" in argv:
        for path, entry in sorted(placed.items()):
            print(f"{entry['fate'] if entry else '??':9} {entry['area'] if entry else 'UNPLACED':60} {path}")
        return 0
    counts = Counter(entry["area"] for entry in placed.values() if entry)
    print(f"{'files':>5}  {'fate':9} area")
    for entry in entries:
        print(f"{counts[entry['area']]:5}  {entry['fate']:9} {entry['area']}")
    unplaced = [path for path, entry in placed.items() if entry is None]
    print(f"{len(files)} tracked files under tests/ and scripts/, {len(unplaced)} unplaced")
    return 1 if unplaced else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
