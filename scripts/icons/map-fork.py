#!/usr/bin/env python3
"""Write the one-line fork sources named by labels.tsv.

Every row of labels.tsv whose last column is `<ForkName> <weight>` becomes
`src/<file>-symbolic.svg` containing a single `<fork mode="exact"/>`: the chat fork's own
outline, untouched. Rows marked `custom` are drawn by hand and left alone.

  map-fork.py           write the sources
  map-fork.py --check   fail when a source differs from labels.tsv
"""

import argparse
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROW = re.compile(r"^([A-Za-z0-9]+) (regular|bold|fill)$")


def rows() -> list[tuple[str, str, str]]:
    result = []
    for line in (HERE / "labels.tsv").read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        parts = [part.strip() for part in line.split("|")]
        if len(parts) != 4:
            raise SystemExit(f"map-fork: bad row: {line}")
        match = ROW.match(parts[3])
        if match:
            result.append((parts[0], match.group(1), match.group(2)))
    return result


def source(name: str, weight: str) -> str:
    return f'<svg viewBox="0 0 16 16">\n  <fork name="{name}" weight="{weight}" mode="exact"/>\n</svg>\n'


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale = []
    for file, name, weight in rows():
        path = HERE / "src" / f"{file}-symbolic.svg"
        text = source(name, weight)
        if args.check:
            if not path.exists() or path.read_text() != text:
                stale.append(path.name)
        else:
            path.write_text(text)
    if stale:
        print("map-fork: out of date with labels.tsv: " + ", ".join(stale), file=sys.stderr)
        return 1
    print(f"map-fork: {'ok' if args.check else 'wrote'} {len(rows())} sources")
    return 0


if __name__ == "__main__":
    sys.exit(main())
