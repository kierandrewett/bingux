"""Parser for scripts/icons/labels.tsv, shared by build.py, map-fork.py and sheets.py.

Rows: `file | where it is used | metaphor | glyph`, where glyph is either
`custom: <how it is drawn>` or `<ForkName> <weight> [key=value ...]` with the
keys `scale` (one factor on both axes), `dx`, `dy` (pixels on the 16 grid), `mirror` (`left` or
`right`: draw the other half as its mirror image) and `align` (`none` turns off the automatic
half pixel shift). A glyph is only ever scaled uniformly and moved.

Directives, one per comment line, add the per-icon metadata the checks and the
review sheets need:

  #policy <category>: file file ...      fill/outline category of those icons
  #symmetric: file file ...              must be left-right symmetric about x = 8
  #mark: file file ...                   a small control mark (chevron, tick, plus)
  #exempt <rule>: file file ...          rule is one of size, centre, soft, dots (golden.py)
  #mirror_rtl: file file ...             drawn with a direction (an arrow, a chevron, a panel
                                         at one side): flipped left to right when the interface
                                         reads right to left (`Icon::mirrors_in_rtl`, design/rtl.md)
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
LABELS = HERE / "labels.tsv"
FORK_SPEC = re.compile(r"^([A-Za-z0-9]+) (regular|bold|fill)((?: [a-z]+=[-0-9.a-z]+)*)$")
ATTR_KEYS = ("scale", "dx", "dy", "mirror", "align")
DIRECTIVE = re.compile(r"^#(policy|symmetric|mark|exempt|mirror_rtl)(?: ([a-z-]+))?:\s*(.*)$")


@dataclass
class Row:
    file: str
    where: str
    metaphor: str
    glyph: str
    fork: str | None = None
    weight: str | None = None
    attrs: dict[str, str] = field(default_factory=dict)

    @property
    def custom(self) -> bool:
        return self.fork is None


@dataclass
class Labels:
    rows: dict[str, Row] = field(default_factory=dict)
    policy: dict[str, str] = field(default_factory=dict)
    symmetric: set[str] = field(default_factory=set)
    marks: set[str] = field(default_factory=set)
    exempt: dict[str, set[str]] = field(default_factory=dict)
    mirror_rtl: set[str] = field(default_factory=set)


def load(path: Path = LABELS) -> Labels:
    labels = Labels()
    for number, line in enumerate(path.read_text().splitlines(), 1):
        if not line.strip():
            continue
        if line.startswith("#"):
            match = DIRECTIVE.match(line)
            if not match:
                continue
            kind, arg, names = match.groups()
            files = names.split()
            if kind == "policy":
                for name in files:
                    labels.policy[name] = arg or ""
            elif kind == "symmetric":
                labels.symmetric.update(files)
            elif kind == "mark":
                labels.marks.update(files)
            elif kind == "mirror_rtl":
                labels.mirror_rtl.update(files)
            else:
                labels.exempt.setdefault(arg or "", set()).update(files)
            continue
        parts = [part.strip() for part in line.split("|")]
        if len(parts) != 4:
            raise SystemExit(f"labels.tsv:{number}: expected 4 columns separated by |")
        row = Row(*parts)
        if not parts[3].startswith("custom"):
            match = FORK_SPEC.match(parts[3])
            if not match:
                raise SystemExit(f"labels.tsv:{number}: glyph must be `custom: ..` or `Name weight [k=v ..]`: {parts[3]}")
            row.fork, row.weight = match.group(1), match.group(2)
            for item in match.group(3).split():
                key, value = item.split("=", 1)
                if key not in ATTR_KEYS:
                    raise SystemExit(f"labels.tsv:{number}: unknown attribute {key}")
                row.attrs[key] = value
        if parts[0] in labels.rows:
            raise SystemExit(f"labels.tsv:{number}: {parts[0]} appears twice")
        labels.rows[parts[0]] = row
    return labels
