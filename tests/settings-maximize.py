#!/usr/bin/env python3
"""Verify Settings' native resize animation inside a private Gnoblin session.

Launch standalone Settings in that session first, using its matching Qt plugin.
"""
import json
import os
from pathlib import Path
import subprocess
import time

if not os.environ.get('WAYLAND_DISPLAY', '').startswith('gnoblin-gs-'):
    raise SystemExit('Run inside a private Gnoblin test session')
root = Path(os.environ['XDG_CONFIG_HOME']) / 'gnoblin'
probe = root / 'scripts/settings-animation-test.js'
report = root / 'settings-animation.json'
qml = Path(__file__).resolve().parents[1] / 'shell/bingux/settings.qml'
qs = os.environ.get('QS_TEST_BIN', 'qs')
probe.parent.mkdir(parents=True, exist_ok=True)
probe.write_text('''
import GLib from 'gi://GLib';
export default function () {
    let timer = 0;
    let samples = [];
    const id = global.window_manager.connect('size-change', (_wm, actor) => {
        if (actor.meta_window.get_title() !== 'Bingux Settings') return;
        if (timer) GLib.source_remove(timer);
        samples = [];
        timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 16, () => {
            samples.push({scale: actor.scale_x, transition: !!actor.get_transition('scale-x')});
            GLib.file_set_contents(REPORT, JSON.stringify(samples));
            if (samples.length < 24) return GLib.SOURCE_CONTINUE;
            timer = 0;
            return GLib.SOURCE_REMOVE;
        });
    });
    return () => { global.window_manager.disconnect(id); if (timer) GLib.source_remove(timer); };
}
'''.replace('REPORT', json.dumps(str(report))))
try:
    subprocess.run(['gnoblinctl', 'script', 'reload'], check=True)
    for direction in ('maximise', 'restore'):
        report.unlink(missing_ok=True)
        subprocess.run([qs, 'ipc', '-p', str(qml), 'call', 'settings', 'maximise'], check=True)
        deadline = time.monotonic() + 3
        samples = []
        while time.monotonic() < deadline:
            try:
                samples = json.loads(report.read_text())
            except (FileNotFoundError, json.JSONDecodeError):
                pass
            if len(samples) >= 24:
                break
            time.sleep(.025)
        animated = sum(s['transition'] and abs(s['scale'] - 1) > .001 for s in samples)
        assert animated >= 5, (direction, samples)
        assert abs(samples[-1]['scale'] - 1) < .001, (direction, samples[-1])
        print(f'PASS {direction}: {animated} native animation samples')
finally:
    probe.unlink(missing_ok=True)
    subprocess.run(['gnoblinctl', 'script', 'reload'], check=True)
