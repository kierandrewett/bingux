#!/usr/bin/env python3
"""Validate and atomically persist user overrides, separate from managed config."""
import hashlib
import fcntl
import copy
import re
from urllib.parse import urlsplit, unquote
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

DEFAULTS = {'search': {'disabledProviders': [], 'ai': None, 'fileRoots': None, 'defaultEngine': 'duckduckgo', 'engines': [
                {'id': 'duckduckgo', 'name': 'DuckDuckGo', 'shortcut': 'ddg', 'url': 'https://duckduckgo.com/?q={query}', 'enabled': True}]},
            'previews': {'enabled': True, 'prewarm': True, 'maxMegabytes': 20},
            'desktop': {'dock': True, 'sidebar': True, 'metrics': True, 'layout': None, 'dockSize': 56, 'dockAlignment': 'center',
                        'dockClick': 'toggle', 'dockMiddleClick': 'launch', 'dockScroll': 'cycle',
                        'dockScrollDirection': 'natural', 'sidebarEdge': None,
                        'layoutVersion': 0, 'dockApps': None, 'controlCentre': None, 'controlOrder': None,
                        'containers': {}, 'widgetOptions': {}}}
PROVIDERS = {'applications', 'files', 'calculation', 'conversions', 'web', 'web-shortcuts', 'external'}


def config_path():
    return Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config'))) / 'bingux/settings.json'


def read():
    data = json.loads(config_path().read_text()) if config_path().exists() else {}
    version = data.get('desktop', {}).get('layoutVersion', 0)
    if type(version) is not int or version not in (0, 1):
        raise ValueError('This desktop layout version is not supported.')
    return {key: DEFAULTS[key] | data.get(key, {}) for key in DEFAULTS}


def validate(data):
    if not isinstance(data, dict) or set(data) - set(DEFAULTS): raise ValueError('Unknown settings section.')
    current = read()
    result = copy.deepcopy(current)
    for key, values in data.items():
        if not isinstance(values, dict) or set(values) - set(DEFAULTS[key]): raise ValueError('Unknown setting.')
        result[key].update(values)
    search = result['search']
    disabled = search['disabledProviders']
    if not isinstance(disabled, list) or any(p not in PROVIDERS for p in disabled): raise ValueError('Unknown search provider.')
    roots = search['fileRoots']
    if roots is not None and (not isinstance(roots, list) or len(roots) > 32 or any(not isinstance(p, str) or not Path(p).is_absolute() or '\0' in p for p in roots)):
        raise ValueError('Use absolute paths for search locations, with at most 32 locations.')
    engines = search['engines']
    if not isinstance(engines, list) or not 1 <= len(engines) <= 32: raise ValueError('Add between 1 and 32 search engines.')
    ids, shortcuts = set(), set()
    for engine in engines:
        if not isinstance(engine, dict) or set(engine) != {'id', 'name', 'shortcut', 'url', 'enabled'}: raise ValueError('Invalid search engine.')
        for field in ('id', 'name', 'shortcut', 'url'):
            if not isinstance(engine[field], str) or any(ord(c) < 32 for c in engine[field]): raise ValueError('Invalid search engine text.')
        if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}', engine['id']) or engine['id'] in ids: raise ValueError('Search engine identifiers must be unique.')
        if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,23}', engine['shortcut']) or engine['shortcut'] in shortcuts or engine['shortcut'] in ('wiki', 'gh', 'maps', 'yt'):
            raise ValueError('Choose a unique shortcut with letters, numbers or hyphens. wiki, gh, maps and yt are reserved.')
        if not engine['name'].strip() or len(engine['name']) > 80: raise ValueError('Enter a name of up to 80 characters.')
        url = urlsplit(engine['url'])
        if url.scheme not in ('https', 'http') or not url.hostname or url.username or url.password or len(engine['url']) > 2048 or engine['url'].count('{query}') != 1 or '{query}' in url.netloc:
            raise ValueError('Use an HTTP or HTTPS URL with {query} in its path or query.')
        if type(engine['enabled']) is not bool: raise ValueError('Invalid engine toggle.')
        ids.add(engine['id']); shortcuts.add(engine['shortcut'])
    if not any(e['id'] == search['defaultEngine'] and e['enabled'] for e in engines): raise ValueError('The default search engine must be enabled.')
    desktop = result['desktop']
    if current['desktop']['layoutVersion'] == 1 and desktop['layoutVersion'] != 1:
        raise ValueError('The desktop layout has changed. Reopen Settings before saving.')
    if type(desktop['layoutVersion']) is not int or desktop['layoutVersion'] not in (0, 1):
        raise ValueError('This desktop layout version is not supported.')
    applications = desktop['dockApps']
    if applications is not None:
        if not isinstance(applications, dict) or set(applications) != {'pinnedApps', 'order'}:
            raise ValueError('Invalid dock application layout.')
        for items in applications.values():
            if not isinstance(items, list) or len(items) > 256 or any(not isinstance(item, str) or not item or len(item) > 256 or any(ord(c) < 32 for c in item) for item in items) or len(items) != len(set(items)):
                raise ValueError('Invalid dock application identifiers.')
    controls = desktop['controlCentre']
    control_order = desktop['controlOrder']
    if control_order is not None and (not isinstance(control_order, list) or any(not isinstance(item, str) or item not in {'network', 'bluetooth', 'vpn', 'dnd', 'nightLight', 'power', 'awake'} for item in control_order) or len(control_order) != len(set(control_order))):
        raise ValueError('Invalid control-centre widget order.')
    if controls is not None and (not isinstance(controls, dict) or set(controls) != {'vpn', 'dnd', 'nightLight', 'power', 'awake'} or any(type(value) is not bool for value in controls.values())):
        raise ValueError('Invalid control-centre preferences.')
    # Native preserves each existing widget's presentation on first import.
    modes = ('native', 'icons', 'text', 'both')
    for key, options in desktop['containers'].items() if isinstance(desktop['containers'], dict) else [(None, None)]:
        if key not in ('top-left', 'top-center', 'top-right', 'dock', 'sidebar', 'control-centre') or not isinstance(options, dict) or set(options) != {'display'} or options['display'] not in modes:
            raise ValueError('Invalid container display options.')
    if not isinstance(desktop['widgetOptions'], dict) or len(desktop['widgetOptions']) > 256:
        raise ValueError('Invalid widget display options.')
    for key, options in desktop['widgetOptions'].items():
        if not isinstance(key, str) or not isinstance(options, dict) or set(options) - {'display', 'label', 'icon'}:
            raise ValueError('Invalid widget display options.')
        if options.get('display', 'inherit') not in modes + ('inherit',):
            raise ValueError('Invalid widget display mode.')
        for field in ('label', 'icon'):
            value = options.get(field, '')
            if not isinstance(value, str) or len(value) > 160 or any(ord(c) < 32 for c in value):
                raise ValueError('Invalid widget label or icon.')
    for key, choices in {'dockAlignment': ('left', 'center', 'right'), 'dockClick': ('toggle', 'focus', 'launch'), 'dockMiddleClick': ('launch', 'close', 'none'), 'dockScroll': ('cycle', 'none'), 'dockScrollDirection': ('natural', 'reverse'), 'sidebarEdge': (None, 'left', 'right', 'top')}.items():
        if desktop[key] not in choices: raise ValueError('Invalid desktop behaviour.')
    if type(desktop['dockSize']) is not int or not 32 <= desktop['dockSize'] <= 80: raise ValueError('Dock icons must be between 32 and 80 pixels.')
    layout = desktop['layout']
    if layout is not None:
        zones = {'top-left', 'top-center', 'top-right', 'dock', 'sidebar'}
        widgets = {'search', 'clock', 'capture', 'tray', 'privacy', 'metrics', 'keyboard', 'overflow', 'controls', 'notifications'}
        panels = {'terminal', 'notes', 'monitor', 'calendar', 'media', 'tasks'}
        if not isinstance(layout, dict) or set(layout) != zones: raise ValueError('Invalid desktop layout.')
        seen = set()
        for zone, items in layout.items():
            allowed = panels if zone == 'sidebar' else widgets
            if not isinstance(items, list) or any(not isinstance(item, str) or item not in allowed or item in seen for item in items): raise ValueError('A widget can only be placed once in a compatible area.')
            if len(items) != len(set(items)): raise ValueError('A widget can only be placed once.')
            seen.update(items)
        if not layout['sidebar']: raise ValueError('Keep at least one sidebar panel.')
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


def _write(data):
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


def write(data):
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    with (path.parent / 'settings.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return _write(data)


def import_layout(snapshot):
    """Import the resolved runtime state once, without replacing later edits."""
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    with (path.parent / 'settings.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        current = read()
        if current['desktop']['layoutVersion'] == 1:
            return current
        if not isinstance(snapshot, dict) or snapshot.get('version') != 1 or snapshot.get('controlCentreReady') is not True:
            raise ValueError('Wait for the current desktop layout to finish loading.')
        desktop = copy.deepcopy(current['desktop'])
        desktop.update(layoutVersion=1, layout=desktop['layout'] or snapshot['layout'],
                       sidebarEdge=desktop['sidebarEdge'] or snapshot['sidebar']['edge'],
                       dockApps={key: snapshot['dock'][key] for key in ('pinnedApps', 'order')},
                       controlCentre=snapshot['controlCentre'])
        candidate = validate({'desktop': desktop})
        backup = path.parent / 'layout-before-import.json'
        if not backup.exists():
            with backup.open('x') as stream:
                os.chmod(backup, 0o600)
                json.dump({'settings': current, 'runtime': snapshot}, stream, indent=2)
                stream.write('\n'); stream.flush(); os.fsync(stream.fileno())
        return _write(candidate)


def wallpaper():
    for key in ('picture-uri-dark', 'picture-uri'):
        reply = subprocess.run(['gsettings', 'get', 'org.gnome.desktop.background', key], capture_output=True, text=True, timeout=3)
        uri = reply.stdout.strip().strip("'")
        parsed = urlsplit(uri)
        if parsed.scheme != 'file': continue
        source = Path(unquote(parsed.path))
        if not source.is_file(): continue
        if source.suffix.lower() in ('.png', '.jpg', '.jpeg', '.webp'): return source.as_uri()
        cache = Path(os.environ.get('XDG_CACHE_HOME', str(Path.home() / '.cache'))) / 'bingux/wallpaper'
        cache.mkdir(parents=True, exist_ok=True)
        fingerprint = hashlib.sha256(f'{source}:{source.stat().st_mtime_ns}:{source.stat().st_size}'.encode()).hexdigest()
        output = cache / (fingerprint + '.png')
        if not output.exists():
            with tempfile.NamedTemporaryFile(suffix='.png', dir=cache) as temporary:
                subprocess.run(['magick', '-limit', 'memory', '256MiB', '-limit', 'map', '512MiB',
                    str(source) + '[0]', '-resize', '2560x1440>', temporary.name], check=True, capture_output=True, timeout=12)
                os.replace(temporary.name, output)
                # NamedTemporaryFile owns the pathname; recreate it for cleanup.
                Path(temporary.name).touch()
        return output.as_uri()
    return ''


def main():
    try:
        if sys.argv[1:] == ['wallpaper']:
            print(json.dumps({'wallpaper': wallpaper()}))
        elif sys.argv[1:] == ['import-layout']:
            incoming = sys.stdin.buffer.read(131073)
            if len(incoming) > 131072: raise ValueError('Layout snapshot is too large.')
            print(json.dumps({'data': import_layout(json.loads(incoming))}))
        elif sys.argv[1:] == ['save']:
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
