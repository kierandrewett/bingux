#!/usr/bin/env python3
"""Send a Gnoblin pad-help event through the real status daemon to its pad-help socket."""

import json
import os
import socket
import subprocess
import tempfile
import threading
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PAD_EVENT = "gnoblin.input.pad-help-requested"
OSD_EVENT = "gnoblin.osd.requested"
LOCK_EVENT = "gnoblin.session.lock-requested"
PRIVACY_EVENT = "gnoblin.privacy.changed"
METHODS = ["input.sources", "input.current_source", "privacy.state"]
OUTPUT_NAMES = ["DP-1", "DP-2"]

FIXTURE_EVENT = {
    "event": PAD_EVENT,
    "sequence": 7,
    "time": 424242,
    "monitor_id": "DP-1",
    "output_names": OUTPUT_NAMES,
    "edition_mode": False,
    "device": {"id": "wacom-pad-0", "name": "Intuos Pro Pad", "device_type": "pad"},
    "buttons": [
        {"button": 0, "label": "Undo", "mode_group": 0},
        {"button": 1},
    ],
    "mode_groups": [{"group": 0, "mode_count": 2, "switch_buttons": [1]}],
    "features": [
        {"type": "ring", "index": 0, "clockwise_label": "Zoom in", "counterclockwise_label": "Zoom out"},
        {"type": "strip", "index": 0, "up_label": "Scroll up"},
    ],
}

EXPECTED_RECORD = {
    "protocolVersion": 1,
    "type": "pad-help",
    "monitorIndex": 2,
    "outputNames": OUTPUT_NAMES,
    "editionMode": False,
    "device": {"id": "wacom-pad-0", "name": "Intuos Pro Pad", "deviceType": "pad"},
    "buttons": [
        {"button": 0, "label": "Undo", "modeGroup": 0},
        {"button": 1},
    ],
    "modeGroups": [{"group": 0, "modeCount": 2, "switchButtons": [1]}],
    "features": [
        {"type": "ring", "index": 0, "positiveLabel": "Zoom in", "negativeLabel": "Zoom out"},
        {"type": "strip", "index": 0, "positiveLabel": "Scroll up"},
    ],
}


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
                    "events": [PAD_EVENT, OSD_EVENT, LOCK_EVENT, PRIVACY_EVENT],
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
            subscription = requests[0]
            send_record(connection, {"event": "subscribed", "events": subscription.get("events", [])})
            while not stop.is_set():
                send_record(connection, FIXTURE_EVENT)
                stop.wait(0.05)
            # Closing the compositor connection must clear the shell overlay.
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

    with tempfile.TemporaryDirectory(prefix="bingux-pad-help-event-") as temporary_directory:
        runtime_directory = Path(temporary_directory)
        gnoblin_directory = runtime_directory / "gnoblin"
        gnoblin_directory.mkdir(mode=0o700)
        compositor_listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        compositor_listener.bind(str(gnoblin_directory / "compositor-v1.sock"))
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
        pad_path = runtime_directory / "bingux" / "pad-help-v1.sock"
        try:
            wait_for_path(pad_path, process)
            pad_client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            pad_client.settimeout(5)
            pad_client.connect(str(pad_path))
            with pad_client, pad_client.makefile("rb") as reader:
                record = read_record(reader)
                if record != EXPECTED_RECORD:
                    raise AssertionError(f"unexpected pad-help record:\n{record!r}\nexpected:\n{EXPECTED_RECORD!r}")

                # Dropping the compositor connection must send a clear record.
                stop.set()
                peer.join(timeout=2)
                compositor_listener.close()
                deadline = time.monotonic() + 5
                clear = None
                while time.monotonic() < deadline:
                    record = read_record(reader)
                    if record.get("type") == "pad-help-clear":
                        clear = record
                        break
                if clear != {"protocolVersion": 1, "type": "pad-help-clear"}:
                    raise AssertionError(f"no clear record after the bridge dropped: {clear!r}")

            if peer_errors:
                raise AssertionError(f"fake compositor failed: {peer_errors[0]}")
            subscription = peer_state["requests"][0]
            if PAD_EVENT not in subscription.get("events", []):
                raise AssertionError(f"pad-help event was not subscribed: {subscription!r}")
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
            if process.returncode not in (-15, 0):
                raise AssertionError(f"bingux-statusd exited with {process.returncode}:\n{stderr}")

    print("PAD_HELP_EVENT_BRIDGE_PASSED")


if __name__ == "__main__":
    main()
