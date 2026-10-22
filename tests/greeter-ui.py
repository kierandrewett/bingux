#!/usr/bin/env python3
"""Run the login greeter against a fake greetd inside a headless Sway session.

The fake server speaks greetd's IPC (native-endian u32 length + JSON). It
expects one wrong password, then the right one, then a start_session whose
command and environment come from the Gnoblin wayland-sessions entry. A
screenshot of each visual state is written to BINGUX_GREETER_SHOTS (default
build/kiosk-shots).
"""

import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parent.parent
QS = os.environ.get("QS_TEST_BIN", "qs")
SESSION_BIN = os.environ.get("BINGUX_SESSION_BIN", str(ROOT / "build/cargo/release/bingux-session"))

def metrics_record(sample):
    return (
        json.dumps(
            {
                "protocolVersion": 1,
                "type": "metrics",
                "cpuPercent": 12.0,
                "memoryTotalBytes": 8 << 30,
                "memoryUsedBytes": 2 << 30,
                "networkReceiveBytesPerSecond": 0,
                "networkTransmitBytesPerSecond": 0,
                "desktopStateAvailable": False,
                "inputSources": [],
                "currentInputSource": None,
                "screenSharing": False,
                "microphoneInUse": False,
                "locationInUse": False,
                "extra": {"sampledAtMs": sample},
            },
            separators=(",", ":"),
        ).encode()
        + b"\n"
    )


class FakeMetrics(threading.Thread):
    """Serves metrics-v1 records so the status bar shows values, not '--'."""

    def __init__(self, path):
        super().__init__(daemon=True)
        self.path = path
        self.stop = threading.Event()

    def run(self):
        self.path.parent.mkdir(mode=0o700, exist_ok=True)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(str(self.path))
        listener.listen(4)
        listener.settimeout(0.2)
        with listener:
            while not self.stop.is_set():
                try:
                    connection, _ = listener.accept()
                except socket.timeout:
                    continue
                connection.settimeout(None)
                threading.Thread(target=self.feed, args=(connection,), daemon=True).start()
        self.path.unlink(missing_ok=True)

    def feed(self, connection):
        sample = 1
        with connection:
            while not self.stop.is_set():
                try:
                    connection.sendall(metrics_record(sample))
                except OSError:
                    return
                sample += 1
                self.stop.wait(1)



class FakeGreetd(threading.Thread):
    def __init__(self, path):
        super().__init__(daemon=True)
        self.path = path
        self.requests = []
        self.failures = []
        self.started = None
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(path)
        self.server.listen(1)

    @staticmethod
    def read(connection):
        header = connection.recv(4)
        if len(header) < 4:
            return None
        (length,) = struct.unpack("=I", header)
        data = b""
        while len(data) < length:
            chunk = connection.recv(length - len(data))
            if not chunk:
                return None
            data += chunk
        return json.loads(data)

    @staticmethod
    def send(connection, message):
        data = json.dumps(message).encode()
        connection.sendall(struct.pack("=I", len(data)) + data)

    def run(self):
        try:
            self.serve()
        except Exception as error:  # noqa: BLE001 - reported through the checks
            self.failures.append("server crashed: " + repr(error))

    def serve(self):
        connection, _ = self.server.accept()
        attempts = 0
        with connection:
            while True:
                request = self.read(connection)
                if request is None:
                    return
                self.requests.append(request)
                kind = request.get("type")
                if kind == "create_session":
                    if request.get("username") != "luna":
                        self.failures.append("unexpected user " + str(request.get("username")))
                    self.send(
                        connection,
                        {"type": "auth_message", "auth_message_type": "secret", "auth_message": "Password: "},
                    )
                elif kind == "post_auth_message_response":
                    attempts += 1
                    if request.get("response") == "secret":
                        self.send(connection, {"type": "success"})
                    else:
                        self.send(
                            connection,
                            {"type": "error", "error_type": "auth_error", "description": "pam_authenticate failed"},
                        )
                elif kind == "cancel_session":
                    self.send(connection, {"type": "success"})
                elif kind == "start_session":
                    self.started = request
                    self.send(connection, {"type": "success"})
                else:
                    self.failures.append("unexpected request " + json.dumps(request))


def main():
    if not Path(SESSION_BIN).is_file():
        print("build bingux-session first (make daemons); missing " + SESSION_BIN)
        return 1
    for tool in ("sway", "grim", QS):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    home = Path(tempfile.mkdtemp(prefix="bingux-greeter-ui-"))
    runtime = home / "run"
    runtime.mkdir(mode=0o700)
    state = home / "state"
    (state / "bingux").mkdir(parents=True)
    (state / "bingux/greeter.json").write_text(json.dumps({"user": "luna", "session": "gnoblin"}))
    bin_dir = home / "bin"
    bin_dir.mkdir()
    # The greeter must see the remembered account; use a passwd-free stand-in
    # for bingux-session users so the test does not depend on host accounts.
    # BINGUX_GREETER_WALLPAPER points at a real image for screenshots; the
    # default is a small generated gradient so the test needs no network.
    wallpaper = home / "luna-wallpaper.png"
    real_wallpaper = os.environ.get("BINGUX_GREETER_WALLPAPER")
    if real_wallpaper and Path(real_wallpaper).is_file():
        wallpaper = home / ("luna-wallpaper" + Path(real_wallpaper).suffix)
        shutil.copy(real_wallpaper, wallpaper)
    else:
        subprocess.run(
            ["magick", "-size", "64x40", "gradient:#1b2a4a-#62a0ea", str(wallpaper)], check=False, capture_output=True
        )
    # The blurred twin the greeter shows behind the card; the compositor
    # background uses the same file.
    if wallpaper.exists():
        shutil.copy(wallpaper, home / "wallpaper-blur")
    users = {
        "users": [
            {
                "name": "luna",
                "displayName": "Luna Lovegood",
                "icon": "",
                "wallpaper": str(wallpaper) if wallpaper.exists() else "",
            },
            {"name": "ada", "displayName": "Ada", "icon": "", "wallpaper": ""},
        ]
    }
    (home / "users.json").write_text(json.dumps(users))
    (bin_dir / "bingux-session").write_text(
        f'#!/bin/sh\ncase "$1" in\n    users) cat {home / "users.json"};;\n    *) exec {SESSION_BIN} "$@";;\nesac\n'
    )
    (bin_dir / "bingux-session").chmod(0o755)
    sessions = home / "share/wayland-sessions"
    sessions.mkdir(parents=True)
    (sessions / "gnoblin.desktop").write_text(
        "[Desktop Entry]\nName=Gnoblin\nExec=/bin/true --from-test\nDesktopNames=Gnoblin;\n"
    )
    (sessions / "other.desktop").write_text("[Desktop Entry]\nName=Other\nExec=/bin/true\n")

    sock = str(runtime / "greetd.sock")
    greetd = FakeGreetd(sock)
    greetd.start()

    sway_config = home / "sway.conf"
    sway_config.write_text("output HEADLESS-1 resolution 1280x800\n")
    ack = home / "greeter-ui.ack"
    ack.write_text("0")
    environment = os.environ | {
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(runtime),
        "XDG_STATE_HOME": str(state),
        "XDG_DATA_DIRS": str(home / "share"),
        "WLR_BACKENDS": "headless",
        "WLR_RENDERER": "pixman",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "GREETD_SOCK": sock,
        "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
        "QT_QPA_PLATFORM": "wayland",
        "PATH": str(bin_dir) + ":" + os.environ.get("PATH", ""),
        "GREETER_UI_ACK": str(ack),
    }
    environment.pop("DISPLAY", None)
    sway = subprocess.Popen(
        ["sway", "-c", str(sway_config)],
        env={k: v for k, v in environment.items() if k != "WAYLAND_DISPLAY"},
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    metrics = FakeMetrics(runtime / "bingux/metrics-v1.sock")
    metrics.start()
    try:
        # Sway picks its own socket name; wait for it in the private runtime dir.
        for _ in range(100):
            sockets = [p for p in runtime.glob("wayland-*") if not p.name.endswith(".lock")]
            if sockets:
                environment["WAYLAND_DISPLAY"] = sockets[0].name
                break
            time.sleep(0.1)
        else:
            print("headless sway did not start")
            return 1
        qml = home / "greeter-ui.qml"
        qml.write_text(
            (ROOT / "tests/greeter-ui.qml").read_text().replace("../shell/bingux", (ROOT / "shell/bingux").as_uri())
        )
        greeter = subprocess.Popen(
            [QS, "-p", str(qml)],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        shots_dir = Path(os.environ.get("BINGUX_GREETER_SHOTS", ROOT / "build/kiosk-shots"))
        shots_dir.mkdir(parents=True, exist_ok=True)
        lines = []
        captured = []
        acks = [0]

        def grab_settled(target):
            # Popups lay out a few frames after they open; grab until two
            # consecutive grabs match so the shot is not taken mid-layout.
            previous = None
            for _ in range(20):
                time.sleep(0.3)
                probe = target.with_suffix(".probe.png")
                subprocess.run(["grim", str(probe)], env=environment, check=False)
                current = probe.read_bytes() if probe.exists() else None
                probe.unlink(missing_ok=True)
                if current is not None and current == previous:
                    target.write_bytes(current)
                    return
                previous = current
            subprocess.run(["grim", str(target)], env=environment, check=False)

        def read_output():
            # Each "GREETER_UI_SHOT <name>" marker is grabbed once the frame
            # is settled, then the QML is resumed by rewriting the ack file.
            for line in greeter.stdout:
                lines.append(line)
                if "GREETER_UI_SHOT " in line:
                    name = line.split("GREETER_UI_SHOT ", 1)[1].strip()
                    target = shots_dir / (name + ".png")
                    grab_settled(target)
                    captured.append(name)
                    acks[0] += 1
                    ack.write_text(str(acks[0]))

        reader = threading.Thread(target=read_output, daemon=True)
        reader.start()
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline and greeter.poll() is None:
            time.sleep(0.1)
        try:
            greeter.wait(timeout=5)
        except subprocess.TimeoutExpired:
            greeter.kill()
            greeter.wait()
        reader.join(timeout=10)
        output = "".join(lines)
        print(output)
        print("greeter exit code:", greeter.returncode)
        started = greetd.started or {}
        checks = {
            "greeter passed": "GREETER_UI_PASSED" in output,
            "all shots captured": captured
            == ["idle", "typing", "wrong-password", "accounts", "sessions", "control-centre", "typed-password"],
            "exit code": greeter.returncode == 0,
            "no server failures": not greetd.failures,
            "start_session command": started.get("cmd") == ["/bin/true", "--from-test"],
            "start_session env": sorted(started.get("env", []))
            == ["BINGUX_CONTINUING=1", "XDG_CURRENT_DESKTOP=Gnoblin", "XDG_SESSION_DESKTOP=gnoblin", "XDG_SESSION_TYPE=wayland"],
            "remembered choice": json.loads((state / "bingux/greeter.json").read_text())
            == {"user": "luna", "session": "gnoblin"},
        }
        for name, ok in checks.items():
            print(("ok   " if ok else "FAIL ") + name)
        if greetd.failures:
            print(greetd.failures)
        return 0 if all(checks.values()) else 1
    finally:
        metrics.stop.set()
        sway.terminate()
        sway.wait(timeout=5)
        if os.environ.get("BINGUX_GREETER_KEEP"):
            print("kept", home)
        else:
            shutil.rmtree(home, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
