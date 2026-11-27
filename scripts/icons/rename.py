#!/usr/bin/env python3
"""Rename the icon set to the shape names of aliases.tsv, everywhere in the repository.

Every icon is named after what it draws (`calendar`, `shield`, `chevron-right`). aliases.tsv has one row
per icon: the shape name, then the names it answers to. This script makes the rest of the repository use
the shape names, and can be run again at any time (after a merge brings icons under their old names):

- the sources (scripts/icons/src) and the compiled crate copy (crates/ui/icons): the first name of a row
  that is a file keeps its drawing under the shape name; any other file of the row is a duplicate drawing
  and is removed (build.py rewrites the shell's copy, shell/bingux/icons, from the sources);
- labels.tsv (rows and directive lines), groups.txt, topbar.txt and the keys of golden.json;
- crates/ui/src/icons.rs: each variant becomes the shape name in CamelCase (`battery-50-bolt` is
  `Battery50Bolt`), and variants that now name one drawing become one;
- every `Icon::Variant` in Rust and Markdown, and every `old-name-symbolic` string in the code, the tests and
  the docs.

Afterwards run scripts/icons/build.py: it writes BinguxIconIndex.js, BinguxIconAliases.js and
crates/ui/src/icon_aliases.rs from the sources and aliases.tsv.

Not touched: aliases.tsv itself, the files build.py generates, native/ (it asks GTK's own theme for
Adwaita icons) and vendor/.

Usage: scripts/icons/rename.py [--dry-run]
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
ALIASES = HERE / "aliases.tsv"
SRC = HERE / "src"
ICON_DIRS = (SRC, REPO / "crates/ui/icons")
ICONS_RS = REPO / "crates/ui/src/icons.rs"
SUFFIX = "-symbolic.svg"
# A line that names an old or outside name on purpose (a test that feeds one in) carries this mark.
KEEP_MARK = "rename.py: keep"
# Docs that name icons in backticks (`go-next`), renamed there too.
ICON_GUIDES = ("docs/bingux-ui.md", "docs/icons.md")

# Generated or out of scope: see the module docstring.
SKIP_PREFIXES = (
    "scripts/icons/src/",
    "crates/ui/icons/",
    "shell/bingux/icons/",
    "native/",
    "vendor/",
)
SKIP_FILES = {
    "scripts/icons/aliases.tsv",
    "scripts/icons/rename.py",
    "scripts/icons/golden.json",
    "scripts/icons/labels.tsv",
    "scripts/icons/groups.txt",
    "scripts/icons/topbar.txt",
    "shell/bingux/BinguxIconIndex.js",
    "shell/bingux/BinguxIconAliases.js",
    "crates/ui/src/icon_aliases.rs",
    "crates/ui/src/icons.rs",
}
TEXT_SUFFIXES = {".rs", ".qml", ".js", ".mjs", ".py", ".md", ".json", ".sh", ".txt", ".toml", ".c", ".h"}


def load_table() -> list[tuple[str, list[str]]]:
    rows = []
    for line in ALIASES.read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        new, *olds = line.split("\t")
        rows.append((new, olds))
    return rows


def old_to_new(rows: list[tuple[str, list[str]]]) -> dict[str, str]:
    return {old: new for new, olds in rows for old in olds if old != new}


def camel(name: str) -> str:
    return "".join(part[:1].upper() + part[1:] for part in name.split("-"))


class Plan:
    def __init__(self, dry_run: bool):
        self.dry_run = dry_run
        self.changed: list[str] = []

    def write(self, path: Path, text: str) -> None:
        if path.read_text() != text:
            self.changed.append(str(path.relative_to(REPO)))
            if not self.dry_run:
                path.write_text(text)

    def remove(self, path: Path) -> None:
        self.changed.append(f"rm {path.relative_to(REPO)}")
        if not self.dry_run:
            path.unlink()

    def move(self, src: Path, dst: Path) -> None:
        self.changed.append(f"mv {src.relative_to(REPO)} -> {dst.name}")
        if not self.dry_run:
            src.rename(dst)


# ------------------------------------------------------------------ files


def kept_and_merged(rows: list[tuple[str, list[str]]], exists) -> tuple[dict[str, str], set[str]]:
    """old name -> shape name for each drawing that stays, and the old names whose drawing goes."""
    keep: dict[str, str] = {}
    merged: set[str] = set()
    for new, olds in rows:
        present = [old for old in olds if exists(old)]
        if not present:
            continue
        kept = present[0]
        keep[kept] = new
        merged |= {old for old in present[1:] if old != kept}
        if kept != new and exists(new):
            merged.add(new)
    return keep, merged


def rename_files(rows, plan: Plan) -> tuple[dict[str, str], set[str]]:
    keep, merged = kept_and_merged(rows, lambda name: (SRC / f"{name}{SUFFIX}").exists())
    for folder in ICON_DIRS:
        for name in sorted(merged):
            path = folder / f"{name}{SUFFIX}"
            if path.exists():
                plan.remove(path)
        for old, new in sorted(keep.items()):
            path = folder / f"{old}{SUFFIX}"
            if old != new and path.exists():
                plan.move(path, folder / f"{new}{SUFFIX}")
    return keep, merged


# ------------------------------------------------------------------ icon lists


def rename_labels(keep: dict[str, str], merged: set[str], plan: Plan) -> None:
    path = HERE / "labels.tsv"
    lines = path.read_text().splitlines()
    rows: dict[str, list[str]] = {}
    for line in lines:
        if not line.startswith("#") and " | " in line:
            cells = line.split(" | ")
            rows[cells[0]] = cells
    # A merged drawing's "where the user sees it" joins the row of the drawing that stays.
    table = load_table()
    for new, olds in table:
        kept = next((old for old in olds if old in keep), None)
        if kept is None or kept not in rows:
            continue
        for old in olds + [new]:
            if old in merged and old in rows and old != kept:
                where = rows[old][1]
                if where and where not in rows[kept][1]:
                    rows[kept][1] = f"{rows[kept][1]}; {where}"
    out = []
    for line in lines:
        if line.startswith("#") and ":" in line and re.match(r"#(policy|symmetric|mark|exempt|mirror_rtl)\b", line):
            head, names = line.split(":", 1)
            renamed: list[str] = []
            for name in names.split():
                if name in merged:
                    continue
                name = keep.get(name, name)
                if name not in renamed:
                    renamed.append(name)
            out.append(f"{head}: {' '.join(renamed)}" if renamed else f"{head}:")
        elif not line.startswith("#") and " | " in line:
            cells = rows[line.split(" | ")[0]]
            if cells[0] in merged:
                continue
            out.append(" | ".join([keep.get(cells[0], cells[0])] + cells[1:]))
        else:
            out.append(line)
    plan.write(path, "\n".join(out) + "\n")


def rename_groups(names: dict[str, str], merged_into: dict[str, str], plan: Plan) -> None:
    path = HERE / "groups.txt"
    out = []
    for line in path.read_text().splitlines():
        if line.startswith(("#", "[")) or not line.strip():
            out.append(line)
            continue
        seen: list[str] = []
        for name in line.split():
            name = names.get(name, merged_into.get(name, name))
            if name not in seen:
                seen.append(name)
        out.append(" ".join(seen))
    plan.write(path, "\n".join(out) + "\n")


def rename_topbar(names: dict[str, str], plan: Plan) -> None:
    """Each line is our file, then the Adwaita file it stands in for: the old name, once the two differ."""
    path = HERE / "topbar.txt"
    out = []
    for line in path.read_text().splitlines():
        parts = line.split()
        if line.startswith("#") or not parts:
            out.append(line)
            continue
        ours = parts[0].removesuffix(SUFFIX)
        reference = parts[1] if len(parts) > 1 else parts[0]
        new = names.get(ours, ours)
        out.append(f"{new}{SUFFIX} {reference}" if f"{new}{SUFFIX}" != reference else reference)
    plan.write(path, "\n".join(out) + "\n")


def rename_golden(names: dict[str, str], merged: set[str], plan: Plan) -> None:
    """Rename the keys of golden.json (one block per icon) and keep them sorted."""
    path = HERE / "golden.json"
    text = path.read_text()
    head, rest = text.split('  "icons": {\n', 1)
    body, tail = rest.rsplit("\n  }\n", 1)
    blocks = re.split(r'(?m)^(?=    "[^"]+": \{$)', body)
    pieces = []
    for block in blocks:
        if not block.strip():
            continue
        key = re.match(r'    "([^"]+)"', block).group(1)
        name = key.removesuffix(SUFFIX)
        if key.endswith(SUFFIX) and name in merged:
            continue
        if key.endswith(SUFFIX):
            key = f"{names.get(name, name)}{SUFFIX}"
        pieces.append((key, re.sub(r'^    "[^"]+"', f'    "{key}"', block, count=1).rstrip().rstrip(",")))
    pieces.sort()
    plan.write(path, head + '  "icons": {\n' + ",\n".join(block for _, block in pieces) + "\n  }\n" + tail)


# ------------------------------------------------------------------ icons.rs


def variants_by_file(text: str) -> dict[str, list[str]]:
    found: dict[str, list[str]] = {}
    for variant, file in re.findall(
        r'Self::(\w+)\s*=>\s*\{?\s*include_bytes!\(\s*"\.\./icons/([a-z0-9-]+)-symbolic\.svg"\s*\)', text
    ):
        found.setdefault(file, []).append(variant)
    return found


def variant_map(names: dict[str, str], merged_into: dict[str, str]) -> dict[str, str]:
    """Old variant -> new variant, from the file each variant draws today."""
    result = {}
    for file, variants in variants_by_file(ICONS_RS.read_text()).items():
        new = names.get(file) or merged_into.get(file) or file
        for variant in variants:
            if variant != camel(new):
                result[variant] = camel(new)
    return result


def dedupe_lines(lines: list[str], start: int, end: int, pattern: str) -> list[str]:
    """Drop repeated lines matching `pattern` between start and end, and the doc comment over each."""
    seen: set[str] = set()
    out: list[str] = []
    for index, line in enumerate(lines):
        match = re.match(pattern, line) if start <= index < end else None
        if match and match.group(1) in seen:
            while out and out[-1].strip().startswith("///"):
                out.pop()
            continue
        if match:
            seen.add(match.group(1))
        out.append(line)
    return out


def rename_icons_rs(variants: dict[str, str], names: dict[str, str], merged_into: dict[str, str], plan: Plan) -> None:
    text = ICONS_RS.read_text()
    if variants:
        alternation = "|".join(sorted(map(re.escape, variants), key=len, reverse=True))
        text = re.sub(rf"\b(Self|Icon)::({alternation})\b", lambda m: f"{m.group(1)}::{variants[m.group(2)]}", text)
        text = re.sub(rf"(?m)^    ({alternation}),$", lambda m: f"    {variants[m.group(1)]},", text)
        text = re.sub(rf"`({alternation})`", lambda m: f"`{variants[m.group(1)]}`", text)
    file_names = {**merged_into, **names}
    text = re.sub(
        r'include_bytes!\(\s*"\.\./icons/([a-z0-9-]+)-symbolic\.svg"\s*\)',
        lambda m: f'include_bytes!("../icons/{file_names.get(m.group(1), m.group(1))}-symbolic.svg")',
        text,
    )
    lines = text.split("\n")
    enum_start = next(i for i, line in enumerate(lines) if line.startswith("pub enum Icon {"))
    enum_end = next(i for i in range(enum_start, len(lines)) if lines[i] == "}")
    lines = dedupe_lines(lines, enum_start, enum_end, r"^    (\w+),$")
    all_start = next(i for i, line in enumerate(lines) if "pub const ALL: [Icon;" in line)
    all_end = next(i for i in range(all_start, len(lines)) if lines[i].strip() == "];")
    lines = dedupe_lines(lines, all_start, all_end, r"^        Self::(\w+),$")
    all_start = next(i for i, line in enumerate(lines) if "pub const ALL: [Icon;" in line)
    all_end = next(i for i in range(all_start, len(lines)) if lines[i].strip() == "];")
    count = sum(1 for line in lines[all_start:all_end] if re.match(r"^        Self::\w+,$", line))
    lines[all_start] = re.sub(r"\[Icon; \d+\]", f"[Icon; {count}]", lines[all_start])
    text = "\n".join(lines)
    # One match arm per variant in `bytes`, one pattern per variant in an or-pattern.
    seen: set[str] = set()

    def arm(m: re.Match) -> str:
        if m.group(1) in seen:
            return ""
        seen.add(m.group(1))
        return m.group(0)

    text = re.sub(
        r'(?m)^ {12}Self::(\w+) => (?:\{\s*include_bytes!\("[^"]+"\)\s*\},?|include_bytes!\("[^"]+"\),)\n', arm, text
    )
    for block in re.findall(r"matches!\(\s*self,[^)]*\)", text):
        seen_or: set[str] = set()
        kept_lines = []
        for line in block.split("\n"):
            variant = re.match(r"^\s*(?:\| )?Self::(\w+)$", line)
            if variant and variant.group(1) in seen_or:
                continue
            if variant:
                seen_or.add(variant.group(1))
            kept_lines.append(line)
        text = text.replace(block, "\n".join(kept_lines))
    plan.write(ICONS_RS, text)


# ------------------------------------------------------------------ the rest of the repository


def tracked_files() -> list[str]:
    out = subprocess.run(["git", "ls-files"], cwd=REPO, check=True, capture_output=True, text=True).stdout
    return [
        path
        for path in out.splitlines()
        if Path(path).suffix in TEXT_SUFFIXES
        and not path.startswith(SKIP_PREFIXES)
        and path not in SKIP_FILES
        and (REPO / path).is_file()
    ]


def rename_text(names: dict[str, str], variants: dict[str, str], plan: Plan) -> None:
    name_alt = "|".join(sorted(map(re.escape, names), key=len, reverse=True))
    symbolic = re.compile(rf"(?<![A-Za-z0-9_-])({name_alt})-symbolic(?![A-Za-z0-9_])")
    # The icon guides name an icon in backticks: `go-next` is `chevron-right`. Only there: elsewhere a
    # backticked `sync` or `presentation` is a word, not an icon.
    quoted = re.compile(rf"`({name_alt})`")
    variant_re = None
    if variants:
        variant_alt = "|".join(sorted(map(re.escape, variants), key=len, reverse=True))
        variant_re = re.compile(rf"\bIcon::({variant_alt})\b")
    for path in tracked_files():
        file = REPO / path
        try:
            text = file.read_text()
        except UnicodeDecodeError:
            continue
        lines = text.split("\n")
        for index, line in enumerate(lines):
            if KEEP_MARK in line:
                continue
            line = symbolic.sub(lambda m: f"{names[m.group(1)]}-symbolic", line)
            if variant_re is not None and file.suffix in (".rs", ".md"):
                line = variant_re.sub(lambda m: f"Icon::{variants[m.group(1)]}", line)
            if path in ICON_GUIDES:
                line = quoted.sub(lambda m: f"`{names[m.group(1)]}`", line)
            lines[index] = line
        new = "\n".join(lines)
        if new != text:
            plan.write(file, new)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dry-run", action="store_true", help="list what would change, change nothing")
    args = parser.parse_args()
    plan = Plan(args.dry_run)
    rows = load_table()
    names = old_to_new(rows)
    keep, merged = rename_files(rows, plan)
    # A merged drawing's old name points at the shape name of the drawing that stays.
    merged_into = {old: new for new, olds in rows for old in olds if old in merged} | {
        new: new for new, olds in rows if new in merged
    }
    variants = variant_map(names, merged_into)
    rename_labels(keep, merged, plan)
    rename_groups(names, merged_into, plan)
    rename_topbar(names, plan)
    rename_golden(names, merged, plan)
    rename_icons_rs(variants, names, merged_into, plan)
    rename_text(names, variants, plan)
    for line in plan.changed:
        print(line)
    print(f"rename.py: {len(plan.changed)} changes{' (dry run)' if args.dry_run else ''}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
