#!/usr/bin/env python3
"""Exercise Gnoblin socket events through the real status daemon OSD socket."""

import json
import os
import socket
import subprocess
import tempfile
import threading
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EVENT_NAME = "gnoblin.osd.requested"
LOCK_EVENT = "gnoblin.session.lock-requested"
PRIVACY_EVENT = "gnoblin.privacy.changed"
METHODS = ["input.sources", "input.current_source", "privacy.state"]
OUTPUT_NAMES = ["DP-1", "DP-2"]


def send_record(connection, record):
    connection.sendall(json.dumps(record, separators=(",", ":")).encode() + b"\n")


def read_record(reader):
    line = reader.readline()
    if not line:
        raise AssertionError("socket closed before sending a record")
    return json.loads(line)


def compositor_peer(listener, stop, peer_state, errors):
    try:
        listener.settimeout(5)
        connection, _ = listener.accept()
        connection.settimeout(5)
        with connection:
            reader = connection.makefile("rb")
            send_record(
                connection,
                {
                    "event": "hello",
                    "api_major": 1,
                    "api_minor": 73,
                    "events": [EVENT_NAME, LOCK_EVENT, PRIVACY_EVENT],
                    "methods": METHODS,
                },
            )
            requests = [json.loads(reader.readline()) for _ in range(5)]
            peer_state["requests"] = requests
            send_record(
                connection,
                {
                    "event": "monitors",
                    "monitors": [{"id": "DP-1", "index": 2, "output_names": OUTPUT_NAMES}],
                },
            )
            send_record(connection, {"event": "subscribed", "events": [PRIVACY_EVENT, EVENT_NAME, LOCK_EVENT]})

            event = {
                "event": EVENT_NAME,
                "sequence": 101,
                "time": 123456,
                "monitor_id": "DP-1",
                "output_names": OUTPUT_NAMES,
                "icon": "audio-volume-high-symbolic",
                "label": "Headphones",
            }
            while not stop.is_set():
                send_record(connection, event)
                stop.wait(0.05)
    except Exception as error:  # surfaced in the test thread
        if not stop.is_set():
            errors.append(error)


def wait_for_path(path, process, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            return
        if process.poll() is not None:
            raise AssertionError(f"bingux-statusd exited early with status {process.returncode}")
        time.sleep(0.02)
    raise AssertionError(f"timed out waiting for {path}")


def main():
    binary = Path(os.environ.get("BINGUX_STATUSD_BIN", ROOT / "build/cargo/release/bingux-statusd"))
    if not binary.is_file():
        raise SystemExit(
            f"status daemon binary not found at {binary}; build it with `make daemons` or set BINGUX_STATUSD_BIN"
        )

    with tempfile.TemporaryDirectory(prefix="bingux-osd-event-") as temporary_directory:
        runtime_directory = Path(temporary_directory)
        gnoblin_directory = runtime_directory / "gnoblin"
        gnoblin_directory.mkdir(mode=0o700)
        compositor_path = gnoblin_directory / "compositor-v1.sock"
        compositor_listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        compositor_listener.bind(str(compositor_path))
        compositor_listener.listen(1)

        stop = threading.Event()
        peer_state = {}
        peer_errors = []
        peer = threading.Thread(
            target=compositor_peer,
            args=(compositor_listener, stop, peer_state, peer_errors),
            daemon=True,
        )
        peer.start()

        environment = os.environ.copy()
        environment.update(
            {
                "XDG_RUNTIME_DIR": str(runtime_directory),
                # The daemon records data usage; keep that out of the real home.
                "XDG_DATA_HOME": str(runtime_directory / "data"),
                "DBUS_SESSION_BUS_ADDRESS": f"unix:path={runtime_directory / 'no-session-bus'}",
            }
        )
        process = subprocess.Popen(
            [str(binary)],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        osd_path = runtime_directory / "bingux" / "osd-v2.sock"
        try:
            wait_for_path(osd_path, process)
            osd_client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            osd_client.settimeout(5)
            osd_client.connect(str(osd_path))
            with osd_client, osd_client.makefile("rb") as reader:
                record = read_record(reader)

            if peer_errors:
                raise AssertionError(f"fake compositor failed: {peer_errors[0]}")
            requests = peer_state.get("requests")
            operations = [request.get("op") for request in requests]
            if not requests or operations != ["events", "monitors", "api", "api", "api"]:
                raise AssertionError(f"unexpected compositor requests: {requests!r}")
            subscription = requests[0]
            if set(subscription.get("events", [])) != {EVENT_NAME, LOCK_EVENT, PRIVACY_EVENT}:
                raise AssertionError(f"unexpected event subscription: {subscription!r}")
            # Gnoblin keeps one API version per connection, the latest request's,
            # and counts a lock subscriber only while it is 1.21 or more (OSD
            # events: 1.27, pad help: 1.73). A later request at a lower version
            # silently ends both subscriptions, so every request must carry the
            # same one: the wire minor is the hello minor capped at 1.73.
            versions = {json.dumps(request.get("api_version"), sort_keys=True) for request in requests}
            if versions != {json.dumps({"major": 1, "minor": 73})}:
                raise AssertionError(f"requests use different API versions: {requests!r}")

            expected = {
                "protocolVersion": 2,
                "type": "osd",
                "monitorIndex": 2,
                "outputNames": OUTPUT_NAMES,
                "icon": "audio-volume-high-symbolic",
                "label": "Headphones",
                "level": -1.0,
                "maxLevel": -1.0,
            }
            if record != expected:
                raise AssertionError(f"unexpected OSD socket record:\n{record!r}\nexpected:\n{expected!r}")
        finally:
            stop.set()
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
            stderr = process.stderr.read().decode(errors="replace") if process.stderr else ""
            compositor_listener.close()
            peer.join(timeout=1)
            if peer_errors:
                raise AssertionError(f"fake compositor failed: {peer_errors[0]}")
            if process.returncode not in (-15, 0):
                raise AssertionError(f"bingux-statusd exited with {process.returncode}:\n{stderr}")

    print("OSD_EVENT_BRIDGE_PASSED")


if __name__ == "__main__":
    main()
