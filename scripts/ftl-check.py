#!/usr/bin/env python3
"""Checks every Fluent catalogue (`locale/<language>/<domain>.ftl`) in the tree.

For each catalogue directory (crates/ui/locale, apps/*/locale) the source
language is en-US. The check fails on:

- syntax: a line that is not a comment, a message, a term, an attribute, a
  variant or the continuation of one; a message with no value and no
  attribute; unbalanced braces;
- duplicates: the same message, term or attribute twice in one file;
- orphans: a translation with a message, term or attribute the source does
  not have, or a variable (`$name`) the source message does not use;
- a translation file with no source file.

Messages a translation does not have yet fall back to en-US at run time, so
they are listed per language as a count (`--missing` lists them), not an
error. Which IDs the code uses is checked by `bingux_ui::i18n::check` in
each crate's tests.

Usage: scripts/ftl-check.py [--missing] [--self-test] [DIR...]
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = "en-US"
IDENT = r"-?[a-zA-Z][a-zA-Z0-9_-]*"
ENTRY = re.compile(rf"^({IDENT})\s*=\s?(.*)$")
ATTRIBUTE = re.compile(r"^\s+\.([a-zA-Z][a-zA-Z0-9_-]*)\s*=\s?(.*)$")
VARIABLE = re.compile(r"\$([a-zA-Z][a-zA-Z0-9_-]*)")


@dataclass
class Entry:
    id: str
    line: int
    value: list[str] = field(default_factory=list)
    attributes: dict[str, list[str]] = field(default_factory=dict)

    def variables(self, attribute: str | None = None) -> set[str]:
        text = "\n".join(self.value if attribute is None else self.attributes[attribute])
        return set(VARIABLE.findall(text))


@dataclass
class Catalogue:
    path: Path
    entries: dict[str, Entry] = field(default_factory=dict)
    problems: list[str] = field(default_factory=list)


def parse(path: Path, text: str | None = None) -> Catalogue:
    """Reads one .ftl file into its entries, noting syntax problems."""
    catalogue = Catalogue(path)
    text = path.read_text(encoding="utf-8") if text is None else text
    current: Entry | None = None
    target: list[str] | None = None

    def problem(line: int, message: str) -> None:
        catalogue.problems.append(f"{path}:{line}: {message}")

    def close(line: int) -> None:
        if current is None:
            return
        body = "\n".join(current.value + [v for a in current.attributes.values() for v in a])
        if body.count("{") != body.count("}"):
            problem(current.line, f"unbalanced braces in `{current.id}`")
        if not "".join(current.value).strip() and not current.attributes:
            problem(current.line, f"`{current.id}` has no value and no attribute")
        if current.id.startswith("-") and not "".join(current.value).strip():
            problem(current.line, f"term `{current.id}` has no value")

    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            target = None if current is None else target
            continue
        if line.startswith("#"):
            close(number)
            current, target = None, None
            continue
        if entry := ENTRY.match(line):
            close(number)
            identifier, value = entry.groups()
            if identifier in catalogue.entries:
                problem(number, f"`{identifier}` is defined twice (first on line {catalogue.entries[identifier].line})")
            current = Entry(identifier, number, [value])
            catalogue.entries[identifier] = current
            target = current.value
            continue
        if current is not None and (attribute := ATTRIBUTE.match(line)):
            name, value = attribute.groups()
            if name in current.attributes:
                problem(number, f"`{current.id}.{name}` is defined twice")
            current.attributes[name] = [value]
            target = current.attributes[name]
            continue
        # A select expression's closing brace may start a line.
        if current is not None and target is not None and (line[0] in " \t" or line.startswith("}")):
            if line.strip().startswith("#"):
                problem(number, "a comment inside a message is read as its text; move it above the message")
            target.append(line.strip())
            continue
        problem(number, f"not Fluent: {line.strip()[:60]!r}")
    close(0)
    return catalogue


def catalogue_dirs(arguments: list[str]) -> list[Path]:
    if arguments:
        return [Path(argument) for argument in arguments]
    dirs = [ROOT / "crates/ui/locale"]
    dirs += sorted(path for path in (ROOT / "apps").glob("*/locale") if path.is_dir())
    return [path for path in dirs if path.is_dir()]


def check_dir(locale_dir: Path, list_missing: bool) -> tuple[list[str], list[str]]:
    """Returns (errors, notes) for one locale directory."""
    errors: list[str] = []
    notes: list[str] = []
    sources = {path.name: parse(path) for path in sorted((locale_dir / SOURCE).glob("*.ftl"))}
    for catalogue in sources.values():
        errors += catalogue.problems
    for language_dir in sorted(path for path in locale_dir.iterdir() if path.is_dir()):
        if language_dir.name == SOURCE:
            continue
        for path in sorted(language_dir.glob("*.ftl")):
            translation = parse(path)
            errors += translation.problems
            source = sources.get(path.name)
            if source is None:
                errors.append(f"{path}: no source catalogue {locale_dir / SOURCE / path.name}")
                continue
            for identifier, entry in translation.entries.items():
                original = source.entries.get(identifier)
                if original is None:
                    errors.append(f"{path}:{entry.line}: `{identifier}` is not in the source catalogue")
                    continue
                extra = entry.variables() - original.variables()
                if extra:
                    errors.append(
                        f"{path}:{entry.line}: `{identifier}` uses ${', $'.join(sorted(extra))}, which the source does not pass"
                    )
                for attribute in entry.attributes:
                    if attribute not in original.attributes:
                        errors.append(f"{path}:{entry.line}: `{identifier}.{attribute}` is not in the source catalogue")
                    elif extra := entry.variables(attribute) - original.variables(attribute):
                        errors.append(
                            f"{path}:{entry.line}: `{identifier}.{attribute}` uses ${', $'.join(sorted(extra))}, which the source does not pass"
                        )
            missing = [i for i in source.entries if i not in translation.entries and not i.startswith("-")]
            if missing:
                notes.append(f"{path}: {len(missing)} of {len(source.entries)} messages fall back to {SOURCE}")
                if list_missing:
                    notes += [f"  {identifier}" for identifier in missing]
    return errors, notes


def self_test() -> None:
    good = parse(
        Path("good.ftl"),
        "## Group\n# Comment\nui-a = A { $n }\n    .tooltip = Tip\nui-b = { $count ->\n    [one] One\n   *[other] Many\n}\n-brand = Bingux\n",
    )
    assert not good.problems, good.problems
    assert good.entries["ui-a"].variables() == {"n"}
    assert "tooltip" in good.entries["ui-a"].attributes
    assert good.entries["ui-b"].variables() == {"count"}
    bad = parse(Path("bad.ftl"), "ui-a = A\nui-a = B\nwhat is this\nui-c = { $x\nui-d =\n")
    joined = "\n".join(bad.problems)
    for expected in ("defined twice", "not Fluent", "unbalanced", "no value"):
        assert expected in joined, (expected, joined)
    print("ftl-check self-test: ok")


def main() -> int:
    arguments = sys.argv[1:]
    if "--self-test" in arguments:
        self_test()
        return 0
    list_missing = "--missing" in arguments
    arguments = [argument for argument in arguments if not argument.startswith("--")]
    errors: list[str] = []
    for locale_dir in catalogue_dirs(arguments):
        found, notes = check_dir(locale_dir, list_missing)
        errors += found
        for note in notes:
            print(note)
    if errors:
        print(f"ftl-check: {len(errors)} problem(s)")
        for error in errors:
            print(f"  {error}")
        return 1
    print("ftl-check: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
