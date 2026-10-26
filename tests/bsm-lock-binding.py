#!/usr/bin/env python3
"""Run the Super+L command from packaging/gnoblin/bingux.lua with and without bsm.

Gnoblin's Lua has no os or io library, so the choice between `bsmctl lock` and
the standalone `bingux-lock` is made by the shell command the shortcut runs.
This takes that command out of the packaged file and runs it with fake
binaries on PATH: with BSM_SESSION_ID set it must call bsmctl only, without it
bingux-lock only. The real Lua loader checks the same entry in
tests/gnoblin-config-loader.py.
"""

import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def packaged_command():
    text = (ROOT / "packaging/gnoblin/bingux.lua").read_text()
    match = re.search(
        r'\["bingux-lock"\] = \{\s*binding = "<Super>l",\s*command = \{\s*"sh",\s*"-c",\s*\'([^\']+)\',', text
    )
    assert match, "the Super+L shortcut is missing from bingux.lua"
    return ["sh", "-c", match.group(1)]


def run(command, directory, environment):
    log = directory / "calls.log"
    log.unlink(missing_ok=True)
    subprocess.run(command, env=environment, check=True, timeout=20)
    return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []


def main():
    command = packaged_command()
    with tempfile.TemporaryDirectory(prefix="bsm-lock-binding-") as temporary:
        directory = Path(temporary)
        for name in ("bsmctl", "bingux-lock", "gnoblinctl"):
            program = directory / name
            program.write_text(
                "#!/bin/sh\n"
                f'printf \'["%s"%s]\\n\' "{name}" "$(for a in "$@"; do printf \',"%s"\' "$a"; done)" >> "{directory}/calls.log"\n'
            )
            program.chmod(program.stat().st_mode | stat.S_IXUSR)
        base = {key: value for key, value in os.environ.items() if key not in ("BSM_SESSION_ID", "BSM_GENERATION")}
        base["PATH"] = str(directory) + os.pathsep + "/usr/bin:/bin"

        calls = run(command, directory, base | {"BSM_SESSION_ID": "s0000000a"})
        assert calls == [["bsmctl", "lock"]], f"hosted by bsm: expected only `bsmctl lock`, got {calls}"

        calls = run(command, directory, base)
        assert calls == [["bingux-lock"]], f"standalone: expected only `bingux-lock`, got {calls}"

        calls = run(command, directory, base | {"BSM_SESSION_ID": ""})
        assert calls == [["bingux-lock"]], f"empty BSM_SESSION_ID must count as standalone, got {calls}"
    print("PASS: Super+L runs bsmctl lock when BSM_SESSION_ID is set and bingux-lock otherwise")
    return 0


if __name__ == "__main__":
    sys.exit(main())
