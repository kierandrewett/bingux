#!/usr/bin/env python3
"""Verify saved search engines through the actual daemon and activation protocol."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
binary = Path(os.environ.get('BINGUX_SEARCHD_BIN', str(repo / 'packages/bingux-searchd/target/debug/bingux-searchd')))
with tempfile.TemporaryDirectory() as directory:
    base = Path(directory)
    (base / 'config/bingux').mkdir(parents=True)
    (base / 'runtime').mkdir(mode=0o700)
    opener = base / 'open'
    opener.write_text('#!/usr/bin/python3\nimport os,sys\nfrom pathlib import Path\nPath(os.environ["OPENED_URL"]).write_text(sys.argv[1])\n')
    opener.chmod(0o700)
    config = {'protocolVersion': 1, 'commands': {'applicationLauncher': ['/usr/bin/true'], 'fileOpener': [str(opener)], 'clipboard': ['/usr/bin/true']}, 'fileRoots': []}
    path = base / 'search.json'; path.write_text(json.dumps(config))
    engines = [{'id': 'default', 'name': 'Example search', 'shortcut': 'ex', 'url': 'https://example.test/search?q={query}', 'enabled': True},
               {'id': 'docs', 'name': 'Documentation', 'shortcut': 'docs', 'url': 'https://docs.example.test/{query}', 'enabled': True}]
    env = os.environ | {'XDG_CONFIG_HOME': str(base / 'config'), 'XDG_RUNTIME_DIR': str(base / 'runtime'), 'OPENED_URL': str(base / 'opened')}
    for disabled in (False, True):
        engines[1]['enabled'] = not disabled
        (base / 'config/bingux/settings.json').write_text(json.dumps({'search': {'engines': engines, 'defaultEngine': 'default', 'disabledProviders': ['applications', 'files', 'external'], 'fileRoots': []}}))
        daemon = subprocess.Popen([str(binary), '--config', str(path)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            address = base / 'runtime/bingux/search-v1.sock'
            for _ in range(200):
                if address.exists(): break
                if daemon.poll() is not None: raise AssertionError(daemon.stderr.read().decode())
                time.sleep(.01)
            connection = socket.socket(socket.AF_UNIX); connection.settimeout(5); connection.connect(str(address))
            reader = connection.makefile('rb')
            def request(kind, request_id, **fields):
                connection.sendall((json.dumps({'protocolVersion': 1, 'type': kind, 'requestId': request_id} | fields) + '\n').encode())
            def query(text):
                request('query', 'query', query=text, limit=20)
                while True:
                    event = json.loads(reader.readline())
                    if event.get('requestId') == 'query' and event.get('complete'): return event['results']
            results = query('café & cats? #1')
            web = next(result for result in results if result['providerId'] == 'web')
            assert web['subtitle'] == 'Example search', web
            request('activate', 'open', resultId=web['resultId'])
            for _ in range(100):
                if (base / 'opened').exists(): break
                time.sleep(.01)
            assert (base / 'opened').read_text() == 'https://example.test/search?q=caf%C3%A9%20%26%20cats%3F%20%231'
            (base / 'opened').unlink()
            docs = [r for r in query('docs: quick start') if r['providerId'] == 'web-shortcuts']
            assert bool(docs) != disabled, docs
            if docs:
                request('activate', 'docs', resultId=docs[0]['resultId'])
                for _ in range(100):
                    if (base / 'opened').exists(): break
                    time.sleep(.01)
                assert (base / 'opened').read_text() == 'https://docs.example.test/quick%20start'
                (base / 'opened').unlink()
            connection.close()
        finally:
            daemon.terminate(); daemon.wait(timeout=5)
            address.unlink(missing_ok=True)
print('SEARCH_ENGINES_PASS')
