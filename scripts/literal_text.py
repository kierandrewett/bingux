"""Fail on a string a person reads that is written in the Rust source instead of a Fluent catalogue.

Every string a person reads comes from a Project Fluent catalogue (`tr!("id")`, `i18n_id!`, a message
attribute): docs/bingux-ui.md, "Localisation". This is the check that holds it, run by
`scripts/ui-lint.py` (and so by `make check-static` and `make rules-check`).

What it finds, in the non-test Rust of the apps and of bingux-ui (SCOPE):

  literal-text     a string literal with a letter in it passed to a bingux-ui call that shows its
                   argument: `.title("Songs")`, `action_button("Retry", ..)`, `.tooltip(format!("Open {}", n))`,
                   `.a11y("Play")`, `.placeholder("Search")`, `.keywords(["play", "file"])`, `toast(..)`,
                   `announce(..)`. Which calls and which argument is read from the source of bingux-ui
                   itself, so a new text API is covered the day it is added: every public function of
                   crates/ui/src whose parameter is a string (or a `Message`) named like text a person reads
                   (TEXT_PARAMS) makes that argument position a place for a message, not a literal. The
                   literal may sit inside `format!`, `String::from`, `Some`, an array, an `if`/`else`.
  plural-in-code   `if n == 1 { "1 file" } else { "files" }`: a plural chosen in Rust. A count is a number
                   variable in the message, which selects on it.
  hand-formatted   `format!("{:.1} MB", x)`, `format!("{} km", n)`: a number written with a unit by hand.
                   Numbers, sizes, durations and dates go through `bingux_ui::format`, or are variables of
                   a message.

Not findings: a literal inside `tr!`/`i18n_id!` (that is the message ID), logs, panics and `eprintln!`,
commands and their arguments, identifiers, key chords (`.shortcut("Ctrl+N")`), code under `#[cfg(test)]`,
test files and the snapshot runners. Anything else needs an entry of ALLOWED, with its reason.

    python3 scripts/literal-text-lint     is run through ui-lint.py; `--self-test` there plants every case.
"""

from __future__ import annotations

import re
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Directories the check reads: the apps, bingux-ui, and the domain crates whose text a window shows.
SCOPE = [
    "apps/*/src",
    "crates/ui/src",
]

# A parameter of a public bingux-ui function with one of these names takes text a person reads.
TEXT_PARAMS = {
    "text",
    "label",
    "title",
    "subtitle",
    "body",
    "message",
    "hint",
    "caption",
    "tooltip",
    "description",
    "detail",
    "note",
    "advice",
    "summary",
    "placeholder",
    "heading",
    "spoken",
    "remove_label",
    "cancel_label",
    "commit_label",
    "back_label",
    "copy_label",
    "accept_label",
    "value_text",
    "suffix",
    "keywords",
    "alt",
    "a11y",
    "name_text",
    "reason",
    "prompt",
    "error",
    "status",
    "empty",
}
# Functions of bingux-ui that take such a parameter but are not for people: automation, tests, probes.
NOT_TEXT_FNS = {
    "click_text",
    "click_name",
    "find_status",
    "type_ahead",
    "completion_text",
    "assert_off_ui_thread",
    "fail_engine",
    "check_chrome",
    "error_text",
    "report",
    "inhibit",
    "inhibit_then",
    "acquire",
    "blocked",
    "note_location",
    "provide_field_label",
    "search",
    "query",
    "hits",
    "lines",
    "rank",
    "picks",
    "fuzzy",
    "find",
    "find_match",
    "find_prefix",
    "by_word",
    "match_ranges",
    "natural_cmp",
    "of",
    "parse",
    "decode",
    "format",
    "format_in",
    "from_key",
    "from_gsettings",
    "invalid",
    "set_prop_override",
    "has",
    "has_override",
    "edit",
    "set",
    "commit",
    "original",
    "run_script",
    "run_quick",
    "run_pack",
    "run_unpack",
    "run_reduce",
    "fallback_colours",
    "artwork",
    "json",
    "probe",
    "shortcut",
    "cached",
    "forget",
    "load",
    "source_terms",
    "locale",
    "for_locale",
    "check",
    "crate_catalogue",
    "new_name",
    "rename",
    "capitalised",
    "abbreviate",
    "elide_path",
    "unused_name",
    "window_title",
    "use_document_title",
    "target",
    "push",
    "index",
    "url",
    "open",
    "open_uri",
    "navigate",
}
# Calls through which a literal is still the argument: the string is what the outer call receives.
WRAPPERS_HEAD = {
    "format!",
    "String::from",
    "Some",
    "Ok",
    "Err",
    "Cow::Borrowed",
    "Box::new",
    "Message::literal",
    "vec!",
    ".unwrap_or",
    ".unwrap_or_else",
    ".map_or",
    ".map_or_else",
    "&",
}
# Text arguments the parameter names do not show: (function, owner) -> argument positions.
EXTRA_TEXT_APIS: dict[tuple[str, str], set[int]] = {
    ("new", "Series"): {0},  # the name of a chart series, read in the legend and by a screen reader
}
# Calls whose strings are never shown to people.
NOT_TEXT_CALLS = re.compile(
    r"^(?:tr!|i18n_id!|bingux_ui::i18n_id!|prelude::i18n_id!|prelude::tr!|bingux_ui::tr!|eprintln!|println!|eprint!|print!|"
    r"panic!|assert\w*!|debug_assert\w*!|unreachable!|todo!|unimplemented!|include_str!|env!|option_env!|concat!|"
    r"matches!|write!|writeln!|tracing::\w+!|log::\w+!|warn!|info!|error!|debug!|trace!|bail!|"
    r"Command::new|\.arg|\.args|\.env|\.expect|\.expect_err|bingux_ui::catalogue!|catalogue!|"
    r"\.shortcut|\.contains|\.starts_with|\.ends_with|\.strip_prefix|\.strip_suffix|\.get|\.join|\.set|\.insert|"
    r"\.component|\.route|\.glob|\.mime|\.query|\.section|\.key|\.str|\.id)$"
)

# Findings that are right by design: (rule, path regex, text regex, reason). The reason says why this
# string is not read by a person in the language they chose, or why it must not change.
ALLOWED: list[tuple[str, str, str, str]] = [
    ("*", r"^apps/browser/src/", r".*", "converted by the browser restructure (feat/apps), remove when done"),
    (
        "*",
        r"^apps/taxonomy/src/demos/",
        r".*",
        "specimen content of the taxonomy: sample labels, rows and sentences that show how a component lays out "
        "(long, CJK, empty, error); the taxonomy's own chrome (switcher, search, toasts, About) is in its catalogue",
    ),
    (
        "*",
        r"^crates/ui/src/inspector/",
        r".*",
        "developer tooling: the inspector is read by the people who write the app, in English, like a log",
    ),
    (
        "*",
        r"^crates/ui/src/motion_stress/",
        r".*",
        "fixtures of `make motion-check`: scripted pages whose text is fixed so the frames can be compared",
    ),
    (
        "*",
        r"^apps/calculator/src/pads\.rs$",
        r"^(?:[ a-z]+|[A-Z]{1,5}|\dnd|[Rr]and|ans|Ro[LR])$",
        "calculator key faces and the operator text they insert: mathematical and programmer notation (XOR, RoL, "
        "mod, ln, Ans) is the same in every language the calculator is localised into",
    ),
    (
        "*",
        r"^crates/ui/src/(?:group|warm)\.rs$",
        r"^(?:Mg|Agj 0)$",
        "text metrics probes: drawn invisible to measure a line's height, never shown",
    ),
    (
        "hand-formatted",
        r"^apps/media/src/strings\.rs$",
        r"^\{text\}×$",
        "playback speed in a menu (1×, 1.5×): a number and a sign, the same in every language",
    ),
    (
        "*",
        r"^crates/ui/src/web_view/",
        r".*",
        "the web view serves only the browser: its engine report and errors are converted with the browser "
        "restructure (feat/apps), remove when done",
    ),
    # Pending conversions: each entry is removed by the commit that converts its crate.
    (
        "*",
        r"^apps/settings/src/",
        r".*",
        "pending conversion by AppsFluent; removed by the commit that converts Settings",
    ),
    (
        "*",
        r"^apps/taxonomy/src/",
        r".*",
        "pending conversion by AppsFluent; removed by the commit that converts the taxonomy chrome",
    ),
    (
        "*",
        r"^crates/ui/src/(?!files/properties|text_editor/commands\.rs)",
        r".*",
        "pending conversion by AppsFluent; removed by the commits that convert the rest of bingux-ui",
    ),
]

RULES = {
    "literal-text": "a string a person reads is written in the source; move it to the app's Fluent catalogue (tr!)",
    "plural-in-code": "a plural is chosen in Rust; write one message that selects on the count",
    "hand-formatted": "a number is written with its unit by hand; use bingux_ui::format or a message variable",
    "plural-pair": "two messages (-one and -many) are picked between in Rust; write one message that selects on the count",
    "sentence-literal": "a phrase a person reads is written in the source; move it to the app's Fluent catalogue (tr!)",
}


# ---- the lexer ------------------------------------------------------------------------------------


class Frame:
    __slots__ = ("kind", "name", "argi", "key")

    def __init__(self, kind: str, name: str):
        self.kind, self.name, self.argi, self.key = kind, name, 0, None


class Literal:
    __slots__ = ("line", "value", "frames", "tail", "head")

    def __init__(self, line: int, value: str, frames: list[tuple[str, str, int, str | None]], tail: str, head: str):
        self.line, self.value, self.frames, self.tail, self.head = line, value, frames, tail, head


KEYWORDS = {
    "if",
    "while",
    "match",
    "for",
    "in",
    "return",
    "else",
    "fn",
    "impl",
    "where",
    "as",
    "let",
    "move",
    "loop",
    "mut",
    "ref",
    "unsafe",
    "async",
    "dyn",
}
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
TEST_ATTR = re.compile(
    r"cfg\(\s*test\s*\)|cfg\(\s*all\(\s*test|cfg\(\s*any\(\s*test|^\[\s*test\s*\]|cfg\(\s*all\(\s*feature[^)]*\),\s*test"
)


def lex(text: str) -> list[Literal]:
    """The string literals of a Rust file outside comments and test blocks, with the calls around each."""
    out: list[Literal] = []
    i, n, line = 0, len(text), 1
    stack: list[Frame] = []
    test_depth: list[int] = []
    pending_test = False
    prev = ""  # the last significant character, to tell a closure's `|` from an `or`
    prev_word = ""
    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if c in " \t\r":
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
                depth, k = 0, j
                while k < n:
                    if text[k] == "[":
                        depth += 1
                    elif text[k] == "]":
                        depth -= 1
                        if depth == 0:
                            break
                    k += 1
                attr = text[j : k + 1]
                if TEST_ATTR.search(attr):
                    pending_test = True
                line += attr.count("\n")
                i = k + 1
                continue
        if c == "'":
            if i + 1 < n and text[i + 1] == "\\":
                j = text.find("'", i + 2)
                i = (j if j >= 0 else n) + 1
            elif i + 2 < n and text[i + 2] == "'":
                i += 3
            else:
                i += 1
            prev = "x"
            continue
        raw = re.match(r'b?r(#*)"', text[i : i + 12])
        if raw and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = raw.group(1)
            start = i + raw.end()
            end = text.find('"' + hashes, start)
            end = n if end < 0 else end
            if not test_depth:
                out.append(
                    Literal(line, text[start:end], snapshot(stack), text[end + 1 + len(hashes) : end + 40], prev)
                )
            line += text.count("\n", i, end)
            i = end + 1 + len(hashes)
            prev = '"'
            continue
        if c == '"' or (c == "b" and text.startswith('b"', i)):
            if c == "b":
                i += 1
            start, j, buf, first = i + 1, i + 1, [], line
            while j < n and text[j] != '"':
                if text[j] == "\\" and j + 1 < n:
                    esc = text[j + 1]
                    if esc == "\n":
                        line += 1
                        j += 2
                        while j < n and text[j] in " \t\r\n":
                            line += text[j] == "\n"
                            j += 1
                        continue
                    if esc == "u":
                        m = re.match(r"u\{([0-9a-fA-F]+)\}", text[j + 1 : j + 12])
                        if m:
                            buf.append(chr(int(m.group(1), 16)))
                            j += 1 + m.end()
                            continue
                    buf.append({"n": "\n", "t": "\t", "r": "\r", "0": "\0"}.get(esc, esc))
                    j += 2
                    continue
                line += text[j] == "\n"
                buf.append(text[j])
                j += 1
            if not test_depth:
                out.append(Literal(first, "".join(buf), snapshot(stack), text[j + 1 : j + 40], prev))
            i = j + 1
            prev = '"'
            continue
        if c.isalpha() or c == "_":
            m = IDENT.match(text, i)
            word = m.group(0)
            i = m.end()
            prev, prev_word = "x", word
            # `key = value` inside a macro such as app_info!(description = "..")
            if stack and stack[-1].kind == "(" and stack[-1].name.endswith("!"):
                rest = text[i : i + 3].lstrip()
                if rest.startswith("=") and not rest.startswith("=="):
                    stack[-1].key = word
            continue
        if c.isdigit():
            while i < n and (text[i].isalnum() or text[i] in "._"):
                i += 1
            prev = "x"
            continue
        if (
            c == "|"
            and prev in ("", "(", ",", "=", "{", ":", ";")
            or (c == "|" and prev == "x" and prev_word == "move")
        ):
            # closure parameters: skip to the closing pipe so their commas are not argument separators
            if text.startswith("||", i):
                i += 2
            else:
                j = text.find("|", i + 1)
                line += text.count("\n", i, j)
                i = j + 1
            prev = "|"
            continue
        if c in "([{":
            name = c
            if c in "([":
                name = callee(text, i) or c
            frame = Frame(c, name)
            stack.append(frame)
            if c == "{" and pending_test:
                test_depth.append(len(stack))
                pending_test = False
            i += 1
            prev = c
            continue
        if c in ")]}":
            if stack:
                if test_depth and test_depth[-1] == len(stack) and c == "}":
                    test_depth.pop()
                stack.pop()
            i += 1
            prev = c
            continue
        if c == ",":
            if stack and stack[-1].kind == "(":
                stack[-1].argi += 1
                stack[-1].key = None
        elif c == ";" and pending_test:
            pending_test = False
        prev = c
        i += 1
    return out


def snapshot(stack: list[Frame]) -> list[tuple[str, str, int, str | None]]:
    return [(f.kind, f.name, f.argi, f.key) for f in stack]


def callee(text: str, open_index: int) -> str | None:
    """`.title`, `Type::new`, `tr!`, `action_button`: what a `(` or `[` at open_index calls."""
    k = open_index - 1
    while k >= 0 and text[k] in " \t\n":
        k -= 1
    bang = False
    if k >= 0 and text[k] == "!":
        bang, k = True, k - 1
    end = k + 1
    while k >= 0 and (text[k].isalnum() or text[k] == "_"):
        k -= 1
    ident = text[k + 1 : end]
    if not ident or ident[0].isdigit() or (ident in KEYWORDS and not bang):
        return None
    p = k
    while p >= 0 and text[p] in " \t\n":
        p -= 1
    prefix = ""
    if p >= 0 and text[p] == ".":
        prefix = "."
    elif p >= 1 and text[p - 1 : p + 1] == "::":
        q = p - 2
        while q >= 0 and text[q] in " \t\n":
            q -= 1
        if q >= 0 and text[q] == ">":
            return ident + ("!" if bang else "")  # a turbofish or a qualified path: the type is not known
        e2 = q + 1
        while q >= 0 and (text[q].isalnum() or text[q] == "_"):
            q -= 1
        prefix = text[q + 1 : e2] + "::"
    return prefix + ident + ("!" if bang else "")


# (name, owner) pairs that bingux-ui defines more than once with the text in different places: a literal is
# then flagged only when it reads as a sentence or a label, not when it could be an identifier.
AMBIGUOUS: set[tuple[str, str]] = set()
# Declared `pub` in bingux-ui: its text parameters are taken at their word. A helper private to an app or to
# one file of bingux-ui is judged by the string alone (LOCAL), because its parameter may be a key.
TRUSTED: set[tuple[str, str]] = set()
LOCAL: set[tuple[str, str]] = set()

# ---- the text APIs of bingux-ui, read from its source ------------------------------------------------

SIGNATURE = re.compile(r"\bfn\s+(\w+)\s*(?:<[^>{(]*(?:\([^)]*\)[^>{(]*)*>)?\s*\(", re.S)
STRINGLY = re.compile(
    r"^(?:&\s*(?:'\w+\s+)?str|String|Message|Cow<[^>]*>|Option<\s*(?:String|&\s*(?:'\w+\s+)?str|Message)\s*>|"
    r"impl\s+(?:Into|AsRef)<\s*(?:String|str|Message|Cow<[^>]*>|Option<(?:String|Message)>)\s*>|"
    r"impl\s+(?:IntoIterator|Into<Vec)<[^>]*(?:String|str|Message)[^>]*>+|"
    r"(?:Vec|&\s*\[)\s*(?:String|&\s*str)\]?>?|\[\s*&\s*str\s*;?\s*\w*\s*\]|I)$"
)


def split_params(text: str, open_index: int) -> list[str]:
    depth, start, params = 0, open_index + 1, []
    i = open_index
    while i < len(text):
        ch = text[i]
        if ch in "(<[{":
            depth += 1
        elif ch in ")>]}":
            depth -= 1
            if depth == 0:
                params.append(text[start:i])
                return params
        elif ch == "," and depth == 1:
            params.append(text[start:i])
            start = i + 1
        i += 1
    return params


def api_files(root: Path) -> list[Path]:
    """Where text APIs are declared: bingux-ui, and the apps' own helpers (a `card(tokens, title: &str)` that
    ends in a bingux-ui call is as much a place for a message as the call itself)."""
    files = []
    for pattern in SCOPE:
        for directory in sorted(root.glob(pattern)):
            files += [p for p in sorted(directory.rglob("*.rs")) if not is_test_file(p.relative_to(root))]
    return files


def text_apis(root: Path = ROOT) -> dict[str, dict[str, set[int]]]:
    """name -> {owner -> argument positions} of the bingux-ui functions that take text a person reads.

    `owner` is the type of the `impl` block ("" for a free function); a method is asked for by its name,
    an associated function (`Type::new`) by its type."""
    apis: dict[str, dict[str, set[int]]] = defaultdict(lambda: defaultdict(set))
    seen: dict[tuple[str, str], list[frozenset[int]]] = defaultdict(list)
    for path in api_files(root):
        text = path.read_text(errors="replace")
        owners = impl_blocks(text)
        for m in SIGNATURE.finditer(text):
            name = m.group(1)
            if name in NOT_TEXT_FNS:
                continue
            params = split_params(text, m.end() - 1)
            is_method = (
                bool(params) and re.match(r"^\s*(?:&\s*(?:'\w+\s+)?(?:mut\s+)?|mut\s+)?self\b", params[0]) is not None
            )
            index = 0
            here: set[int] = set()
            owner = ""
            for param in params:
                body = param.strip()
                if not body or re.match(r"^(?:&\s*(?:'\w+\s+)?(?:mut\s+)?|mut\s+)?self\b", body):
                    continue
                pm = re.match(r"^(?:mut\s+)?(\w+)\s*:\s*(.+)$", body, re.S)
                if pm and pm.group(1) in TEXT_PARAMS and STRINGLY.match(re.sub(r"\s+", " ", pm.group(2).strip())):
                    line_start = text.rfind("\n", 0, m.start()) + 1
                    top_level = text[line_start : line_start + 1] not in (" ", "\t")
                    owner = "" if top_level else next((o for at, o in reversed(owners) if at < m.start()), "")
                    apis[name]["." if is_method else owner].add(index)
                    here.add(index)
                index += 1
            if here:
                seen[(name, "." if is_method else owner)].append(frozenset(here))
                public = path.relative_to(root).parts[:2] == ("crates", "ui") and bool(
                    re.search(r"\bpub\b[^;{}]{0,40}$", text[max(0, m.start() - 60) : m.start()])
                )
                (TRUSTED if public else LOCAL).add((name, "." if is_method else owner))
    for (name, owner), variants in seen.items():
        if len(set(variants)) > 1:
            AMBIGUOUS.add((name, owner))
    for pair, positions in EXTRA_TEXT_APIS.items():
        apis[pair[0]][pair[1]] |= positions
        TRUSTED.add(pair)
    return apis


def impl_blocks(text: str) -> list[tuple[int, str]]:
    """(offset, type name) of each `impl Type {` and `impl Trait for Type {`."""
    found = []
    for m in re.finditer(r"^impl\b(?:<[^>]*>)?\s+(?:[\w:<>, &']+\s+for\s+)?([A-Z]\w*)", text, re.M):
        found.append((m.start(), m.group(1)))
    return found


# ---- reading one file --------------------------------------------------------------------------------


def is_test_file(path: Path) -> bool:
    name = path.name
    parts = path.parts
    return (
        name == "tests.rs"
        or name.endswith("_tests.rs")
        or name.startswith("snapshot")
        or "tests" in parts
        or "examples" in parts
        or "benches" in parts
        or "stress" in parts
    )


def has_letter(value: str) -> bool:
    return any(ch.isalpha() for ch in value)


def reads_as_text(value: str) -> bool:
    """A string with a space or a capital reads as text; `save_as` and `discard` could be identifiers."""
    text = value.strip()
    if re.fullmatch(r"[A-Z0-9_]+", text) and "_" in text:
        return False
    return " " in text or text[:1].isupper()


def text_position(literal: Literal, apis: dict[str, dict[str, set[int]]]) -> str | None:
    """The name of the bingux-ui call that shows this literal, or None when it is not one."""
    frames = literal.frames
    k = len(frames) - 1
    while k >= 0:
        kind, name, argi, key = frames[k]
        if kind in "[{":
            k -= 1
            continue
        if kind == "(" and name == "(":
            k -= 1
            continue
        if name in WRAPPERS_HEAD and name != "&":
            if name in ("format!", "String::from", "Message::literal") and argi != 0:
                return None  # an argument of format!, not its template
            k -= 1
            continue
        if NOT_TEXT_CALLS.match(name):
            return None
        short = name[1:] if name.startswith(".") else name
        if name == "app_info!" or name.endswith("::app_info!"):
            return "app_info!" if key == "description" else None
        owners = apis.get(short.split("::")[-1])
        if not owners:
            return None
        if name.startswith("."):
            positions = owners.get(".", set())
        elif "::" in name:
            positions = owners.get(name.split("::")[-2], set())
        else:
            positions = owners.get("", set())
        if argi not in positions:
            return None
        owner = "." if name.startswith(".") else (name.split("::")[-2] if "::" in name else "")
        pair = (short.split("::")[-1], owner)
        if (pair in AMBIGUOUS or pair not in TRUSTED) and not reads_as_text(literal.value):
            return None
        return name
    return None


PLURAL = re.compile(r"(?:==|!=)\s*1\s*\{\s*(?:&?format!\(\s*)?$|\.len\(\)\s*==\s*1\s*\{\s*(?:&?format!\(\s*)?$")
HAND_FORMAT = re.compile(r"^\{[^}]*\}\s*[A-Za-zµ°][A-Za-z/µ°²³]{0,5}\.?$|^\{[^}]*\}\s*(?:%|°[CF]?|×)$")


def findings_of(
    text: str, rel: str, apis: dict[str, dict[str, set[int]]], ids: set[str] | None = None
) -> list[tuple[str, int, str, str]]:
    """(rule, line, literal, call) for each finding of one file."""
    found = []
    for literal in lex(text):
        pair = plural_pair(literal, ids)
        if pair:
            found.append(("plural-pair", literal.line, literal.value, pair))
            continue
        if not has_letter(literal.value):
            continue
        call = text_position(literal, apis)
        if call:
            found.append(("literal-text", literal.line, literal.value, call))
            continue
        before = text_before(text, literal)
        if before is not None and PLURAL.search(before) and not frame_blocks_text(literal):
            found.append(("plural-in-code", literal.line, literal.value, "if n == 1"))
            continue
        if (
            HAND_FORMAT.match(literal.value.strip())
            and literal.frames
            and literal.frames[-1][1] == "format!"
            and not frame_blocks_text(literal)
        ):
            found.append(("hand-formatted", literal.line, literal.value, "format!"))
            continue
        if reads_as_sentence(literal.value) and not frame_blocks_text(literal):
            found.append(
                ("sentence-literal", literal.line, literal.value, literal.frames[-1][1] if literal.frames else "-")
            )
    return found


SENTENCE = re.compile(r"^[A-Z\u00c0-\u00de].*[ ].*$", re.S)


def reads_as_sentence(value: str) -> bool:
    """A capitalised phrase of two words or more, wherever it is written: `Could not remove “{}”.`, `Original size`.
    Such a string is shown to a person or is a fixture; either way it is not source."""
    text = value.strip()
    words = re.findall(r"[A-Za-z\u00c0-\u024f’']{2,}", text)
    return (
        bool(SENTENCE.match(text))
        and len(words) >= 2
        and not text.startswith("Usage:")
        and not re.search(r"[=<>\\_$@#]", text)
        and not re.fullmatch(r"[A-Z0-9 ./+\-]+", text)
    )


def frame_blocks_text(literal: Literal) -> bool:
    """A literal inside a log, a command or an id call is not shown."""
    return any(
        NOT_TEXT_CALLS.match(name) or (name.endswith("::key_hint") and argi >= 2)
        for kind, name, argi, _ in literal.frames
        if kind == "("
    )


def text_before(text: str, literal: Literal) -> str | None:
    lines = text.split("\n")
    at = literal.line - 1
    if at >= len(lines):
        return None
    chunk = "\n".join(lines[max(0, at - 1) : at + 1])
    idx = chunk.rfind('"')
    prefix = chunk[: idx if idx >= 0 else len(chunk)]
    # the literal's own opening quote is the last quote before its text
    first = chunk.find(literal.value[:12]) if literal.value else -1
    if first > 0:
        prefix = chunk[: first - 1]
    return prefix


def source_files(root: Path = ROOT) -> list[Path]:
    files: list[Path] = []
    for pattern in SCOPE:
        for directory in sorted(root.glob(pattern)):
            files += [
                p
                for p in sorted(directory.rglob("*.rs"))
                if "target" not in p.parts and not is_test_file(p.relative_to(root))
            ]
    return files


def allowed(rule: str, rel: str, value: str) -> bool:
    return any(
        (r == "*" or r == rule) and re.search(path, rel) and re.search(text, value, re.S)
        for r, path, text, _why in ALLOWED
    )


def catalogue_ids(root: Path = ROOT) -> set[str]:
    """Every message ID of the en-US catalogues of the apps and of bingux-ui."""
    ids: set[str] = set()
    for path in list(root.glob("apps/*/locale/en-US/*.ftl")) + list(root.glob("crates/*/locale/en-US/*.ftl")):
        for line in path.read_text(errors="replace").splitlines():
            m = re.match(r"^([a-z][a-z0-9-]*)\s*=", line)
            if m:
                ids.add(m.group(1))
    return ids


PAIR = re.compile(r"^(.+)-(one|many)((?:\.[a-z-]+)?)$")
MESSAGE_CALLS = re.compile(r"^(?:\w+::)*(?:tr|i18n_id)!$")


def plural_pair(literal: Literal, ids: set[str] | None) -> str | None:
    """The message ID when a message and its `-one`/`-many` twin both exist: Rust picks between them."""
    if not ids or not literal.frames:
        return None
    kind, name, argi, _key = literal.frames[-1]
    m = PAIR.match(literal.value)
    if not (kind == "(" and MESSAGE_CALLS.match(name) and argi == 0 and m):
        return None
    twin = m.group(1) + ("-many" if m.group(2) == "one" else "-one")
    return name if twin in ids else None


def scan(root: Path = ROOT, files: list[Path] | None = None) -> list[tuple[str, str, int, str, str]]:
    """(rule, file, line, literal, call) for every finding not in ALLOWED."""
    apis = text_apis(root)
    ids = catalogue_ids(root)
    found = []
    for path in files if files is not None else source_files(root):
        rel = str(path.relative_to(root))
        for rule, line, value, call in findings_of(path.read_text(errors="replace"), rel, apis, ids):
            if not allowed(rule, rel, value):
                found.append((rule, rel, line, value, call))
    return found


def problems(root: Path = ROOT) -> list[str]:
    out = []
    for rule, rel, line, value, call in scan(root):
        shown = value if len(value) <= 60 else value[:57] + "..."
        out.append(f"{rel}:{line}: literal text {shown!r} in {call}: {RULES[rule]}")
    return out


def per_crate(found: list[tuple[str, str, int, str, str]]) -> Counter:
    counts: Counter = Counter()
    for _rule, rel, *_ in found:
        counts["/".join(rel.split("/")[:2])] += 1
    return counts


# ---- self-test -----------------------------------------------------------------------------------------

PLANTED_API = """
pub struct Row;
impl Row {
    pub fn title(self, text: impl Into<String>) -> Self { self }
    pub fn tooltip(self, text: impl Into<String>) -> Self { self }
    pub fn keywords(self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self { self }
    pub fn id(self, id: &str) -> Self { self }
}
pub fn action_button(label: &str, on: u8) -> Row { Row }
pub struct Overlays;
impl Overlays {
    pub fn toast(&self, message: impl Into<String>) {}
}
"""

PLANTED_BAD = {
    "a literal passed to .title": 'fn f() { Row.title("Songs"); }',
    "a format! passed to .tooltip": 'fn f(n: u32) { Row.tooltip(format!("Open {n} files")); }',
    "a literal passed to a free function": 'fn f() { action_button("Retry", 1); }',
    "a literal in an array of keywords": 'fn f() { Row.keywords(["play", "file"]); }',
    "a String::from literal in a toast": 'fn f() { overlays.toast(String::from("Saved")); }',
    "a branch of an if": 'fn f(on: bool) { Row.title(if on { "On" } else { "Off" }); }',
    "a to_string literal": 'fn f() { Row.title("Songs".to_string()); }',
    "a plural chosen in code": 'fn f(n: usize) -> String { if n == 1 { "1 file".to_string() } else { format!("{n} files") } }',
    "a unit written by hand": 'fn f(n: u32) -> String { format!("{} km", n) }',
    "a closure body that returns text": 'fn f() { Row.title((|| "Songs")()); }',
    "a sentence returned from a match arm": 'fn f(n: u8) -> &\'static str { match n { 0 => "Similar to the actual temperature.", _ => "" } }',
}
PLANTED_GOOD = {
    "a message": 'fn f() { Row.title(tr!("songs-title")); }',
    "an id": 'fn f() { Row.id("songs"); }',
    "a message with a variable": 'fn f(n: u32) { Row.tooltip(tr!("files-open", count = n)); }',
    "a log": 'fn f() { eprintln!("could not open the file"); }',
    "a command": 'fn f() { Command::new("xdg-open").arg("--help"); }',
    "a comment": 'fn f() {\n    // Row.title("Songs");\n}',
    "a test module": '#[cfg(test)]\nmod t { fn f() { Row.title("Songs"); } }',
    "a closure with parameters": 'fn f() { Row.title(tr!("a")).tooltip(items.iter().map(|a, b| a).count().to_string()); }',
    "a format! without text": 'fn f(n: u32) -> String { format!("{n}") }',
    "a key chord": 'fn f() { Row.shortcut("Ctrl+N"); }',
    "an identifier with a dash": 'fn f() -> &\'static str { "files-new-folder" }',
    "a name passed as a message variable": 'fn f() { tr!("files-moved", name = "Report final.pdf"); }',
}


def self_test() -> list[str]:
    import tempfile

    failures = []
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "crates/ui/src").mkdir(parents=True)
        (root / "crates/ui/src/api.rs").write_text(PLANTED_API)
        apis = text_apis(root)
        for expected in ("title", "tooltip", "keywords", "action_button", "toast"):
            if expected not in apis:
                failures.append(f"the API read from bingux-ui lacks {expected}: {sorted(apis)}")
        if "id" in apis:
            failures.append("a parameter named id was taken for text")
        for what, source in PLANTED_BAD.items():
            if not findings_of(source, "apps/x/src/a.rs", apis):
                failures.append(f"not reported: {what}")
        for what, source in PLANTED_GOOD.items():
            hits = findings_of(source, "apps/x/src/a.rs", apis)
            if hits:
                failures.append(f"reported though it is fine ({what}): {hits}")
        if not allowed("literal-text", "apps/browser/src/a.rs", "Songs"):
            failures.append("the apps/browser entry of ALLOWED does not cover the browser")
        if allowed("literal-text", "apps/clock/src/a.rs", "Songs"):
            failures.append("ALLOWED covers a file it should not")
        for entry in ALLOWED:
            if not entry[3].strip():
                failures.append(f"an ALLOWED entry has no reason: {entry[:3]}")
        ids = {"files-moved-one", "files-moved-many", "files-repeat-one", "files-title"}
        pair = 'fn f(n: usize) -> String { if n == 1 { tr!("files-moved-one") } else { tr!("files-moved-many", count = n) } }'
        if not any(hit[0] == "plural-pair" for hit in findings_of(pair, "apps/x/src/a.rs", apis, ids)):
            failures.append("not reported: two messages picked between in Rust")
        lone = 'fn f() -> String { tr!("files-repeat-one") }'
        if any(hit[0] == "plural-pair" for hit in findings_of(lone, "apps/x/src/a.rs", apis, ids)):
            failures.append("reported though the message has no twin: files-repeat-one")
    return failures


def main(argv: list[str]) -> int:
    """`literal_text.py [--pending] [PATH-REGEX]`: list the findings; --pending also lists the ones a
    pending-conversion entry of ALLOWED holds back (what is left to convert)."""
    if "--pending" in argv:
        ALLOWED[:] = [entry for entry in ALLOWED if not entry[3].startswith("pending conversion")]
    patterns = [arg for arg in argv if not arg.startswith("--")]
    found = [item for item in scan() if not patterns or any(re.search(p, item[1]) for p in patterns)]
    for rule, rel, line, value, call in found:
        print(f"{rel}:{line}: [{rule}] {call} {value[:70]!r}")
    counts = per_crate(found)
    print(f"{len(found)} finding(s): " + ", ".join(f"{name} {count}" for name, count in sorted(counts.items())))
    return 1 if found else 0


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv[1:]))
