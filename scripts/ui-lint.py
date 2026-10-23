#!/usr/bin/env python3
"""Fail when Bingux UI code styles a box outside the token system.

Every spacing, radius, size, colour, line, shadow and text style comes from
packages/bingux-ui/src/style/ (tokens.rs and recipes.rs) and
packages/bingux-ui/src/typography.rs. Boxes are styled with `.tw(&tokens,
RECIPE)` or `.tw(&tokens, &[px(S4), ...])`. Calling Freya's styling methods
directly is how a UI drifts, so this script fails on them.

Rules, for every file except the style system itself:
  styling calls      .padding .spacing .margin .corner_radius .rounded*
                     .background .border .shadow .font_size .font_weight .color
                     .direction .content .cross_align .main_align .center
                     .overflow                                      not allowed
  colour fields      tokens.text, tokens.accent, ...                 use Paint or Ink
  colour values      Color::..., FontWeight::...                     not allowed
  sizes              .width/.height/.min_*/.max_* with a number      use a Dim
                     literal (other than 0) or a token's .px()       utility
  icon sizes         icons::icon(glyph, SIZE, ..) needs IconSize::

A run-time value (a measured width, a drag offset, an animation) may still be
passed to Size::px, offset_x, offset_y, opacity and rotate.
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
# The style system defines the values; the palette and icon table hold raw
# colours and SVG; the snapshot runner is test tooling.
SKIP_DIRS = {ROOT / "packages/bingux-ui/src/style"}
SKIP_FILES = {
    ROOT / "packages/bingux-ui/src/theme.rs",
    ROOT / "packages/bingux-ui/src/typography.rs",
    ROOT / "packages/bingux-ui/src/icons.rs",
    ROOT / "packages/bingux-ui/src/snapshot.rs",
    ROOT / "packages/bingux-ui/src/surface.rs",
    ROOT / "packages/bingux-ui/src/charts/palette.rs",
    ROOT / "packages/bingux-ui/src/charts/svg.rs",
}

STYLING = re.compile(
    r"\.(padding|spacing|margin|corner_radius|rounded(?:_[a-z0-9]+)?|background|border|shadow|"
    r"font_size|font_weight|color|direction|content|cross_align|main_align|overflow)\(|\.center\(\)"
)
COLOUR_FIELD = re.compile(
    r"\btokens\.(bg|surface|sidebar|elevated|control|separator|card|hover|pressed|selection|border|"
    r"outline|text|muted|bar_divider|success|warning|danger|accent_foreground|tooltip_surface|"
    r"tooltip_outline|popover|accent)\b(?!_name)"
)
RAW_VALUE = re.compile(r"\b(Color::[A-Za-z_]+|FontWeight::[A-Z_]+)")
SIZE_CALL = re.compile(r"\.(width|height|min_width|max_width|min_height|max_height)\(")
LITERAL = re.compile(r"(?<![\w.])(\d+\.\d+|\d+)(?![\w.])")
ICON = re.compile(r"icons::icon\(")


def args_of(text: str, open_index: int) -> str:
    depth = 0
    for index in range(open_index, len(text)):
        if text[index] == "(":
            depth += 1
        elif text[index] == ")":
            depth -= 1
            if depth == 0:
                return text[open_index + 1 : index]
    return text[open_index + 1 :]


def strip_comments(text: str) -> str:
    # Blank out line comments so doc examples are not checked; keep offsets.
    return re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), text)


def check(path: Path) -> list[str]:
    text = strip_comments(path.read_text())
    problems = []
    rel = path.relative_to(ROOT)

    def report(index: int, message: str) -> None:
        problems.append(f"{rel}:{text.count(chr(10), 0, index) + 1}: {message}")

    for match in STYLING.finditer(text):
        # `Paint::X.color(&tokens)` and `Ink::X.color(&tokens)` resolve a
        # token; only Freya's `.color(value)` builder is styling.
        args = args_of(text, match.end() - 1).strip() if match.group(1) else ""
        if match.group(1) == "color" and (args.startswith("&") or args == "tokens"):
            continue
        # Component methods that share a name (a dialog's `.content(..)`).
        if match.group(1) == "content" and not args.startswith("Content::"):
            continue
        report(match.start(), f"{match.group(0).rstrip('(')}(..) styles a box directly; use .tw(&tokens, ..)")
    for match in COLOUR_FIELD.finditer(text):
        report(match.start(), f"{match.group(0)} is a raw colour; use Paint::.. or Ink::..")
    for match in RAW_VALUE.finditer(text):
        if match.group(0) in ("Color::TRANSPARENT",):
            report(match.start(), "Color::TRANSPARENT; use Paint::Transparent")
            continue
        report(match.start(), f"{match.group(0)} is a raw value; use a token")
    for match in SIZE_CALL.finditer(text):
        args = args_of(text, match.end() - 1)
        if args.strip().startswith(("Size::percent(", "Size::flex(")):
            continue
        numbers = [m.group(1) for m in LITERAL.finditer(args) if m.group(1) not in ("0", "0.0")]
        if numbers or ".px()" in args:
            report(
                match.start(), f".{match.group(1)}({args.strip()[:40]}) uses a fixed size; use w(Dim::..)/h(Dim::..)"
            )
    for match in ICON.finditer(text):
        parts = [part.strip() for part in args_of(text, match.end() - 1).split(",")]
        if len(parts) > 1 and not parts[1].startswith(("IconSize::", "size", "self.size")):
            if LITERAL.search(parts[1]):
                report(match.start(), f"icon size {parts[1]} is not an IconSize")
    return problems


def skipped(path: Path) -> bool:
    return path in SKIP_FILES or any(parent in SKIP_DIRS for parent in path.parents)


def main() -> int:
    problems = []
    for source in SOURCES:
        for path in sorted(source.rglob("*.rs")):
            if "target" in path.parts or skipped(path):
                continue
            problems.extend(check(path))
    for problem in problems:
        print(problem)
    if problems:
        print(f"\nui-lint: {len(problems)} problem(s). Style boxes with tokens from packages/bingux-ui/src/style/.")
        return 1
    print("ui-lint: every box is styled with tokens.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
