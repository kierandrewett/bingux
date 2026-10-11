#!/usr/bin/env python3
"""Check that the microphone watcher's pactl child dies with its watcher.

Uses a temporary pactl replacement and an explicit watcher path. It does not
connect to PulseAudio or PipeWire.
"""

import argparse
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time


parser = argparse.ArgumentParser()
parser.add_argument("watcher", type=Path)
args = parser.parse_args()


def wait_for(path, predicate, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            value = int(path.read_text())
            if predicate(value):
                return value
        time.sleep(0.02)
    raise AssertionError(f"Timed out waiting for {path}")


def exited(pid, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        try:
            if Path(f"/proc/{pid}/stat").read_text().split()[2] == "Z":
                return True
        except FileNotFoundError:
            return True
        time.sleep(0.02)
    return False


with tempfile.TemporaryDirectory(prefix="bingux-microphone-lifecycle-") as directory:
    fixture = Path(directory)
    child_pid = fixture / "pactl.pid"
    fake_pactl = fixture / "pactl"
    fake_pactl.write_text(
        "#!/usr/bin/env python3\n"
        "import os, pathlib, signal, sys, time\n"
        "if sys.argv[1:] != ['subscribe']:\n"
        "    print('[]')\n"
        "    raise SystemExit(0)\n"
        "pathlib.Path(os.environ['BINGUX_FAKE_PACTL_PID']).write_text(str(os.getpid()))\n"
        "while True: time.sleep(1)\n"
    )
    fake_pactl.chmod(0o755)
    for termination in (signal.SIGTERM, signal.SIGKILL):
        environment = os.environ | {
            "PATH": str(fixture) + os.pathsep + os.environ["PATH"],
            "BINGUX_FAKE_PACTL_PID": str(child_pid),
        }
        watcher = None
        pid = None
        try:
            watcher = subprocess.Popen(
                ["python3", str(args.watcher)], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
            )
            pid = wait_for(child_pid, lambda value: value > 1)
            watcher.send_signal(termination)
            watcher.wait(timeout=3)
            assert exited(pid), f"pactl child {pid} survived watcher {termination.name}"
        finally:
            if watcher and watcher.poll() is None:
                watcher.terminate()
                watcher.wait(timeout=3)
            if pid and not exited(pid, timeout=0):
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                assert exited(pid), f"could not clean up pactl child {pid}"
        child_pid.unlink(missing_ok=True)
        print(f"PASS: pactl child exited after watcher {termination.name}")
