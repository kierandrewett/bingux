#!/usr/bin/env python3
"""Run the tablet-pad help overlay in a headless Sway session with two outputs.

tests/pad-help.qml drives the production PadHelpState and PadHelpSurface in
two modes. "ingest" feeds records straight into the state while a silent
bridge holds the socket open. "socket" connects to a fake bingux-statusd on
pad-help-v1.sock: it sends the first request on connection, answers each
PAD_HELP_STEP the QML prints with the next record, drops the connection on
await-disconnect, and sends the next request when the client reconnects.
Headless Sway starts with one output, so the runner creates the second over
IPC.
"""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parent.parent
QS = os.environ.get("QS_TEST_BIN", "qs")
MODES = (
    ("ingest", {"PAD_HELP_MODE": "ingest"}),
    ("socket", {"PAD_HELP_MODE": "socket"}),
)
FIXTURE_FILES = ("Theme", "PanelOutline", "PadHelpState", "PadHelpSurface")
SOCKET_NAME = "pad-help-v1.sock"


def pad_record(name, output, monitor_index):
    return {
        "protocolVersion": 1,
        "type": "pad-help",
        "monitorIndex": monitor_index,
        "outputNames": [output],
        "editionMode": False,
        "device": {"id": "pad-1", "name": name, "deviceType": "pad"},
        "buttons": [
            {"button": 0},
            {"button": 1, "label": "Toggle overlay", "modeGroup": 0},
        ],
        "modeGroups": [{"group": 0, "modeCount": 2, "switchButtons": [1]}],
        "features": [{"type": "ring", "index": 0}],
    }


FIRST_REQUEST = pad_record("Fixture pad", "HEADLESS-2", 1)
RECONNECT_REQUEST = pad_record("Reconnected pad", "HEADLESS-2", 1)
REPLACEMENT_REQUEST = pad_record("Replacement pad", "HEADLESS-1", 0)
ESCAPE_REQUEST = pad_record("Escape pad", "HEADLESS-2", 1)
EXPIRY_REQUEST = pad_record("Expiry pad", "HEADLESS-1", 0)
CLEAR_RECORD = {"protocolVersion": 1, "type": "pad-help-clear"}


def encode(record):
    return (json.dumps(record, separators=(",", ":")) + "\n").encode()


def wait_for(predicate, timeout=10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.1)
    return False


def seat_capabilities(environment):
    """Returns the wl_seat capability names sway reports for seat0."""
    result = subprocess.run(
        ["swaymsg", "-t", "get_seats"],
        env=environment,
        capture_output=True,
        text=True,
        check=True,
    )
    mask = max((entry.get("capabilities", 0) for entry in json.loads(result.stdout)), default=0)
    return {name for bit, name in ((1, "pointer"), (2, "keyboard"), (4, "touch")) if mask & bit}


def outputs(environment):
    result = subprocess.run(
        ["swaymsg", "-t", "get_outputs"],
        env=environment,
        capture_output=True,
        text=True,
        check=True,
    )
    return {entry["name"] for entry in json.loads(result.stdout)}


class FakeStatusd:
    """Serves pad-help-v1.sock. The accept thread owns new connections; the
    main thread sends step responses to the newest one."""

    def __init__(self, path, scripted):
        self.scripted = scripted
        self.lock = threading.Lock()
        self.connections = []
        self.errors = []
        self.stop = threading.Event()
        self.listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.listener.bind(str(path))
        self.listener.listen(4)
        self.thread = threading.Thread(target=self.accept_loop, daemon=True)

    def start(self):
        self.thread.start()

    def accept_loop(self):
        self.listener.settimeout(0.1)
        while not self.stop.is_set():
            try:
                connection, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            connection.settimeout(5)
            with self.lock:
                self.connections.append(connection)
                first = len(self.connections) == 1
            if not self.scripted:
                continue
            try:
                connection.sendall(encode(FIRST_REQUEST if first else RECONNECT_REQUEST))
            except OSError as error:
                self.errors.append(error)

    def latest(self):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            with self.lock:
                if self.connections:
                    return self.connections[-1]
            time.sleep(0.02)
        raise AssertionError("the pad help client never connected")

    def send_latest(self, record):
        try:
            with self.lock:
                count = len(self.connections)
            print(f"fake statusd: sending {record.get('type', 'pad-help')} {record.get('device', {}).get('name', '')} to connection {count}", flush=True)
            self.latest().sendall(encode(record))
        except OSError as error:
            self.errors.append(error)

    def drop_latest(self):
        connection = self.latest()
        try:
            connection.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        connection.close()

    def close(self):
        self.stop.set()
        self.thread.join(timeout=2)
        self.listener.close()
        with self.lock:
            for connection in self.connections:
                connection.close()


STEP_ACTIONS = {
    "await-replace": lambda server: server.send_latest(REPLACEMENT_REQUEST),
    "await-disconnect": lambda server: server.drop_latest(),
    "await-clear": lambda server: server.send_latest(CLEAR_RECORD),
    "await-escape": lambda server: server.send_latest(ESCAPE_REQUEST),
    "await-expiry": lambda server: server.send_latest(EXPIRY_REQUEST),
}


def press_escape(environment):
    """Sends a real Escape key through the headless compositor.

    The QML item reports activeFocus a moment before the compositor has sent
    wl_keyboard.enter to the layer surface, and nothing observable marks that
    point. A key sent earlier is dropped, so the test waits one second first.
    """
    time.sleep(1.0)
    subprocess.run(["wtype", "-k", "Escape"], env=environment, check=True, capture_output=True)


def drive(client, server, environment):
    """Reads the QML output and answers each step marker as it arrives."""
    lines = []
    unknown = []
    for line in client.stdout:
        lines.append(line)
        if "PAD_HELP_STEP " not in line:
            continue
        step = line.split("PAD_HELP_STEP ", 1)[1].strip()
        if step == "press-escape":
            press_escape(environment)
            continue
        action = STEP_ACTIONS.get(step)
        if action is None:
            unknown.append(step)
            continue
        action(server)
    return "".join(lines), unknown


def run_mode(label, extra_env):
    home = Path(tempfile.mkdtemp(prefix="bingux-pad-help-"))
    runtime = home / "run"
    runtime.mkdir(mode=0o700)
    bridge_directory = runtime / "bingux"
    bridge_directory.mkdir(mode=0o700)
    fixture = home / "fixture"
    fixture.mkdir()
    for name in FIXTURE_FILES:
        shutil.copy2(ROOT / f"shell/bingux/{name}.qml", fixture)
    shutil.copy2(ROOT / "tests/pad-help.qml", fixture / "shell.qml")
    (fixture / "qmldir").write_text(
        "singleton Theme 1.0 Theme.qml\n"
        + "PanelOutline 1.0 PanelOutline.qml\n"
        + "PadHelpState 1.0 PadHelpState.qml\n"
        + "PadHelpSurface 1.0 PadHelpSurface.qml\n"
    )
    sway_config = home / "sway.conf"
    sway_config.write_text("output HEADLESS-1 resolution 1280x800\n")
    base = os.environ | {
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(runtime),
        "WLR_BACKENDS": "headless",
        "WLR_RENDERER": "gles2",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
    }
    base.pop("DISPLAY", None)
    base.pop("WAYLAND_DISPLAY", None)
    sway = subprocess.Popen(
        ["sway", "-c", str(sway_config)],
        env=base,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    server = None
    client = None
    keyboard_holder = None
    try:
        if not wait_for(lambda: any(not p.name.endswith(".lock") for p in runtime.glob("wayland-*"))):
            print("headless sway did not start")
            return False
        wayland = sorted(p.name for p in runtime.glob("wayland-*") if not p.name.endswith(".lock"))[0]
        ipc = sorted(runtime.glob("sway-ipc.*.sock"))
        if not ipc:
            print("headless sway IPC socket did not appear")
            return False
        environment = base | {
            "WAYLAND_DISPLAY": wayland,
            "SWAYSOCK": str(ipc[0]),
            "QT_QPA_PLATFORM": "wayland",
        } | extra_env
        subprocess.run(["swaymsg", "create_output"], env=environment, check=True, capture_output=True)
        if not wait_for(lambda: {"HEADLESS-1", "HEADLESS-2"} <= outputs(environment)):
            print("second headless output did not appear")
            return False
        subprocess.run(
            ["swaymsg", "output", "HEADLESS-2", "resolution", "1280x800"],
            env=environment,
            check=True,
            capture_output=True,
        )

        # Headless sway with no libinput devices gives the seat no keyboard,
        # so a layer surface would never receive keyboard focus. A wtype
        # process that sleeps before its only key keeps a virtual keyboard
        # attached to the seat for the whole run.
        keyboard_holder = subprocess.Popen(
            ["wtype", "-s", "120000", "-k", "F24"],
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if not wait_for(lambda: "keyboard" in seat_capabilities(environment)):
            print("the virtual keyboard did not attach to the seat")
            return False

        server = FakeStatusd(bridge_directory / SOCKET_NAME, scripted=label == "socket")
        server.start()
        client = subprocess.Popen(
            [QS, "-p", str(fixture), "--no-color"],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        watchdog = threading.Timer(90, client.kill)
        watchdog.start()
        try:
            output, unknown = drive(client, server, environment)
            client.wait()
        finally:
            watchdog.cancel()
        print(f"--- {label} ---")
        print(output)
        if unknown:
            print("unknown pad help steps: " + ", ".join(unknown))
        passed = (
            client.returncode == 0
            and f"PAD_HELP_TEST_PASSED {label}" in output
            and "FAIL!" not in output
            and "PAD_HELP_NOT_OK" not in output
            and not unknown
            and not server.errors
        )
        if server.errors:
            print("fake statusd send failed: " + repr(server.errors[0]))
        print(("ok   " if passed else "FAIL ") + "pad help " + label)
        return passed
    finally:
        if keyboard_holder is not None and keyboard_holder.poll() is None:
            keyboard_holder.kill()
        if client is not None and client.poll() is None:
            client.kill()
        if server is not None:
            server.close()
        sway.terminate()
        sway.wait(timeout=5)
        shutil.rmtree(home, ignore_errors=True)


def main():
    for tool in ("sway", "swaymsg", QS):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    results = [run_mode(label, extra_env) for label, extra_env in MODES]
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
