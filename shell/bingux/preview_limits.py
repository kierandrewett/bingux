from pathlib import Path
import json
import os

MAX_FILE_BYTES = 20_000_000


def check_file(path):
    path = Path(path)
    if not path.is_absolute() or not path.is_file():
        raise ValueError('This file is no longer available.')
    stat = path.stat()
    limit = MAX_FILE_BYTES
    try:
        config = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config'))) / 'bingux/settings.json'
        preferences = json.loads(config.read_text()).get('previews', {})
        if preferences.get('enabled') is False: raise ValueError('File previews are disabled in Bingux Settings.')
        maximum = preferences.get('maxMegabytes', 20)
        if type(maximum) is int: limit = min(MAX_FILE_BYTES, max(1, maximum) * 1_000_000)
    except (OSError, json.JSONDecodeError): pass
    if stat.st_size > limit:
        raise ValueError(f'This file is too large to preview ({limit // 1_000_000} MB limit).')
    return stat
