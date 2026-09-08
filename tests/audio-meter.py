#!/usr/bin/env python3
"""Measure a silent/tone/silent stream on an owned null sink; never play to speakers."""
import argparse
import json
import math
import os
from pathlib import Path
import queue
import struct
import subprocess
import tempfile
import threading
import time
import uuid
import wave

parser = argparse.ArgumentParser()
parser.add_argument('meter', type=Path)
args = parser.parse_args()

def pactl(*arguments):
    return subprocess.check_output(['pactl', *arguments], text=True).strip()

name = 'bingux_meter_test_' + uuid.uuid4().hex[:8]
previous = pactl('get-default-sink')
module = pactl('load-module', 'module-null-sink', 'sink_name=' + name)
processes = []
try:
    assert pactl('get-default-sink') == previous, 'Test sink changed the default output'
    with tempfile.TemporaryDirectory(prefix='bingux-meter-') as directory:
        samples = []
        # A brief pulse must not trigger; a 600ms gap must not flicker.
        for seconds, amplitude in [(1, 0), (.1, .08), (1, 0), (1, .08), (.6, 0), (1, .08), (3, 0)]:
            samples.extend(struct.pack('<h', round(32767 * amplitude * math.sin(2 * math.pi * 440 * i / 48000)))
                           for i in range(round(seconds * 48000)))
        sound = Path(directory) / 'levels.wav'
        with wave.open(str(sound), 'wb') as output:
            output.setparams((1, 2, 48000, 0, 'NONE', 'not compressed'))
            output.writeframes(b''.join(samples))
        events = queue.Queue()
        meter = subprocess.Popen([str(args.meter.resolve())], stdout=subprocess.PIPE, text=True)
        processes.append(meter)
        def read():
            for line in meter.stdout:
                events.put((time.monotonic(), json.loads(line)))
        reader = threading.Thread(target=read, daemon=True)
        reader.start()
        time.sleep(.2)
        started = time.monotonic()
        playback = subprocess.Popen(['paplay', '--device=' + name, '--property=application.id=' + name, str(sound)])
        processes.append(playback)
        playback.wait(timeout=12)
        time.sleep(.3)
        records = []
        active = False
        while not events.empty():
            timestamp, streams = events.get_nowait()
            present = any(stream['properties'].get('application.id') == name for stream in streams)
            if present != active:
                active = present
                records.append((round(timestamp - started, 3), active))
        assert len(records) == 2, records
        assert 2.3 <= records[0][0] <= 3.1 and records[0][1], records
        assert 5.8 <= records[1][0] <= 6.7 and not records[1][1], records
        print('PASS: silence and short pulse ignored; delayed attack, gap hold and delayed release:', records)
finally:
    for process in reversed(processes):
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=3)
    pactl('unload-module', module)
    assert pactl('get-default-sink') == previous, 'Default output changed during the test'
