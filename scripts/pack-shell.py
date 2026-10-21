#!/usr/bin/env python3
"""Pack shell/bingux into the bingux-shell archive.

usage: pack-shell.py [--source shell/bingux] [--version 0.1.0] OUTPUT

The archive is a tar.xz with manifest.json at its root and the QML tree,
Python helpers and assets under shell/. bingux-shell unpacks it once per
content into the user's cache and runs the manifest entry for its mode.
The tar is deterministic (sorted names, fixed owner and mtime), so the same
tree always packs to the same bytes and the cache key stays stable.
"""

import argparse
import io
import json
import lzma
from pathlib import Path
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


def members(source):
    for path in sorted(source.rglob("*")):
        relative = path.relative_to(source)
        if any(part in SKIP_DIRS for part in relative.parts) or path.suffix in SKIP_SUFFIXES:
            continue
        if path.is_symlink():
            raise SystemExit(f"pack-shell: symlinks are not packed: {relative}")
        yield path, relative


def add(tar, name, data, mode, mtime):
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = mode
    info.mtime = mtime
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    tar.addfile(info, io.BytesIO(data))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", default=str(Path(__file__).resolve().parents[1] / "shell/bingux"))
    parser.add_argument("--version", default="0.1.0")
    parser.add_argument("output")
    arguments = parser.parse_args()
    source = Path(arguments.source).resolve()
    for name, entry in ENTRIES.items():
        if not (source / entry).is_file():
            raise SystemExit(f"pack-shell: {source} has no {entry} for mode {name}")
    manifest = {"format": "bingux-shell", "version": arguments.version, "entries": ENTRIES}
    mtime = 0
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.PAX_FORMAT) as tar:
        add(tar, "manifest.json", (json.dumps(manifest, indent=2) + "\n").encode(), 0o644, mtime)
        for path, relative in members(source):
            name = "shell/" + relative.as_posix()
            if path.is_dir():
                info = tarfile.TarInfo(name)
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                info.mtime = mtime
                info.uname = info.gname = "root"
                tar.addfile(info)
                continue
            executable = bool(path.stat().st_mode & stat.S_IXUSR)
            add(tar, name, path.read_bytes(), 0o755 if executable else 0o644, mtime)
    output = Path(arguments.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(output.suffix + ".tmp")
    temporary.write_bytes(lzma.compress(buffer.getvalue(), preset=6 | lzma.PRESET_EXTREME))
    temporary.replace(output)
    print(f"[pack-shell] {output} ({output.stat().st_size // 1024} KiB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
