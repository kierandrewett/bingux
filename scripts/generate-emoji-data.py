#!/usr/bin/env python3
"""Generate the bundled picker catalogue from Unicode's emoji-test.txt (17.0).

Input may be gzipped. Generation is offline and preserves fully-qualified ZWJ,
variation-selector and skin-tone sequences. Data is under Unicode License v3:
https://www.unicode.org/license.txt
"""

import gzip
import json
from pathlib import Path
import re
import sys


def catalogue(text):
    group, subgroup, result = "", "", []
    for line in text.splitlines():
        if line.startswith("# group: "):
            group = line.removeprefix("# group: ")
        elif line.startswith("# subgroup: "):
            subgroup = line.removeprefix("# subgroup: ")
        elif "; fully-qualified" in line:
            codes = line.split(";")[0].strip()
            match = re.search(r"#\s+\S+\s+E[\d.]+\s+(.+)$", line)
            if match:
                result.append(
                    {
                        "emoji": "".join(chr(int(code, 16)) for code in codes.split()),
                        "name": match[1],
                        "group": group,
                        "keywords": subgroup.replace("-", " "),
                    }
                )
    return result


if __name__ == "__main__":
    source, destination = map(Path, sys.argv[1:])
    raw = gzip.decompress(source.read_bytes()) if source.suffix == ".gz" else source.read_bytes()
    rows = catalogue(raw.decode("utf-8"))
    if len(rows) < 3000:
        raise ValueError("Incomplete Unicode emoji catalogue")
    destination.write_text(
        json.dumps(
            {"version": "17.0", "license": "Unicode-3.0", "emoji": rows}, ensure_ascii=False, separators=(",", ":")
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"Generated {len(rows)} emoji")
