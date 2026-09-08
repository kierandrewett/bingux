#!/usr/bin/env python3
"""Check fullscreen collapse and edge dragging in an isolated Wayland session."""
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

repo = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='sidebar-fullscreen-') as directory:
    root = Path(directory)
    (root / 'runtime').mkdir(mode=0o700)
    env = os.environ.copy()
    env.update(XDG_RUNTIME_DIR=str(root / 'runtime'), WLR_BACKENDS='headless',
               WLR_RENDERER='pixman', WLR_LIBINPUT_NO_DEVICES='1',
               XDG_CURRENT_DESKTOP='sway', IBUS_ADDRESS='unix:path=' + str(root / 'no-ibus'))
    for key in ('WAYLAND_DISPLAY', 'DISPLAY', 'SWAYSOCK'):
        env.pop(key, None)
    command = shlex.join([str(repo / 'tests/sidebar-notes.sh'), 'sidebar-fullscreen'])
    runner = root / 'run.sh'
    runner.write_text('#!/bin/sh\nBINGUX_NOTES_NATIVE=1 ' + command
                      + ' > ' + shlex.quote(str(root / 'test.log')) + ' 2>&1\n'
                      + 'echo $? > ' + shlex.quote(str(root / 'exit')) + '\nswaymsg exit\n')
    runner.chmod(0o755)
    config = root / 'config'
    config.write_text('output * mode 640x480\nexec ' + shlex.quote(str(runner)) + '\n')
    with (root / 'sway.log').open('w') as log:
        subprocess.run(['dbus-run-session', '--', 'sway', '-c', str(config)], env=env,
                       stdout=log, stderr=log, timeout=30, check=True)
    if not (root / 'exit').exists():
        raise RuntimeError((root / 'sway.log').read_text())
    print((root / 'test.log').read_text(), end='')
    raise SystemExit(int((root / 'exit').read_text()))
