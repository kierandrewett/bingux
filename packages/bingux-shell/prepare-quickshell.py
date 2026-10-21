#!/usr/bin/env python3
"""Copy the pinned Quickshell submodule into the build tree and patch it.

usage: prepare-quickshell.py SUBMODULE PATCH_DIR OUTPUT

The submodule stays pristine so `git submodule status` keeps meaning
something; the patches from packages/bingux-quickshell are applied to the
copy. A stamp file records the inputs, so an unchanged tree is not copied
again on every configure.
"""

import hashlib
from pathlib import Path
import shutil
import subprocess
import sys


def checkout_identity(submodule):
    """Bytes that change when the submodule checkout changes.

    In a git checkout that is HEAD (and the ref it points at). A source
    archive has no .git, or a pointer file whose target was not shipped; then
    the names, sizes and mtimes of the tree stand in.
    """
    head = submodule / ".git"
    if head.is_file():
        gitdir = Path(head.read_text().split(":", 1)[1].strip())
        if not gitdir.is_absolute():
            gitdir = submodule / gitdir
        if (gitdir / "HEAD").is_file():
            identity = (gitdir / "HEAD").read_bytes()
            ref = identity.decode().strip()
            if ref.startswith("ref: ") and (gitdir / ref[5:]).is_file():
                identity += (gitdir / ref[5:]).read_bytes()
            return identity
    lines = []
    for path in sorted(submodule.rglob("*")):
        if path.is_file() and ".git" not in path.parts:
            info = path.stat()
            lines.append(f"{path.relative_to(submodule)}\0{info.st_size}\0{info.st_mtime_ns}\n")
    return "".join(lines).encode()


def digest(submodule, patches):
    hasher = hashlib.sha256()
    hasher.update(checkout_identity(submodule))
    for patch in patches:
        hasher.update(patch.name.encode())
        hasher.update(patch.read_bytes())
    return hasher.hexdigest()


def main():
    submodule, patch_dir, output = (Path(p) for p in sys.argv[1:4])
    if not (submodule / "CMakeLists.txt").is_file():
        print(f"prepare-quickshell: {submodule} is empty; run git submodule update --init", file=sys.stderr)
        return 1
    patches = sorted(patch_dir.glob("*.patch"))
    stamp = output.parent / (output.name + ".stamp")
    wanted = digest(submodule, patches)
    if stamp.is_file() and stamp.read_text().strip() == wanted and (output / "CMakeLists.txt").is_file():
        return 0
    if output.exists():
        shutil.rmtree(output)
    shutil.copytree(submodule, output, ignore=shutil.ignore_patterns(".git"), symlinks=True)
    for patch in patches:
        subprocess.run(["patch", "-p1", "-s", "-i", str(patch)], cwd=output, check=True)
        print(f"[bingux-shell] applied {patch.name}")
    stamp.write_text(wanted + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
