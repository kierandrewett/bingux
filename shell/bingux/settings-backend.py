#!/usr/bin/env python3
"""Validate and atomically persist user overrides, separate from managed config."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

DEFAULTS = {'search': {'disabledProviders': [], 'ai': None},
            'previews': {'enabled': True, 'prewarm': True, 'maxMegabytes': 20},
            'desktop': {'dock': True, 'sidebar': True, 'metrics': True}}
PROVIDERS = {'applications', 'files', 'calculation', 'conversions', 'web', 'web-shortcuts', 'external'}


def config_path():
    return Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config'))) / 'bingux/settings.json'


def read():
    data = json.loads(config_path().read_text()) if config_path().exists() else {}
    return {key: DEFAULTS[key] | data.get(key, {}) for key in DEFAULTS}


def validate(data):
    if not isinstance(data, dict) or set(data) - set(DEFAULTS): raise ValueError('Unknown settings section.')
    result = read()
    for key, values in data.items():
        if not isinstance(values, dict) or set(values) - set(DEFAULTS[key]): raise ValueError('Unknown setting.')
        result[key].update(values)
    search = result['search']
    disabled = search['disabledProviders']
    if not isinstance(disabled, list) or any(p not in PROVIDERS for p in disabled): raise ValueError('Unknown search provider.')
    ai = search['ai']
    if ai is not None:
        if not isinstance(ai, dict) or set(ai) - {'harness', 'executable', 'model'}: raise ValueError('Invalid AI settings.')
        if ai.get('harness') not in ('pi', 'claude'): raise ValueError('Choose Pi or Claude Code.')
        model = ai.get('model', '')
        if not isinstance(model, str) or len(model) > 256 or any(ord(c) < 32 for c in model): raise ValueError('Invalid model.')
        executable = ai.get('executable') or shutil.which(ai['harness'])
        if not executable or not Path(executable).is_absolute() or not os.access(executable, os.X_OK): raise ValueError('The selected CLI is not installed or executable.')
        ai['executable'] = executable
    for section, fields in [('previews', ('enabled', 'prewarm')), ('desktop', ('dock', 'sidebar', 'metrics'))]:
        if any(type(result[section][key]) is not bool for key in fields): raise ValueError('Invalid toggle value.')
    limit = result['previews']['maxMegabytes']
    if type(limit) is not int or not 1 <= limit <= 20: raise ValueError('Preview limit must be between 1 and 20 MB.')
    return result


def write(data):
    data = validate(data)
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            json.dump(data, stream, indent=2)
            stream.write('\n'); stream.flush(); os.fsync(stream.fileno())
            os.chmod(temporary, 0o600)
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)
    return data


def main():
    try:
        if sys.argv[1:] == ['save']:
            incoming = sys.stdin.buffer.read(32769)
            if len(incoming) > 32768: raise ValueError('Settings are too large.')
            before = read()
            data = write(json.loads(incoming))
            warning = ''
            if before['search'] != data['search']:
                result = subprocess.run(['systemctl', '--user', 'try-restart', 'bingux-searchd.service'], capture_output=True, timeout=8)
                if result.returncode: warning = 'Saved. Restart the search service to apply search settings.'
            print(json.dumps({'data': data, 'warning': warning}))
        else:
            print(json.dumps({'data': read(), 'harnesses': [{'id': name, 'path': shutil.which(name) or ''} for name in ('pi', 'claude')]}))
    except Exception as error:
        print(json.dumps({'error': str(error)}))
        sys.exit(1)

if __name__ == '__main__': main()
