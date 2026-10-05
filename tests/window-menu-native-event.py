#!/usr/bin/env python3
"""Exercise Bingux's native window menu against a fake peer in Gnoblin devkit."""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading


ROOT = Path(__file__).resolve().parents[1]
RUNTIME = Path(os.environ["XDG_RUNTIME_DIR"])
EVENT_NAME = "gnoblin.window.menu-requested"


def send_record(connection, record):
    connection.sendall(json.dumps(record, separators=(",", ":")).encode() + b"\n")


def read_record(reader):
    line = reader.readline()
    if not line:
        raise AssertionError("Bingux disconnected before completing the window-menu scenario")
    return json.loads(line)


def read_api_request(reader, method):
    request = read_record(reader)
    if request.get("op") != "api" or request.get("method") != method:
        raise AssertionError(f"expected API method {method}, got {request!r}")
    return request


def send_menu_event(connection, window_id, menu_type, x, y):
    event = {
        "event": EVENT_NAME,
        "window_id": window_id,
        "menu_type": menu_type,
        "x": x,
        "y": y,
    }
    if menu_type == "wm":
        event["menu_context"] = "menu-context-101"
    send_record(connection, event)


def main():
    display = os.environ.get("WAYLAND_DISPLAY", "")
    if not display.startswith("gnoblin-devkit-"):
        raise SystemExit("Run this check inside Gnoblin's nested devkit session.")

    quickshell = os.environ.get("QS_TEST_BIN", "qs")
    if not shutil.which(quickshell):
        raise SystemExit(f"Quickshell executable not found: {quickshell}")

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
    x = int(monitor.get("x", 0)) + 120
    y = int(monitor.get("y", 0)) + 80

    runtime_dir = RUNTIME / "gnoblin"
    runtime_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="bingux-window-menu-", dir=RUNTIME) as temporary:
        fixture = Path(temporary) / "shell"
        shutil.copytree(ROOT / "shell/bingux", fixture)
        shutil.copy2(ROOT / "tests/window-menu.qml", fixture / "shell.qml")

        for scenario in ("stale", "live"):
            socket_path = runtime_dir / f"bingux-window-menu-{os.getpid()}-{scenario}.sock"
            if socket_path.exists():
                raise SystemExit(f"Test socket already exists: {socket_path}")

            listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            listener.bind(str(socket_path))
            os.chmod(socket_path, 0o600)
            listener.listen(1)
            listener.settimeout(10)
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
                                "api_minor": 30,
                                "methods": ["shortcut.bind", "shortcut.unbind"],
                            },
                        )
                        subscription = read_record(reader)
                        if subscription.get("op") != "events" or EVENT_NAME not in subscription.get("events", []):
                            raise AssertionError(f"unexpected native event subscription: {subscription!r}")
                        send_record(connection, {"event": "subscribed", "events": [EVENT_NAME]})

                        target_id = "404" if scenario == "stale" else "101"
                        send_menu_event(connection, target_id, "wm", x, y)
                        list_request = read_api_request(reader, "window.list")
                        windows = [
                            {
                                "id": "202",
                                "title": "Focused after menu opened",
                                "focused": True,
                                "minimized": False,
                                "maximized": False,
                                "above": False,
                                "sticky": False,
                                "movable": True,
                                "resizable": True,
                                "minimizable": True,
                                "maximizable": True,
                                "closable": True,
                            }
                        ]
                        if scenario == "live":
                            windows.insert(
                                0,
                                {
                                    "id": "101",
                                    "title": "Requested window",
                                    "focused": False,
                                    "minimized": False,
                                    "maximized": False,
                                    "above": False,
                                    "sticky": False,
                                    "movable": True,
                                    "resizable": True,
                                    "minimizable": True,
                                    "maximizable": True,
                                    "closable": True,
                                },
                            )
                        send_record(
                            connection,
                            {
                                "event": "reply",
                                "id": list_request["id"],
                                "result": {"windows": windows},
                            },
                        )

                        if scenario == "live":
                            move_request = read_api_request(reader, "window.begin_move")
                            if move_request.get("arguments") != {"menu_context": "menu-context-101"}:
                                raise AssertionError(f"move did not use its event capability: {move_request!r}")
                            send_record(
                                connection,
                                {"event": "reply", "id": move_request["id"], "result": {"id": "101"}},
                            )

                            action_request = read_api_request(reader, "window.set_above")
                            if action_request.get("arguments") != {"id": "101", "enabled": True}:
                                raise AssertionError(
                                    f"action was not bound to the requested window: {action_request!r}"
                                )
                            send_record(
                                connection,
                                {
                                    "event": "error",
                                    "id": action_request["id"],
                                    "message": "synthetic action rejection",
                                },
                            )
                            send_menu_event(connection, "202", "app", x, y)

                        stop.wait(2)
                except Exception as error:
                    if not stop.is_set():
                        peer_errors.append(error)

            peer = threading.Thread(target=compositor_peer, daemon=True)
            peer.start()
            child_env = os.environ.copy()
            child_env.update(
                {
                    "BINGUX_COMPOSITOR": "gnoblin",
                    "BINGUX_WINDOW_MENU_SCENARIO": scenario,
                    "GNOBLIN_COMPOSITOR_SOCKET": str(socket_path),
                    "QT_QPA_PLATFORM": "wayland",
                }
            )
            try:
                result = subprocess.run(
                    [quickshell, "-p", str(fixture), "--no-color"],
                    cwd=ROOT,
                    env=child_env,
                    capture_output=True,
                    text=True,
                    timeout=14,
                )
                output = result.stdout + result.stderr
                print(output)
                marker = f"WINDOW_MENU_{scenario.upper()}_PASSED"
                if result.returncode or marker not in output:
                    raise SystemExit(f"Quickshell {scenario} scenario failed with exit {result.returncode}")
                if peer_errors:
                    raise AssertionError(f"fake compositor peer failed: {peer_errors[0]}")
            finally:
                stop.set()
                listener.close()
                peer.join(timeout=1)
                try:
                    socket_path.unlink()
                except FileNotFoundError:
                    pass
                if peer_errors:
                    raise AssertionError(f"fake compositor peer failed: {peer_errors[0]}")

    print("WINDOW_MENU_NATIVE_EVENT_E2E_PASSED")


if __name__ == "__main__":
    main()
