#!/usr/bin/env python3
"""Fail on copy that breaks the written rules about words in the apps.

CK-06 of design/rules.md. The rules this holds are Kieran's, by ID:

  TX-05  percent-space      no space before a percent sign: "100%", never "100 %"
  TX-06  filler, instruction-subtitle, subtitle-period, subtitle-long
         copy sounds like a person: no filler or cliché words, a subtitle states a
         value and does not give an instruction (docs/settings.md, "Copy"), a
         one-line subtitle has no full stop and is short
  TX-07  subtitle-restates  a subtitle that says again what its title says is removed
  TX-09  computer-label     the local place is the hostname, not "Computer"
  TX-10  show-more          no "Show N more" button: long lists lazy-load
  TX-11  password-mask      a password field shows bullets, not asterisks
  TX-01  value-ellipsis     a value is never cut with an ellipsis (the layout lint
                            holds the rendered cut; this holds a value that is
                            written with one)
  also   exclamation, click-here, three-dots, title-case, quote-mix
         no exclamation marks, no "click here" (say what the control does), the
         ellipsis is the one character U+2026 (136 strings use it, so three dots
         are the odd one out), sentence case rather than Title Case, and one string
         does not mix straight and curly quotes

Two sources:

  * the source: every string literal in the apps' and bingux-ui's non-test Rust
    that reads as copy (a lexer skips comments, attributes, `#[cfg(test)]` blocks,
    log and panic messages, command arguments and test files),
  * the captures (`--captures`, on by default when the three binaries are built):
    the text of every label in every Settings page, popup, taxonomy page and Files
    state (BINGUX_UI_PROBE, scripts/probe_lib.py), which is also where a subtitle
    can be held against its title, as a row lays the two out.

    python3 scripts/copy-lint.py                 # source and captures
    python3 scripts/copy-lint.py --no-captures   # source only: fast, needs nothing built
    python3 scripts/copy-lint.py --fixtures      # the rules on known-bad and clean strings only
    python3 scripts/copy-lint.py --show-allowed

Exit status is 1 on a finding that ALLOWED does not cover, an ALLOWED entry that
matches nothing, and a fixture that is not judged as it should be. Fix the string:
an entry in ALLOWED needs its reason (a name that has to be quoted as it is, a
sentence that is a licence text).
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import probe_lib as lib  # noqa: E402
from probe_lib import Allow, Finding  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
SOURCES = [
    "apps/settings/src",
    "apps/files/src",
    "crates/ui/src",
    "apps/taxonomy/src",
]

# ---- the rules ------------------------------------------------------------------

# Words that mark copy written to sound like a feature list, not like a person.
FILLER = re.compile(
    r"\b(?:here you can|you can (?:also )?(?:easily|simply|quickly)|simply|easily|effortlessly|seamless(?:ly)?|"
    r"powerful|robust|cutting-edge|state-of-the-art|leverage|elevate|streamline|supercharge|delve|"
    r"curated|crafted|empower(?:s|ed)?|whether you(?:'re| are)|in order to|at a glance|ensure that|"
    r"enhance your|take (?:full )?control|make it your own|to your liking|best experience|oops|sorry|"
    r"unfortunately|successfully|please note|feel free|something went wrong|your digital)\b",
    re.I,
)
INSTRUCTION = re.compile(
    r"^(?:Choose|Select|Use|Click|Tap|Press|Enable|Disable|Turn|Allow|Let|Make|Control|Manage|Customi[sz]e|"
    r"Configure|Set up|Pick|Change|Adjust|Enter|Type)\b"
)
CLICK_HERE = re.compile(r"\b(?:click|tap|press) here\b", re.I)
PERCENT_SPACE = re.compile(r"(?:\d|\})\s%")
SHOW_MORE = re.compile(r"\b(?:show|load|see|view)\s+(?:\d+|\{[^}]*\}|more)\s+more\b|\b(?:show|load|see) more\b", re.I)
EXCLAMATION = re.compile(r"(?<![(\[<])[A-Za-z0-9)\u2026.]!(?:\s|$)")
THREE_DOTS = re.compile(r"(?<!\.)\.\.\.(?!\.)")
ASCII_QUOTE = re.compile('"')
CURLY_QUOTE = re.compile("[\u201c\u201d\u2018\u2019]")
ASTERISKS = re.compile(r"^\*{2,}$")
COMPUTER = re.compile(r"^Computer$")
# Words of a title that keep their capital in a sentence: names of products, standards and the like.
PROPER = {
    "Wi-Fi", "Bluetooth", "Linux", "GNOME", "Bingux", "Gnoblin", "Files", "Settings", "Trash", "Wayland", "PIN",
    "PDF", "USB", "VPN", "DNS", "HTTP", "HTTPS", "SSID", "MAC", "IPv4", "IPv6", "SIM", "APN", "NFS", "SMB", "FTP",
    "SSH", "GPU", "CPU", "RAM", "HDMI", "Fedora", "Arch", "Flatpak", "Snap", "Mac", "Windows", "Google", "Adwaita",
    "Mutter", "PipeWire", "PulseAudio", "NetworkManager", "ModemManager", "BlueZ", "GTK", "Qt", "Freya", "Rust",
    "Quickshell", "Hz", "MB", "GB", "KB", "TB", "I", "OK", "AM", "PM", "Enter", "Escape", "Tab", "Shift", "Ctrl", "Alt",
    "Super", "Backspace", "Delete", "Home", "End", "Page", "Up", "Down", "Left", "Right", "Space", "Return",
    "Num", "Lock", "Caps", "Scroll",
}
SENTENCE_WORD = re.compile(r"^[A-Z][a-z]+(?:[-'][A-Za-z]+)?$")
STOP = {
    "a", "an", "the", "of", "to", "for", "and", "or", "is", "are", "on", "in", "with", "your", "you", "this", "that",
    "it", "its", "at", "by", "as", "be", "from", "off",
}

# Calls whose strings are not copy: logs, panics, commands, environment, asserts.
NOT_COPY = re.compile(
    r"(?:(?:eprintln|println|eprint|print|panic|assert\w*|debug_assert\w*|unreachable|todo|unimplemented|include_str|env|"
    r"concat|bail|anyhow|warn|info|error|debug|trace)!\(\s*|(?:tracing|log)::\w+!\(\s*|"
    r"\.expect\(\s*|\.expect_err\(\s*|Command::new\(\s*|\.arg\(\s*|\.args\(\s*|env::var\w*\(\s*|\bvar_os\(\s*|"
    r"\bvar\(\s*)$"
)

# Findings that are right by design: rule, capture regex (file:line or capture name), target regex, reason.
ALLOWED: list[dict] = [
    {"rule": "computer-label", "capture": r"bluetooth\.rs", "where": r"Computer", "why": "A Bluetooth device's class (phone, headset, computer): the kind of a peer, not a name for this machine."},
    {"rule": "computer-label", "capture": r"files/named\.rs", "where": r"Computer", "why": "Root::name is the canonical name of the file system root in history rows and tab titles; the crumb and the address bar already say host_name() (docs/files.md, 'Computer | file:/// | the computer's name'). TX-09 stays open for the history menu and tab title."},
    {"rule": "computer-label", "capture": r"inputs\.rs|controls_selection", "where": r"Computer", "why": "A sample option in the taxonomy's selection specimens."},
    {"rule": "filler", "capture": r"resilience/(boundary|mod)\.rs", "where": r"Something went wrong", "why": "The last-resort crash toast, documented in docs/bingux-ui.md ('Panics are never fatal') and held by resilience/tests.rs; the text after the colon says what failed."},
    {"rule": "show-more", "capture": r"preview/viewers/text\.rs", "where": r"Show more", "why": "The text preview reads a big file in chunks and the button asks for the next one: a file's bytes, not a list (TX-10 is about lists, which lazy-load). A scroll-triggered read would be the better design: open."},
    {"rule": "show-more", "capture": r"foundations\.rs|foundations_motion", "where": r"Show more", "why": "The label of a switch in the Reveal specimen, which shows a block that opens and closes; it loads nothing."},
    {"rule": "title-case", "capture": r"vpn/plugins\.rs", "where": r"Juniper Network Connect|Pulse Connect Secure", "why": "Product names of two VPN clients."},
    {"rule": "three-dots", "capture": r"apps/taxonomy/src/main\.rs", "where": r"Usage: bingux-taxonomy", "why": "Command-line usage text: `[PAGE,PAGE...]` is syntax, not an ellipsis in copy."},
    {"on": "captures", "rule": "value-ellipsis", "capture": r"settings-background-desktop", "where": r"Add picture", "why": "A button label: the ellipsis says it opens a file chooser."},
    {"on": "captures", "rule": "(instruction|subtitle)-.*|subtitle-period", "capture": r"controls_text-input", "where": r".*", "why": "The validators' messages, shown under the field that failed: an error says what to enter and ends as a sentence (they are error text, not row subtitles)."},
    {"on": "captures", "rule": "subtitle-restates", "capture": r"containers_rows|containers_groups-and-cards", "where": r"ネットワーク|A name far too long", "why": "Stress specimens: one long or CJK string is the title and the subtitle on purpose, to show wrapping."},
]

FIXTURE_BAD = {
    "percent-space": "Battery at 87 %",
    "percent-space-fmt": "{} %",
    "show-more": "Show 60 more",
    "click-here": "Click here to change the wallpaper",
    "exclamation": "Saved!",
    "three-dots": "Choose...",
    "filler": "Easily manage your seamless experience",
    "title-case": "Add An Input Source",
    "quote-mix": 'Turn off \u201cAirplane mode" first',
    "password-mask": "****",
    "computer-label": "Computer",
}
FIXTURE_CLEAN = [
    "Battery at 87%",
    "Choose\u2026",
    "Add an input source",
    "Turn off \u201cAirplane mode\u201d first",
    "Wi-Fi and Bluetooth",
    "Moved \u201creport.txt\u201d to Trash",
    "{n} of {total}",
    "Connected",
]


# ---- lexer ------------------------------------------------------------------------


class Literal:
    __slots__ = ("line", "value", "before")

    def __init__(self, line: int, value: str, before: str):
        self.line, self.value, self.before = line, value, before


def literals(text: str) -> list[Literal]:
    """The string literals of one Rust file outside comments, attributes and test blocks."""
    found: list[Literal] = []
    i, n, line = 0, len(text), 1
    depth = 0
    skip_depth: list[int] = []  # brace depths at which a test block opened
    pending_test = False
    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if text.startswith("/*", i):
            level, i = 1, i + 2
            while i < n and level:
                if text.startswith("/*", i):
                    level, i = level + 1, i + 2
                elif text.startswith("*/", i):
                    level, i = level - 1, i + 2
                else:
                    line += text[i] == "\n"
                    i += 1
            continue
        if c == "#" and i + 1 < n and text[i + 1] in "[!":
            j = i + 1 + (text[i + 1] == "!")
            if j < n and text[j] == "[":
                end = text.find("]", j)
                attr = text[j : end + 1 if end >= 0 else n]
                if re.search(r"cfg\(\s*test\s*\)|cfg\(all\(test|^\[test\]|cfg\(any\(test", attr):
                    pending_test = True
                line += attr.count("\n")
                i = j + len(attr)
                continue
        if c == "'":
            # A char literal or a lifetime.
            if i + 1 < n and text[i + 1] == "\\":
                j = text.find("'", i + 2)
                i = (j if j >= 0 else n) + 1
                continue
            if i + 2 < n and text[i + 2] == "'":
                i += 3
                continue
            i += 1
            continue
        raw = re.match(r'b?r(#*)"', text[i : i + 12])
        if raw and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = raw.group(1)
            start = i + raw.end()
            end = text.find('"' + hashes, start)
            end = n if end < 0 else end
            value = text[start:end]
            if not skip_depth:
                found.append(Literal(line, value, text[max(0, i - 80) : i]))
            line += text.count("\n", i, end)
            i = end + 1 + len(hashes)
            continue
        if c == '"' or (c == "b" and text.startswith('b"', i)):
            if c == "b":
                i += 1
            start = i + 1
            j = start
            buf = []
            while j < n and text[j] != '"':
                if text[j] == "\\" and j + 1 < n:
                    esc = text[j + 1]
                    if esc == "\n":  # a line continuation swallows the next line's indent
                        line += 1
                        j += 2
                        while j < n and text[j] in " \t\n":
                            line += text[j] == "\n"
                            j += 1
                        continue
                    if esc == "u":
                        m = re.match(r"u\{([0-9a-fA-F]+)\}", text[j + 1 : j + 12])
                        if m:
                            buf.append(chr(int(m.group(1), 16)))
                            j += 1 + m.end()
                            continue
                    mapping = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", "\\": "\\", '"': '"', "'": "'"}
                    buf.append(mapping.get(esc, esc))
                    j += 2
                    continue
                line += text[j] == "\n"
                buf.append(text[j])
                j += 1
            if not skip_depth:
                found.append(Literal(line, "".join(buf), text[max(0, i - 80) : i]))
            i = j + 1
            continue
        if c == "{":
            depth += 1
            if pending_test:
                skip_depth.append(depth)
                pending_test = False
        elif c == "}":
            if skip_depth and skip_depth[-1] == depth:
                skip_depth.pop()
            depth -= 1
        elif c == ";" and pending_test:
            pending_test = False  # `#[cfg(test)] mod tests;` or a use: nothing to skip here
        i += 1
    return found


def is_test_file(path: Path) -> bool:
    name = path.name
    return name == "tests.rs" or name.endswith("_tests.rs") or "tests" in path.parts or "examples" in path.parts or "benches" in path.parts


def reads_as_copy(value: str) -> bool:
    text = value.strip()
    if not text or not re.search(r"[A-Za-z]", text):
        return False
    if "://" in text or text.startswith(("/", "~", ".", "#", "--", "$")) or text.endswith((".rs", ".svg", ".png", ".desktop")):
        return False
    if re.fullmatch(r"[a-z0-9_.:\-+/]+", text):
        return False
    if "::" in text or re.search(r"[=<>]{1}\S", text) and " " not in text:
        return False
    return " " in text or text[0].isupper()


# ---- the rules on one string --------------------------------------------------------


def judge(value: str, subtitle: bool = False, titles: bool = True) -> list[tuple[str, list[str], str]]:
    """The findings on one string: (rule key, rule IDs, what)."""
    out: list[tuple[str, list[str], str]] = []
    text = value.strip()
    if PERCENT_SPACE.search(text):
        out.append(("percent-space", ["TX-05"], 'a space before "%": write "100%"'))
    if SHOW_MORE.search(text):
        out.append(("show-more", ["TX-10"], 'no "show more" button: long lists lazy-load on scroll'))
    if CLICK_HERE.search(text):
        out.append(("click-here", ["TX-06"], 'no "click here": say what the control does'))
    if EXCLAMATION.search(text):
        out.append(("exclamation", ["TX-06"], "no exclamation marks: copy is calm and direct"))
    if THREE_DOTS.search(text):
        out.append(("three-dots", ["TX-06"], 'three full stops: write the one ellipsis character "\u2026"'))
    match = FILLER.search(text)
    if match:
        out.append(("filler", ["TX-06"], f'filler or cliché "{match.group(0)}": say it the way a person would'))
    if ASTERISKS.match(text):
        out.append(("password-mask", ["TX-11"], "a password field shows bullets (U+2022), not asterisks"))
    if COMPUTER.match(text):
        out.append(("computer-label", ["TX-09"], "the local place is called by the hostname, not Computer"))
    if ASCII_QUOTE.search(text) and CURLY_QUOTE.search(text):
        out.append(("quote-mix", ["TX-08"], "straight and curly quotes in one string: use one style"))
    words = re.findall(r"[A-Za-z][A-Za-z'\-]*", text)
    # One short sentence: a heading, a button, a menu item. A longer text begins sentences with capitals.
    if titles and 3 <= len(words) <= 5 and text[0].isupper() and not re.search(r"[.:;?!/\n(\[{]", text):
        capitals = [w for w in words[1:] if SENTENCE_WORD.match(w) and w not in PROPER]
        if len(capitals) >= 2 and len(capitals) * 2 >= len(words) - 1:
            out.append(("title-case", ["TX-06"], f"Title Case ({', '.join(capitals[:3])}): write sentences in sentence case"))
    if subtitle:
        if INSTRUCTION.match(text) and "+" not in text:
            out.append(("instruction-subtitle", ["TX-06"], "a subtitle states a value or a state; it does not instruct or explain a setting"))
        if text.endswith(".") and text.count(". ") == 0 and len(text) < 90:
            out.append(("subtitle-period", ["TX-06"], "a one-line subtitle has no full stop"))
        if len(text) > 90:
            out.append(("subtitle-long", ["TX-06"], f"a subtitle of {len(text)} characters: say less"))
    return out


def tokens(text: str) -> set[str]:
    return {w for w in re.findall(r"[a-z0-9]+", text.lower()) if w not in STOP}


def restates(title: str, subtitle: str) -> float:
    """How much of the subtitle's words the title already has."""
    sub = tokens(subtitle)
    if not sub:
        return 0.0
    return len(sub & tokens(title)) / len(sub)


# ---- source ---------------------------------------------------------------------------


def source_findings(files: list[Path]) -> tuple[list[Finding], int]:
    found: list[Finding] = []
    count = 0
    for path in files:
        if is_test_file(path):
            continue
        rel = path.relative_to(ROOT)
        text = path.read_text(errors="replace")
        for literal in literals(text):
            if NOT_COPY.search(literal.before) or not reads_as_copy(literal.value):
                continue
            count += 1
            subtitle = bool(re.search(r"\.subtitle\(\s*(?:format!\(\s*)?$", literal.before))
            for rule, ids, what in judge(literal.value, subtitle):
                shown = literal.value if len(literal.value) <= 70 else literal.value[:67] + "..."
                found.append(Finding(rule, ids, f"{rel}:{literal.line}", f'"{shown}"', what))
    return found, count


def source_files() -> list[Path]:
    files: list[Path] = []
    for root in SOURCES:
        files += sorted((ROOT / root).rglob("*.rs"))
    return files


# ---- captures -------------------------------------------------------------------------

SUBTITLED = {"Row", "PageHead", "StatusPage", "PropertyRow", "Banner", "ProfileCard"}
ACTION_PARENTS = {"ActionButton", "MenuRow", "DropdownItem", "SidebarRow", "IconButton", "Row", "TabFrame"}


def capture_findings(probes: list) -> tuple[list[Finding], int]:
    seen: set[tuple[str, str]] = set()
    found: list[Finding] = []
    texts = 0

    def add(rule: str, ids: list[str], probe, target: str, what: str, numbers: dict | None = None) -> None:
        key = (rule, target)
        if key in seen:
            return
        seen.add(key)
        found.append(Finding(rule, ids, probe.name, target, what, numbers or {}))

    for probe in probes:
        children = probe.children()
        text_nodes = [at for at, node in enumerate(probe.nodes) if node[6] & lib.TEXT]
        for at in text_nodes:
            if not probe.flags(at) & lib.VISIBLE or probe.flags(at) & lib.INERT:
                continue
            value = probe.text(at)
            if not reads_as_copy(value) and not ASTERISKS.match(value.strip()):
                continue
            texts += 1
            shown = value if len(value) <= 70 else value[:67] + "..."
            # Names of apps, fonts, places and networks are data, not copy: Title Case is held in the source only.
            for rule, ids, what in judge(value, titles=False):
                add(rule, ids, probe, f'"{shown}"', what)
            stripped = value.strip()
            if stripped.endswith(("\u2026", "...")) and stripped not in ("\u2026", "..."):
                parent = probe.title(probe.parent(at)) if probe.parent(at) >= 0 else ""
                holders = {probe.title(a) for a in probe.ancestors(at)}
                if not holders & ACTION_PARENTS and parent not in ACTION_PARENTS:
                    add("value-ellipsis", ["TX-01"], probe, f'"{shown}"', "a value ends in an ellipsis: show it in full, wrapped or stacked")
        # A subtitle is the text under the first line of a titled component, at the same left edge.
        for at, node in enumerate(probe.nodes):
            if probe.title(at) not in SUBTITLED or not node[6] & lib.VISIBLE or node[6] & lib.INERT:
                continue
            inside = []
            stack = list(children.get(at, []))
            while stack:
                cur = stack.pop()
                if probe.flags(cur) & lib.TEXT and probe.flags(cur) & lib.VISIBLE:
                    inside.append(cur)
                stack += children.get(cur, [])
            inside = [cur for cur in inside if probe.text(cur).strip()]
            if len(inside) < 2:
                continue
            inside.sort(key=lambda cur: (probe.box(cur)[1], probe.box(cur)[0]))
            title = inside[0]
            tx, ty, _, th = probe.box(title)
            subs = [cur for cur in inside[1:] if abs(probe.box(cur)[0] - tx) <= 1.0 and probe.box(cur)[1] >= ty + th - 2]
            if not subs:
                continue
            sub = subs[0]
            title_text, sub_text = probe.text(title).strip(), probe.text(sub).strip()
            shown = f'"{title_text}" / "{sub_text[:60]}"'
            overlap = restates(title_text, sub_text)
            if overlap >= 0.6 and len(tokens(sub_text)) >= 1 and sub_text.lower() != title_text.lower() or sub_text.lower() == title_text.lower():
                add("subtitle-restates", ["TX-07", "TX-06"], probe, shown, f"the subtitle repeats {overlap:.0%} of its words from the title", {"overlap": round(overlap, 2)})
            # A banner or status page holds a message, which may instruct; only a row's or a page's subtitle states.
            if probe.title(at) in ("Row", "PropertyRow", "PageHead"):
                for rule, ids, what in judge(sub_text, subtitle=True, titles=False):
                    if rule in ("instruction-subtitle", "subtitle-period", "subtitle-long"):
                        add(rule, ids, probe, shown, what)
    return found, texts


# ---- fixtures ---------------------------------------------------------------------------


def run_fixtures() -> list[str]:
    problems = []
    for key, value in FIXTURE_BAD.items():
        rules = {rule for rule, _, _ in judge(value)}
        want = key.removesuffix("-fmt")
        if want not in rules:
            problems.append(f"fixture {key!r} ({value!r}) is known bad but {want!r} was not found (found {sorted(rules)})")
    for value in FIXTURE_CLEAN:
        rules = {rule for rule, _, _ in judge(value)}
        if rules:
            problems.append(f"fixture {value!r} is clean but found {sorted(rules)}")
    sub_bad = {
        "instruction-subtitle": "Choose how the screen looks",
        "subtitle-period": "Connected to the network.",
    }
    for want, value in sub_bad.items():
        rules = {rule for rule, _, _ in judge(value, subtitle=True)}
        if want not in rules:
            problems.append(f"fixture subtitle {value!r} is known bad but {want!r} was not found (found {sorted(rules)})")
    if judge("Connected to Home", subtitle=True):
        problems.append("fixture subtitle 'Connected to Home' is clean but found something")
    if restates("Night light", "Night light is on") < 0.6:
        problems.append("fixture: 'Night light is on' under 'Night light' should read as restating the title")
    if restates("Night light", "Sunset to sunrise") > 0.1:
        problems.append("fixture: 'Sunset to sunrise' under 'Night light' should not read as restating the title")
    # The lexer: strings in tests, comments and logs are not copy.
    sample = (
        '// "Click here!"\nfn a() { let s = "Saved!"; eprintln!("Oops!"); }\n'
        '#[cfg(test)]\nmod tests { fn b() { let s = "Choose..."; } }\nfn c() { let t = r#"100 %"#; }\n'
    )
    got = [lit.value for lit in literals(sample) if not NOT_COPY.search(lit.before)]
    if got != ["Saved!", "100 %"]:
        problems.append(f"fixture lexer: expected ['Saved!', '100 %'], got {got}")
    return problems


# ---- main -------------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    lib.add_sweep_args(parser, variants="dark")
    parser.add_argument("--no-captures", action="store_true", help="the source only")
    parser.add_argument("--fixtures", action="store_true", help="only check the rules on their own fixtures")
    args = parser.parse_args()

    problems = run_fixtures()
    for problem in problems:
        print(problem)
    print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
    if args.fixtures or problems:
        return 1 if problems else 0

    findings, strings = source_findings(source_files())
    print(f"source: {strings} strings that read as copy in {len(source_files())} files")
    results: list[dict] = []
    texts = 0
    workdir = Path("/nonexistent")
    if not args.no_captures:
        if args.build:
            lib.lint.build()
        workdir = lib.workdir_of(args, "copy-lint-")
        results = lib.run_sweep(args, workdir, lib.parse_variants(args.variants))
        probes = [probe for result in results for probe in result["probes"]]
        more, texts = capture_findings(probes)
        findings += more
        print(f"captures: {len(probes)} captures, {texts} texts")
    style = Counter()
    for path in source_files():
        if is_test_file(path):
            continue
        app = "settings" if "bingux-settings" in str(path) else "files" if "files" in str(path) else "ui"
        for literal in literals(path.read_text(errors="replace")):
            if reads_as_copy(literal.value) and not NOT_COPY.search(literal.before):
                style[(app, "curly")] += bool(CURLY_QUOTE.search(literal.value))
                style[(app, "straight")] += bool(ASCII_QUOTE.search(literal.value))
    print("quote style in copy (info, no rule fixes one style): " + ", ".join(f"{app} {kind} {n}" for (app, kind), n in sorted(style.items()) if n))
    if args.no_captures:
        # finish() wants a workdir and results; there are none to look at.
        for finding in findings:
            finding.allowed = None
        allow = Allow(ALLOWED)
        for finding in findings:
            finding.allowed = allow.why(finding)
        failures = [f for f in findings if not f.allowed]
        for finding in sorted(failures, key=lambda f: (f.rule, f.capture)):
            print(f"[{finding.rule}] {'/'.join(finding.ids)} {finding.capture}\n    {finding.target}: {finding.what}")
        unused = [entry for entry in allow.unused() if entry.get("on") != "captures"]
        for entry in unused:
            print(f"unused allowed entry: {entry['rule']} {entry['capture']} {entry['where']}: {entry['why']}")
        by_rule = Counter(f.rule for f in failures)
        print(f"\ncopy-lint: {strings} strings, {len(failures)} findings" + (": " + ", ".join(f"{r} {n}" for r, n in sorted(by_rule.items())) if by_rule else "") + f", {len(findings) - len(failures)} allowed")
        if args.json:
            import json

            Path(args.json).write_text(json.dumps([f.json() for f in findings], indent=2))
        return 1 if failures or unused else 0
    return lib.finish("copy-lint", findings, Allow(ALLOWED), args, results, strings + texts, workdir, "strings")


if __name__ == "__main__":
    sys.exit(main())
