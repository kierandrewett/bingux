#!/usr/bin/env python3
"""Route a Gnoblin OSD event through statusd into Bingux on nested Wayland."""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time


ROOT = Path(__file__).resolve().parents[1]
STATUSD = Path(os.environ.get("BINGUX_STATUSD_BIN", ROOT / "build/cargo/release/bingux-statusd"))
RUNTIME = Path(os.environ["XDG_RUNTIME_DIR"])
EVENT_NAME = "gnoblin.osd.requested"


def send_record(connection, record):
    connection.sendall(json.dumps(record, separators=(",", ":")).encode() + b"\n")


def main():
    display = os.environ.get("WAYLAND_DISPLAY", "")
    if not display.startswith(("gnoblin-devkit-", "gnoblin-gs-")):
        raise SystemExit("Run this check inside Gnoblin's nested devkit session.")
    if not STATUSD.is_file():
        raise SystemExit(
            f"Status daemon not found at {STATUSD}; build it with `make daemons` or set BINGUX_STATUSD_BIN."
        )

    monitor_result = subprocess.run(
        ["gnoblinctl", "--json", "monitor", "list"],
        capture_output=True,
        text=True,
        timeout=5,
        check=True,
    )
    monitors = json.loads(monitor_result.stdout).get("monitors", [])
    if not monitors:
        raise SystemExit("Gnoblin devkit has no active monitor.")
    monitor = monitors[0]
    monitor_id = monitor["id"]
    monitor_index = monitor["index"]
    output_names = [monitor_id]

    fake_socket = RUNTIME / "gnoblin" / "bingux-osd-test-compositor.sock"
    runtime_dir = RUNTIME / "bingux"
    osd_socket = runtime_dir / "osd-v2.sock"
    if fake_socket.exists() or osd_socket.exists():
        raise SystemExit("An OSD test socket already exists; use a fresh devkit runtime.")

    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(fake_socket))
    listener.listen(1)
    listener.settimeout(8)
    stop = threading.Event()
    peer_errors = []

    def compositor_peer():
        try:
            connection, _ = listener.accept()
            connection.settimeout(8)
            with connection, connection.makefile("rb") as reader:
                send_record(
                    connection,
                    {
                        "event": "hello",
                        "api_major": 1,
                        "api_minor": 73,
                        "events": [EVENT_NAME],
                    },
                )
                requests = [json.loads(reader.readline()), json.loads(reader.readline())]
                if [request.get("op") for request in requests] != ["events", "monitors"]:
                    raise AssertionError(f"unexpected compositor requests: {requests!r}")
                if requests[0].get("events") != [EVENT_NAME]:
                    raise AssertionError(f"unexpected event subscription: {requests[0]!r}")
                if requests[0].get("api_version") != {"major": 1, "minor": 27}:
                    raise AssertionError(f"unexpected event API version: {requests[0]!r}")
                if requests[1].get("api_version") != {"major": 1, "minor": 1}:
                    raise AssertionError(f"unexpected monitor snapshot version: {requests[1]!r}")
                send_record(
                    connection,
                    {
                        "event": "monitors",
                        "monitors": [
                            {
                                "id": monitor_id,
                                "index": monitor_index,
                                "output_names": output_names,
                            }
                        ],
                    },
                )
                send_record(connection, {"event": "subscribed", "events": [EVENT_NAME]})
                # Mutter's OSD event has icon and label but no numeric level.
                event = {
                    "event": EVENT_NAME,
                    "sequence": 101,
                    "time": 123456,
                    "monitor_id": monitor_id,
                    "output_names": output_names,
                    "icon": "dialog-information-symbolic",
                    "label": "Touchpad disabled",
                }
                while not stop.is_set():
                    send_record(connection, event)
                    stop.wait(0.05)
        except Exception as error:
            if not stop.is_set():
                peer_errors.append(error)

    peer = threading.Thread(target=compositor_peer, daemon=True)
    peer.start()
    runtime_dir.mkdir(mode=0o700, parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="bingux-osd-native-", dir=RUNTIME) as temp:
        fixture = Path(temp)
        for name in ("Theme", "SymbolicIcon", "AnimatedCount", "OsdState", "OsdSurface", "PanelOutline"):
            shutil.copy2(ROOT / f"shell/bingux/{name}.qml", fixture / f"{name}.qml")

        (fixture / "qmldir").write_text(
            "singleton Theme 1.0 Theme.qml\n"
            + "".join(
                f"{name} 1.0 {name}.qml\n"
                for name in ("SymbolicIcon", "AnimatedCount", "OsdState", "OsdSurface", "PanelOutline")
            )
        )

        surface = fixture / "OsdSurface.qml"
        surface.write_text(
            surface.read_text()
            .replace("property var osdWindows", "property var testWindows: []\n    property var osdWindows")
            .replace(
                "id: osdWindow",
                """id: osdWindow
            property alias testCard: osdCard
            Component.onCompleted: root.testWindows = root.testWindows.concat([osdWindow])""",
            )
        )

        (fixture / "shell.qml").write_text(
            """import QtQuick
import QtTest
import Quickshell

ShellRoot {
    OsdState { id: state }
    OsdSurface {
        id: surface
        state: state
    }
    TestCase {
        name: "NativeOsdEvent"
        when: surface.testWindows.length > 0
        function test_status_only_event_reaches_surface() {
            const screen = Quickshell.screens[0];
            const window = surface.testWindows[0];
            tryVerify(() => state.requestForOutputName(screen.name) !== null, 5000);
            tryCompare(window.testCard, "opacity", 1);
            waitForRendering(window.testCard);
            verify(window.visible, "OSD surface is visible in the nested compositor");
            verify(!window.focusable, "OSD does not take keyboard focus");
            verify(!window.hasLevel, "Mutter event without numeric level stays status-only");
            verify(window.presentedRequest.level === -1 && window.presentedRequest.maxLevel === -1,
                   "missing numeric level remains unavailable");
            verify(window.presentedRequest.label === "Touchpad disabled",
                   "native event label reaches the OSD");
            verify(!findChild(window.testCard, "osdTrack").visible,
                   "status-only request has no level bar");
            verify(!findChild(window.testCard, "osdPercent").visible,
                   "status-only request has no percentage");
            console.info("OSD_NATIVE_EVENT_SURFACE_PASSED");
        }
        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
"""
        )

        daemon_env = os.environ.copy()
        daemon_env["GNOBLIN_COMPOSITOR_SOCKET"] = str(fake_socket)
        daemon = subprocess.Popen(
            [str(STATUSD)],
            cwd=ROOT,
            env=daemon_env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        try:
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline and not osd_socket.exists():
                if daemon.poll() is not None:
                    error = daemon.stderr.read().decode(errors="replace")
                    raise AssertionError(f"bingux-statusd exited before OSD socket startup: {error}")
                time.sleep(0.02)
            if not osd_socket.exists():
                raise AssertionError(f"timed out waiting for OSD socket: {osd_socket}")

            result = subprocess.run(
                [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture), "--no-color"],
                env=os.environ | {"QT_QPA_PLATFORM": "wayland"},
                capture_output=True,
                text=True,
                timeout=12,
            )
            output = result.stdout + result.stderr
            print(output)
            if result.returncode or "OSD_NATIVE_EVENT_SURFACE_PASSED" not in output:
                raise SystemExit(1)
            if peer_errors:
                raise AssertionError(f"fake Mutter peer failed: {peer_errors[0]}")
            print("OSD_NATIVE_EVENT_E2E_PASSED")
        finally:
            stop.set()
            daemon.terminate()
            try:
                daemon.wait(timeout=2)
            except subprocess.TimeoutExpired:
                daemon.kill()
                daemon.wait(timeout=2)
            stderr = daemon.stderr.read().decode(errors="replace") if daemon.stderr else ""
            listener.close()
            peer.join(timeout=1)
            for path in (osd_socket, fake_socket):
                try:
                    path.unlink()
                except FileNotFoundError:
                    pass
            if peer_errors:
                raise AssertionError(f"fake Mutter peer failed: {peer_errors[0]}")
            if daemon.returncode not in (-15, 0):
                raise AssertionError(f"bingux-statusd exited with {daemon.returncode}:\n{stderr}")


if __name__ == "__main__":
    main()
