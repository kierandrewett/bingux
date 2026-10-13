#!/usr/bin/env python3
"""Exercise statusd's optional pactl capture feed without a desktop session."""

import json
import os
import signal
import socket
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES = '[{"index":1,"name":"mic","description":"Virtual microphone","mute":false}]'
OUTPUTS = '[{"source":1,"corked":false,"mute":false,"properties":{"application.name":"Recorder","application.id":"org.example.Recorder"}}]'


def wait_for(path, process, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            return
        if process.poll() is not None:
            raise AssertionError(f"statusd exited early: {process.returncode}")
        time.sleep(0.02)
    raise AssertionError(f"timed out waiting for {path}")


def receive(reader, predicate, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        line = reader.readline()
        if not line:
            raise AssertionError("metrics socket closed")
        record = json.loads(line)
        if predicate(record):
            return record
    raise AssertionError("timed out waiting for matching metrics record")


def process_exists(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def write_pactl(path, root):
    path.write_text(
        f"""#!/usr/bin/env python3
import os, pathlib, signal, sys, time
root = pathlib.Path({str(root)!r})
mode = (root / "mode").read_text()
if sys.argv[1:] == ["subscribe"]:
    (root / "subscriber.pid").write_text(str(os.getpid()))
    attempts = root / "subscribe-attempts"
    attempt = int(attempts.read_text()) + 1 if attempts.exists() else 1
    attempts.write_text(str(attempt))
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    if mode == "reconnect" and attempt == 1:
        time.sleep(.1); sys.exit(0)
    if mode == "oversized-subscribe":
        print("x" * 9000, flush=True)
    while True:
        event = root / "event"
        if event.exists():
            event.unlink()
            print("Event 'change' on source-output #1", flush=True)
        time.sleep(.01)
elif sys.argv[1:3] == ["-f", "json"] and sys.argv[3:5] == ["list", "sources"]:
    if mode == "oversized-json":
        while True:
            sys.stdout.write("x" * 4096); sys.stdout.flush()
    print({SOURCES!r})
elif sys.argv[1:3] == ["-f", "json"] and sys.argv[3:5] == ["list", "source-outputs"]:
    if (root / "state").read_text() == "initial": print({OUTPUTS!r})
    else: print("[]")
else: sys.exit(2)
"""
    )
    path.chmod(0o700)


def run_case(binary, mode, expect_available, kill_parent=False):
    with tempfile.TemporaryDirectory(prefix="bingux-statusd-microphone-") as temporary:
        root = Path(temporary)
        pactl = root / "pactl"
        subscriber_pid = root / "subscriber.pid"
        (root / "state").write_text("initial")
        (root / "mode").write_text(mode)
        write_pactl(pactl, root)
        environment = os.environ | {
            "XDG_RUNTIME_DIR": str(root),
            "PATH": f"{root}:{os.environ['PATH']}",
            "DBUS_SESSION_BUS_ADDRESS": f"unix:path={root / 'no-bus'}",
        }
        daemon = subprocess.Popen(
            [str(binary)],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            socket_path = root / "bingux" / "metrics-v1.sock"
            wait_for(socket_path, daemon)
            client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            client.settimeout(5)
            client.connect(str(socket_path))
            with client, client.makefile("r") as reader:
                record = receive(
                    reader,
                    lambda value: value.get("microphoneCapturesAvailable") is expect_available,
                )
                if expect_available:
                    expected = [
                        {
                            "app": "Recorder",
                            "appId": "org.example.Recorder",
                            "device": "Virtual microphone",
                            "muted": False,
                        }
                    ]
                    if record.get("microphoneCaptures") != expected:
                        raise AssertionError(record)
                    wait_for(subscriber_pid, daemon)
                    (root / "state").write_text("empty")
                    (root / "event").touch()
                    empty = receive(
                        reader,
                        lambda value: (
                            value.get("microphoneCapturesAvailable") is True and value.get("microphoneCaptures") == []
                        ),
                    )
                    if empty.get("microphoneAvailable") is not False or empty.get("microphoneInUse") is not False:
                        raise AssertionError("capture feed overwrote Gnoblin privacy fields")
                    if mode == "reconnect":
                        deadline = time.monotonic() + 5
                        while (
                            not (root / "subscribe-attempts").exists()
                            or (root / "subscribe-attempts").read_text() != "2"
                        ) and time.monotonic() < deadline:
                            time.sleep(0.02)
                        if (root / "subscribe-attempts").read_text() != "2":
                            raise AssertionError("pactl subscribe did not reconnect")
            if not subscriber_pid.exists():
                return
            pid = int(subscriber_pid.read_text())
        finally:
            if kill_parent:
                daemon.kill()
            else:
                daemon.terminate()
            try:
                daemon.wait(timeout=3)
            except subprocess.TimeoutExpired:
                daemon.kill()
                daemon.wait(timeout=3)
            stderr = daemon.stderr.read() if daemon.stderr else ""
        time.sleep(0.1)
        if subscriber_pid.exists() and process_exists(pid):
            raise AssertionError(f"pactl subscribe child {pid} survived statusd")
        expected_exit = -signal.SIGKILL if kill_parent else -signal.SIGTERM
        if daemon.returncode not in (expected_exit, 0):
            raise AssertionError(stderr)


def main():
    binary = Path(
        os.environ.get(
            "BINGUX_STATUSD_BIN",
            ROOT / "packages/bingux-statusd/target/debug/bingux-statusd",
        )
    )
    if not binary.is_file():
        raise SystemExit(
            "build with cargo +1.95.0 build --locked --offline --manifest-path packages/bingux-statusd/Cargo.toml"
        )
    run_case(binary, "initial", True, kill_parent=True)
    run_case(binary, "reconnect", True)
    run_case(binary, "oversized-subscribe", False)
    run_case(binary, "oversized-json", False)
    print("STATUSD_MICROPHONE_PASSED")


if __name__ == "__main__":
    main()
