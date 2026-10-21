#!/usr/bin/env python3
"""Fail on raw numbers in Bingux UI layout code.

Every spacing, radius, type size, icon size and box size comes from
packages/bingux-ui/src/scale.rs. A raw number in a layout call means a value
that is not on the scale, which is how a UI drifts. Add the value to the
scale first, then use its name.

Checked calls:
  .spacing .padding .margin .corner_radius .font_size   no literal except 0.0 and 1.0
  .width .height .min_* .max_*                           no literal except 0.0, 1.0 and "x / 2.0"
  icons::icon(glyph, SIZE, color)                        SIZE is a name
  FontWeight::BOLD                                       use SEMI_BOLD
Allowed everywhere: a half or double of a name ("BOX_SM / 2.0", "px * 2.0").
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES = [
    ROOT / "packages/bingux-ui/src",
    ROOT / "packages/bingux-ui/examples",
    ROOT / "packages/bingux-settings/src",
]
# The scale itself, the palette and the icon table define values; the
# snapshot runner is test tooling.
SKIP = {"scale.rs", "theme.rs", "icons.rs", "snapshot.rs"}

LITERAL = re.compile(r"(?<![\w.])(\d+\.\d+)(?![\w.])")
CALL = re.compile(
    r"\.(spacing|padding|margin|corner_radius|font_size|width|height|min_width|max_width|min_height|max_height)\("
)
ICON = re.compile(r"icons::icon\(")


def args_of(text: str, open_index: int) -> tuple[str, int]:
    depth = 0
    for index in range(open_index, len(text)):
        if text[index] == "(":
            depth += 1
        elif text[index] == ")":
            depth -= 1
            if depth == 0:
                return text[open_index + 1 : index], index
    return text[open_index + 1 :], len(text)


def literals(args: str, allow_scaling: bool) -> list[str]:
    found = []
    for match in LITERAL.finditer(args):
        value = match.group(1)
        if value in ("0.0", "1.0"):
            continue
        before = args[: match.start()].rstrip()
        after = args[match.end() :].lstrip()
        if allow_scaling and value == "2.0" and (before.endswith(("/", "*")) or after.startswith(("*", "/"))):
            continue
        found.append(value)
    return found


def check(path: Path) -> list[str]:
    text = path.read_text()
    problems = []

    def line_of(index: int) -> int:
        return text.count("\n", 0, index) + 1

    for match in CALL.finditer(text):
        method = match.group(1)
        args, _ = args_of(text, match.end() - 1)
        if method in ("width", "height", "min_width", "max_width", "min_height", "max_height"):
            if args.lstrip().startswith(("Size::percent(", "Size::flex(", "Size::window_percent(")):
                continue
            bad = literals(args, True)
        else:
            bad = literals(args, True)
        if bad:
            problems.append(
                f"{path.relative_to(ROOT)}:{line_of(match.start())}: .{method}({args.strip()[:50]}) uses {', '.join(bad)}"
            )
    for match in ICON.finditer(text):
        args, _ = args_of(text, match.end() - 1)
        parts = [part.strip() for part in args.split(",")]
        if len(parts) > 1 and LITERAL.fullmatch(parts[1]):
            problems.append(f"{path.relative_to(ROOT)}:{line_of(match.start())}: icon size {parts[1]} is a literal")
    for match in re.finditer(r"FontWeight::BOLD\b", text):
        problems.append(f"{path.relative_to(ROOT)}:{line_of(match.start())}: use FontWeight::SEMI_BOLD")
    return problems


def main() -> int:
    problems = []
    for source in SOURCES:
        for path in sorted(source.rglob("*.rs")):
            if path.name in SKIP or "target" in path.parts:
                continue
            problems.extend(check(path))
    for problem in problems:
        print(problem)
    if problems:
        print(f"\nui-lint: {len(problems)} raw value(s). Use a name from packages/bingux-ui/src/scale.rs.")
        return 1
    print("ui-lint: layout values all come from the scale.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
