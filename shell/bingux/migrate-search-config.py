#!/usr/bin/env python3
"""Migrate Bingux-owned search launcher defaults without replacing user settings."""

import json
import os
from pathlib import Path
import sys
import tempfile


def migrate(path, launcher, steam_catalog):
    if not path.is_file() or path.is_symlink():
        return False
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return False
    if not isinstance(data, dict) or not isinstance(data.get("commands"), dict):
        return False
    commands = data["commands"]
    command = commands.get("applicationLauncher")
    changed = False
    legacy_gtk = command in (["gtk-launch"], ["/usr/bin/gtk-launch"])
    legacy_helper = (
        isinstance(command, list)
        and len(command) == 2
        and all(isinstance(part, str) for part in command)
        and Path(command[0]).name.startswith("python")
        and Path(command[1]).name == "launch-application.py"
    )
    if command != launcher and (legacy_gtk or legacy_helper):
        commands["applicationLauncher"] = launcher
        changed = True

    old_steam_helper = (
        isinstance(commands.get("steamGameCatalog"), list)
        and len(commands["steamGameCatalog"]) in (2, 3)
        and all(isinstance(part, str) for part in commands["steamGameCatalog"])
        and Path(commands["steamGameCatalog"][0]).name.startswith("python")
        and Path(commands["steamGameCatalog"][1]).name == "steam-games.py"
        and commands["steamGameCatalog"][2:] in ([], ["--search"])
    )
    if "steamGameCatalog" not in commands or old_steam_helper:
        if commands.get("steamGameCatalog") != steam_catalog:
            commands["steamGameCatalog"] = steam_catalog
            changed = True

    if not changed:
        return False
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
    if len(argv) != 5:
        raise SystemExit("usage: migrate-search-config.py CONFIG INTERPRETER LAUNCHER STEAM_GAMES")
    interpreter = argv[2]
    launcher = [interpreter, argv[3]]
    steam_catalog = [interpreter, argv[4], "--search"]
    migrate(Path(argv[1]), launcher, steam_catalog)


if __name__ == "__main__":
    main(sys.argv)
