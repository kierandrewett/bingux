#!/usr/bin/env python3
"""Migrate Bingux-owned search launcher defaults without replacing user settings.

Usage: migrate-search-config.py CONFIG INTERPRETER LAUNCHER STEAM_GAMES

Rewrites only the applicationLauncher and steamGameCatalog commands that
earlier Bingux releases wrote. Running it again changes nothing: the file is
replaced only when a command actually differs, and never otherwise.
"""

import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bingux_runtime import atomic_write, log  # noqa: E402

TAG = "migrate-search"


def migrate(path, launcher, steam_catalog):
    """Return True when the file was rewritten; False when nothing needed changing."""
    if not path.is_file() or path.is_symlink():
        return False
    try:
        data = json.loads(path.read_text())
    except (OSError, ValueError) as error:
        log(TAG, f"left {path} alone: {error}")
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
    text = json.dumps(data, indent=2) + "\n"
    atomic_write(path, text, mode=path.stat().st_mode & 0o777)
    return True


def build_parser():
    parser = argparse.ArgumentParser(
        description="Point Bingux's search launcher config at the current helper scripts.",
    )
    parser.add_argument("config", type=Path, help="path to search.json")
    parser.add_argument("interpreter", help="Python interpreter that runs the helpers")
    parser.add_argument("launcher", help="absolute path to launch-application.py")
    parser.add_argument("steam_games", help="absolute path to steam-games.py")
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    migrate(args.config, [args.interpreter, args.launcher], [args.interpreter, args.steam_games, "--search"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
