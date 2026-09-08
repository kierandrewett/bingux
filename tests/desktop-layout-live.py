#!/usr/bin/env python3
"""Apply an editor layout to the actual shell in a private compositor."""
import os
from pathlib import Path
import shutil
import tempfile
from private_shell import run_reported_shell

repo = Path(__file__).resolve().parent.parent
if not os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-'):
    raise SystemExit('Run through Gnoblin\'s private test session')
with tempfile.TemporaryDirectory(prefix='bingux-layout-live-') as directory:
    fixture = Path(directory)
    for source in (repo / 'shell/bingux').iterdir():
        if source.suffix in ('.qml', '.js', '.py') or source.name == 'qmldir': shutil.copy2(source, fixture)
    shell = (fixture / 'shell.qml').read_text().replace('import QtQuick\n', 'import QtQuick\nimport QtTest\n', 1)
    shell = shell.rstrip()[:-1] + (repo / 'tests/desktop-layout-live.inc.qml').read_text() + '\n}\n'
    (fixture / 'shell.qml').write_text(shell)
    for folder in ('icons', 'preview-assets'): shutil.copytree(repo / 'shell/bingux' / folder, fixture / folder)
    (fixture / 'bin').mkdir()
    systemctl = fixture / 'bin/systemctl'; systemctl.write_text('#!/bin/sh\nexit 0\n'); systemctl.chmod(0o700)
    environment = os.environ | {'QT_QPA_PLATFORM': 'wayland', 'XDG_CONFIG_HOME': str(fixture / 'config'),
        'XDG_STATE_HOME': str(fixture / 'state'), 'BINGUX_LAYOUT_IMPORT': '0', 'PATH': str(fixture / 'bin') + ':' + os.environ['PATH']}
    environment.pop('BINGUX_SETTINGS_HELPER', None)
    report, output = run_reported_shell(fixture, environment, 'BINGUX_LAYOUT_REPORT', timeout=25)
    print(output)
    if report != 'PASS' or any(error in output for error in ('CUSTOMISE_TEST_FAILED', 'TypeError', 'ReferenceError', 'has crashed', 'property "maximumPopupHeight"', 'property "preferredY"')):
        raise SystemExit(report if report != 'PASS' else 'Runtime errors during layout test')
