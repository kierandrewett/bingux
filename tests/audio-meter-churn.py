#!/usr/bin/env python3
"""Run with a meter binary on PipeWire: short-lived playback streams must leave no monitor behind.

A sink input that vanishes while the meter's record stream for it is still being created used
to leave that stream on the server until the meter exited (about 200 of them piled up over a
session). This starts the meter, plays and kills COUNT silent playback streams that live 50 to
800 ms (default 100, more than the 64 orphans one sweep can name), then waits up to SETTLE
seconds for the meter to end every monitor whose sink input is gone. The test fails if one is
left, and prints how many were left at each check.

    python3 tests/audio-meter-churn.py BINARY [COUNT] [SETTLE]

The streams are silence from /dev/zero played to the default sink; no stream of another
application is touched. The meter is started and stopped by this script.
"""

import json
import subprocess
import sys
import time

binary = sys.argv[1]
count = int(sys.argv[2]) if len(sys.argv) > 2 else 100
settle = float(sys.argv[3]) if len(sys.argv) > 3 else 60.0
DELAYS = (0.05, 0.1, 0.2, 0.35, 0.5, 0.8)

meter = subprocess.Popen([binary], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def pactl(kind):
    result = subprocess.run(["pactl", "-f", "json", "list", kind], capture_output=True, check=True)
    return json.loads(result.stdout)


def stale_monitors():
    """Monitors this meter owns whose target sink input no longer exists."""
    live = {str(item["index"]) for item in pactl("sink-inputs")}
    mine = [
        output
        for output in pactl("source-outputs")
        if output["properties"].get("application.process.id") == str(meter.pid)
    ]
    # A second read, so a stream that ended between the two calls is not counted.
    live |= {str(item["index"]) for item in pactl("sink-inputs")}
    return [o["properties"].get("target.object") for o in mine if o["properties"].get("target.object") not in live]


try:
    time.sleep(2)
    for number in range(count):
        playback = subprocess.Popen(
            [
                "pacat",
                "--playback",
                "--rate=44100",
                "--channels=2",
                "--format=s16le",
                "--client-name=meter-churn",
                "/dev/zero",
            ],
            stderr=subprocess.DEVNULL,
        )
        time.sleep(DELAYS[number % len(DELAYS)])
        playback.kill()
        playback.wait()
    deadline = time.monotonic() + settle
    left = stale_monitors()
    print(f"after {count} streams: {len(left)} monitors without a sink input", flush=True)
    while left and time.monotonic() < deadline:
        time.sleep(5)
        left = stale_monitors()
        print(f"  {len(left)} left", flush=True)
    assert not left, f"{len(left)} monitors still run for sink inputs that are gone: {left}"
    print("ok: no monitor outlives its sink input")
finally:
    meter.terminate()
    meter.wait()
