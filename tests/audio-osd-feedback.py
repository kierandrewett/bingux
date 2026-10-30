#!/usr/bin/env python3
"""Drive the real status daemon with a fake pactl and check the volume OSD records.

Gnoblin raises no OSD event for volume or microphone keys, so bingux-statusd
watches the default output and input itself. The fake `pactl` serves scripted
state and prints a subscribe event whenever that state changes. The test
checks what a shell client reads from osd-v2.sock.
"""

import json
import os
import socket
import stat
import subprocess
import tempfile
import threading
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MONITORS = [
    {"id": "DP-1", "index": 0, "output_names": ["DP-1"]},
    {"id": "HDMI-1", "index": 1, "output_names": ["HDMI-1", "HDMI-2"]},
]

FAKE_PACTL = r'''#!/usr/bin/env python3
import json, os, sys, time

state_path = os.environ["FAKE_PACTL_STATE"]
log_path = os.environ["FAKE_PACTL_LOG"]
args = sys.argv[1:]
with open(log_path, "a") as log:
    log.write(" ".join(args) + "\n")


def load():
    with open(state_path) as handle:
        return json.load(handle)


if args == ["subscribe"]:
    last = os.stat(state_path).st_mtime_ns
    while True:
        current = os.stat(state_path).st_mtime_ns
        if current != last:
            last = current
            print("Event 'change' on sink #7", flush=True)
            print("Event 'change' on client #3", flush=True)
        time.sleep(0.02)
elif args == ["-f", "json", "info"]:
    state = load()
    print(json.dumps({"default_sink_name": state["default_sink"], "default_source_name": state["default_source"]}))
elif args == ["-f", "json", "list", "sinks"]:
    print(json.dumps(load()["sinks"]))
elif args == ["-f", "json", "list", "sources"]:
    print(json.dumps(load()["sources"]))
elif args == ["-f", "json", "list", "source-outputs"]:
    print("[]")
else:
    sys.exit("unexpected pactl arguments: %r" % args)
'''


def device(name, description, percent, muted=False):
    raw = round(65536 * percent / 100)
    return {
        "name": name,
        "description": description,
        "mute": muted,
        "volume": {"front-left": {"value": raw}, "front-right": {"value": raw}},
    }


def write_state(path, state):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(state))
    os.replace(temporary, path)


def send_record(connection, record):
    connection.sendall(json.dumps(record, separators=(",", ":")).encode() + b"\n")


def compositor_peer(listener, stop, errors):
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
                    "events": ["gnoblin.osd.requested", "gnoblin.session.lock-requested", "gnoblin.privacy.changed"],
                    "methods": ["input.sources", "input.current_source", "privacy.state"],
                },
            )
            for _ in range(5):
                reader.readline()
            send_record(connection, {"event": "monitors", "monitors": MONITORS})
            stop.wait()
    except Exception as error:  # surfaced in the test thread
        if not stop.is_set():
            errors.append(error)


class Records:
    def __init__(self, connection):
        self.connection = connection
        self.pending = b""

    def next(self, timeout=3):
        deadline = time.monotonic() + timeout
        while b"\n" not in self.pending:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise AssertionError("timed out waiting for an OSD record")
            self.connection.settimeout(remaining)
            try:
                chunk = self.connection.recv(4096)
            except socket.timeout:
                raise AssertionError("timed out waiting for an OSD record") from None
            if not chunk:
                raise AssertionError("OSD socket closed")
            self.pending += chunk
        line, self.pending = self.pending.split(b"\n", 1)
        return json.loads(line)

    def expect_silence(self, seconds):
        try:
            record = self.next(seconds)
        except AssertionError as error:
            if "timed out" in str(error):
                return
            raise
        raise AssertionError(f"unexpected OSD record: {record!r}")


def expect_pair(records, icon, label, level):
    """One record per monitor, in monitor order, each with that monitor's connectors."""
    received = [records.next(), records.next()]
    for record, monitor in zip(received, MONITORS):
        expected_keys = {
            "protocolVersion": 2,
            "type": "osd",
            "monitorIndex": monitor["index"],
            "outputNames": monitor["output_names"],
            "icon": icon,
            "label": label,
            "maxLevel": 1.0,
        }
        for key, value in expected_keys.items():
            if record.get(key) != value:
                raise AssertionError(f"{key}: {record.get(key)!r} != {value!r} in {record!r}")
        if abs(record["level"] - level) > 0.001:
            raise AssertionError(f"level {record['level']!r} != {level!r} in {record!r}")


def wait_until(predicate, process, what, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        if process.poll() is not None:
            raise AssertionError(f"bingux-statusd exited early with status {process.returncode}")
        time.sleep(0.02)
    raise AssertionError(f"timed out waiting for {what}")


def main():
    binary = Path(os.environ.get("BINGUX_STATUSD_BIN", ROOT / "build/cargo/release/bingux-statusd"))
    if not binary.is_file():
        raise SystemExit(
            f"status daemon binary not found at {binary}; build it with `make daemons` or set BINGUX_STATUSD_BIN"
        )

    with tempfile.TemporaryDirectory(prefix="bingux-audio-osd-") as temporary_directory:
        runtime_directory = Path(temporary_directory)
        fake_bin = runtime_directory / "bin"
        fake_bin.mkdir()
        pactl = fake_bin / "pactl"
        pactl.write_text(FAKE_PACTL)
        pactl.chmod(pactl.stat().st_mode | stat.S_IXUSR)
        state_path = runtime_directory / "audio.json"
        log_path = runtime_directory / "pactl.log"
        log_path.touch()

        speakers = "alsa_output.speakers"
        headset = "alsa_output.headset"
        microphone = "alsa_input.microphone"
        state = {
            "default_sink": speakers,
            "default_source": microphone,
            "sinks": [device(speakers, "Speakers", 30), device(headset, "Headset", 90)],
            "sources": [device(microphone, "Desk Microphone", 80)],
        }
        write_state(state_path, state)

        gnoblin_directory = runtime_directory / "gnoblin"
        gnoblin_directory.mkdir(mode=0o700)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(str(gnoblin_directory / "compositor-v1.sock"))
        listener.listen(1)
        stop = threading.Event()
        peer_errors = []
        peer = threading.Thread(target=compositor_peer, args=(listener, stop, peer_errors), daemon=True)
        peer.start()

        environment = os.environ.copy()
        environment.update(
            {
                "XDG_RUNTIME_DIR": str(runtime_directory),
                "PATH": f"{fake_bin}:{environment['PATH']}",
                "FAKE_PACTL_STATE": str(state_path),
                "FAKE_PACTL_LOG": str(log_path),
                "DBUS_SESSION_BUS_ADDRESS": f"unix:path={runtime_directory / 'no-session-bus'}",
            }
        )
        process = subprocess.Popen(
            [str(binary)], cwd=ROOT, env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE
        )
        osd_path = runtime_directory / "bingux" / "osd-v2.sock"
        try:
            wait_until(osd_path.exists, process, "the OSD socket")
            wait_until(
                lambda: log_path.read_text().splitlines().count("-f json list sources") >= 2
                and "subscribe" in log_path.read_text().splitlines(),
                process,
                "the first audio snapshot",
            )
            client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            client.connect(str(osd_path))
            with client:
                records = Records(client)
                # The compositor's monitor list and the first snapshot are in by now.
                time.sleep(0.5)
                records.expect_silence(0.5)

                # 30% to 45%: a volume key step. 0.45 sits between a third and two thirds.
                state["sinks"][0] = device(speakers, "Speakers", 45)
                write_state(state_path, state)
                expect_pair(records, "audio-volume-medium-symbolic", "Speakers", 0.45)

                # A change under the rounding step is not a user change.
                state["sinks"][0] = device(speakers, "Speakers", 45.2)
                write_state(state_path, state)
                records.expect_silence(0.6)

                state["sinks"][0] = device(speakers, "Speakers", 45, muted=True)
                write_state(state_path, state)
                expect_pair(records, "audio-volume-muted-symbolic", "Speakers", 0.0)

                state["sinks"][0] = device(speakers, "Speakers", 100)
                write_state(state_path, state)
                expect_pair(records, "audio-volume-high-symbolic", "Speakers", 1.0)

                state["sinks"][0] = device(speakers, "Speakers", 120)
                write_state(state_path, state)
                expect_pair(records, "audio-volume-overamplified-symbolic", "Speakers", 1.2)

                state["sources"][0] = device(microphone, "Desk Microphone", 80, muted=True)
                write_state(state_path, state)
                expect_pair(records, "microphone-sensitivity-muted-symbolic", "Desk Microphone", 0.0)

                state["sources"][0] = device(microphone, "Desk Microphone", 80)
                write_state(state_path, state)
                expect_pair(records, "microphone-sensitivity-high-symbolic", "Desk Microphone", 0.8)

                # Switching the default output changes the level without a user step.
                state["default_sink"] = headset
                write_state(state_path, state)
                records.expect_silence(0.6)

                state["sinks"][1] = device(headset, "Headset", 10)
                write_state(state_path, state)
                expect_pair(records, "audio-volume-low-symbolic", "Headset", 0.10)

            if peer_errors:
                raise AssertionError(f"fake compositor failed: {peer_errors[0]}")
        finally:
            stop.set()
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
            stderr = process.stderr.read().decode(errors="replace") if process.stderr else ""
            listener.close()
            peer.join(timeout=1)
            if process.returncode not in (-15, 0):
                raise AssertionError(f"bingux-statusd exited with {process.returncode}:\n{stderr}")

    print("AUDIO_OSD_FEEDBACK_PASSED")


if __name__ == "__main__":
    main()
