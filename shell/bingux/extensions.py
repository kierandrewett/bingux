#!/usr/bin/env python3
"""Discover and manage trusted Bingux QML extensions without importing their code."""
import argparse
import json
import os
from pathlib import Path
import re
import tempfile

ID = re.compile(r"[a-z0-9][a-z0-9._-]{0,127}")


def config_path():
    return Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "bingux/extensions.json"


def roots():
    user = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    return [user / "bingux/extensions"] + [Path(p) / "bingux/extensions" for p in os.environ.get("XDG_DATA_DIRS", "/usr/local/share:/usr/share").split(":") if p]


def component(folder, value):
    if not isinstance(value, str) or not value:
        raise ValueError("Component must be a relative QML filename")
    path = (folder / value).resolve()
    if Path(value).is_absolute() or not path.is_relative_to(folder.resolve()) or path.suffix != ".qml" or not path.is_file():
        raise ValueError(f"Missing or invalid QML component: {value}")
    return path.as_uri()


def manifest(folder):
    data = json.loads((folder / "extension.json").read_text())
    if not isinstance(data, dict) or not isinstance(data.get("id"), str) or not ID.fullmatch(data["id"]):
        raise ValueError("Manifest needs a valid id")
    if data["id"] != folder.name:
        raise ValueError("Folder name must match extension id")
    if not isinstance(data.get("name"), str) or not data["name"].strip():
        raise ValueError("Manifest needs a name")
    if type(data.get("apiVersion")) is not int or data["apiVersion"] != 1:
        raise ValueError("Unsupported extension API version; this host provides version 1")
    widgets = data.get("widgets", [])
    if not isinstance(widgets, list):
        raise ValueError("widgets must be a list")
    result = dict(data, directory=str(folder.resolve()), widgets=[])
    seen = set()
    for widget in widgets:
        if not isinstance(widget, dict) or not isinstance(widget.get("id"), str) or not ID.fullmatch(widget["id"]) or widget["id"] in seen:
            raise ValueError("Widget ids must be valid and unique within the extension")
        seen.add(widget["id"])
        if not isinstance(widget.get("name"), str) or not widget["name"].strip():
            raise ValueError("Each widget needs a name")
        result["widgets"].append(dict(widget, id=f"extension:{data['id']}/{widget['id']}",
            extensionId=data["id"], label=widget["name"], source=component(folder, widget.get("component")),
            previewSource=component(folder, widget.get("preview", widget.get("component")))))
    result["settingsSource"] = component(folder, data["settings"]) if data.get("settings") else ""
    result["entrySource"] = component(folder, data["entry"]) if data.get("entry") else ""
    return result


def state():
    try:
        data = json.loads(config_path().read_text())
    except FileNotFoundError:
        return {"enabled": []}
    if not isinstance(data, dict) or not isinstance(data.get("enabled"), list) or any(not isinstance(i, str) or not ID.fullmatch(i) for i in data["enabled"]):
        raise ValueError("extensions.json must contain an enabled list of extension ids")
    return data


def discover():
    result = {"extensions": [], "widgets": [], "errors": []}
    try:
        enabled = state()["enabled"]
    except (ValueError, OSError) as error:
        result["errors"].append({"id": "configuration", "message": str(error)})
        enabled = []
    safe = os.environ.get("BINGUX_NO_EXTENSIONS") == "1"
    seen = set()
    for root in roots():
        if not root.is_dir():
            continue
        for folder in sorted(root.iterdir()):
            if not folder.is_dir() or not (folder / "extension.json").is_file() or folder.name in seen:
                continue
            seen.add(folder.name)
            try:
                item = manifest(folder)
                item["enabled"] = item["id"] in enabled and not safe
                result["extensions"].append(item)
                if item["enabled"]:
                    result["widgets"].extend(item["widgets"])
            except (ValueError, OSError, TypeError) as error:
                result["errors"].append({"id": folder.name, "message": str(error)})
    for identifier in enabled:
        if identifier not in seen:
            result["errors"].append({"id": identifier, "message": "Enabled extension is not installed"})
    return result


def set_enabled(identifier, enabled):
    if not ID.fullmatch(identifier):
        raise ValueError("Invalid extension id")
    if enabled and not any(item["id"] == identifier for item in discover()["extensions"]):
        raise ValueError("Extension is missing or its manifest is invalid")
    data = state()
    data["enabled"] = [item for item in data["enabled"] if item != identifier]
    if enabled:
        data["enabled"].append(identifier)
    target = config_path()
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=target.parent, delete=False) as output:
        temporary = Path(output.name)
        json.dump(data, output, indent=2)
        output.write("\n")
    try:
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["list", "enable", "disable"], nargs="?", default="list")
    parser.add_argument("id", nargs="?")
    args = parser.parse_args()
    try:
        if args.command != "list":
            if not args.id:
                parser.error("enable and disable need an extension id")
            set_enabled(args.id, args.command == "enable")
        print(json.dumps(discover()))
    except (ValueError, OSError) as error:
        parser.exit(1, f"[extensions] {error}\n")


if __name__ == "__main__":
    main()
