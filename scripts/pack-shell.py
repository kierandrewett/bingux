#!/usr/bin/env python3
"""Pack shell/bingux into the bingux-shell archive.

usage: pack-shell.py [--source shell/bingux] [--version 0.1.0]
                     [--overlay DIR] [--entries JSON] [--tree-out DIR] OUTPUT

The archive is a tar.xz with manifest.json at its root and the QML tree,
Python helpers and assets under shell/. bingux-shell unpacks it once per
content into the user's cache and runs the manifest entry for its mode.
The tar is deterministic (sorted names, fixed owner and mtime), so the same
tree always packs to the same bytes and the cache key stays stable.

--overlay DIR lays another QML tree over --source as one tree, so overlay
files use the base types and share its singletons (one Theme). A file that
exists in both trees is an error. An overlay qmldir.fragment is appended to
the qmldir of the same directory; its types must not repeat a base type or
file. --entries JSON adds manifest entries (mode name to file name) next to
the built-in ENTRIES. --tree-out DIR writes the merged tree unpacked, for
development and tests; the directory must be absent, empty, or one this
script wrote before.
"""

import argparse
import io
import json
import lzma
from pathlib import Path
import shutil
import stat
import sys
import tarfile

ENTRIES = {
    "desktop": "shell.qml",
    "lock": "LockShell.qml",
    "greeter": "GreeterShell.qml",
    "popouts": "PopoutsShell.qml",
    "wallpaper": "WallpaperShell.qml",
    "search": "SearchShell.qml",
    "switcher": "SwitcherShell.qml",
    "emoji": "EmojiShell.qml",
    "capture": "CaptureShell.qml",
}
SKIP_DIRS = {"__pycache__", ".ruff_cache", ".qmlls.ini"}
# Python helpers are installed under libexec/bingux/shell and reached through
# Python.helper(); the archive carries QML, JavaScript and assets only.
SKIP_SUFFIXES = {".py", ".pyc", ".orig", ".rej", ".swp", ".md"}
QMLDIR = "qmldir"
FRAGMENT = "qmldir.fragment"
TREE_MARKER = ".pack-shell-tree"


class PackError(SystemExit):
    def __init__(self, message):
        super().__init__(f"pack-shell: {message}")


def scan(root):
    """(relative Path, absolute Path) for every packed entry under root, sorted."""
    found = []
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if any(part in SKIP_DIRS for part in relative.parts) or path.suffix in SKIP_SUFFIXES:
            continue
        if path.is_symlink():
            raise PackError(f"symlinks are not packed: {relative}")
        found.append((relative, path))
    return sorted(found, key=lambda item: item[0])


class Tree:
    """The merged tree: files map relative Path to (bytes, mode); dirs is a set of Paths."""

    def __init__(self):
        self.files = {}
        self.dirs = set()

    def add_base(self, source):
        for relative, path in scan(source):
            if path.is_dir():
                self.dirs.add(relative)
            else:
                self.files[relative] = read_file(path)

    def add_overlay(self, overlay):
        for relative, path in scan(overlay):
            if relative.name == QMLDIR:
                raise PackError(f"overlay {relative} would replace the base qmldir; use {FRAGMENT}")
            if path.is_dir():
                if relative in self.files:
                    raise PackError(f"overlay directory {relative} collides with a base file")
                self.dirs.add(relative)
                continue
            if relative in self.files or relative in self.dirs:
                raise PackError(f"overlay file {relative} collides with the base tree")
            if relative.name == FRAGMENT:
                continue
            self.files[relative] = read_file(path)
        for relative, path in scan(overlay):
            if relative.name == FRAGMENT and not path.is_dir():
                self.merge_fragment(relative.parent, path.read_text())

    def merge_fragment(self, directory, text):
        qmldir = directory / QMLDIR
        if qmldir not in self.files:
            raise PackError(f"{FRAGMENT} in {directory or '.'} has no base qmldir to merge into")
        base, mode = self.files[qmldir]
        base_text = base.decode()
        if base_text and not base_text.endswith("\n"):
            base_text += "\n"
        names = qmldir_names(base_text)
        added = []
        for number, raw in enumerate(text.splitlines(), 1):
            line = raw.strip()
            if not line:
                continue
            tokens = line.split()
            if len(tokens) not in (3, 4) or not tokens[-1].endswith(".qml"):
                raise PackError(f"{directory / FRAGMENT}:{number}: not a qmldir line: {line!r}")
            if tokens[-3] in names or tokens[-1] in names:
                raise PackError(f"{directory / FRAGMENT}:{number}: {line!r} repeats a type or file of the base qmldir")
            names.update({tokens[-3], tokens[-1]})
            if not (directory / tokens[-1]) in self.files:
                raise PackError(f"{directory / FRAGMENT}:{number}: {tokens[-1]} is not in the tree")
            added.append(line)
        self.files[qmldir] = ((base_text + "".join(line + "\n" for line in added)).encode(), mode)

    def check(self, entries):
        for mode, entry in entries.items():
            if Path(entry) not in self.files:
                raise PackError(f"tree has no {entry} for mode {mode}")
        for relative in self.files:
            if relative.suffix == ".qml" and (relative.parent / QMLDIR) not in self.files:
                raise PackError(f"{relative.parent or '.'} has .qml files but no qmldir")

    def ordered(self):
        return sorted(set(self.dirs) | set(self.files), key=lambda item: item)


def read_file(path):
    return (path.read_bytes(), 0o755 if path.stat().st_mode & stat.S_IXUSR else 0o644)


def qmldir_names(text):
    names = set()
    for line in text.splitlines():
        tokens = line.split()
        if len(tokens) in (3, 4) and tokens[-1].endswith(".qml"):
            names.update({tokens[-3], tokens[-1]})
    return names


def add(tar, name, data, mode, mtime):
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = mode
    info.mtime = mtime
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    tar.addfile(info, io.BytesIO(data))


def parse_entries(text):
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise PackError(f"--entries is not JSON: {error}")
    if not isinstance(value, dict) or not all(isinstance(k, str) and isinstance(v, str) for k, v in value.items()):
        raise PackError("--entries must be a JSON object of mode name to file name")
    return value


def write_tree_out(directory, tree):
    if directory.exists():
        if (directory / TREE_MARKER).is_file():
            shutil.rmtree(directory)
        elif any(directory.iterdir()):
            raise PackError(f"--tree-out {directory} is not empty and was not written by pack-shell")
    directory.mkdir(parents=True, exist_ok=True)
    (directory / TREE_MARKER).write_text("written by scripts/pack-shell.py\n")
    for relative in tree.ordered():
        target = directory / relative
        if relative in tree.dirs and relative not in tree.files:
            target.mkdir(parents=True, exist_ok=True)
            continue
        data, mode = tree.files[relative]
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        target.chmod(mode)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", default=str(Path(__file__).resolve().parents[1] / "shell/bingux"))
    parser.add_argument("--version", default="0.1.0")
    parser.add_argument("--overlay", action="append", default=[], help="QML tree laid over --source")
    parser.add_argument("--entries", default="{}", help="extra manifest entries as a JSON object")
    parser.add_argument("--tree-out", help="also write the merged tree unpacked to this directory")
    parser.add_argument("output")
    arguments = parser.parse_args()

    source = Path(arguments.source).resolve()
    entries = dict(ENTRIES)
    for mode, entry in parse_entries(arguments.entries).items():
        if mode in entries:
            raise PackError(f"--entries repeats built-in mode {mode}")
        entries[mode] = entry

    tree = Tree()
    tree.add_base(source)
    for overlay in arguments.overlay:
        tree.add_overlay(Path(overlay).resolve())
    tree.check(entries)

    manifest = {"format": "bingux-shell", "version": arguments.version, "entries": entries}
    mtime = 0
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.PAX_FORMAT) as tar:
        add(tar, "manifest.json", (json.dumps(manifest, indent=2) + "\n").encode(), 0o644, mtime)
        for relative in tree.ordered():
            name = "shell/" + relative.as_posix()
            if relative in tree.dirs and relative not in tree.files:
                info = tarfile.TarInfo(name)
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                info.mtime = mtime
                info.uname = info.gname = "root"
                tar.addfile(info)
                continue
            data, mode = tree.files[relative]
            add(tar, name, data, mode, mtime)

    output = Path(arguments.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(output.suffix + ".tmp")
    temporary.write_bytes(lzma.compress(buffer.getvalue(), preset=6 | lzma.PRESET_EXTREME))
    temporary.replace(output)
    print(f"[pack-shell] {output} ({output.stat().st_size // 1024} KiB)")
    if arguments.tree_out:
        write_tree_out(Path(arguments.tree_out).resolve(), tree)
        print(f"[pack-shell] tree {arguments.tree_out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
