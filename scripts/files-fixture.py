#!/usr/bin/env python3
"""A HOME for the Files app's scroll and tooltip checks: Big/ with 20000 files.

    python3 scripts/files-fixture.py [ROOT [COUNT]]

ROOT defaults to /var/tmp/FileTips-home (what scripts/frame-time.py --app files
opens) and COUNT to 20000. Every tenth file is a small PNG (a few distinct
gradients, hardlinked, so the folder costs next to nothing on disk and the
thumbnails have pictures to draw); the rest are .txt files of 0 to 96 bytes with
dates spread over three years. Runs again over an existing folder without
touching what is there.
"""
import os
import struct
import sys
import zlib
from pathlib import Path

root = Path(sys.argv[1] if len(sys.argv) > 1 else "/var/tmp/FileTips-home")
count = int(sys.argv[2]) if len(sys.argv) > 2 else 20000


def png(width: int, height: int, tint: int) -> bytes:
    """A gradient, as a PNG, with no imaging library."""
    rows = b""
    for y in range(height):
        row = b"".join(
            bytes(((x * 255 // width + tint * 40) % 256, y * 255 // height, (tint * 90) % 256))
            for x in range(width)
        )
        rows += b"\x00" + row

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


for folder in ("Desktop", "Documents", "Downloads", "Pictures", "Big"):
    (root / folder).mkdir(parents=True, exist_ok=True)
pictures = []
for tint in range(8):
    path = root / "Pictures" / f"gradient-{tint}.png"
    if not path.exists():
        path.write_bytes(png(240, 160, tint))
    pictures.append(path)
THREE_YEARS = 3 * 365 * 24 * 3600
now = 1_790_000_000
for index in range(count):
    base = root / "Big" / f"file-{index:05d}"
    if index % 10 == 0:
        path = base.with_suffix(".png")
        if not path.exists():
            os.link(pictures[index // 10 % len(pictures)], path)
    else:
        path = base.with_suffix(".txt")
        if not path.exists():
            path.write_text("x" * (index % 97))
            stamp = now - THREE_YEARS * (count - index) // count
            os.utime(path, (stamp, stamp))
print(f"{root}/Big: {count} files")
