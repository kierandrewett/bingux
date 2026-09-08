#!/usr/bin/env python3
"""Exercise desktop customisation in a private compositor and private settings directory."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

repo = Path(__file__).resolve().parent.parent
if not os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-'):
    raise SystemExit('Run through Gnoblin\'s private test session')
with tempfile.TemporaryDirectory(prefix='bingux-customise-') as directory:
    fixture = Path(directory)
    for source in (repo / 'shell/bingux').iterdir():
        if source.suffix in ('.qml', '.js', '.py') or source.name == 'qmldir': shutil.copy2(source, fixture)
    shutil.copy2(repo / 'tests/desktop-customise.qml', fixture / 'shell.qml')
    (fixture / 'bin').mkdir()
    systemctl = fixture / 'bin/systemctl'; systemctl.write_text('#!/bin/sh\nexit 0\n'); systemctl.chmod(0o700)
    environment = os.environ | {'QT_QPA_PLATFORM': 'wayland', 'XDG_CONFIG_HOME': str(fixture / 'config'),
        'PATH': str(fixture / 'bin') + ':' + os.environ['PATH']}
    environment.pop('BINGUX_SETTINGS_HELPER', None)
    try:
        result = subprocess.run([os.environ.get('QS_TEST_BIN', 'qs'), '-p', str(fixture), '--no-color'], env=environment,
            capture_output=True, text=True, timeout=30)
    except subprocess.TimeoutExpired as error:
        print((error.stdout or b" ").decode() if isinstance(error.stdout, bytes) else error.stdout)
        print((error.stderr or b" ").decode() if isinstance(error.stderr, bytes) else error.stderr)
        raise
    output = result.stdout + result.stderr
    print(output)
    if 'CUSTOMISE_TEST_FAILED' in output or result.returncode or 'DESKTOP_CUSTOMISE_PASS' not in output or any(error in output for error in ('ReferenceError', 'TypeError', 'Binding loop', 'FAIL!')):
        raise SystemExit(1)
