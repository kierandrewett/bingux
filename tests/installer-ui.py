#!/usr/bin/env python3
"""Runs the installer QML against a fake NDJSON daemon on a unix socket.

Packs shell/bingux plus distro/installer/shell into a merged tree (as
installer-merged-tree.py does), copies tests/installer-ui.qml into it as the
entry point, and runs it under bingux-shell -p (offscreen, no compositor).
The fake daemon speaks the protocol in distro/installer/README.md:
  probe    -> two 8 GiB disks, UEFI
  install /dev/vda -> progress through every phase, then result ok
  install /dev/vdb -> progress up to mount, then error storage_failed
  log mode -> no daemon; 5000 appended log lines must stay within InstallerState.logLimit
  unreachable mode -> no daemon listens, so the probe must fail
After the shell exits the test checks the plans the daemon received.

usage: installer-ui.py [--shell BINARY]   (default build/bingux-shell/quickshell/src/bingux-shell)
"""

import argparse
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

ROOT = Path(__file__).resolve().parents[1]
SCRATCH_BASE = Path.home() / ".cache" / "bingux-InstallerUI"
GIB = 1024 ** 3
PHASES = ["validate", "preflight", "partition", "mount", "copy", "strip", "configure", "bootloader", "finish"]
PROBE_RESULT = {
    "firmware": "uefi",
    "live_disk": None,
    "disks": [
        {"path": "/dev/vda", "size_bytes": 8 * GIB, "model": "virtio", "removable": False, "is_live_medium": False, "other_os": [], "partition_table": "gpt", "free_bytes": 8 * GIB, "partitions": []},
        {"path": "/dev/vdb", "size_bytes": 8 * GIB, "model": "virtio", "removable": False, "is_live_medium": False, "other_os": [], "partition_table": "gpt", "free_bytes": 8 * GIB, "partitions": []},
    ],
    "tools_missing": [],
    "installer_version": "0.1.0",
    "capabilities": {
        "storage_modes": ["erase"],
        "filesystems": ["ext4", "btrfs", "xfs"],
        "swap": ["none", "zram", "file", "partition"],
        "encryption": {"uefi": True, "bios": True},
        "dual_boot": False,
    },
}


class FakeDaemon:
    """One request per connection; records every request it receives."""

    def __init__(self, path):
        self.path = path
        self.requests = []
        self.lock = threading.Lock()
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(path)
        self.server.listen(4)
        self.thread = threading.Thread(target=self.serve, daemon=True)

    def start(self):
        self.thread.start()

    def serve(self):
        while True:
            try:
                connection, _ = self.server.accept()
            except OSError:
                return
            threading.Thread(target=self.handle, args=(connection,), daemon=True).start()

    def handle(self, connection):
        with connection:
            data = b""
            while b"\n" not in data:
                chunk = connection.recv(65536)
                if not chunk:
                    break
                data += chunk
            request = json.loads(data.split(b"\n", 1)[0].decode())
            with self.lock:
                self.requests.append(request)
            for line in self.reply(request):
                connection.sendall((json.dumps(line) + "\n").encode())
                time.sleep(0.02)

    def reply(self, request):
        op = request.get("op")
        if op == "probe":
            return [{"event": "result", "ok": True, "data": PROBE_RESULT}]
        if op != "install":
            return [{"event": "error", "code": "bad_request", "message": f"unknown op: {op}"}]
        plan = request["plan"]
        disk = plan["disk"]
        if disk == "/dev/vdb":
            lines = [{"event": "progress", "phase": phase, "percent": None, "message": ""} for phase in PHASES[:4]]
            lines.append({"event": "error", "code": "storage_failed", "message": "storage_failed: RuntimeError: fake partition table write failed"})
            return lines
        if disk != "/dev/vda" or plan.get("confirm_erase") != disk:
            return [{"event": "error", "code": "invalid_plan", "message": "invalid plan: disk: fake daemon rule"}]
        lines = [{"event": "progress", "phase": phase, "percent": round(100 * index / len(PHASES)), "message": f"{phase} step"} for index, phase in enumerate(PHASES)]
        lines.append({"event": "result", "ok": True, "data": {"disk": disk, "firmware": "uefi", "filesystem": plan["filesystem"]}})
        return lines


def probe_socket_path(scratch, name):
    return str(scratch / name)


def run_shell(shell, tree, mode, socket_path):
    """Runs the driver and returns its output once it reports a verdict.

    The shell does not exit on Qt.quit() under bingux-shell -p, so the verdict
    is the INSTALLER_UI_PASSED or INSTALLER_UI_FAILED marker; the shell is killed after it.
    """
    env = {k: v for k, v in os.environ.items() if k != "WAYLAND_DISPLAY"}
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["BINGUX_INSTALLER_SOCKET"] = socket_path
    env["INSTALLER_UI_MODE"] = mode
    log = Path(socket_path).with_suffix(".log")
    with open(log, "w") as out:
        process = subprocess.Popen([shell, "-p", str(tree / "InstallerUiDriver.qml")], stdout=out, stderr=subprocess.STDOUT, env=env)
        deadline = time.monotonic() + 60
        try:
            while time.monotonic() < deadline:
                text = log.read_text(errors="replace")
                if "INSTALLER_UI_PASSED" in text or "INSTALLER_UI_FAILED" in text or process.poll() is not None:
                    break
                time.sleep(0.2)
        finally:
            process.kill()
            process.wait()
    return log.read_text(errors="replace")


def check_plan(plan, disk):
    expected = {
        "version": 2,
        "disk": disk,
        "confirm_erase": disk,
        "firmware": "auto",
        "filesystem": "xfs",
        "storage": {"mode": "erase"},
        "encryption": {"enabled": False},
        "swap": "zram",
        "swap_bytes": None,
        "ntp": False,
        "root_password": "root secret",
        "autologin": "ada",
        "other_os": "refuse",
        "hostname": "bingux",
        "timezone": "Europe/London",
        "locale": "en_GB.UTF-8",
        "keymap": "uk",
    }
    for key, value in expected.items():
        if plan.get(key) != value:
            raise SystemExit(f"plan {key} is {plan.get(key)!r}, expected {value!r}")
    users = plan.get("users")
    if not isinstance(users, list) or len(users) != 1:
        raise SystemExit(f"plan users is {users!r}")
    user = users[0]
    if user.get("name") != "ada" or user.get("password") != "correct horse" or user.get("admin") is not True or user.get("full_name") != "Ada Lovelace":
        raise SystemExit(f"plan user is {user!r}")


def pack_tree(scratch):
    """Packs shell/bingux and the installer overlay into scratch/tree and returns the tree."""
    tree = scratch / "tree"
    packed = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts/pack-shell.py"),
            "--overlay",
            str(ROOT / "distro/installer/shell"),
            "--entries",
            '{"installer": "InstallerShell.qml"}',
            "--tree-out",
            str(tree),
            str(scratch / "installer.sxa"),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if packed.returncode:
        raise SystemExit(f"pack-shell failed: {packed.stderr}")
    return tree


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--shell", default=str(ROOT / "build/bingux-shell/quickshell/src/bingux-shell"))
    arguments = parser.parse_args()
    SCRATCH_BASE.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="installer-ui-", dir=SCRATCH_BASE))
    try:
        tree = pack_tree(scratch)
        shutil.copy(ROOT / "tests/installer-ui.qml", tree / "InstallerUiDriver.qml")

        # Flow: probe, install /dev/vda (success), install /dev/vdb (storage failure).
        flow_path = probe_socket_path(scratch, "flow.sock")
        daemon = FakeDaemon(flow_path)
        daemon.start()
        output = run_shell(arguments.shell, tree, "flow", flow_path)
        if "INSTALLER_UI_FAILED" in output or "INSTALLER_UI_PASSED" not in output:
            raise SystemExit("installer-ui flow failed:\n" + "\n".join(line for line in output.splitlines() if "INSTALLER_UI" in line or "ERROR" in line or "Error" in line))
        for marker in ["INSTALLER_UI_PROBE_OK", "INSTALLER_UI_INSTALL_OK", "INSTALLER_UI_FAILURE_OK"]:
            if marker not in output:
                raise SystemExit(f"missing {marker} in output")
        ops = [request["op"] for request in daemon.requests]
        if ops != ["probe", "install", "install"]:
            raise SystemExit(f"daemon saw {ops}")
        check_plan(daemon.requests[1]["plan"], "/dev/vda")
        check_plan(daemon.requests[2]["plan"], "/dev/vdb")

        # Unreachable: nothing listens on the socket, so the probe must fail cleanly.
        missing_path = probe_socket_path(scratch, "absent.sock")
        output = run_shell(arguments.shell, tree, "unreachable", missing_path)
        if "INSTALLER_UI_FAILED" in output or "INSTALLER_UI_UNREACHABLE_OK" not in output:
            raise SystemExit("installer-ui unreachable failed:\n" + "\n".join(line for line in output.splitlines() if "INSTALLER_UI" in line or "ERROR" in line))

        # Log: the install log holds at most logLimit lines, keeps the newest and resetInstall clears it.
        log_path = probe_socket_path(scratch, "log.sock")
        output = run_shell(arguments.shell, tree, "log", log_path)
        if "INSTALLER_UI_FAILED" in output or "INSTALLER_UI_LOG_OK" not in output:
            raise SystemExit("installer-ui log failed:\n" + "\n".join(line for line in output.splitlines() if "INSTALLER_UI" in line or "ERROR" in line))

        print("installer-ui: probe, install and failure flows pass against the fake daemon; unreachable probe fails cleanly; the install log stays capped")
        print("INSTALLER_UI_PASSED")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
