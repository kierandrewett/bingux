#!/usr/bin/env python3
"""Check that every QML directory has a complete, accurate qmldir.

For each tree (default: shell/bingux; a tree relative to the repo, or an absolute
directory such as the merged tree from scripts/pack-shell.py --tree-out, with
--entry for any extra entry file that tree's archive manifest names) and each
directory under it that contains .qml files:

- the directory has a qmldir;
- every .qml file is listed, unless it is a scripts/pack-shell.py ENTRIES entry
  point or is in ALLOW_UNLISTED with a reason;
- a qmldir line is `singleton` exactly when the file has `pragma Singleton`;
- every qmldir line names an existing file, listed once, whose type name is the
  file stem.

`--committed` checks the HEAD tree (git archive into a temp dir), so uncommitted
edits in the working tree do not decide the result; it accepts repo trees only.
`--self-test` checks a copy of shell/bingux with three deliberate breaks.
"""

import argparse
import ast
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DEFAULT_TREES = ["shell/bingux"]
SKIP_DIRS = {"__pycache__", ".ruff_cache", "target", "node_modules"}

# .qml files that are deliberately absent from qmldir. Each needs a reason.
ALLOW_UNLISTED = {
    "BackgroundEffectNative.qml": "loaded by file name via Qt.createComponent in BackgroundEffects.qml",
    "SurfaceFadeNative.qml": "loaded by file name via Qt.createComponent in SurfaceFades.qml",
    "DockAppMenu.qml": "loaded by file name via Qt.createComponent in Dock.qml",
    "SidebarTerminal.qml": "loaded by file name via source: in TerminalSidebar.qml",
    "PreviewMedia.qml": "loaded via Qt.resolvedUrl in SearchPreview.qml",
    "CaptureHost.qml": "private host instantiated by name in CaptureShell.qml and PopoutsShell.qml",
    "EmojiHost.qml": "private host instantiated by name in EmojiShell.qml and PopoutsShell.qml",
    "SearchHost.qml": "private host instantiated by name in SearchShell.qml and PopoutsShell.qml",
    "SwitcherHost.qml": "private host instantiated by name in SwitcherShell.qml and PopoutsShell.qml",
    "ShortcutHost.qml": "private host instantiated by name in PopoutsShell.qml",
}

# Unlisted files the shell already instantiates by name. These are real load breaks, reported as
# WARNING on every run without failing the check. Drop the entry when qmldir lists the file.
KNOWN_BREAKS = {}
WARNING = "WARNING: "

LINE = re.compile(r"^(singleton\s+)?(\w+)\s+(\d+\.\d+)\s+(\S+\.qml)$")


def entry_files(root):
    """ENTRIES values from scripts/pack-shell.py under root: entry points, not qmldir types."""
    for node in ast.parse((root / "scripts/pack-shell.py").read_text()).body:
        if isinstance(node, ast.Assign) and any(getattr(t, "id", None) == "ENTRIES" for t in node.targets):
            return set(ast.literal_eval(node.value).values())
    raise SystemExit("qmldir-complete: scripts/pack-shell.py has no ENTRIES dict")


def qml_directories(tree):
    dirs = {p.parent for p in tree.rglob("*.qml") if not set(p.relative_to(tree).parts[:-1]) & SKIP_DIRS}
    return sorted(dirs)


def check_directory(root, directory, entries):
    """Problems in one QML directory; root is only used to name the directory."""
    rel = directory.relative_to(root)
    qmldir = directory / "qmldir"
    if not qmldir.is_file():
        return [f"{rel}: has .qml files but no qmldir"]
    problems = []
    files = {p.name for p in directory.glob("*.qml")}
    listed = set()
    for number, raw in enumerate(qmldir.read_text().splitlines(), 1):
        if not raw.strip():
            continue
        where = f"{rel}/qmldir:{number}"
        match = LINE.match(raw.strip())
        if not match:
            problems.append(f"{where}: not a 'type version file' line: {raw.strip()!r}")
            continue
        singleton, name, _version, filename = match.groups()
        if filename in listed:
            problems.append(f"{where}: {filename} listed twice")
        listed.add(filename)
        if filename not in files:
            problems.append(f"{where}: {filename} does not exist")
            continue
        if name != filename[: -len(".qml")]:
            problems.append(f"{where}: type {name} does not match file {filename}")
        is_singleton_file = re.search(r"^pragma Singleton\b", (directory / filename).read_text(), re.M) is not None
        if is_singleton_file != bool(singleton):
            wanted = "singleton" if is_singleton_file else "non-singleton"
            problems.append(f"{where}: {filename} is {wanted} (pragma Singleton) but the qmldir line disagrees")
    for filename in sorted(files - listed):
        if filename in entries or filename in ALLOW_UNLISTED:
            continue
        if filename in KNOWN_BREAKS:
            problems.append(f"{WARNING}{rel}/{filename}: {KNOWN_BREAKS[filename]}")
            continue
        problems.append(
            f"{rel}/{filename}: not in qmldir, not a pack-shell entry, not in ALLOW_UNLISTED or KNOWN_BREAKS"
        )
    for filename in sorted((set(ALLOW_UNLISTED) | set(KNOWN_BREAKS)) & listed):
        problems.append(f"{rel}/{filename}: listed in qmldir but also in an allow/known-break dict; drop the entry")
    return problems


def check_trees(root, trees, entries):
    problems = []
    for tree in trees:
        target = root / tree
        if not target.is_dir():
            problems.append(f"{tree}: not a directory")
            continue
        for directory in qml_directories(target):
            problems.extend(check_directory(root, directory, entries))
    return problems


def targets(arguments):
    """(root, tree) pairs: repo-relative trees, or absolute directories checked as their own root."""
    found = []
    for argument in arguments or DEFAULT_TREES:
        path = Path(argument)
        if not path.is_absolute():
            found.append((REPO, argument))
            continue
        try:
            found.append((REPO, str(path.relative_to(REPO))))
        except ValueError:
            found.append((path, "."))
    return found


def committed_copy(trees):
    scratch = Path(tempfile.mkdtemp(prefix="qmldir-complete-"))
    archive = subprocess.run(
        ["git", "-C", str(REPO), "archive", "HEAD", "--", "scripts/pack-shell.py", *trees],
        check=True,
        stdout=subprocess.PIPE,
    ).stdout
    subprocess.run(["tar", "-x", "-C", str(scratch)], input=archive, check=True)
    return scratch


def failures(problems):
    """Problems that fail the check; WARNING entries are reported but do not."""
    return [p for p in problems if not p.startswith(WARNING)]


def self_test():
    """A pristine copy passes; each deliberate break is reported."""
    scratch = Path(tempfile.mkdtemp(prefix="qmldir-selftest-"))
    try:
        (scratch / "scripts").mkdir()
        shutil.copy2(REPO / "scripts/pack-shell.py", scratch / "scripts/pack-shell.py")
        shutil.copytree(REPO / "shell/bingux", scratch / "shell/bingux")
        trees = ["shell/bingux"]
        entries = entry_files(scratch)
        baseline = failures(check_trees(scratch, trees, entries))
        if baseline:
            raise SystemExit(f"qmldir-complete self-test: pristine copy fails: {baseline[0]}")
        qmldir = scratch / "shell/bingux/qmldir"
        original = qmldir.read_text()
        breaks = {
            "unlisted type": original.replace("singleton Theme 1.0 Theme.qml\n", "", 1),
            "dangling entry": original + "Ghost 1.0 Ghost.qml\n",
            "singleton mismatch": original.replace("singleton Theme 1.0 Theme.qml", "Theme 1.0 Theme.qml", 1),
        }
        for label, text in breaks.items():
            qmldir.write_text(text)
            found = failures(check_trees(scratch, trees, entries))
            if not found:
                raise SystemExit(f"qmldir-complete self-test: '{label}' was not detected")
            print(f"self-test detected {label}: {found[0]}")
            qmldir.write_text(original)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    print("qmldir-complete self-test passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("trees", nargs="*", help="QML trees to check (default: shell/bingux)")
    parser.add_argument("--entry", action="append", default=[], help="extra entry file name, e.g. InstallerShell.qml")
    parser.add_argument("--committed", action="store_true", help="check the HEAD tree, not the working tree")
    parser.add_argument("--self-test", action="store_true", help="prove the checks fail on deliberate breaks")
    arguments = parser.parse_args()
    if arguments.self_test:
        self_test()
        return
    entries = entry_files(REPO) | set(arguments.entry)
    pairs = targets(arguments.trees)
    if arguments.committed:
        if any(root != REPO for root, _ in pairs):
            raise SystemExit("qmldir-complete: --committed takes repo trees only")
        trees = [tree for _, tree in pairs]
        scratch = committed_copy(trees)
        try:
            problems = check_trees(scratch, trees, entries)
        finally:
            shutil.rmtree(scratch, ignore_errors=True)
        label = "HEAD"
    else:
        problems = []
        for root, tree in pairs:
            problems.extend(check_trees(root, [tree], entries))
        label = "working tree"
    trees = [tree for _, tree in pairs]
    for problem in problems:
        print(problem, file=sys.stderr)
    hard = failures(problems)
    if hard:
        raise SystemExit(f"qmldir-complete: {len(hard)} problem(s) in {label}")
    known = len(problems) - len(hard)
    suffix = f" ({known} known break(s) reported above)" if known else ""
    print(f"qmldir-complete: {', '.join(trees)} complete in {label}{suffix}")


if __name__ == "__main__":
    main()
