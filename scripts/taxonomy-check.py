#!/usr/bin/env python3
"""Fail when bingux-ui exports something the taxonomy app does not show.

bingux-taxonomy (packages/bingux-taxonomy) is the one place every component,
icon and token of bingux-ui is seen running. Its registry
(src/registry.rs) names, per entry, the Rust items it shows in `covers`. This
check enumerates what bingux-ui exports and fails on:

  uncovered item   a public item no entry covers and the allowlist does not
                   excuse. Add or extend an entry, or allowlist it with a reason.
  unknown cover    a name in `covers` that bingux-ui no longer exports.
  icon variant     an `Icon` variant missing from `Icon::ALL`, from
                   `Icon::bytes()`, or no entry that shows the whole set.
  icon file        a file in packages/bingux-ui/icons that no variant names and
                   no label row describes is still drawn (the page lists the
                   directory); this only checks the directory is non-empty.
  bad allowlist    an allowlist line with no reason, one naming nothing, or one
                   naming an item an entry already covers.
  bad doc anchor   an entry whose `doc` is not a heading of docs/bingux-ui.md.
  policy           AGENTS.md or CONTRIBUTING.md no longer states the rule.

What counts as exported: every item written at column 0 as `pub fn`, `pub
const`, `pub struct`, `pub enum`, `pub trait`, `pub type` or `pub static` in a
module that is public from `lib.rs` down, plus any item of a private module
that a `pub use` re-exports. Methods are covered by their type's entry.

The allowlist is packages/bingux-taxonomy/coverage-allowlist.txt: one `name`
or `file:path/under/bingux-ui/src` per line, then `#` and the reason.

Run it from anywhere: `python3 scripts/taxonomy-check.py`. `--list` prints
every exported item with where it is covered.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
UI = ROOT / "packages/bingux-ui"
SRC = UI / "src"
TAXONOMY = ROOT / "packages/bingux-taxonomy"
REGISTRY = TAXONOMY / "src/registry.rs"
ALLOWLIST = TAXONOMY / "coverage-allowlist.txt"
DOCS = ROOT / "docs/bingux-ui.md"

ITEM = re.compile(
    r"^pub (?:const |async |unsafe )*(fn|struct|enum|trait|const|static|type) ([A-Za-z_][A-Za-z0-9_]*)"
)
PUB_USE = re.compile(r"^pub use ([^;]*);", re.M | re.S)
MOD = re.compile(r"^(pub(?:\(crate\))? )?mod ([a-z_0-9]+);", re.M)
# `pub use` re-exports of items from other crates (bingux-files) are not
# bingux-ui's own definitions; they are shown through their views.


def reexported(text):
    names = set()
    for match in PUB_USE.finditer(text):
        names.update(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", match.group(1)))
    return names


def walk(path, public, inherited, out):
    text = path.read_text()
    exported = inherited | reexported(text)
    in_tests = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.startswith("#[cfg(test)]"):
            in_tests = True
        item = ITEM.match(line)
        if item and not in_tests and (public or item.group(2) in inherited):
            out.append((item.group(2), item.group(1), str(path.relative_to(SRC)), number))
    base = path.parent if path.name in ("mod.rs", "lib.rs") else path.parent / path.stem
    for match in MOD.finditer(text):
        visibility, name = match.group(1), match.group(2)
        child = base / f"{name}.rs"
        if not child.exists():
            child = base / name / "mod.rs"
        if not child.exists():
            continue
        is_public = public and visibility == "pub "
        walk(child, is_public, inherited if is_public else exported, out)


def exports():
    out = []
    walk(SRC / "lib.rs", True, set(), out)
    # `#[cfg(feature = "testing")] pub mod snapshot;` is test tooling, but it is
    # still a public module: it must be covered or allowlisted like any other.
    return out


def icon_variants():
    text = (SRC / "icons.rs").read_text()
    enum = re.search(r"pub enum Icon \{(.*?)\n\}", text, re.S)
    variants = re.findall(r"^\s+([A-Z]\w*),", enum.group(1), re.M) if enum else []
    listed = re.search(r"pub const ALL: \[Icon; \d+\] = \[(.*?)\];", text, re.S)
    in_all = re.findall(r"Self::(\w+)", listed.group(1)) if listed else []
    in_bytes = set(re.findall(r"Self::(\w+)\s*=>", text))
    return variants, in_all, in_bytes


def registry_entries():
    """The entries of registry.rs: (name, doc, covers, body, line)."""
    text = REGISTRY.read_text()
    entries = []
    for match in re.finditer(r"Entry \{(.*?)\n    \},", text, re.S):
        block = match.group(1)
        line = text[: match.start()].count("\n") + 1
        name = re.search(r'name: "([^"]+)"', block)
        doc = re.search(r'doc: "([^"]+)"', block)
        covers = re.search(r"covers: &\[(.*?)\],\s*body", block, re.S)
        body = re.search(r"body: (.*?),\s*$", block, re.S)
        entries.append(
            (
                name.group(1) if name else "?",
                doc.group(1) if doc else "",
                re.findall(r'"([^"]+)"', covers.group(1)) if covers else [],
                body.group(1).strip() if body else "",
                line,
            )
        )
    return entries


def allowlist():
    names, files, problems = {}, {}, []
    if not ALLOWLIST.exists():
        return names, files, [f"{ALLOWLIST.relative_to(ROOT)} is missing"]
    for number, line in enumerate(ALLOWLIST.read_text().splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        what, _, reason = line.partition("#")
        what, reason = what.strip(), reason.strip()
        if not reason:
            problems.append(f"{ALLOWLIST.name}:{number}: `{what}` has no reason")
            continue
        if what.startswith("file:"):
            files[what[5:]] = (reason, number)
        else:
            names[what] = (reason, number)
    return names, files, problems


def slug(heading):
    # GitHub's anchor: lower case, punctuation dropped, spaces to dashes.
    heading = re.sub(r"[`*_]", "", heading.strip().lower())
    heading = re.sub(r"[^\w\s-]", "", heading)
    return re.sub(r"\s", "-", heading)


def doc_anchors():
    anchors = set()
    fenced = False
    for line in DOCS.read_text().splitlines():
        if line.startswith("```"):
            fenced = not fenced
        match = re.match(r"^#{1,6} (.*)$", line)
        if match and not fenced:
            anchors.add(slug(match.group(1)))
    return anchors


def main():
    listing = "--list" in sys.argv
    found = exports()
    by_name = {}
    for name, kind, file, line in found:
        by_name.setdefault(name, []).append((kind, file, line))

    entries = registry_entries()
    covered = {}
    for name, _doc, covers, _body, _line in entries:
        for item in covers:
            covered.setdefault(item, []).append(name)

    names, files, problems = allowlist()
    failures = list(problems)

    # Exported but neither covered nor allowlisted.
    for name in sorted(by_name):
        places = by_name[name]
        if name in covered:
            continue
        remaining = [p for p in places if p[1] not in files]
        if name in names or not remaining:
            continue
        kind, file, line = remaining[0]
        failures.append(
            f"uncovered: {kind} {name} (packages/bingux-ui/src/{file}:{line}) "
            "has no taxonomy entry; add it to `covers` in "
            "packages/bingux-taxonomy/src/registry.rs or allowlist it with a reason"
        )

    # Covered but not exported.
    for item, entry_names in sorted(covered.items()):
        if item not in by_name:
            failures.append(
                f"unknown cover: `{item}` in entry {entry_names[0]!r} is not exported by bingux-ui"
            )

    # Allowlist hygiene.
    for name, (_reason, number) in names.items():
        if name not in by_name:
            failures.append(f"{ALLOWLIST.name}:{number}: `{name}` is not exported by bingux-ui")
        elif name in covered:
            failures.append(
                f"{ALLOWLIST.name}:{number}: `{name}` is covered by entry {covered[name][0]!r}; remove it"
            )
    present = {file for _n, _k, file, _l in found}
    for file, (_reason, number) in files.items():
        if file not in present:
            failures.append(f"{ALLOWLIST.name}:{number}: file:{file} exports nothing public")

    # Icons: every variant is listed, embedded and shown by the icon set entry.
    variants, in_all, in_bytes = icon_variants()
    if not variants:
        failures.append("icons: could not read the Icon enum from icons.rs")
    for variant in variants:
        if variant not in in_all:
            failures.append(f"icon variant: Icon::{variant} is missing from Icon::ALL")
        if variant not in in_bytes:
            failures.append(f"icon variant: Icon::{variant} is missing from Icon::bytes()")
    shown = [e for e in entries if "Icon" in e[2] and "icon_set" in e[3]]
    if not shown:
        failures.append(
            "icon variant: no entry covers `Icon` with `Body::Custom(demos::icon_set::icon_set)`, "
            "the page that draws every icon file"
        )
    if not list((UI / "icons").glob("*-symbolic.svg")):
        failures.append("icon file: packages/bingux-ui/icons holds no icons")

    # Doc anchors.
    anchors = doc_anchors()
    for name, doc, _covers, _body, line in entries:
        if doc not in anchors:
            failures.append(
                f"bad doc anchor: entry {name!r} (registry.rs:{line}) points at "
                f"docs/bingux-ui.md#{doc}, which is not a heading"
            )

    # The requirement is written down where contributors read it.
    for path in (ROOT / "AGENTS.md", ROOT / "CONTRIBUTING.md"):
        if not path.exists() or "bingux-taxonomy" not in path.read_text():
            failures.append(f"policy: {path.name} does not state the bingux-taxonomy requirement")

    if listing:
        for name in sorted(by_name):
            where = covered.get(name) or (["allowlisted"] if name in names else ["-"])
            print(f"{name}\t{', '.join(where)}")
    if failures:
        print(f"taxonomy-check: {len(failures)} problem(s)", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print(
        f"taxonomy-check: {len(by_name)} exported items, {len(covered)} covered by "
        f"{len(entries)} entries, {len(names) + len(files)} allowlisted; "
        f"{len(variants)} icon variants"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
