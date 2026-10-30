#!/usr/bin/env python3
"""Check the design system against Kieran's rules (design/rules.json) in one run.

The 242 rules of design/rules.json are enforced by a dozen scripts and by the
Rust rules behind `layout-lint.py` and `state-check.py`. This script is the one
place that says which check holds which rule, runs the checks, and reports the
state of every rule from what the run found.

  python3 scripts/rules-check.py                 # every check, report in design/
  python3 scripts/rules-check.py --offline       # the checks that need no app, no report written
  python3 scripts/rules-check.py --only ui-lint,copy-lint
  python3 scripts/rules-check.py --skip layout-lint,state-check
  python3 scripts/rules-check.py --reuse DIR     # take a check's output from an earlier run's DIR
  python3 scripts/rules-check.py --registry      # print the registry and its integrity problems
  python3 scripts/rules-check.py --sync-coverage # append the registry's checks to `enforced_by` in rules.json (only that field)
  python3 scripts/rules-check.py --strict        # also fail on known open defects and unenforced P0 rules

The registry (check -> rule key -> rule IDs) is read from the checks themselves,
never typed here: the `Meta { key, ids }` tables of the Rust rules
(inspector/checks/mod.rs, inspector/state_checks/mod.rs), the `(key, [IDs], ..)`
pairs and `[SP-01]` prefixes in the Python checks, and the `RULE_IDS` /
`RULE_MESSAGES` constants a script declares when its findings carry no ID. The
table CHECKS below holds only how to run each check. The registry also validates the declared
coverage of rules.json (`enforced_by`): a rule that names a check which does not cite it, and a
check that cites a rule whose `enforced_by` omits it, are listed in the report. Run results are
never written to rules.json; they live in design/rules-report.md and .json.

Each check runs through scripts/app_env.py (memory GSettings backend, no
WAYLAND_DISPLAY, no DISPLAY), one at a time; `--jobs` is how many captures a
check renders at once. A rule's status comes from evidence:

  passing       every check that cites it ran in this run and found nothing
  failing       a check found it violated (failing findings)
  known-open    only findings the check lists as known open defects (state-check KNOWN)
  allowlisted   only findings an ALLOWED entry covers, each with its reason
  not-run       the checks that cite it were not executed in this run
  partly-run    some of the checks that cite it ran clean, others did not run
  errored       a check that cites it exited non-zero without a verdict
  unenforced    no check cites it
  manual        no check can judge it (design/rules-manual.json says why)

Exit status 1 when a rule is failing or errored (--strict: or known-open, or a
P0 rule is unenforced). A check that ran but is red for a reason no finding
names (a crash, a missing binary) is `errored`. A rule is never reported as
passing unless its check ran, with the evidence kept in DIR/raw.
"""

from __future__ import annotations

import argparse
import ast
import copy
import datetime as dt
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
RULES_JSON = ROOT / "design" / "rules.json"
MANUAL_JSON = ROOT / "design" / "rules-manual.json"
REPORT_MD = ROOT / "design" / "rules-report.md"
REPORT_JSON = ROOT / "design" / "rules-report.json"
DEFAULT_TARGET = "/var/tmp/bingux-settings-target"
DEFAULT_OUT = Path("/var/tmp/rules-check")

PASSING, FAILING, KNOWN_OPEN, ALLOWLISTED = "passing", "failing", "known-open", "allowlisted"
NOT_RUN, PARTLY_RUN, ERRORED = "not-run", "partly-run", "errored"
UNENFORCED, MANUAL = "unenforced", "manual"
STATUSES = [FAILING, ERRORED, KNOWN_OPEN, PASSING, ALLOWLISTED, PARTLY_RUN, NOT_RUN, UNENFORCED, MANUAL]
LABEL = {
    PASSING: "enforced+passing",
    FAILING: "enforced+failing",
    KNOWN_OPEN: "enforced+known-open",
    ALLOWLISTED: "allowlisted",
    NOT_RUN: "not run",
    PARTLY_RUN: "partly run",
    ERRORED: "errored",
    UNENFORCED: "unenforced",
    MANUAL: "manual",
}

# ---- the checks: how to run each one (what it enforces is read from the check) ---------------


@dataclass
class Check:
    id: str
    title: str
    script: str  # relative to the repository
    args: list[str] = field(default_factory=list)
    run: str = "python"  # "python": python3 script; "direct": the script's own shebang (uv)
    parser: str = "text"  # "json" (--json file of findings), "frame-gate" (--json timings), "text"
    json_flag: bool = False  # takes --json FILE
    jobs_flag: bool = False  # takes --jobs N
    build_flag: bool = False  # takes --build
    offline_args: list[str] | None = None  # how to run it with no app; None: it cannot
    offline_scope: str = ""  # what the offline run leaves out
    gate_only: bool = False  # a non-zero exit with no parsed finding is a violation, not a crash
    timeout: int = 3600
    sources: list[str] = field(default_factory=list)  # files the registry reads, beside `script`
    rust_meta: list[str] = field(default_factory=list)  # Rust files with `Meta { key, ids }` tables
    aliases: list[str] = field(default_factory=list)  # regexes beside the id by which rules.json names it
    kind: str = "script"  # "unit-tests": cargo tests that tests/rules-tests.json holds to rules (`script` is that map)

    @property
    def apps(self) -> bool:
        return self.kind == "script" and self.offline_args is None

    def needs_binaries(self, offline: bool) -> bool:
        """Whether the check renders the debug binaries of the apps in this mode (so a stale binary taints it). A check
        whose offline command is its normal one (ui-lint, the icon checks) renders nothing in either."""
        if self.kind != "script":
            return False
        return self.offline_args is None or (not offline and list(self.offline_args) != list(self.args))


CHECKS: list[Check] = [
    Check("ui-lint", "source: tokens, launch, blocking calls, panics, raw presses, loops", "scripts/ui-lint.py",
          offline_args=[], timeout=900),
    Check("taxonomy-check", "every exported bingux-ui item has a taxonomy entry", "scripts/taxonomy-check.py",
          offline_args=[], gate_only=True, timeout=900),
    Check("icons-map-fork", "icon sources match labels.tsv", "scripts/icons/map-fork.py", ["--check"],
          offline_args=["--check"], gate_only=True, timeout=900, aliases=[r"map-fork"]),
    Check("icons-build", "icon weight policy, fit rules, output current", "scripts/icons/build.py", ["--check"],
          run="direct", offline_args=["--check"], gate_only=True, timeout=1800, aliases=[r"build\.py"]),
    Check("icons-colour", "colour file and folder icons", "scripts/icons/colour.py", ["--check"],
          run="direct", offline_args=["--check"], gate_only=True, timeout=1800, aliases=[r"colour\.py"]),
    Check("icons-golden", "every icon at every size against golden.json", "scripts/icons/golden.py",
          run="direct", offline_args=[], timeout=1800, aliases=[r"golden(?:\.py)?"]),
    Check("copy-lint", "forbidden copy over source strings and captured text", "scripts/copy-lint.py",
          parser="json", json_flag=True, jobs_flag=True, build_flag=True,
          offline_args=["--no-captures"], offline_scope="source strings only, no captures"),
    Check("layout-lint", "17 layout rules and the padding model over every page, popup and window, parity and RTL",
          "scripts/layout-lint.py", ["--parity", "--rtl"], parser="json", json_flag=True, jobs_flag=True, build_flag=True,
          sources=["scripts/layout-lint.py"], rust_meta=["packages/bingux-ui/src/inspector/checks/mod.rs"]),
    Check("overflow-check", "text rules under pseudo-locale, 200% text, long data, narrow windows",
          "scripts/overflow-check.py", parser="json", json_flag=True, jobs_flag=True, build_flag=True),
    Check("state-check", "hover, press, focus, selection, touch targets, Tab walk, menus, cursors",
          "scripts/state-check.py", parser="json", json_flag=True, jobs_flag=True, build_flag=True,
          rust_meta=["packages/bingux-ui/src/inspector/state_checks/mod.rs"]),
    Check("focus-ring-check", "each focus ring is its face's box plus the outset",
          "scripts/focus-ring-check.py", parser="json", json_flag=True, jobs_flag=True, build_flag=True),
    Check("tooltip-check", "tooltip bubble width, placement and inside the window",
          "scripts/tooltip-check.py", parser="json", json_flag=True, jobs_flag=True, build_flag=True, timeout=21600),
    Check("pixel-check", "text contrast and popup perimeter measured on the PNGs",
          "scripts/pixel-check.py", parser="json", json_flag=True, jobs_flag=True, build_flag=True),
    Check("structure-stability", "first frame against the settled frame of every page",
          "scripts/structure-stability.py", parser="json", json_flag=True, jobs_flag=True, build_flag=True,
          aliases=[r"structure-stab(?:ility)?"]),
    Check("scroll-continuity", "scroll offset kept through push, pop, tab switch and data landing",
          "scripts/scroll-continuity.py", parser="json", json_flag=True, jobs_flag=True, build_flag=True,
          aliases=[r"scroll-cont(?:inuity)?"]),
    Check("frame-gate", "mount cost of every page against its budget",
          "scripts/frame-gate.py", parser="frame-gate", json_flag=True),
    Check("loop-check", "loops move and nothing else does", "scripts/loop-check.py", ["--all"]),
    Check("number-roll-check", "animated numbers roll", "scripts/number-roll-check.py", aliases=[r"number-roll"]),
    Check("nav-slide", "a subpage slides in and its close is the open mirrored", "scripts/nav-slide.py"),
    Check("unit-tests", "cargo unit tests of bingux-ui that tests/rules-tests.json holds to rules", "tests/rules-tests.json",
          run="cargo", kind="unit-tests", timeout=7200, aliases=[r"unit:", r"unit tests?", r"cargo test"]),
]
CHECK_BY_ID = {check.id: check for check in CHECKS}

ID_RE = re.compile(r"\b(?!CK-)[A-Z]{2}-\d{2}\b")
GROUP_RE = re.compile(r"\[((?!CK-)[A-Z]{2}-\d{2}(?:\s*[,/]\s*(?!CK-)[A-Z]{2}-\d{2})*)\]")
KEY_RE = re.compile(r"^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$")


def ids_in(text: str) -> list[str]:
    return ID_RE.findall(text)


def group_ids(text: str) -> list[str]:
    """The rule IDs of every `[SP-01, AL-09]` group in `text`, in order, once each."""
    seen: list[str] = []
    for group in GROUP_RE.findall(text):
        for rule_id in ids_in(group):
            if rule_id not in seen:
                seen.append(rule_id)
    return seen


# ---- the rule book -------------------------------------------------------------------------------


class RuleBook:
    def __init__(self, data: dict):
        self.data = data
        self.rules: dict[str, dict] = {rule["id"]: rule for rule in data["rules"]}
        self.order: list[str] = [rule["id"] for rule in data["rules"]]
        self.sections: dict[str, dict] = {section["code"]: section for section in data["sections"]}

    @classmethod
    def load(cls, path: Path | None = None) -> "RuleBook":
        return cls(json.loads((path or RULES_JSON).read_text()))

    def section_of(self, rule_id: str) -> str:
        return self.rules[rule_id]["section"]


def load_manual(path: Path | None = None) -> dict[str, str]:
    path = path or MANUAL_JSON
    if not path.exists():
        return {}
    return json.loads(path.read_text())["rules"]


# ---- the registry: what each check enforces, read from the check ---------------------------------


@dataclass
class Derived:
    """What one source file says about the rules it enforces."""

    keys: dict[str, set[str]] = field(default_factory=dict)  # rule key -> IDs
    cited: set[str] = field(default_factory=set)  # every ID the file enforces
    documented: set[str] = field(default_factory=set)  # IDs only its docstrings mention
    messages: dict[str, str] = field(default_factory=dict)  # RULE_MESSAGES: key -> regex
    sources: dict[str, str] = field(default_factory=dict)  # rule key -> where it was read
    unmapped: dict[str, str] = field(default_factory=dict)  # RULE_UNMAPPED: key -> why no rule of the book holds it
    tests: list[dict] = field(default_factory=list)  # unit-tests: the entries of tests/rules-tests.json

    def merge(self, other: "Derived") -> None:
        for key, ids in other.keys.items():
            self.keys.setdefault(key, set()).update(ids)
            self.sources.setdefault(key, other.sources.get(key, ""))
        self.cited |= other.cited
        self.documented |= other.documented
        self.messages.update(other.messages)
        self.unmapped.update(other.unmapped)
        self.tests += other.tests


def rel_of(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return path.name


def _str_const(node: ast.AST) -> str | None:
    return node.value if isinstance(node, ast.Constant) and isinstance(node.value, str) else None


def _id_list(node: ast.AST) -> list[str] | None:
    """The IDs of a list or tuple that holds nothing but rule IDs."""
    if isinstance(node, (ast.List, ast.Tuple, ast.Set)) and node.elts:
        values = [_str_const(el) for el in node.elts]
        if all(v is not None and re.fullmatch(ID_RE, v) for v in values):
            return [v for v in values if v]
    return None


def _branches(node: ast.AST) -> list[ast.AST]:
    if isinstance(node, ast.IfExp):
        return _branches(node.body) + _branches(node.orelse)
    return [node]


def derive_python(path: Path) -> Derived:
    out = Derived()
    rel = rel_of(path)
    tree = ast.parse(path.read_text(), filename=str(path))
    docstrings: set[int] = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) and node.body:
            first = node.body[0]
            if isinstance(first, ast.Expr) and _str_const(first.value) is not None:
                docstrings.add(id(first.value))
                out.documented.update(ids_in(first.value.value))
    for stmt in tree.body:
        if isinstance(stmt, ast.Assign) and len(stmt.targets) == 1 and isinstance(stmt.targets[0], ast.Name):
            name = stmt.targets[0].id
            if name in ("RULE_IDS", "RULE_MESSAGES", "RULE_UNMAPPED"):
                value = ast.literal_eval(stmt.value)
                if name == "RULE_IDS":
                    for key, ids in value.items():
                        out.keys.setdefault(key, set()).update(ids)
                        out.sources.setdefault(key, f"{rel} RULE_IDS")
                        out.cited.update(ids)
                elif name == "RULE_UNMAPPED":
                    for key, why in value.items():
                        out.unmapped[key] = why
                        out.keys.setdefault(key, set())
                        out.sources.setdefault(key, f"{rel} RULE_UNMAPPED")
                else:
                    out.messages.update(value)
    # A fixture drives the check on made-up input; its findings are not the tree's.
    fixtures = {id(inner) for node in ast.walk(tree) if isinstance(node, ast.FunctionDef) and node.name in ("fixtures", "run_fixtures")
                for inner in ast.walk(node)}
    for node in ast.walk(tree):
        if id(node) in fixtures:
            continue
        text = _str_const(node)
        if text is not None and id(node) not in docstrings:
            out.cited.update(group_ids(text))
        ids = _id_list(node)
        if ids:
            out.cited.update(ids)
        sequence: list[ast.AST] | None = None
        if isinstance(node, ast.Call):
            sequence = list(node.args)
        elif isinstance(node, ast.Tuple):
            sequence = list(node.elts)
        if not sequence:
            continue
        for at in range(1, len(sequence)):
            keys = [_str_const(b) for b in _branches(sequence[at - 1])]
            lists = [_id_list(b) for b in _branches(sequence[at])]
            if not keys or not all(k and KEY_RE.match(k) for k in keys) or not all(lists):
                continue
            pairs = zip(keys, lists) if len(keys) == len(lists) else [(k, ids) for k in keys for ids in lists]
            for key, ids in pairs:
                out.keys.setdefault(key, set()).update(ids or [])
                out.sources.setdefault(key, f"{rel}:{node.lineno}")
    out.documented -= out.cited
    return out


META_RE = re.compile(
    r'Rule::(\w+)\s*=>\s*Meta\s*\{\s*key:\s*"([^"]+)",\s*label:\s*"(?:[^"\\]|\\.)*",\s*ids:\s*"([^"]*)"', re.S
)


def derive_rust_meta(path: Path) -> Derived:
    out = Derived()
    rel = rel_of(path)
    for match in META_RE.finditer(path.read_text()):
        key, ids = match.group(2), ids_in(match.group(3))
        out.keys.setdefault(key, set()).update(ids)
        out.sources[key] = rel
        out.cited.update(ids)
    return out


def derive_unit_tests(path: Path) -> Derived:
    """The entries of tests/rules-tests.json: each a Rust test module and the rules its tests hold."""
    out = Derived()
    rel = rel_of(path)
    data = json.loads(path.read_text())
    for entry in data["tests"]:
        item = {"package": entry.get("package", data.get("package", "bingux-ui")),
                "features": entry.get("features", data.get("features", "testing")),
                "module": entry["module"], "rules": list(entry["rules"]), "covers": entry.get("covers", "")}
        out.tests.append(item)
        out.keys.setdefault(item["module"], set()).update(item["rules"])
        out.sources.setdefault(item["module"], rel)
        out.cited.update(item["rules"])
    return out


PATH_MOD = re.compile(r'#\[path\s*=\s*"([^"]+)"\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([a-z]+\))?\s+)?mod\s+(\w+)\s*;')


_PATH_MOUNTS: dict[Path, dict[Path, tuple[Path, str]]] = {}


def path_mounts(src: Path) -> dict[Path, tuple[Path, str]]:
    """Every `#[path = "x.rs"] mod name;` under `src`: the mounted file -> (the file that declares it, name)."""
    if src not in _PATH_MOUNTS:
        mounts = {}
        for other in sorted(src.rglob("*.rs")):
            text = other.read_text(errors="replace")
            if "#[path" not in text:
                continue
            for target, name in PATH_MOD.findall(text):
                mounts[(other.parent / target).resolve()] = (other, name)
        _PATH_MOUNTS[src] = mounts
    return _PATH_MOUNTS[src]


def module_of_file(src: Path, file: Path) -> str:
    """The Rust module path a source file is mounted at: its name under the module whose file holds it
    (`a/b.rs` and `a/b/mod.rs` are `a::b` when `a` is a module of the crate), unless a
    `#[path = "file.rs"] mod name;` elsewhere mounts it under another name (a test file that sits beside
    its subject but is declared inside it: `controls/tooltip_tests.rs` is `controls::tooltip::tests`)."""
    file = file.resolve()
    mounted = path_mounts(src).get(file)
    if mounted:
        parent, name = mounted
        return "::".join(part for part in (module_of_file(src, parent), name) if part)
    if file.parent == src.resolve() and file.stem in ("lib", "main"):
        return ""
    name = file.parent.name if file.stem == "mod" else file.stem
    directory = file.parent.parent if file.stem == "mod" else file.parent
    if directory == src.resolve():
        parent = ""
    else:
        holder = next((c for c in (directory / "mod.rs", directory.parent / f"{directory.name}.rs") if c.is_file()), None)
        parent = module_of_file(src, holder) if holder else "::".join(directory.relative_to(src.resolve()).parts)
    return "::".join(part for part in (parent, name) if part)


def declares_module(file: Path, path: list[str]) -> bool:
    """Whether `file` declares the modules of `path` one inside the next (inline or by `#[path]`)."""
    text = file.read_text(errors="replace")
    return all(re.search(rf"\bmod\s+{re.escape(name)}\b", text) for name in path)


def test_files_for(entry: dict, root: Path | None = None) -> list[Path]:
    """The Rust files with `#[test]` that hold the module path of an entry: the file mounted at exactly that
    path (a file, `mod.rs`, or a `#[path]` file), a directory of them, or the file that declares the modules
    of the rest of the path on the way. A path that only a file's location suggests, when the file is mounted
    elsewhere or declares no such module, holds nothing: cargo would run no test of it."""
    src = (root or ROOT) / "packages" / entry["package"] / "src"
    parts = entry["module"].split("::")
    full = src.joinpath(*parts)
    if full.is_dir():
        found = [p for p in sorted(full.rglob("*.rs")) if "#[test]" in p.read_text(errors="replace")]
        if found:
            return found
    for cut in range(len(parts), 0, -1):
        base = src.joinpath(*parts[:cut])
        for candidate in (base.with_suffix(".rs"), base / "mod.rs"):
            if not (candidate.is_file() and "#[test]" in candidate.read_text(errors="replace")):
                continue
            if module_of_file(src, candidate) == "::".join(parts[:cut]) and declares_module(candidate, parts[cut:]):
                return [candidate]
    # A file mounted elsewhere by `#[path]`: found by the path it is mounted at.
    wanted = entry["module"]
    for candidate in sorted(src.rglob("*.rs")):
        if "#[test]" in candidate.read_text(errors="replace"):
            mounted = module_of_file(src, candidate)
            if mounted == wanted or mounted.startswith(wanted + "::"):
                return [candidate]
    return []


@dataclass
class Registry:
    book: RuleBook
    derived: dict[str, Derived]  # check id -> what it enforces
    problems: dict[str, list[str]]  # kind -> lines

    def keys_of(self, check_id: str) -> dict[str, set[str]]:
        return self.derived[check_id].keys

    def ids_of(self, check_id: str) -> set[str]:
        return self.derived[check_id].cited

    def ids_for(self, check_id: str, key: str) -> list[str]:
        """The IDs a finding of `key` stands for: the key's own, else the check's default `*`."""
        keys = self.derived[check_id].keys
        return sorted(keys.get(key) or keys.get("*") or [])

    def rule_checks(self) -> dict[str, list[tuple[str, list[str]]]]:
        """rule ID -> [(check id, keys that cite it)], from the registry."""
        out: dict[str, list[tuple[str, list[str]]]] = defaultdict(list)
        for check_id, derived in self.derived.items():
            for rule_id in sorted(derived.cited):
                keys = sorted(key for key, ids in derived.keys.items() if rule_id in ids and key != "*")
                out[rule_id].append((check_id, keys))
        return out


def build_registry(book: RuleBook, root: Path | None = None) -> Registry:
    root = root or ROOT
    derived: dict[str, Derived] = {}
    problems: dict[str, list[str]] = {"unknown": [], "uncited": [], "documented": [], "unkeyed": [], "tests_unmatched": []}
    for check in CHECKS:
        item = Derived()
        if check.kind == "unit-tests":
            item.merge(derive_unit_tests(root / check.script))
            for entry in item.tests:
                if not test_files_for(entry, root):
                    problems["tests_unmatched"].append(
                        f"{entry['module']}: no `#[test]` in {entry['package']} under that module path (tests/rules-tests.json is stale)")
        else:
            for rel in dict.fromkeys([check.script, *check.sources]):
                item.merge(derive_python(root / rel))
            for rel in check.rust_meta:
                item.merge(derive_rust_meta(root / rel))
        item.documented -= item.cited
        derived[check.id] = item
        for rule_id in sorted(item.cited):
            if rule_id not in book.rules:
                problems["unknown"].append(f"{check.id}: cites {rule_id}, which is not in design/rules.json")
        for rule_id in sorted(item.documented):
            if rule_id in book.rules:
                problems["documented"].append(f"{check.id}: its docstring mentions {rule_id} but no finding of it carries that ID")
            else:
                problems["unknown"].append(f"{check.id}: its docstring mentions {rule_id}, which is not in design/rules.json")
        if not item.cited:
            problems["uncited"].append(f"{check.id}: no rule ID found: declare RULE_IDS in {check.script}")
    return Registry(book, derived, problems)


def rust_rule_keys(path: Path) -> list[str]:
    """Every rule key a Rust file registers (`key: "x"` of a Meta or the lint's `Rule::key`)."""
    return re.findall(r'^\s*Rule::\w+\s*=>\s*"([a-z][a-z0-9-]*)",\s*$', path.read_text(), re.M)


def layout_keys_without_ids(registry: Registry) -> list[str]:
    """Rule keys layout-lint can report that cite no rule ID and are not declared in its RULE_UNMAPPED."""
    if "layout-lint" not in registry.derived:
        return []
    path = ROOT / "packages/bingux-ui/src/inspector/layout_checks.rs"
    keys = set(re.findall(r'Rule::\w+\s*=>\s*"([a-z][a-z0-9-]*)"', path.read_text()))
    derived = registry.derived["layout-lint"]
    return sorted(key for key in keys if not derived.keys.get(key) and key not in derived.unmapped)


def unmapped_keys(registry: Registry) -> list[dict]:
    """The rule keys a check finds that no rule of the book states (RULE_UNMAPPED), with the reason."""
    return [{"check": check_id, "key": key, "reason": why}
            for check_id, derived in registry.derived.items() for key, why in sorted(derived.unmapped.items())]


def stray_citations(book: RuleBook) -> list[str]:
    """IDs cited by files that are not checks (docs, Rust comments, tests) and absent from rules.json."""
    # The book, the generated report, and the tests of this script (which cite made-up IDs on purpose).
    skip = {"design/rules.json", "design/rules.md", "design/rules-report.md", "design/rules-report.json", "tests/rules-check.test.py"}
    found: list[str] = []
    for base in ("scripts", "packages/bingux-ui/src", "packages/bingux-settings/src", "docs", "design", "tests"):
        for path in sorted((ROOT / base).rglob("*")):
            if not path.is_file() or path.suffix not in (".py", ".rs", ".md", ".json", ".qml") or "target" in path.parts:
                continue
            rel = path.relative_to(ROOT).as_posix()
            if rel in skip:
                continue
            try:
                text = path.read_text()
            except UnicodeDecodeError:
                continue
            for number, line in enumerate(text.splitlines(), 1):
                for rule_id in dict.fromkeys(group_ids(line)):
                    if rule_id not in book.rules:
                        found.append(f"{rel}:{number}: [{rule_id}]")
    return found


# ---- allowlists: every entry with its reason -------------------------------------------------------

ALLOW_NAMES = re.compile(r"(^ALLOWED$|^KNOWN$|ALLOWED$|ALLOW$|^ALLOWED_MOTION$)")


def python_allowlists(check: Check, registry: Registry) -> list[dict]:
    entries: list[dict] = []
    if check.kind != "script":
        return entries  # a map of tests, not a script: it allowlists nothing
    for rel in dict.fromkeys([check.script, *check.sources]):
        path = ROOT / rel
        tree = ast.parse(path.read_text())
        strings: dict[str, str] = {}
        for stmt in tree.body:
            if isinstance(stmt, ast.Assign) and len(stmt.targets) == 1 and isinstance(stmt.targets[0], ast.Name):
                text = _str_const(stmt.value)
                if text is not None:
                    strings[stmt.targets[0].id] = text

        def literal(node: ast.AST):
            if isinstance(node, ast.Name) and node.id in strings:
                return strings[node.id]
            return ast.literal_eval(node)

        for stmt in tree.body:
            target = value = None
            if isinstance(stmt, ast.Assign) and len(stmt.targets) == 1 and isinstance(stmt.targets[0], ast.Name):
                target, value = stmt.targets[0].id, stmt.value
            elif isinstance(stmt, ast.AnnAssign) and isinstance(stmt.target, ast.Name) and stmt.value is not None:
                target, value = stmt.target.id, stmt.value
            if target is None or not ALLOW_NAMES.search(target):
                continue
            kind = "known-open" if target == "KNOWN" else "allowed"
            where = f"{rel}:{stmt.lineno}"
            try:
                if isinstance(value, ast.Dict):
                    for key, why in zip(value.keys, value.values):
                        entries.append({"check": check.id, "list": target, "kind": kind, "rule": "", "where": literal(key),
                                        "reason": literal(why), "source": where, "counted": check.parser == "json"})
                elif isinstance(value, (ast.List, ast.Tuple)):
                    for element in value.elts:
                        if isinstance(element, ast.Dict):
                            item = {literal(k): literal(v) for k, v in zip(element.keys, element.values)}
                            entries.append({"check": check.id, "list": target, "kind": kind, "rule": item.get("rule", ""),
                                            "where": item.get("where") or item.get("node") or item.get("step") or item.get("capture", ""),
                                            "reason": item.get("why", ""), "source": where, "counted": check.parser == "json"})
                        elif isinstance(element, ast.Call) and len(element.args) == 3:
                            rule, loc, why = (literal(a) for a in element.args)
                            entries.append({"check": check.id, "list": target, "kind": kind, "rule": rule, "where": loc,
                                            "reason": why, "source": where, "counted": check.parser == "json"})
            except (ValueError, SyntaxError):
                continue
    return entries


ALLOW_RUST = re.compile(
    r'Rule::(\w+),\s*component:\s*"([^"]*)",\s*(?:name:\s*"([^"]*)",\s*)?reason:\s*"((?:[^"\\]|\\.)*)"', re.S
)
VARIANT_KEY = re.compile(r'Rule::(\w+)\s*=>\s*Meta\s*\{\s*key:\s*"([^"]+)"', re.S)


def rust_allowlists(check: Check) -> list[dict]:
    entries: list[dict] = []
    for rel in check.rust_meta:
        path = ROOT / rel
        text = path.read_text()
        variant_key = {m.group(1): m.group(2) for m in VARIANT_KEY.finditer(text)}
        for match in ALLOW_RUST.finditer(text):
            variant, component, name, reason = match.groups()
            reason = re.sub(r"\\\n\s*", "", reason).replace('\\"', '"').replace("\\\\", "\\")
            # state_checks' ALLOW findings carry their reason into the check's JSON; checks' EXEMPT findings are never emitted.
            entries.append({"check": check.id, "list": "Rust ALLOW" if "state_checks" in rel else "Rust EXEMPT", "kind": "allowed",
                            "rule": variant_key.get(variant, variant), "where": f"{component}{' / ' + name if name else ''}",
                            "reason": " ".join(reason.split()), "source": rel, "counted": "state_checks" in rel})
    return entries


# ---- findings --------------------------------------------------------------------------------------


def make_finding(check: str, key: str, ids: list[str], severity: str, location: str, message: str, reason: str = "") -> dict:
    return {"check": check, "key": key, "ids": ids, "severity": severity, "location": location,
            "message": " ".join(message.split())[:400], "reason": reason}


def split_ids(value) -> list[str]:
    if isinstance(value, str):
        return ids_in(value)
    if isinstance(value, list):
        return [item for item in value if isinstance(item, str) and ID_RE.fullmatch(item)]
    return []


def findings_from_json(check: Check, data, registry: Registry) -> list[dict]:
    """Normalise the findings of a `--json` file of layout-lint, state-check, probe_lib and the like."""
    out: list[dict] = []
    unmapped = registry.derived[check.id].unmapped
    for row in data:
        key = str(row.get("rule", ""))
        message = str(row.get("what") or row.get("detail") or row.get("note") or "")
        ids = split_ids(row.get("ids")) or group_ids(message) or registry.ids_for(check.id, key)
        location = ": ".join(str(part) for part in (row.get("capture", ""), row.get("target") or row.get("name") or row.get("label") or "") if part)
        reason = row.get("allowed") or ""
        known = row.get("known") or ""
        severity = "allowed" if reason else "known-open" if known else "fail"
        finding = make_finding(check.id, key, ids, severity, location, message, reason or known)
        if check.id == "overflow-check":
            finding["scenario"] = str(row.get("capture", "")).split("-", 1)[0]  # baseline, pseudo, scale200, longdata, worst
        if key in unmapped and not ids:
            finding["unmapped"] = unmapped[key]
        out.append(finding)
    return out


def findings_from_frame_gate(check: Check, rows, registry: Registry) -> list[dict]:
    out = []
    for row in rows:
        if row.get("level") == "fail":
            out.append(make_finding(check.id, "page-mount", registry.ids_for(check.id, "page-mount") or registry.ids_for(check.id, "*"),
                                    "fail", f"{row.get('page', '')}@{row.get('size', '')}", str(row.get("note", "over the fail budget"))))
    return out


# `partly run: ...` is a line a check prints when it left part of its job out on purpose and says what.
PROBLEM_LINE = re.compile(r"^(unused .*entry|no captures|CRASH|no control was swept|.*nothing was measured|.*: nothing was checked|no captures: |partly run: )", re.I)
LOCATION = re.compile(r"^(\S+?:\d+)(?::\d+)?: ")
HYGIENE = re.compile(r"^unused ", re.I)  # an allowlist entry that matched nothing: the check is red, what it measured is sound
STOPPED_EARLY = re.compile(r"(\d+) sweeps stopped early")


def incomplete(problems: list[str], log: str) -> bool:
    """Whether a check left part of its job undone: a capture that timed out or crashed, a sweep stopped early, nothing measured."""
    if any(not HYGIENE.match(problem) for problem in problems):
        return True
    stopped = STOPPED_EARLY.search(log)
    return bool(stopped and int(stopped.group(1)) > 0)


def findings_from_text(check: Check, text: str, registry: Registry) -> tuple[list[dict], list[str]]:
    derived = registry.derived[check.id]
    patterns = {key: re.compile(rx) for key, rx in derived.messages.items()}
    findings: list[dict] = []
    problems: list[str] = []
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line.strip():
            continue
        if PROBLEM_LINE.search(line.strip()):
            problems.append(line.strip())
            continue
        # A line the check prints as `note:` does not fail it: an open defect it reports and tolerates.
        noted = line.strip().startswith("note:")
        key = next((name for name, rx in patterns.items() if rx.search(line)), "")
        bracket = group_ids(line)
        if not key and not bracket:
            continue
        ids = list(dict.fromkeys(bracket + registry.ids_for(check.id, key))) if key else bracket
        where = LOCATION.match(line.strip())
        findings.append(make_finding(check.id, key or "cited", ids, "known-open" if noted else "fail", where.group(1) if where else "",
                                     line.strip(), "printed by the check as a note, which does not fail it" if noted else ""))
    return findings, problems


# ---- running ---------------------------------------------------------------------------------------


@dataclass
class Context:
    out: Path
    jobs: int
    build: bool
    offline: bool
    reuse: Path | None
    timeout_scale: float = 1.0
    extra: dict[str, list[str]] = field(default_factory=dict)  # --arg CHECK="options": appended to that check's command
    allow_stale: bool = False  # report a check that rendered a binary older than its sources as if it had measured the tree

    @property
    def raw(self) -> Path:
        return self.out / "raw"


def command_for(check: Check, ctx: Context, json_path: Path) -> list[str]:
    offline = ctx.offline and check.offline_args is not None
    args = list(check.offline_args if offline else check.args)
    if check.json_flag:
        args += ["--json", str(json_path)]
    if check.jobs_flag and not offline:
        args += ["--jobs", str(ctx.jobs)]
    if check.build_flag and ctx.build and not offline:
        args += ["--build"]
    head = [sys.executable, str(ROOT / check.script)] if check.run == "python" else [str(ROOT / check.script)]
    return head + args + ctx.extra.get(check.id, [])


def child_env(scratch: Path | None = None) -> dict[str, str]:
    env = isolated(os.environ)
    env.setdefault("CARGO_TARGET_DIR", DEFAULT_TARGET)
    if scratch is not None:
        env["TMPDIR"] = str(scratch)  # a check's captures land here and go when it is done
    return env


# The app binaries the checks render, and the package each is built from. A binary is stale when a source it was built
# from (its cargo dep-info, `<name>.d`) changed after it was: a check that ran on it measured an older tree.
APP_BINARIES = {"bingux-settings": "bingux-settings", "bingux-taxonomy": "bingux-taxonomy", "bingux-files": "bingux-files-app"}


def dep_sources(dep_file: Path) -> list[str]:
    """The source files cargo's dep-info lists for a binary (the first `target: dep dep ...` line)."""
    try:
        first = dep_file.read_text(errors="replace").split("\n", 1)[0]
    except OSError:
        return []
    _, _, rest = first.partition(": ")
    return [item.replace("\\ ", " ") for item in re.findall(r"(?:\\ |[^ ])+", rest)]


def package_sources(root: Path, packages: tuple[str, ...]) -> list[str]:
    found: list[str] = []
    for package in packages:
        for base, dirs, files in os.walk(root / "packages" / package):
            dirs[:] = [d for d in dirs if d != "target"]
            found += [os.path.join(base, name) for name in files if name.endswith((".rs", ".toml"))]
    return found


def binary_freshness(target: Path | None = None, root: Path | None = None) -> dict[str, dict]:
    """For each app binary: when it was built, the newest source it was built from (and which file), and whether it is stale."""
    target = target or Path(os.environ.get("CARGO_TARGET_DIR", DEFAULT_TARGET))
    root = root or ROOT
    out: dict[str, dict] = {}
    for name, package in APP_BINARIES.items():
        path = target / "debug" / name
        sources = dep_sources(target / "debug" / f"{name}.d") or package_sources(root, (package, "bingux-ui"))  # no dep-info: the packages
        sources += [str(root / "packages" / pkg / "Cargo.toml") for pkg in (package, "bingux-ui")]
        newest, newest_path = 0.0, ""
        for source in sources:
            try:
                if Path(source).resolve().is_relative_to(root.resolve()):
                    mtime = os.stat(source).st_mtime
                    if mtime > newest:
                        newest, newest_path = mtime, source
            except OSError:
                continue
        built = path.stat().st_mtime if path.exists() else None
        out[name] = {"built_epoch": built, "newest_source_epoch": newest or None,
                     "newest_source": os.path.relpath(newest_path, root) if newest_path else "",
                     "stale": built is None or (newest > 0 and built < newest)}
    return out


def unit_test_groups(check: Check) -> list[tuple[str, str, list[str]]]:
    """(package, features, filters) of the cargo invocations a unit-tests check makes, from its map."""
    data = json.loads((ROOT / check.script).read_text())
    groups: dict[tuple[str, str], list[str]] = {}
    for entry in data["tests"]:
        key = (entry.get("package", data.get("package", "bingux-ui")), entry.get("features", data.get("features", "testing")))
        groups.setdefault(key, []).append(entry["module"] + "::")
    return [(package, features, filters) for (package, features), filters in groups.items()]


def run_command(argv: list[str], env: dict[str, str], timeout: float) -> tuple[int, str]:
    try:
        proc = subprocess.run(argv, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout, check=False)
        return proc.returncode, proc.stdout.decode(errors="replace")
    except subprocess.TimeoutExpired as error:
        return 124, (error.stdout or b"").decode(errors="replace") + f"\ntimed out after {timeout:.0f}s\n"
    except OSError as error:
        return 127, f"could not start {argv[0]}: {error}\n"


def execute(check: Check, ctx: Context) -> dict:
    """Run one check and keep its output; returns the run record without findings."""
    ctx.raw.mkdir(parents=True, exist_ok=True)
    log = ctx.raw / f"{check.id}.log"
    json_path = ctx.raw / f"{check.id}.json"
    meta_path = ctx.raw / f"{check.id}.meta.json"
    json_path.unlink(missing_ok=True)
    started = time.time()
    scratch = ctx.out / "tmp" / check.id
    shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir(parents=True, exist_ok=True)
    scope = check.offline_scope if ctx.offline and check.offline_args is not None else ""
    env = child_env(scratch)
    timeout = check.timeout * ctx.timeout_scale
    if check.kind == "unit-tests":
        argv_all: list[list[str]] = []
        exit_code, output = 0, ""
        for package, features, filters in unit_test_groups(check):
            argv = ["cargo", "test", "--manifest-path", str(ROOT / "packages" / package / "Cargo.toml"), "--lib", "--features", features,
                    "--", *filters, "--test-threads", str(ctx.jobs)] + ctx.extra.get(check.id, [])
            argv_all.append(argv)
            code, text = run_command(argv, env, timeout)
            output += text
            exit_code = exit_code or code
        argv = [part for command in argv_all for part in command + [";"]][:-1]
    else:
        argv = command_for(check, ctx, json_path)
        exit_code, output = run_command(argv, env, timeout)
    shutil.rmtree(scratch, ignore_errors=True)
    log.write_text(output)
    offline = ctx.offline and check.offline_args is not None
    fresh = binary_freshness() if check.needs_binaries(offline) else {}
    meta = {"check": check.id, "argv": [os.path.relpath(a, ROOT) if a.startswith(str(ROOT)) else a for a in argv],
            "exit": exit_code, "seconds": round(time.time() - started, 1), "scope": scope, "offline": offline,
            "ran_at": dt.datetime.now().astimezone().isoformat(timespec="seconds"), "json": json_path.exists(),
            "stale_binaries": sorted(name for name, info in fresh.items() if info["stale"]) if fresh else None}
    meta_path.write_text(json.dumps(meta, indent=1) + "\n")
    return meta


def load_reused(check: Check, directory: Path) -> dict | None:
    meta_path = directory / "raw" / f"{check.id}.meta.json"
    if not meta_path.exists() or not (directory / "raw" / f"{check.id}.log").exists():
        return None
    return json.loads(meta_path.read_text())


TEST_LINE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored)", re.M)
TEST_RESULT = re.compile(r"^test result: ", re.M)


def evaluate_unit_tests(check: Check, meta: dict, raw_dir: Path, registry: Registry) -> dict:
    """The outcome of a unit-tests check: each entry of the map has run its module's tests, each rule passes or fails with them."""
    log = (raw_dir / f"{check.id}.log").read_text(errors="replace")
    outcome = {name: status for name, status in TEST_LINE.findall(log)}
    findings: list[dict] = []
    problems: list[str] = []
    tests: list[dict] = []
    for entry in registry.derived[check.id].tests:
        pattern = re.compile("^" + re.escape(entry["module"]) + "::")  # test paths are from the crate root
        names = sorted(name for name in outcome if pattern.match(name))
        passed = sum(outcome[name] == "ok" for name in names)
        failed = [name for name in names if outcome[name] == "FAILED"]
        ignored = sum(outcome[name] == "ignored" for name in names)
        tests.append({"module": entry["module"], "rules": entry["rules"], "covers": entry["covers"],
                      "passed": passed, "failed": len(failed), "ignored": ignored})
        for name in failed:
            findings.append(make_finding(check.id, entry["module"], entry["rules"], "fail", name, f"test {name} failed"))
        if passed + len(failed) == 0 and TEST_RESULT.search(log):
            findings.append(make_finding(check.id, entry["module"], entry["rules"], "fail", entry["module"],
                                         "no test of this module ran: tests/rules-tests.json names a module the tests no longer have"
                                         if not names else "every test of this module is ignored"))
    exit_code = int(meta["exit"])
    if not TEST_RESULT.search(log):
        tail = [line.strip() for line in log.splitlines() if line.strip()][-2:]
        problems.append(f"cargo test ran no tests (exit {exit_code})" + (": " + " | ".join(tail) if tail else ""))
    failing = [f for f in findings if f["severity"] == "fail"]
    state = "red" if failing else "errored" if exit_code != 0 or problems else "clean"
    if exit_code != 0 and not failing and not problems:
        problems.append(f"exit {exit_code}")
    return {
        "id": check.id, "title": check.title, "state": state, "exit": exit_code, "seconds": meta.get("seconds", 0),
        "argv": meta.get("argv", []), "scope": meta.get("scope", ""), "ran_at": meta.get("ran_at", ""),
        "log": str(raw_dir / f"{check.id}.log"), "reused": bool(meta.get("reused")),
        "counts": {"fail": len(failing), "allowed": 0, "known-open": 0, "unattributed": sum(not f["ids"] for f in failing)},
        "notes": problems[:8], "findings": findings, "complete": not problems, "tests": tests, "stale": [],
    }


def evaluate(check: Check, meta: dict, raw_dir: Path, registry: Registry, allow_stale: bool = True) -> dict:
    """Turn a check's output into its run record, with findings. A check that rendered a binary older than the source it
    was built from is incomplete (so its rules are never `passing`) unless `allow_stale`."""
    if check.kind == "unit-tests":
        return evaluate_unit_tests(check, meta, raw_dir, registry)
    log = (raw_dir / f"{check.id}.log").read_text(errors="replace")
    json_path = raw_dir / f"{check.id}.json"
    findings: list[dict] = []
    problems: list[str] = []
    if check.parser in ("json", "frame-gate") and json_path.exists():
        try:
            data = json.loads(json_path.read_text())
            findings = (findings_from_json if check.parser == "json" else findings_from_frame_gate)(check, data, registry)
        except (json.JSONDecodeError, AttributeError, TypeError) as error:
            problems.append(f"{check.id}.json could not be read: {error}")
        _, problems_text = findings_from_text(check, log, registry)
        problems += problems_text
    else:
        findings, problems = findings_from_text(check, log, registry)
        if check.parser != "text":
            problems.append(f"no {check.id}.json was written")
    exit_code = int(meta["exit"])
    no_verdict = check.parser != "text" and not json_path.exists()  # success without a findings file proves nothing
    stopped = STOPPED_EARLY.search(log)
    if stopped and int(stopped.group(1)) > 0:
        problems.append(f"{stopped.group(1)} sweeps stopped early: part of the controls were not swept")
    failing = [f for f in findings if f["severity"] == "fail"]
    if exit_code != 0 and not failing and check.gate_only:
        tail = [line.strip() for line in log.splitlines() if line.strip()][-3:]
        findings.append(make_finding(check.id, "*", registry.ids_for(check.id, "*"), "fail", "", "exit {}: {}".format(exit_code, " | ".join(tail))))
        failing = [findings[-1]]
    if failing:
        state = "red"
    elif exit_code != 0 or no_verdict:
        hygiene_only = bool(problems) and all(HYGIENE.match(problem) for problem in problems) and not no_verdict
        state = "red" if hygiene_only else "errored"
        if not problems:
            tail = [line.strip() for line in log.splitlines() if line.strip()][-2:]
            problems.append(f"exit {exit_code}" + (": " + " | ".join(tail) if tail else ""))
    else:
        state = "clean"
    complete = not (incomplete(problems, log) or no_verdict)
    problems.sort(key=lambda problem: bool(HYGIENE.match(problem)))  # what stopped the check first, unused allowlist entries after
    # A binary older than the source it was built from measured an older tree. `stale_binaries` is what the run recorded
    # (None: an earlier run did not record it, which counts as stale for a check that renders a binary).
    stale: list[str] = []
    if check.needs_binaries(bool(meta.get("offline", bool(meta.get("scope"))))):
        recorded = meta.get("stale_binaries")
        stale = list(APP_BINARIES) if recorded is None else list(recorded)
        if stale and not allow_stale:
            problems.insert(0, ("binaries older than their sources: " if recorded is not None else "binary freshness was not recorded: ")
                            + ", ".join(stale) + " (rebuild, or --allow-stale)")
            complete = False
    unattributed = [f for f in failing if not f["ids"]]
    return {
        "id": check.id, "title": check.title, "state": state, "exit": exit_code, "seconds": meta.get("seconds", 0),
        "argv": meta.get("argv", []), "scope": meta.get("scope", ""), "ran_at": meta.get("ran_at", ""),
        "log": str(raw_dir / f"{check.id}.log"), "reused": bool(meta.get("reused")),
        "counts": {"fail": len(failing), "allowed": sum(f["severity"] == "allowed" for f in findings),
                   "known-open": sum(f["severity"] == "known-open" for f in findings), "unattributed": len(unattributed)},
        "notes": problems[:8], "findings": findings, "complete": complete, "stale": [] if allow_stale else stale,
    }


def not_run_record(check: Check, why: str) -> dict:
    return {"id": check.id, "title": check.title, "state": "not-run", "exit": None, "seconds": 0, "argv": [], "scope": "",
            "ran_at": "", "log": "", "reused": False, "counts": {"fail": 0, "allowed": 0, "known-open": 0, "unattributed": 0},
            "notes": [why], "findings": []}


def run_checks(selected: list[Check], skipped: dict[str, str], ctx: Context, registry: Registry, say=print) -> list[dict]:
    runs: dict[str, dict] = {}
    for check in CHECKS:
        if check.id in skipped:
            runs[check.id] = not_run_record(check, skipped[check.id])
    total = len(selected)
    for index, check in enumerate(selected, 1):
        meta = load_reused(check, ctx.reuse) if ctx.reuse else None
        if meta is not None and meta.get("scope", "") != (check.offline_scope if ctx.offline and check.offline_args is not None else ""):
            meta = None  # the earlier run measured less (or more) than this one asks for
        if meta is not None:
            meta["reused"] = True
            raw_dir = ctx.reuse / "raw"
            say(f"[{index}/{total}] {check.id}: reusing {raw_dir}")
        else:
            say(f"[{index}/{total}] {check.id}: running ...")
            meta = execute(check, ctx)
            raw_dir = ctx.raw
        record = evaluate(check, meta, raw_dir, registry, allow_stale=ctx.allow_stale)
        runs[check.id] = record
        c = record["counts"]
        say(f"[{index}/{total}] {check.id}: {record['state']} in {record['seconds']}s, exit {record['exit']}, "
            f"{c['fail']} failing, {c['allowed']} allowed, {c['known-open']} known open")
    return [runs[check.id] for check in CHECKS]


# ---- statuses ----------------------------------------------------------------------------------------


def compute_statuses(book: RuleBook, registry: Registry, runs: list[dict], manual: dict[str, str]) -> list[dict]:
    """The evidence for every rule of the book: status, citing checks, finding counts."""
    run_by = {run["id"]: run for run in runs}
    cited = registry.rule_checks()
    findings_by_rule: dict[str, list[dict]] = defaultdict(list)
    for run in runs:
        for finding in run["findings"]:
            for rule_id in finding["ids"]:
                findings_by_rule[rule_id].append(finding)
    rows = []
    for rule_id in book.order:
        rule = book.rules[rule_id]
        citing = cited.get(rule_id, [])
        found = findings_by_rule.get(rule_id, [])
        counts = {"fail": 0, "allowed": 0, "known-open": 0}
        for finding in found:
            counts["fail" if finding["severity"] == "fail" else finding["severity"]] += 1
        states = [run_by[check_id]["state"] for check_id, _ in citing]
        if not citing:
            status = MANUAL if rule_id in manual else UNENFORCED
        elif counts["fail"]:
            status = FAILING
        elif counts["known-open"]:
            status = KNOWN_OPEN
        elif "errored" in states:
            status = ERRORED
        elif all(state in ("clean", "red") for state in states):
            whole = all(run_by[check_id].get("complete", True) for check_id, _ in citing)
            status = (ALLOWLISTED if counts["allowed"] else PASSING) if whole else PARTLY_RUN
        elif any(state in ("clean", "red") for state in states):
            status = PARTLY_RUN
        else:
            status = NOT_RUN
        first = [f for f in found if f["severity"] == "fail"][:3] or [f for f in found if f["severity"] == "known-open"][:3]
        tests = [t for check_id, _ in citing for t in run_by[check_id].get("tests", []) if rule_id in t["rules"]]
        rows.append({
            "id": rule_id, "section": rule["section"], "priority": rule["priority"], "repeats": rule["repeats"],
            "rule": rule["rule"], "status": status,
            "checks": [{"check": check_id, "keys": keys, "state": run_by[check_id]["state"],
                        "scope": run_by[check_id]["scope"], "reused": run_by[check_id]["reused"]} for check_id, keys in citing],
            "counts": counts, "first": first, "tests": tests,
            "manual": manual.get(rule_id, ""),
            "declared": {"status": rule["status"], "enforced_by": rule["enforced_by"]},
        })
    return rows


def section_counts(book: RuleBook, rows: list[dict]) -> list[dict]:
    out = []
    for code, section in book.sections.items():
        counter = Counter(row["status"] for row in rows if row["section"] == code)
        out.append({"code": code, "title": section["title"], "group": section["group"], "rules": sum(counter.values()),
                    "status": {status: counter.get(status, 0) for status in STATUSES}})
    return out


def exit_code_of(report: dict, strict: bool) -> int:
    statuses = Counter(row["status"] for row in report["rules"])
    red_unattributed = any(run["counts"]["unattributed"] for run in report["checks"])
    bad = statuses[FAILING] or statuses[ERRORED] or red_unattributed
    bad = bad or any(run["state"] in ("red", "errored") for run in report["checks"])
    bad = bad or any(run.get("stale") for run in report["checks"])  # measured on binaries older than the source (not --allow-stale)
    if strict:
        bad = bad or statuses[KNOWN_OPEN] or any(r["status"] == UNENFORCED and r["priority"] == "P0" for r in report["rules"])
        problems = report.get("problems", {})
        bad = bad or any(problems.get(key) for key in ("named_not_cited", "cited_not_named", "declared_none_but_cited"))
    return 1 if bad else 0


# ---- tree state --------------------------------------------------------------------------------------


def git(*args: str) -> str:
    try:
        return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=False).stdout.strip()
    except OSError:
        return ""


def iso(epoch: float | None) -> str | None:
    return dt.datetime.fromtimestamp(epoch).astimezone().isoformat(timespec="seconds") if epoch else None


def tree_state() -> dict:
    target = Path(os.environ.get("CARGO_TARGET_DIR", DEFAULT_TARGET))
    fresh = binary_freshness(target)
    newest = max((info["newest_source_epoch"] or 0.0 for info in fresh.values()), default=0.0)
    binaries = {name: {"built": iso(info["built_epoch"]), "newest_source": iso(info["newest_source_epoch"]),
                       "newest_source_file": info["newest_source"],
                       "older_than_source": info["stale"] if info["built_epoch"] is not None else None}
                for name, info in fresh.items()}
    dirty = [line for line in git("status", "--porcelain").splitlines() if line.strip()]
    return {"commit": git("rev-parse", "--short=10", "HEAD"), "branch": git("rev-parse", "--abbrev-ref", "HEAD"),
            "dirty_files": len(dirty), "target_dir": str(target), "newest_source": iso(newest),
            "binaries": binaries}


# ---- the report --------------------------------------------------------------------------------------


def registry_table(registry: Registry) -> list[dict]:
    out = []
    for check_id, derived in registry.derived.items():
        check = CHECK_BY_ID.get(check_id)
        keyed = {key: sorted(ids) for key, ids in sorted(derived.keys.items())}
        out.append({"check": check_id, "script": check.script if check else "", "title": check.title if check else "",
                    "offline": bool(check and check.offline_args is not None),
                    "ids": sorted(derived.cited), "keys": keyed,
                    "sources": {key: derived.sources.get(key, "") for key in keyed}})
    return out


def build_report(book: RuleBook, registry: Registry, runs: list[dict], manual: dict[str, str], mode: dict,
                 tree: dict, allowlists: list[dict], now: dt.datetime | None = None) -> dict:
    rows = compute_statuses(book, registry, runs, manual)
    sections = section_counts(book, rows)
    totals = Counter(row["status"] for row in rows)
    cap = 200
    findings: list[dict] = []
    truncated: dict[str, int] = {}
    for run in runs:
        keep = [f for f in run["findings"] if f["severity"] != "allowed"]
        findings += keep[:cap]
        if len(keep) > cap:
            truncated[run["id"]] = len(keep) - cap
    allowed_counts: Counter = Counter()
    for run in runs:
        for finding in run["findings"]:
            if finding["severity"] in ("allowed", "known-open") and finding["reason"]:
                allowed_counts[(run["id"], " ".join(finding["reason"].split()))] += 1
    entries = []
    for entry in allowlists:
        counted = entry.get("counted", True)
        entries.append(dict(entry, counted=counted, matched=allowed_counts.get((entry["check"], " ".join(entry["reason"].split())), 0) if counted else None))
    runs_light = [{k: v for k, v in run.items() if k != "findings"} for run in runs]
    when = now or dt.datetime.now().astimezone()
    coverage = validate_coverage(book, registry)
    return {
        "version": 1, "generated": when.isoformat(timespec="seconds"), "mode": mode, "tree": tree,
        "totals": {status: totals.get(status, 0) for status in STATUSES} | {"rules": len(rows)},
        "sections": sections, "checks": runs_light, "rules": rows, "findings": findings,
        "findings_truncated": truncated, "findings_by_kind": findings_by_kind(runs), "unmapped": unmapped_report(registry, runs),
        "registry": registry_table(registry),
        "problems": {"unknown_ids": registry.problems["unknown"], "documented_only": registry.problems["documented"],
                     "checks_without_ids": registry.problems["uncited"], "layout_keys_without_ids": layout_keys_without_ids(registry),
                     "tests_unmatched": registry.problems.get("tests_unmatched", []),
                     "stray_unknown_citations": stray_citations(book),
                     "manual_but_enforced": sorted(rule_id for rule_id in manual if rule_id in registry.rule_checks()),
                     "unknown_manual": sorted(rule_id for rule_id in manual if rule_id not in book.rules),
                     "named_not_cited": coverage["named_not_cited"], "cited_not_named": coverage["cited_not_named"],
                     "declared_enforced_no_check": coverage["declared_enforced_no_check"],
                     "declared_none_but_cited": coverage["declared_none_but_cited"]},
        "allowlists": entries,
    }


def findings_by_kind(runs: list[dict]) -> list[dict]:
    """Every check's findings counted by the kind of finding (its rule key) and, for overflow-check, the scenario, with the rules
    each kind stands for: so a report never shows one rule's count as if it were one thing."""
    counted: dict[tuple[str, str, str], dict] = {}
    for run in runs:
        for finding in run["findings"]:
            key = (run["id"], finding["key"], finding.get("scenario", ""))
            row = counted.setdefault(key, {"check": key[0], "key": key[1], "scenario": key[2], "ids": finding["ids"],
                                           "fail": 0, "known-open": 0, "allowed": 0, "unmapped": finding.get("unmapped", "")})
            row[finding["severity"]] += 1
    return sorted(counted.values(), key=lambda row: (-row["fail"], -row["known-open"], row["check"], row["key"], row["scenario"]))


def unmapped_report(registry: Registry, runs: list[dict]) -> list[dict]:
    """Rule keys declared in RULE_UNMAPPED, with the findings of this run that fall in them (never dropped, never a rule's)."""
    run_by = {run["id"]: run for run in runs}
    out = []
    for item in unmapped_keys(registry):
        found = [f for f in run_by.get(item["check"], {}).get("findings", []) if f["key"] == item["key"]]
        out.append(dict(item, fail=sum(f["severity"] == "fail" for f in found), allowed=sum(f["severity"] == "allowed" for f in found),
                        ran=run_by.get(item["check"], {}).get("state", "not-run") not in ("not-run",)))
    return out


def table(headers: list[str], rows: list[list[str]]) -> str:
    def cell(value) -> str:
        return str(value).replace("|", "\\|").replace("\n", " ")

    lines = ["| " + " | ".join(headers) + " |", "|" + "|".join("---" for _ in headers) + "|"]
    lines += ["| " + " | ".join(cell(value) for value in row) + " |" for row in rows]
    return "\n".join(lines)


def status_cell(row: dict) -> str:
    status = row["status"]
    text = LABEL[status]
    c = row["counts"]
    if status == FAILING:
        text += f" ({c['fail']})"
    elif status == KNOWN_OPEN:
        text += f" ({c['known-open']})"
    elif status == ALLOWLISTED:
        text += f" ({c['allowed']})"
    if status in (PASSING, ALLOWLISTED, FAILING, KNOWN_OPEN):
        notes = sorted({c_["scope"] for c_ in row["checks"] if c_["scope"]})
        if notes:
            text += " [" + "; ".join(notes) + "]"
        # (which checks ran, and when, is in the Checks table)
    return text


def render_markdown(report: dict) -> str:
    book_rows = report["rules"]
    mode = report["mode"]
    tree = report["tree"]
    lines = ["# Design rules report", "",
             "Generated by `python3 scripts/rules-check.py`; do not edit. Rules: `design/rules.json`; how it works: "
             "`design/rules-checks.md`.", ""]
    lines += [f"- Run: {report['generated']}, mode **{mode['name']}**, checks selected: {mode['selected']}, jobs {mode['jobs']}"
              + (f", reusing {mode['reuse']}" if mode.get("reuse") else "") + (", **--allow-stale**" if mode.get("allow_stale") else ""),
              f"- Tree: commit `{tree['commit']}` on `{tree['branch']}`, {tree['dirty_files']} changed files, newest source {tree['newest_source']}",
              "- Binaries (" + tree["target_dir"] + "/debug), built against the newest source each was built from: " + "; ".join(
                  f"{name} built {info['built'] or 'missing'}, newest source {info.get('newest_source') or '?'} ({info.get('newest_source_file') or '?'})"
                  + (" **STALE: older than its source**" if info["older_than_source"] else "")
                  for name, info in tree["binaries"].items()), ""]
    totals = report["totals"]
    lines += ["## Counts", "",
              table(["rules"] + [LABEL[s] for s in STATUSES], [[totals["rules"]] + [totals[s] for s in STATUSES]]), ""]
    lines += ["A rule is `enforced+passing` only when every check that cites it ran, to the end, and found nothing; the Checks table says when "
              "each check ran (output taken from an earlier invocation with `--reuse` is marked). `not run` means a check exists but was not "
              "executed; `partly run` means a check did not finish its job or only some of the checks that cite the rule ran; `unenforced` "
              "means no check cites the rule.", ""]

    lines += ["## Checks", ""]
    rows = []
    for run in report["checks"]:
        c = run["counts"]
        notes = "; ".join(note[:120] + ("..." if len(note) > 120 else "") for note in run["notes"][:2])
        rows.append([f"`{run['id']}`", run["state"] + ("" if run.get("complete", True) else " (incomplete)"), run["exit"] if run["exit"] is not None else "",
                     run["seconds"], (run["ran_at"][:16].replace("T", " ") + (" (reused output)" if run["reused"] else "")) if run["ran_at"] else "",
                     c["fail"], c["allowed"], c["known-open"], c["unattributed"], run["scope"] or "", notes])
    lines += [table(["check", "state", "exit", "seconds", "ran", "failing", "allowed", "known open", "no rule ID", "scope", "notes"], rows), ""]

    lines += ["## Findings by kind", "",
              "Every finding counted by its rule key (and, for overflow-check, its scenario) with the rules the kind stands for. "
              "A kind with no rule is not dropped: see the next section.", ""]
    kinds = report["findings_by_kind"]
    rows = [[f"`{k['check']}`", f"`{k['key']}`", k["scenario"], ", ".join(k["ids"]) or ("none (unmapped)" if k["unmapped"] else "none"),
             k["fail"], k["known-open"], k["allowed"]] for k in kinds]
    lines += [table(["check", "kind (rule key)", "scenario", "rules", "failing", "known open", "allowed"], rows) if rows else "No findings.", ""]

    lines += ["## Findings that hold no rule of the book", "",
              "Layout kinds the inspector finds that no rule of `design/rules.json` states (declared `RULE_UNMAPPED` in the check). "
              "They fail the check like any finding and are counted here, not under a rule; no rule was invented for them.", ""]
    rows = [[f"`{u['check']}`", f"`{u['key']}`", u["fail"] if u["ran"] else "not run", u["allowed"], u["reason"]] for u in report["unmapped"]]
    lines += [table(["check", "kind", "failing", "allowed", "why no rule"], rows) if rows else "None declared.", ""]

    tested = [r for r in book_rows if r["tests"]]
    lines += ["## Rules held by unit tests", "",
              "From `tests/rules-tests.json`: the Rust test modules run in the `unit-tests` check, by rule. A rule is only as covered as the "
              "`covers` text says.", ""]
    rows = [[r["id"], r["priority"], status_cell(r), "; ".join(f"{t['module']} ({t['passed']} passed, {t['failed']} failed"
                                                          + (f", {t['ignored']} ignored" if t["ignored"] else "") + ")" for t in r["tests"])]
            for r in tested]
    lines += [table(["rule", "pri", "status (this run)", "modules (passed, failed)"], rows) if rows else "None.", ""]

    lines += ["## Sections", ""]
    rows = []
    for section in report["sections"]:
        s = section["status"]
        rows.append([section["code"], section["title"], section["rules"]] + [s[x] for x in STATUSES])
    lines += [table(["", "section", "rules"] + [LABEL[s] for s in STATUSES], rows), ""]

    failing = sorted([r for r in book_rows if r["status"] in (FAILING, KNOWN_OPEN)],
                     key=lambda r: (r["status"] != FAILING, -(r["counts"]["fail"] + r["counts"]["known-open"]), r["id"]))
    lines += ["## Top failing rules", ""]
    if not failing:
        lines += ["None in the checks that ran.", ""]
    for row in failing[:15]:
        lines.append(f"### {row['id']} ({row['priority']}, {row['repeats']} repeats): {status_cell(row)}")
        lines += ["", row["rule"], ""]
        for finding in row["first"]:
            lines.append(f"- `{finding['check']}:{finding['key']}` {finding['location']}: {finding['message']}")
        lines.append("")

    ranked = sorted([r for r in book_rows if r["status"] == UNENFORCED and r["priority"] in ("P0", "P1")],
                    key=lambda r: (-r["repeats"], r["priority"], r["id"]))
    lines += ["## Unenforced P0 and P1 rules, by repeats", ""]
    lines += [table(["rule", "priority", "repeats", "rule text", "declared in rules.json (hand-written)"],
                    [[r["id"], r["priority"], r["repeats"], r["rule"][:140], (r["declared"]["status"] + ": " + r["declared"]["enforced_by"])[:120]]
                     for r in ranked]), ""]

    problems = report["problems"]
    lines += ["## Registry integrity", ""]
    for title, key in (("Rule IDs a check cites that are not in rules.json", "unknown_ids"),
                       ("Rule IDs cited outside the checks that are not in rules.json", "stray_unknown_citations"),
                       ("Checks that cite no rule ID", "checks_without_ids"),
                       ("layout-lint rule keys that cite no rule ID", "layout_keys_without_ids"),
                       ("tests/rules-tests.json modules with no `#[test]` in the tree", "tests_unmatched"),
                       ("IDs a check's docstring mentions that none of its findings carry", "documented_only"),
                       ("Rules marked manual that a check cites", "manual_but_enforced"),
                       ("Rules in rules-manual.json that are not in rules.json", "unknown_manual"),
                       ("Declared coverage: `enforced_by` names a check that does not cite the rule", "named_not_cited"),
                       ("Declared coverage: a check cites the rule and `enforced_by` omits it", "cited_not_named"),
                       ("Declared coverage: `status` enforced or partial, no check cites the rule (unit test or hand note only)", "declared_enforced_no_check"),
                       ("Declared coverage: `status` none or design-only, a check cites the rule", "declared_none_but_cited")):
        items = problems[key]
        lines.append(f"- {title}: " + ("none" if not items else f"{len(items)}"))
        for item in items[:40]:
            lines.append(f"  - {item}")
        if len(items) > 40:
            lines.append(f"  - and {len(items) - 40} more (rules-report.json)")
    lines.append("")

    lines += ["## Rules by section", ""]
    for section in report["sections"]:
        lines += [f"### {section['code']} {section['title']}", ""]
        rows = []
        for row in book_rows:
            if row["section"] != section["code"]:
                continue
            cell = ", ".join(f"`{c['check']}`" + (f"[{','.join(c['keys'])}]" if c["keys"] else "") for c in row["checks"]) or ""
            if row["status"] == MANUAL:
                cell = "manual: " + row["manual"][:80]
            rows.append([row["id"], row["priority"], row["repeats"], status_cell(row), cell, row["declared"]["status"]])
        lines += [table(["rule", "pri", "repeats", "status (this run)", "checks (registry)", "declared"], rows), ""]

    lines += ["## Allowlist entries", "",
              "Every entry of an ALLOWED, KNOWN or Rust ALLOW list, with its reason. `matched` is how many findings of this run it "
              "covered (blank: applied inside the app and not counted).", ""]
    rows = [[e["check"], e["kind"], e["rule"], e["where"], e["reason"][:260], "" if e["matched"] is None else e["matched"], e["source"]]
            for e in report["allowlists"]]
    lines += [table(["check", "kind", "rule key", "where", "reason", "matched", "source"], rows), ""]

    lines += ["## Registry", "", "Which check holds which rule, read from the checks (see `design/rules-checks.md`).", ""]
    rows = [[f"`{r['check']}`", r["script"], ", ".join(r["ids"])] for r in report["registry"]]
    lines += [table(["check", "script", "rule IDs"], rows), ""]
    rows = []
    for r in report["registry"]:
        for key, ids in r["keys"].items():
            rows.append([f"`{r['check']}`", f"`{key}`", ", ".join(ids), r["sources"][key]])
    lines += [table(["check", "rule key", "rule IDs", "read from"], rows), ""]
    return "\n".join(lines).rstrip() + "\n"


def render_summary(report: dict) -> str:
    totals = report["totals"]
    tree = report["tree"]
    mode = report["mode"]
    checks = report["checks"]
    by_state: dict[str, list[str]] = defaultdict(list)
    for run in checks:
        by_state[run["state"]].append(run["id"])
    lines = [f"rules-check {report['generated'][:16]}  tree {tree['commit']} ({tree['dirty_files']} changed)  mode {mode['name']}  jobs {mode['jobs']}",
             "checks: " + ", ".join(f"{len(ids)} {state}" for state, ids in sorted(by_state.items())),
             ]
    for state in ("red", "errored"):
        if by_state.get(state):
            lines.append(f"  {state}: " + ", ".join(by_state[state]))
    if by_state.get("not-run"):
        lines.append("  not run: " + ", ".join(by_state["not-run"]))
    head = f"{'':3}{'section':34}{'rules':>5} {'pass':>5} {'fail':>5} {'known':>5} {'allow':>5} {'part':>5} {'norun':>5} {'err':>4} {'unenf':>5} {'man':>4}"
    lines.append(head)
    for section in report["sections"]:
        s = section["status"]
        lines.append(f"{section['code']:3}{section['title'][:33]:34}{section['rules']:>5} {s[PASSING]:>5} {s[FAILING]:>5} {s[KNOWN_OPEN]:>5} "
                     f"{s[ALLOWLISTED]:>5} {s[PARTLY_RUN]:>5} {s[NOT_RUN]:>5} {s[ERRORED]:>4} {s[UNENFORCED]:>5} {s[MANUAL]:>4}")
    lines.append(f"{'':3}{'TOTAL':34}{totals['rules']:>5} {totals[PASSING]:>5} {totals[FAILING]:>5} {totals[KNOWN_OPEN]:>5} "
                 f"{totals[ALLOWLISTED]:>5} {totals[PARTLY_RUN]:>5} {totals[NOT_RUN]:>5} {totals[ERRORED]:>4} {totals[UNENFORCED]:>5} {totals[MANUAL]:>4}")
    problems = report["problems"]
    lines.append(f"registry: {len(problems['unknown_ids'])} unknown IDs in checks, {len(problems['stray_unknown_citations'])} elsewhere, "
                 f"{len(problems['checks_without_ids'])} checks without IDs, {len(problems['layout_keys_without_ids'])} layout keys without IDs")
    lines.append(f"declared coverage (rules.json enforced_by): {len(problems['named_not_cited'])} name a check that does not cite the rule, "
                 f"{len(problems['cited_not_named'])} omit a check that cites it, {len(problems['declared_enforced_no_check'])} enforced or partial with no check")
    unenforced = [r for r in report["rules"] if r["status"] == UNENFORCED and r["priority"] == "P0"]
    lines.append(f"unenforced P0: {len(unenforced)}" + (" (" + ", ".join(r["id"] for r in unenforced[:12]) + (" ..." if len(unenforced) > 12 else "") + ")" if unenforced else ""))
    worst = sorted([r for r in report["rules"] if r["status"] in (FAILING, KNOWN_OPEN)], key=lambda r: -(r["counts"]["fail"] + r["counts"]["known-open"]))[:8]
    if worst:
        lines.append("failing: " + ", ".join(f"{r['id']} {r['counts']['fail'] + r['counts']['known-open']}" for r in worst))
    kinds: dict[str, int] = {}
    for kind in report["findings_by_kind"]:
        if kind["fail"]:
            kinds[f"{kind['check']}:{kind['key']}" + (f"/{kind['scenario']}" if kind["scenario"] else "")] = kind["fail"]
    if kinds:
        top = sorted(kinds.items(), key=lambda item: -item[1])[:8]
        lines.append("findings by kind: " + ", ".join(f"{name} {count}" for name, count in top) + (" ..." if len(kinds) > 8 else ""))
    held = [u for u in report["unmapped"] if u["ran"]]
    if held:
        lines.append("hold no rule of the book: " + ", ".join(f"{u['check']}:{u['key']} {u['fail']}" for u in held))
    stale = sorted({name for run in checks for name in run.get("stale", [])})
    if stale:
        lines.append("STALE binaries (checks that rendered them are partly run, not passing): " + ", ".join(stale))
    tested = [r for r in report["rules"] if r["tests"]]
    if tested:
        lines.append(f"rules held by unit tests: {len(tested)}, " + ", ".join(f"{status} {sum(r['status'] == status for r in tested)}"
                                                                         for status in (PASSING, FAILING, PARTLY_RUN, NOT_RUN, ERRORED) if any(r['status'] == status for r in tested)))
    return "\n".join(lines)


# ---- rules.json: declared coverage ------------------------------------------------------------------
# `enforced_by` in rules.json is what a person declared holds each rule. This script validates it against the
# registry and, only when asked (--sync-coverage), appends the registry's checks to it. It never writes
# `status` or any result of a run: those live in design/rules-report.md and .json.

SYNC_SEPARATOR = " || registry: "


def check_patterns() -> dict[str, "re.Pattern[str]"]:
    """For each check, a pattern for the ways rules.json names it (its id, `scripts/<id>.py`, an alias)."""
    out = {}
    for check in CHECKS:
        names = [re.escape(check.id), *check.aliases]
        out[check.id] = re.compile(r"(?<![\w-])(?:" + "|".join(names) + r")(?![\w-])")
    return out


def named_checks(text: str, patterns: dict[str, "re.Pattern[str]"]) -> set[str]:
    return {check_id for check_id, pattern in patterns.items() if pattern.search(text)}


def validate_coverage(book: RuleBook, registry: Registry) -> dict[str, list[str]]:
    """Declared coverage (status, enforced_by of rules.json) against what the checks cite."""
    cited = registry.rule_checks()
    patterns = {check_id: pattern for check_id, pattern in check_patterns().items() if check_id in registry.derived}
    out: dict[str, list[str]] = {"named_not_cited": [], "cited_not_named": [], "declared_enforced_no_check": [],
                                 "declared_none_but_cited": []}
    for rule_id in book.order:
        rule = book.rules[rule_id]
        text = rule.get("enforced_by", "").split(SYNC_SEPARATOR)[0]
        named = named_checks(text, patterns)
        cites = {check_id for check_id, _ in cited.get(rule_id, [])}
        out["named_not_cited"] += [f"{rule_id}: enforced_by names {c}, which does not cite it" for c in sorted(named - cites)]
        out["cited_not_named"] += [f"{rule_id}: {c} cites it, enforced_by omits it" for c in sorted(cites - named)]
        if rule.get("status") in ("enforced", "partial") and not cites:
            out["declared_enforced_no_check"].append(f"{rule_id} ({rule.get('status')}): {text[:100]}")
        if rule.get("status") in ("none", "design-only") and cites:
            out["declared_none_but_cited"].append(f"{rule_id} ({rule.get('status')}): cited by {', '.join(sorted(cites))}")
    return out


def registry_segment(entries: list[tuple[str, list[str]]]) -> str:
    return ", ".join(check_id + (f"[{'/'.join(keys)}]" if keys else "") for check_id, keys in entries)


def sync_coverage(data: dict, registry: Registry) -> tuple[dict, list[str]]:
    """`data` with the registry's checks appended to each rule's `enforced_by` (after SYNC_SEPARATOR, replacing an
    earlier registry segment). Nothing else is changed: not `status`, not a quote, not a field's order."""
    out = copy.deepcopy(data)
    cited = registry.rule_checks()
    changed: list[str] = []
    for rule in out["rules"]:
        old = rule["enforced_by"]
        base = "none" if old.startswith(SYNC_SEPARATOR.lstrip(" |")) else old.split(SYNC_SEPARATOR)[0]
        entries = cited.get(rule["id"], [])
        if not entries:
            new = base
        elif base == "none":
            new = SYNC_SEPARATOR.lstrip(" |") + registry_segment(entries)
        else:
            new = base + SYNC_SEPARATOR + registry_segment(entries)
        if new != old:
            rule["enforced_by"] = new
            changed.append(rule["id"])
    return out, changed


def dump_rules(data: dict) -> str:
    return json.dumps(data, indent=1, ensure_ascii=False) + "\n"


# ---- main -------------------------------------------------------------------------------------------


def write_atomic(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(path.name + ".tmp")
    temp.write_text(text)
    os.replace(temp, path)


def parse_names(value: str, option: str) -> list[str]:
    names = [name for name in value.split(",") if name]
    unknown = [name for name in names if name not in CHECK_BY_ID]
    if unknown:
        sys.exit(f"{option}: unknown check {', '.join(unknown)} (see --registry for the list)")
    return names


def print_registry(registry: Registry) -> None:
    for entry in registry_table(registry):
        print(f"{entry['check']:20} {entry['script']}" + ("  [offline]" if entry["offline"] else ""))
        for key, ids in entry["keys"].items():
            print(f"    {key:28} {', '.join(ids):32} {entry['sources'][key]}")
        loose = sorted(set(entry["ids"]) - {i for ids in entry["keys"].values() for i in ids})
        if loose:
            print(f"    {'(cited without a key)':28} {', '.join(loose)}")
    print()
    for kind, lines in registry.problems.items():
        print(f"{kind}: {len(lines)}")
        for line in lines:
            print(f"    {line}")
    print(f"layout keys without IDs: {', '.join(layout_keys_without_ids(registry)) or 'none'}")
    print(f"unknown IDs cited elsewhere: {len(stray_citations(registry.book))}")
    for line in stray_citations(registry.book)[:40]:
        print(f"    {line}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--only", default="", help="comma-separated check ids to run")
    parser.add_argument("--skip", default="", help="comma-separated check ids to leave out")
    parser.add_argument("--offline", action="store_true", help="run only the checks that need no app (copy-lint on its source strings)")
    parser.add_argument("--reuse", type=Path, help="a DIR of an earlier run (its raw/ folder): use a check's output from there when it has one")
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT / "latest", help="where this run keeps its raw output and, for a partial run, the report")
    parser.add_argument("--jobs", type=int, default=2, help="captures a check renders at once")
    parser.add_argument("--build", action="store_true", help="let the checks build the binaries first")
    parser.add_argument("--strict", action="store_true",
                        help="also fail on known open defects, unenforced P0 rules and declared coverage that disagrees with the registry")
    parser.add_argument("--registry", action="store_true", help="print the registry and its integrity problems, then exit")
    parser.add_argument("--sync-coverage", action="store_true", help="append the registry's checks to `enforced_by` in design/rules.json (that field only; off by default), then exit")
    parser.add_argument("--write-report", action="store_true", help="write design/rules-report.* even for a partial or offline run")
    parser.add_argument("--no-report", action="store_true", help="write no report files")
    parser.add_argument("--allow-stale", action="store_true",
                        help="report a check that rendered a binary older than the source it was built from as if the binary were current "
                             "(default: its rules are `partly run`, never passing, and the exit status is 1)")
    parser.add_argument("--arg", action="append", default=[], metavar='CHECK="OPTIONS"',
                        help='options appended to one check\'s command, e.g. --arg state-check="--timeout 1800" (repeatable)')
    args = parser.parse_args(argv)

    book = RuleBook.load()
    manual = load_manual()
    registry = build_registry(book)
    if args.registry:
        print_registry(registry)
        return 1 if registry.problems["unknown"] else 0
    if args.sync_coverage:
        if dump_rules(book.data) != RULES_JSON.read_text():
            print("design/rules.json is not in the format json.dumps(indent=1) writes: refusing to rewrite it", file=sys.stderr)
            return 2
        synced, changed = sync_coverage(book.data, registry)
        if changed:
            write_atomic(RULES_JSON, dump_rules(synced))
        print(f"design/rules.json: enforced_by of {len(changed)} rules updated from the registry; status and every other field untouched")
        return 0

    only = parse_names(args.only, "--only")
    skip = parse_names(args.skip, "--skip")
    selected: list[Check] = []
    skipped: dict[str, str] = {}
    for check in CHECKS:
        if only and check.id not in only:
            skipped[check.id] = "not selected (--only)"
        elif check.id in skip:
            skipped[check.id] = "skipped (--skip)"
        elif args.offline and check.offline_args is None:
            skipped[check.id] = "builds the apps or the tests (--offline)" if check.kind == "unit-tests" else "needs the apps (--offline)"
        else:
            selected.append(check)
    partial = bool(only or skip or args.offline)
    extra: dict[str, list[str]] = {}
    for item in args.arg:
        name, _, options = item.partition("=")
        parse_names(name, "--arg")
        extra.setdefault(name, []).extend(shlex.split(options))
    ctx = Context(out=args.out, jobs=max(1, args.jobs), build=args.build, offline=args.offline, reuse=args.reuse, extra=extra,
                  allow_stale=args.allow_stale)
    mode = {"name": "offline" if args.offline else "partial" if partial else "full", "selected": len(selected), "jobs": ctx.jobs,
            "reuse": str(args.reuse) if args.reuse else "", "allow_stale": args.allow_stale}
    runs = run_checks(selected, skipped, ctx, registry)
    allowlists = [entry for check in CHECKS for entry in python_allowlists(check, registry) + rust_allowlists(check)]
    report = build_report(book, registry, runs, manual, mode, tree_state(), allowlists)
    print()
    print(render_summary(report))
    if not args.no_report:
        if partial and not args.write_report:
            md, js = args.out / "rules-report.md", args.out / "rules-report.json"
        else:
            md, js = REPORT_MD, REPORT_JSON
        write_atomic(js, json.dumps(report, indent=1, ensure_ascii=False) + "\n")
        write_atomic(md, render_markdown(report))
        print(f"report: {md}, {js}")
    return exit_code_of(report, args.strict)


if __name__ == "__main__":
    sys.exit(main())
