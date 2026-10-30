#!/usr/bin/env python3
"""Write the one-line fork sources named by labels.tsv.

Every row of labels.tsv whose glyph column is `<ForkName> <weight> [scale=.. dx=.. dy=..
mirror=.. stroke=..]` becomes `src/<file>-symbolic.svg` containing a single `<fork/>`: the
chat fork's own outline, fitted to the 16 grid by build.py (see "the chat fork's
geometry" there). Rows marked `custom` are drawn by hand and left alone.

  map-fork.py           write the sources
  map-fork.py --check   fail when a source differs from labels.tsv
"""

import argparse
import sys
from pathlib import Path

import labels

HERE = Path(__file__).resolve().parent
# The rules of design/rules.json this check holds (read by scripts/rules-check.py).
RULE_IDS = {"*": ["IC-01"]}


def source(row: labels.Row) -> str:
    extra = "".join(f' {key}="{row.attrs[key]}"' for key in labels.ATTR_KEYS if key in row.attrs)
    return f'<svg viewBox="0 0 16 16">\n  <fork name="{row.fork}" weight="{row.weight}" mode="exact"{extra}/>\n</svg>\n'


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    rows = [row for row in labels.load().rows.values() if not row.custom]
    stale = []
    for row in rows:
        path = HERE / "src" / f"{row.file}-symbolic.svg"
        text = source(row)
        if args.check:
            if not path.exists() or path.read_text() != text:
                stale.append(path.name)
        elif not path.exists() or path.read_text() != text:
            path.write_text(text)
    if stale:
        print("map-fork: out of date with labels.tsv: " + ", ".join(stale), file=sys.stderr)
        return 1
    print(f"map-fork: {'ok' if args.check else 'wrote'} {len(rows)} sources")
    return 0


if __name__ == "__main__":
    sys.exit(main())
