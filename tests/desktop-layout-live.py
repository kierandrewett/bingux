#!/usr/bin/env python3
"""Apply an editor layout to the actual shell in a private compositor."""
import os
import argparse
from pathlib import Path
import shutil
import tempfile
import subprocess
import time
from private_shell import run_reported_shell, stage_compositor_bridge

parser = argparse.ArgumentParser()
parser.add_argument('--case', choices=('desktop-layout', 'control-layout', 'control-audio', 'control-media', 'dock-unpin'), default='desktop-layout')
case = parser.parse_args().case
repo = Path(__file__).resolve().parent.parent
if not os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-'):
    raise SystemExit('Run through Gnoblin\'s private test session')
with tempfile.TemporaryDirectory(prefix='bingux-layout-live-') as directory:
    fixture = Path(directory)
    for source in (repo / 'shell/bingux').iterdir():
        if source.suffix in ('.qml', '.js', '.py') or source.name == 'qmldir': shutil.copy2(source, fixture)
    shell = (fixture / 'shell.qml').read_text().replace('import QtQuick\n', 'import QtQuick\nimport QtQuick.Window\nimport QtTest\n', 1)
    if case == 'control-layout': shell = 'import "ControlLayout.js" as ControlLayout\n' + shell
    shell = shell.rstrip()[:-1] + (repo / ('tests/' + case + '-live.inc.qml')).read_text() + '\n}\n'
    (fixture / 'shell.qml').write_text(shell)
    for folder in ('icons', 'preview-assets'): shutil.copytree(repo / 'shell/bingux' / folder, fixture / folder)
    (fixture / 'bin').mkdir()
    systemctl = fixture / 'bin/systemctl'; systemctl.write_text('#!/bin/sh\nexit 0\n'); systemctl.chmod(0o700)
    environment = os.environ | {'BINGUX_TEST_COMPOSITOR_CONFIG': os.environ['XDG_CONFIG_HOME'], 'BINGUX_TEST_NATIVE_INPUT': str(repo / 'tests/customise-native-input.py'), 'QT_QPA_PLATFORM': 'wayland', 'XDG_CONFIG_HOME': str(fixture / 'config'),
        'XDG_STATE_HOME': str(fixture / 'state'), 'BINGUX_LAYOUT_IMPORT': '0', 'PATH': str(fixture / 'bin') + ':' + os.environ['PATH']}
    if case == 'control-layout':
        (fixture / 'action-avatar.svg').write_text('<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><circle cx="24" cy="24" r="24" fill="#77aaff"/></svg>')
        environment['BINGUX_ACTION_REPORT'] = str(fixture / 'actions.jsonl')
        for name in ('gnome-control-center', 'loginctl'):
            helper = fixture / 'bin' / name
            helper.write_text('#!/usr/bin/python3\nimport json,os,sys\nwith open(os.environ["BINGUX_ACTION_REPORT"],"a") as output: output.write(json.dumps({"command":os.path.basename(sys.argv[0]),"arguments":sys.argv[1:]})+"\\n")\n')
            helper.chmod(0o700)
    environment.pop('BINGUX_SETTINGS_HELPER', None)
    stage_compositor_bridge(repo, os.environ['XDG_CONFIG_HOME'])
    subprocess.run(['python3', str(repo / 'tests/customise-native-input.py'), '--prepare'], env=environment, check=True)
    time.sleep(.3)
    report, output = run_reported_shell(fixture, environment, 'BINGUX_LAYOUT_REPORT', timeout=55)
    print(output)
    if report != 'PASS' or any(error in output for error in ('CUSTOMISE_TEST_FAILED', 'TypeError', 'ReferenceError', 'has crashed', 'property "maximumPopupHeight"', 'property "preferredY"', 'Cannot use same item on different windows', 'Updates can only be scheduled', 'QGridLayoutEngine::addItem')):
        raise SystemExit(report if report != 'PASS' else 'Runtime errors during layout test')
