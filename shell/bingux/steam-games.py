#!/usr/bin/env python3
"""Read installed Steam app names and cached icons without contacting Steam."""

import json
import os
from pathlib import Path
import re
import struct
import sys


VDF_TOKEN = re.compile(r'"((?:\\.|[^"\\])*)"|([{}])')
VDF_PATH = re.compile(r'"path"\s+"((?:\\.|[^"\\])*)"', re.IGNORECASE)
IMAGE_SUFFIXES = {".jpg", ".jpeg", ".png"}
JPEG_START_OF_FRAME = {
    0xC0,
    0xC1,
    0xC2,
    0xC3,
    0xC5,
    0xC6,
    0xC7,
    0xC9,
    0xCA,
    0xCB,
    0xCD,
    0xCE,
    0xCF,
}


def unescape_vdf(value):
    return re.sub(r"\\(.)", r"\1", value)


def top_level_fields(text):
    fields = {}
    depth = 0
    pending_key = None
    for match in VDF_TOKEN.finditer(text):
        value, brace = match.groups()
        if brace == "{":
            depth += 1
            pending_key = None
        elif brace == "}":
            depth = max(0, depth - 1)
            pending_key = None
        elif depth == 1:
            value = unescape_vdf(value)
            if pending_key is None:
                pending_key = value
            else:
                fields[pending_key] = value
                pending_key = None
    return fields


def steam_roots():
    home = Path.home()
    data_home = Path(os.environ.get("XDG_DATA_HOME", home / ".local/share"))
    candidates = [
        os.environ.get("STEAM_DIR", ""),
        os.environ.get("STEAM_COMPAT_CLIENT_INSTALL_PATH", ""),
        str(data_home / "Steam"),
        str(home / ".steam/steam"),
        str(home / ".steam/root"),
        str(home / ".steam/debian-installation"),
        str(home / ".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        str(home / ".var/app/com.valvesoftware.Steam/data/Steam"),
    ]
    roots = []
    seen = set()
    for candidate in candidates:
        if not candidate:
            continue
        try:
            path = Path(candidate).expanduser().resolve()
        except OSError:
            continue
        key = str(path)
        if key not in seen and (path / "steamapps").is_dir():
            roots.append(path)
            seen.add(key)
    return roots


def steam_libraries(roots):
    libraries = []
    seen = set()

    def add(path):
        try:
            resolved = Path(path).expanduser().resolve()
        except OSError:
            return
        key = str(resolved)
        if key not in seen and (resolved / "steamapps").is_dir():
            libraries.append(resolved)
            seen.add(key)

    for root in roots:
        add(root)
        folders = root / "steamapps/libraryfolders.vdf"
        try:
            text = folders.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for match in VDF_PATH.finditer(text):
            add(unescape_vdf(match.group(1)))
    return libraries


def jpeg_size(path):
    try:
        with path.open("rb") as image:
            data = image.read(131072)
    except OSError:
        return None
    if not data.startswith(b"\xff\xd8"):
        return None
    index = 2
    while index + 9 < len(data):
        if data[index] != 0xFF:
            index += 1
            continue
        while index < len(data) and data[index] == 0xFF:
            index += 1
        if index >= len(data):
            break
        marker = data[index]
        index += 1
        if marker in {0xD8, 0xD9, 0x01} or 0xD0 <= marker <= 0xD7:
            continue
        if index + 2 > len(data):
            break
        length = struct.unpack_from(">H", data, index)[0]
        if length < 2 or index + length > len(data):
            break
        if marker in JPEG_START_OF_FRAME:
            height, width = struct.unpack_from(">HH", data, index + 3)
            return width, height
        index += length
    return None


def image_size(path):
    try:
        with path.open("rb") as image:
            header = image.read(24)
    except OSError:
        return None
    if header.startswith(b"\x89PNG\r\n\x1a\n") and len(header) >= 24:
        return struct.unpack_from(">II", header, 16)
    if path.suffix.lower() in {".jpg", ".jpeg"}:
        return jpeg_size(path)
    return None


def icon_path(app_id, roots, icon_roots):
    icon_name = f"steam_icon_{app_id}"
    for icon_root in icon_roots:
        try:
            candidates = sorted(icon_root.glob(f"hicolor/*/apps/{icon_name}.*"))
        except OSError:
            candidates = []
        for candidate in candidates:
            if candidate.is_file():
                return candidate.resolve().as_uri()

    cached_images = []
    for root in roots:
        directories = [root / "appcache/librarycache" / app_id]
        try:
            directories.extend(root.glob(f"userdata/*/config/librarycache/{app_id}"))
        except OSError:
            pass
        for directory in directories:
            try:
                cached_images.extend(
                    path
                    for path in directory.rglob("*")
                    if path.is_file() and path.suffix.lower() in IMAGE_SUFFIXES
                )
            except OSError:
                continue
    cached_images.sort()
    for image in cached_images:
        size = image_size(image)
        if size and size[0] == size[1]:
            return image.resolve().as_uri()
    for preferred in ("library_capsule.jpg", "library_header.jpg", "logo.png", "library_hero.jpg"):
        for image in cached_images:
            if image.name == preferred:
                return image.resolve().as_uri()
    return cached_images[0].resolve().as_uri() if cached_images else ""


def installed_games():
    roots = steam_roots()
    libraries = steam_libraries(roots)
    home = Path.home()
    data_home = Path(os.environ.get("XDG_DATA_HOME", home / ".local/share"))
    icon_roots = [
        data_home / "icons",
        home / ".local/share/icons",
        home / ".var/app/com.valvesoftware.Steam/.local/share/icons",
        home / ".var/app/com.valvesoftware.Steam/data/icons",
        home / ".local/share/flatpak/exports/share/icons",
        Path("/usr/share/icons"),
    ]
    result = {}
    for library in libraries:
        for manifest in (library / "steamapps").glob("appmanifest_*.acf"):
            try:
                fields = top_level_fields(manifest.read_text(encoding="utf-8", errors="replace"))
            except OSError:
                continue
            app_id = fields.get("appid", "")
            name = fields.get("name", "").strip()
            if not app_id.isdigit() or not name:
                continue
            result.setdefault(
                app_id,
                {
                    "appId": app_id,
                    "name": name,
                    "iconPath": icon_path(app_id, roots, icon_roots),
                },
            )
    return list(result.values())


def searchable_games(games):
    """Exclude Steam compatibility runtimes and redistributables from app search."""
    infrastructure_prefixes = (
        "proton",
        "steam linux runtime",
        "steamworks common redistributables",
    )
    return [
        game
        for game in games
        if not game["name"].strip().casefold().startswith(infrastructure_prefixes)
    ]


if __name__ == "__main__":
    games = installed_games()
    if sys.argv[1:] == ["--search"]:
        games = searchable_games(games)
    elif sys.argv[1:]:
        raise SystemExit("usage: steam-games.py [--search]")
    print(json.dumps(games, ensure_ascii=False, separators=(",", ":")))
