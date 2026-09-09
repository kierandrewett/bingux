#!/usr/bin/env python3
"""Read a previous Tiling Shell setup without modifying its settings."""
import ast
import json
from pathlib import Path
import subprocess

try:
    schema = Path.home() / '.local/share/gnome-shell/extensions/tilingshell@ferrarodomenico.com/schemas'
    def read(key):
        return subprocess.check_output(['gsettings', '--schemadir', str(schema), 'get',
            'org.gnome.shell.extensions.tilingshell', key], text=True, timeout=2).strip()
    print(json.dumps({'layouts': json.loads(ast.literal_eval(read('layouts-json'))),
        'inner': int(read('inner-gaps').split()[-1]), 'outer': int(read('outer-gaps').split()[-1])}))
except (OSError, ValueError, SyntaxError, subprocess.SubprocessError):
    print('{}')
