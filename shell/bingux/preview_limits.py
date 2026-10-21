import json
import os
from pathlib import Path
import shutil
import subprocess

MAX_FILE_BYTES = 20_000_000


def preview_preferences():
    """Read the previews section through bingux-config, which owns the schema.

    Any failure (missing binary, invalid file, timeout) falls back to the
    built-in defaults so previews keep working with the hard cap.
    """
    executable = os.environ.get("BINGUX_CONFIG_BIN") or shutil.which("bingux-config")
    if not executable:
        return {}
    try:
        reply = subprocess.run([executable, "dump"], capture_output=True, text=True, timeout=5, check=False)
        data = json.loads(reply.stdout)
    except (OSError, subprocess.SubprocessError, ValueError):
        return {}
    if not isinstance(data, dict) or "error" in data:
        return {}
    preferences = data.get("data", {}).get("previews", {})
    return preferences if isinstance(preferences, dict) else {}


def check_file(path):
    path = Path(path)
    if not path.is_absolute() or not path.is_file():
        raise ValueError("This file is no longer available.")
    stat = path.stat()
    limit = MAX_FILE_BYTES
    preferences = preview_preferences()
    if preferences.get("enabled") is False:
        raise ValueError("File previews are disabled.")
    maximum = preferences.get("maxMegabytes", 20)
    if type(maximum) is int:
        limit = min(MAX_FILE_BYTES, max(1, maximum) * 1_000_000)
    if stat.st_size > limit:
        raise ValueError(f"This file is too large to preview ({limit // 1_000_000} MB limit).")
    return stat
