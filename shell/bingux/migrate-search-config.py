#!/usr/bin/env python3
"""Migrate Bingux-owned search launcher defaults without replacing user settings."""

import json
import os
from pathlib import Path
import sys
import tempfile


def migrate(path, launcher):
    if not path.is_file() or path.is_symlink():
        return False
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return False
    if not isinstance(data, dict) or not isinstance(data.get("commands"), dict):
        return False
    command = data["commands"].get("applicationLauncher")
    if command == launcher:
        return False
    legacy_gtk = command in (["gtk-launch"], ["/usr/bin/gtk-launch"])
    legacy_helper = (
        isinstance(command, list)
        and len(command) == 2
        and all(isinstance(part, str) for part in command)
        and Path(command[0]).name.startswith("python")
        and Path(command[1]).name == "launch-application.py"
    )
    if not legacy_gtk and not legacy_helper:
        return False
    data["commands"]["applicationLauncher"] = launcher
    mode = path.stat().st_mode & 0o777
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            json.dump(data, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
            os.chmod(temporary, mode)
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)
    return True


def main(argv):
    if len(argv) != 4:
        raise SystemExit("usage: migrate-search-config.py CONFIG INTERPRETER LAUNCHER")
    migrate(Path(argv[1]), argv[2:])


if __name__ == "__main__":
    main(sys.argv)
