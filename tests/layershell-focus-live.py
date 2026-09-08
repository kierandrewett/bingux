#!/usr/bin/env python3
"""Verify a newly mapped panel can immediately disable keyboard focus."""
import os
from pathlib import Path
import re
import subprocess
import tempfile

if not os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-'):
    raise SystemExit('Run through Gnoblin\'s private test session')
with tempfile.TemporaryDirectory(prefix='bingux-layer-focus-') as directory:
    fixture = Path(directory)
    (fixture / 'shell.qml').write_text('''
import QtQuick
import Quickshell
import Quickshell.Wayland
ShellRoot {
    PanelWindow {
        id: panel
        visible: false
        implicitWidth: 200
        implicitHeight: 80
        WlrLayershell.namespace: "bingux-focus-regression"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand
    }
    Timer {
        interval: 100; running: true
        onTriggered: {
            panel.visible = true;
            panel.WlrLayershell.keyboardFocus = WlrKeyboardFocus.None;
        }
    }
    Timer { interval: 1000; running: true; onTriggered: Qt.quit() }
}
''')
    result = subprocess.run([os.environ.get('QS_TEST_BIN', 'qs'), '-p', str(fixture), '--no-color'],
                            env=os.environ | {'WAYLAND_DEBUG': 'client'},
                            capture_output=True, text=True, timeout=8)
    output = result.stdout + result.stderr
    surface = re.search(r'get_layer_surface\(new id zwlr_layer_surface_v1[#@](\d+).*"bingux-focus-regression"', output)
    if result.returncode or not surface:
        raise SystemExit(output)
    states = re.findall(r'zwlr_layer_surface_v1[#@]' + surface[1] + r'\.set_keyboard_interactivity\((\d+)\)', output)
    print('Layer keyboard focus requests:', states)
    assert states and states[-1] == '0', 'The compositor must receive None after the panel maps'
