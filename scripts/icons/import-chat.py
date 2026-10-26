#!/usr/bin/env python3
"""Vendor the ~/dev/chat icon fork's geometry that the icon sources use.

Sources in scripts/icons/src/ pull fork icons with `<fork name="Power" weight="fill"/>`.
This script reads those names, finds `src/generated/<Name>.ts` in the chat icon package
(`@chat/icons`, a rounded fork of Phosphor with continuous corners; its custom icons such
as ChatCompose live in the same folder) and writes fork/geometry.json: the 256 unit
outlines of the regular, fill and bold weights of exactly those icons, so that
build.py needs no checkout of ~/dev/chat. Output is deterministic.

Usage:
  import-chat.py [--chat ~/dev/chat]          rewrite fork/geometry.json
  import-chat.py --check [--chat ~/dev/chat]  fail if it differs from the chat package
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SRC = HERE / "src"
OUT = HERE / "fork" / "geometry.json"
FORK = re.compile(r"""<fork\b[^>]*\bname\s*=\s*["']([A-Za-z0-9]+)["']""")
WEIGHTS = ("regular", "fill", "bold")


def used_names(src_dir: Path) -> list[str]:
    names: set[str] = set()
    for src in sorted(src_dir.glob("*.svg")):
        names.update(FORK.findall(src.read_text()))
    return sorted(names)


def read_icon(generated: Path, name: str) -> dict[str, list[str]]:
    path = generated / f"{name}.ts"
    if not path.is_file():
        raise SystemExit(f"import-chat: {name}: {path} does not exist")
    text = path.read_text()
    icon: dict[str, list[str]] = {}
    for weight in WEIGHTS:
        match = re.search(weight + r":\s*\[(.*?)\],", text, re.S)
        if match:
            icon[weight] = re.findall(r'"([^"]+)"', match.group(1))
    if "regular" not in icon:
        raise SystemExit(f"import-chat: {name}: no regular weight in {path}")
    return icon


def build(chat: Path, src_dir: Path) -> str:
    package = chat / "packages/icons"
    generated = package / "src/generated"
    if not generated.is_dir():
        raise SystemExit(f"import-chat: {generated} not found; pass --chat")
    header = (generated / "Power.ts").read_text().splitlines()[0]
    upstream = re.search(r"from (@phosphor-icons/core [0-9.]+)", header)
    try:
        commit = subprocess.run(
            ["git", "-C", str(chat), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        commit = "unknown"
    data = {
        "source": "@chat/icons (packages/icons in the chat repository), a rounded fork of Phosphor Icons (MIT)",
        "upstream": upstream.group(1) if upstream else "unknown",
        "chat-commit": commit,
        "grid": 256,
        "icons": {name: read_icon(generated, name) for name in used_names(src_dir)},
    }
    return json.dumps(data, indent=1, sort_keys=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--chat", type=Path, default=Path.home() / "dev/chat")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--src", type=Path, default=SRC, help="icon sources to scan (default scripts/icons/src)")
    parser.add_argument("--output", type=Path, default=OUT, help="geometry file to write")
    args = parser.parse_args()
    OUT_FILE = args.output
    text = build(args.chat, args.src)
    if args.check:
        if not OUT_FILE.exists() or OUT_FILE.read_text() != text:
            print("import-chat: fork/geometry.json is stale; run scripts/icons/import-chat.py", file=sys.stderr)
            return 1
        print("import-chat: ok")
        return 0
    OUT_FILE.parent.mkdir(parents=True, exist_ok=True)
    OUT_FILE.write_text(text)
    print(f"import-chat: wrote {len(json.loads(text)['icons'])} icons to {OUT_FILE}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
