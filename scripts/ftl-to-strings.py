#!/usr/bin/env python3
"""Exports a Fluent catalogue to the myvoice strings TSV (`kind<TAB>text`).

    scripts/ftl-to-strings.py crates/ui/locale/en-GB/bingux-ui.ftl \\
        | node ~/dev/myvoice/dist/index.js strings - --profile app-copy

The kind of each string comes from its name (docs/bingux-ui.md,
"Localisation"):

- an attribute is its own kind: `.tooltip`, `.a11y`, `.placeholder`,
  `.title`, `.description` ...;
- otherwise the ID's last segment, when it names a kind: `files-empty-title`
  is a title, `files-delete-confirm` a confirmation, `ui-chooser-missing-error`
  an error, `ui-tab-close-button` a button, `ui-tab-close` plain text;
- anything else is `text`. `heading` counts as title; `caption`, `body` and
  `hint` as description.

A message with a select expression gives one line per variant. A
placeable is written as `{name}` so the linter reads it as a slot. Terms
(`-brand-name`) are skipped.
"""

from __future__ import annotations

import importlib.util
import re
import sys
from pathlib import Path

KINDS = {
    "button",
    "menu",
    "title",
    "label",
    "subtitle",
    "description",
    "tooltip",
    "placeholder",
    "empty",
    "error",
    "confirm",
    "notification",
    "status",
    "a11y",
    "onboarding",
    "text",
}
ALIASES = {"heading": "title", "caption": "description", "body": "description", "hint": "description"}

spec = importlib.util.spec_from_file_location("ftl_check", Path(__file__).with_name("ftl-check.py"))
assert spec and spec.loader
ftl_check = importlib.util.module_from_spec(spec)
sys.modules["ftl_check"] = ftl_check
spec.loader.exec_module(ftl_check)

PLACEABLE = re.compile(r"\{\s*(?:NUMBER|DATETIME)?\(?\s*\$([a-zA-Z0-9_-]+)[^{}]*\}")
TERM = re.compile(r"\{\s*-([a-zA-Z0-9_-]+)\s*\}")
VARIANT = re.compile(r"^\*?\[[^\]]+\]\s*(.*)$")


def kind_of(identifier: str, attribute: str | None) -> str:
    """The myvoice kind of a message or attribute, from its name."""
    if attribute is not None:
        name = ALIASES.get(attribute, attribute)
        if name in KINDS:
            return name
    segment = identifier.rsplit("-", 1)[-1]
    segment = ALIASES.get(segment, segment)
    return segment if segment in KINDS else "text"


def texts(lines: list[str], terms: dict[str, str]) -> list[str]:
    """The readable text of a value: one entry per variant of its first
    select expression, with the text around the select kept."""

    def clean(text: str) -> str:
        text = TERM.sub(lambda match: terms.get(match.group(1), match.group(1)), text)
        return " ".join(PLACEABLE.sub(lambda match: "{" + match.group(1) + "}", text).split())

    joined = "\n".join(line for line in lines if line)
    if not joined.strip():
        return []
    select = re.search(r"\{\s*\$[a-zA-Z0-9_-]+\s*->", joined)
    if select is None:
        return [clean(joined)]
    depth, end = 0, len(joined)
    for index in range(select.start(), len(joined)):
        depth += {"{": 1, "}": -1}.get(joined[index], 0)
        if depth == 0:
            end = index + 1
            break
    before, body, after = joined[: select.start()], joined[select.end() : end - 1], joined[end:]
    variants = [match.group(1) for line in body.splitlines() if (match := VARIANT.match(line.strip()))]
    return [clean(before + variant + after) for variant in variants]


def export(path: Path) -> list[tuple[str, str]]:
    catalogue = ftl_check.parse(path)
    terms = {
        identifier[1:]: " ".join(entry.value).strip()
        for identifier, entry in catalogue.entries.items()
        if identifier.startswith("-")
    }
    rows = []
    for identifier, entry in catalogue.entries.items():
        if identifier.startswith("-"):
            continue
        rows += [(kind_of(identifier, None), text) for text in texts(entry.value, terms)]
        for attribute, lines in entry.attributes.items():
            rows += [(kind_of(identifier, attribute), text) for text in texts(lines, terms)]
    return rows


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    for argument in sys.argv[1:]:
        for kind, text in export(Path(argument)):
            print(f"{kind}\t{text}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
