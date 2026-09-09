"""Opt-in native CaptureTool regression: recording and Stop survive QML reload."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

source = Path(__file__).resolve().parents[1] / 'shell/bingux'
workspace = Path(tempfile.mkdtemp(prefix='bingux-capture-reload-'))
folder = workspace / 'ui'
folder.mkdir()
for pattern in ('*.qml', '*.js', '*.py', 'qmldir'):
    for path in source.glob(pattern):
        shutil.copy2(path, folder / path.name)
shutil.copytree(source / 'icons', folder / 'icons')
qml = folder / 'shell.qml'
qml.write_text('''import QtQuick
import Quickshell
ShellRoot {
 Connections { target: Quickshell; function onReloadCompleted() { Quickshell.inhibitReloadPopup(); } }
 CaptureTool { screen: Quickshell.screens[0] }
}
''')
env = dict(os.environ, BINGUX_CAPTURE_SETTINGS_PATH=(workspace / 'settings.ini').as_uri())
env.pop('BINGUX_CAPTURE_HELPER', None)
log = (workspace / 'shell.log').open('w')
process = subprocess.Popen([os.environ.get('QUICKSHELL_BIN', 'quickshell'), '-p', str(folder)], env=env, stdout=log, stderr=log)

def call(method, *args):
    result = subprocess.run(['qs', 'ipc', '--any-display', '-p', str(folder), 'call', 'capture', method, *args],
                            capture_output=True, text=True, timeout=3, check=True)
    return json.loads(result.stdout) if result.stdout.strip() else None

def wait(state):
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        try:
            status = call('status')
            assert status['state'] != 'error', status
            if status['state'] == state and status['ready']:
                return status
        except (ValueError, subprocess.CalledProcessError):
            pass
        time.sleep(.1)
    raise TimeoutError(state)

try:
    wait('idle')
    assert call('configure', json.dumps(dict(kind='recording', target='screen', directory=str(workspace / 'recordings'), audio='none', maxHeight=1080)))['ok']
    call('open')
    wait('selecting')
    assert call('take')['ok']
    wait('recording')
    time.sleep(2)
    qml.write_text(qml.read_text() + '\n// reload during recording\n')
    time.sleep(3)
    status = wait('recording')
    assert status['elapsed'] >= 4, status
    call('stop')
    saved = wait('saved')['savedPath']
    info = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'json', saved]))
    assert float(info['format']['duration']) >= 4, info
    subprocess.run(['ffmpeg', '-v', 'error', '-i', saved, '-f', 'null', '-'], check=True)
    print('PASS: native CaptureTool reload retained timer, Stop and a decodable recording', info)
finally:
    try:
        call("stop")
    except (OSError, subprocess.SubprocessError):
        pass
    process.terminate()
    process.wait(timeout=5)
    log.close()
