#!/usr/bin/env python3
"""Prove Metrics retries repeated refused connections and accepts later recovery."""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def metrics_record():
    return (
        json.dumps(
            {
                "protocolVersion": 1,
                "type": "metrics",
                "cpuPercent": None,
                "memoryTotalBytes": 100,
                "memoryUsedBytes": 50,
                "networkReceiveBytesPerSecond": None,
                "networkTransmitBytesPerSecond": None,
                "desktopStateAvailable": False,
                "inputSources": [],
                "currentInputSource": None,
                "screenSharing": False,
                "microphoneInUse": False,
                "locationInUse": False,
                "extra": {"sampledAtMs": 1},
            },
            separators=(",", ":"),
        ).encode()
        + b"\n"
    )


def main():
    with tempfile.TemporaryDirectory(prefix="bingux-metrics-reconnect-") as temporary:
        root = Path(temporary)
        shell = root / "shell"
        shell.mkdir()
        for name in ("Metrics.qml", "MetricsHistory.js"):
            shutil.copy2(ROOT / "shell/bingux" / name, shell / name)
        (shell / "shell.qml").write_text(
            """import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
    Metrics { id: metrics }
    FileView { id: result; path: Quickshell.env("BINGUX_METRICS_RECONNECT_RESULT") }
    Timer {
        interval: 25; running: true; repeat: true
        onTriggered: if (metrics.available && metrics.latest.memoryUsedBytes === 50) {
            result.setText("METRICS_RECONNECT_PASSED")
            Qt.quit()
        }
    }
    Timer { interval: 9000; running: true; onTriggered: { result.setText("METRICS_RECONNECT_TIMEOUT"); Qt.quit() } }
}
"""
        )
        runtime = root / "runtime"
        runtime.mkdir(mode=0o700)
        environment = os.environ | {
            "XDG_RUNTIME_DIR": str(runtime),
            "QT_QPA_PLATFORM": "offscreen",
            "QT_QUICK_BACKEND": "software",
            "BINGUX_METRICS_RECONNECT_RESULT": str(root / "result"),
        }
        process = subprocess.Popen(
            [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(shell), "--no-color"],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        socket_path = runtime / "bingux" / "metrics-v1.sock"
        try:
            # The first two attempts must fail while no socket exists.
            time.sleep(0.9)
            socket_path.parent.mkdir(mode=0o700)
            listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            listener.bind(str(socket_path))
            listener.listen(4)
            listener.settimeout(5)
            with listener:
                try:
                    connection, _ = listener.accept()
                except TimeoutError as error:
                    raise AssertionError("Metrics did not retry after repeated refused connections") from error
                with connection:
                    connection.sendall(metrics_record())
                    output, _ = process.communicate(timeout=10)
        except Exception:
            if process.poll() is None:
                process.kill()
                output, _ = process.communicate(timeout=2)
            else:
                output = process.stdout.read() if process.stdout else ""
            print(output)
            raise
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=2)
            socket_path.unlink(missing_ok=True)
        print(output)
        if (
            process.returncode
            or not (root / "result").is_file()
            or (root / "result").read_text() != "METRICS_RECONNECT_PASSED"
        ):
            raise SystemExit(1)
        print("METRICS_RECONNECT_PASSED")


if __name__ == "__main__":
    main()
