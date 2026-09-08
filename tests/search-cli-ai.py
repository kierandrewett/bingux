"""Exercise the real daemon/socket with deterministic, streaming CLI harnesses."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / 'packages/bingux-searchd/target/debug/bingux-searchd'
with tempfile.TemporaryDirectory() as directory:
    base = Path(directory)
    (base / 'config').mkdir()
    (base / 'runtime').mkdir(mode=0o700)
    harness = base / 'harness'
    harness.write_text('''#!/usr/bin/python3
import json,sys,time,os,subprocess
from pathlib import Path
messages=json.load(sys.stdin)
Path(os.environ['AI_MARKER']).write_text(json.dumps({'args':sys.argv[1:], 'messages':messages,'pid':os.getpid()}))
pi='--mode' in sys.argv
assert '--no-tools' in sys.argv if pi else '--tools' in sys.argv and sys.argv[sys.argv.index('--tools')+1] == ''
def emit(text):
    event={'type':'message_update','assistantMessageEvent':{'type':'text_delta','delta':text}} if pi else {'type':'stream_event','event':{'delta':{'type':'text_delta','text':text}}}
    print(json.dumps(event),flush=True)
emit('First line\\n')
if messages[-1]['content']=='cancel':
    child=subprocess.Popen(['/usr/bin/sleep','30'])
    Path(os.environ['AI_CHILD']).write_text(str(child.pid))
    time.sleep(30)
time.sleep(.25)
emit('Second line')
if messages[-1]['content']=='fail': sys.exit(1)
''')
    harness.chmod(0o700)
    environment = os.environ | {'XDG_RUNTIME_DIR': str(base / 'runtime'), 'XDG_CONFIG_HOME': str(base / 'config'), 'AI_MARKER': str(base / 'called'), 'AI_CHILD': str(base / 'child')}
    for adapter in ('pi', 'claude'):
        config = {'protocolVersion': 1, 'commands': {'applicationLauncher': ['/usr/bin/true'], 'fileOpener': ['/usr/bin/true'], 'clipboard': ['/usr/bin/true']}, 'fileRoots': [], 'ai': {'harness': adapter, 'executable': str(harness)}}
        path = base / 'search.json'; path.write_text(json.dumps(config))
        daemon = subprocess.Popen([str(BINARY), '--config', str(path)], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            address = base / 'runtime/bingux/search-v1.sock'
            for attempt in range(200):
                if address.exists(): break
                time.sleep(.01)
            connection = socket.socket(socket.AF_UNIX); connection.settimeout(3); connection.connect(str(address))
            reader = connection.makefile('rb')
            def send(kind, request, **fields): connection.sendall((json.dumps({'protocolVersion': 1, 'type': kind, 'requestId': request} | fields) + '\n').encode())
            def read(request):
                while True:
                    line = reader.readline()
                    assert line, 'Daemon closed socket'
                    event = json.loads(line)
                    if event.get('requestId') == request: return event
            def query(text):
                send('query', 'query', query=text, limit=20)
                while True:
                    event = read('query')
                    if event.get('complete'): return event['results']
            marker = base / 'called'; marker.unlink(missing_ok=True)
            results = query('! hello')
            assert len(results) == 1 and results[0]['kind'] == 'chat'
            assert not marker.exists(), 'Typing started the harness'
            send('activate', 'answer', resultId=results[0]['resultId'])
            started = time.monotonic(); events = []
            while True:
                event = read('answer'); events.append(event)
                if event['type'] == 'chat-progress' and len(events) == 1:
                    first = time.monotonic() - started
                if event['type'] == 'chat-response': break
                assert event['type'] != 'error', event
            assert first < .25, first
            assert events[-1]['message'] == 'First line\nSecond line'
            assert len(events) >= 2 and time.monotonic() - started > .2
            results = query('! follow-up'); send('activate', 'follow', resultId=results[0]['resultId'])
            while read('follow')['type'] != 'chat-response': pass
            assert len(json.loads(marker.read_text())['messages']) == 3
            results = query('! cancel'); send('activate', 'cancelled', resultId=results[0]['resultId'])
            assert read('cancelled')['type'] == 'chat-progress'
            for attempt in range(100):
                if (base / 'child').exists(): break
                time.sleep(.01)
            pid = json.loads(marker.read_text())['pid']; child = int((base / 'child').read_text())
            send('cancel', 'cancelled')
            for attempt in range(100):
                if not Path(f'/proc/{pid}').exists(): break
                time.sleep(.01)
            assert not Path(f'/proc/{pid}').exists(), 'Harness survived cancellation'
            child_stat = Path(f'/proc/{child}/stat')
            assert not child_stat.exists() or child_stat.read_text().split()[2] == 'Z', 'Descendant survived'
            (base / 'child').unlink()
            results = query('! fail'); send('activate', 'failure', resultId=results[0]['resultId'])
            while True:
                event = read('failure')
                if event['type'] != 'chat-progress': break
            assert event['type'] == 'error', event
            assert any(r['title'] == '6.213712 mi' for r in query('10 km to mi'))
            assert any(r['title'] == '32 f' for r in query('0 c to f'))
            assert not any(r['providerId'] == 'conversions' for r in query('10 kg to km'))
            assert any(r['subtitle'] == 'Wikipedia' for r in query('wiki: rust'))
            print(f'PASS {adapter}: explicit activation, first streamed text {first*1000:.0f} ms, multiline final, history, cancellation, process cleanup, failure, conversion and website providers')
            reader.close(); connection.close()
        finally:
            daemon.terminate(); daemon.wait(timeout=5)
            address.unlink(missing_ok=True)
