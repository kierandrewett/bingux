#!/usr/bin/env python3
"""Deliver capture IPC across a brief shell reload without double-toggling."""
import subprocess
import sys
import time

NOT_DISPATCHED = {"Not ready to accept queries yet.", "Function not found."}


def launch(command, run=subprocess.run, sleep=time.sleep):
    for attempt in range(12):
        result = run(command, capture_output=True, text=True, timeout=3)
        response = (result.stdout + result.stderr).strip()
        if response not in NOT_DISPATCHED:
            # A timeout/unknown result may have executed the toggle. Never
            # blindly repeat those, even when their exit status is nonzero.
            if response: print(response, file=sys.stderr)
            return result.returncode
        if attempt < 11: sleep(.1)
    print("Capture unavailable: shell is still reloading", file=sys.stderr)
    return 1


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("Usage: capture-launch.py qs ipc ... call capture open")
    try:
        raise SystemExit(launch(sys.argv[1:]))
    except (OSError, subprocess.TimeoutExpired) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
