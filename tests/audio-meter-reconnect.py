#!/usr/bin/env python3
"""Run with a meter binary and active audio playback on PipeWire.

Disconnect only a monitor owned by this test process, then check recovery.
The application's playback stream is never modified.
"""

import json
import subprocess
import time
import sys

p = subprocess.Popen([sys.argv[1]], stdout=subprocess.DEVNULL)


def streams():
    result = subprocess.run(["pactl", "-f", "json", "list", "source-outputs"], capture_output=True)
    if result.returncode:
        return []
    data = json.loads(result.stdout)
    return [s for s in data if s["properties"].get("application.process.id") == str(p.pid)]


try:
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        found = streams()
        if found:
            break
        time.sleep(0.1)
    assert found, "no monitor created"
    target = found[-1]
    old = target["index"]
    node = target["properties"]["object.id"]
    subprocess.run(["pw-cli", "destroy", node], check=True, capture_output=True)
    for _ in range(40):
        live = streams()
        if any(
            s["index"] != old and s["properties"].get("target.object") == target["properties"].get("target.object")
            for s in live
        ):
            print("PASS monitor recovered after server-side disconnection")
            break
        time.sleep(0.1)
    else:
        raise AssertionError("monitor did not reconnect")
finally:
    p.terminate()
    p.wait(timeout=5)
