#!/usr/bin/env python3
"""Controlled capture transport for the private Customise UI action test.

No screen capture or encoder is started. Normal capture/stop commands produce
recording, countdown and saving events consumed by the real CaptureTool.
"""
import json
import os
import sys
import threading
import time

state = "idle"

def emit(event, **fields):
    print(json.dumps({"event": event, **fields}), flush=True)

def finish():
    global state
    state = "saved"
    emit("saved", kind="recording", path="/tmp/customise-test-recording.webm", notified=True)

emit("ready", window=True)
for line in sys.stdin:
    command = json.loads(line)
    with open(os.environ["BINGUX_CAPTURE_ACTION_REPORT"], "a") as report:
        report.write(json.dumps(command) + "\n")
    if command["command"] == "capture":
        state = "countdown" if command["delay"] else "recording"
        if state == "countdown":
            emit(state, seconds=command["delay"])
        else:
            emit(state, started=time.time() - 125)
    elif command["command"] == "stop":
        if state == "recording":
            state = "finalizing"
            emit(state)
            timer = threading.Timer(3, finish)
            timer.daemon = True
            timer.start()
        elif state == "countdown":
            state = "idle"
            emit("cancelled")
    elif command["command"] == "cancel":
        state = "idle"
        emit("cancelled")
