"""Opt-in screen recording test: kill the UI transport, reconnect, stop and decode."""
import json
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading
import time

root = Path(__file__).resolve().parents[1] / 'shell/bingux'
folder = Path(tempfile.mkdtemp(prefix='bingux-recording-reconnect-'))
for name in ('capture_service.py', 'capture_backend.py', 'capture-notify.py'):
    shutil.copy2(root / name, folder / name)


def start():
    process = subprocess.Popen(['python3', '-u', str(folder / 'capture_service.py')],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    events = queue.Queue()
    def read():
        for line in process.stdout:
            events.put(json.loads(line))
    threading.Thread(target=read, daemon=True).start()
    return process, events


def wait(events, kind):
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        event = events.get(timeout=max(.1, deadline - time.monotonic()))
        print(event, flush=True)
        assert event['event'] != 'error', event
        if event['event'] == kind:
            return event
    raise TimeoutError(kind)


def send(process, **record):
    process.stdin.write(json.dumps(record) + '\n')
    process.stdin.flush()


process, events = start()
try:
    wait(events, 'ready')
    send(process, command='capture', kind='recording', target='screen',
         maxHeight=1080, fps=30, audio='none', encoder='auto', directory=str(folder))
    original = wait(events, 'recording')
    time.sleep(2)
    process.kill()  # Quickshell destroys Process with SIGKILL on reload.
    process.wait(timeout=3)
    time.sleep(2)
    process, events = start()
    wait(events, 'ready')
    resumed = wait(events, 'recording')
    assert resumed['path'] == original['path']
    assert resumed['started'] == original['started']
    time.sleep(2)
    send(process, command='stop')
    saved = wait(events, 'saved')
    info = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_entries',
        'format=duration:stream=width,height', '-of', 'json', saved['path']]))
    assert float(info['format']['duration']) >= 5.5, info
    assert info['streams'][0]['height'] <= 1080, info
    subprocess.run(['ffmpeg', '-v', 'error', '-i', saved['path'], '-f', 'null', '-'], check=True)
    print('PASS: recording survived UI death, reconnected, saved and fully decoded', info)
finally:
    if process.poll() is None:
        try:
            send(process, command="stop")
        except (OSError, BrokenPipeError):
            pass
    process.terminate()
    process.wait(timeout=3)
