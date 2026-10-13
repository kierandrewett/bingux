#!/usr/bin/env python3
"""Check the production helper against a private PipeWire server, without devices."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import selectors
import subprocess
import sys
import tempfile
import time

CONFIG = """
context.properties = { core.daemon = true core.name = camera-test support.dbus = false }
context.spa-libs = { support.* = support/libspa-support }
context.modules = [
 { name = libpipewire-module-protocol-native }
 { name = libpipewire-module-access args = {
     access.socket = { camera-test = unrestricted camera-test-manager = unrestricted }
 } }
 { name = libpipewire-module-spa-node-factory }
 { name = libpipewire-module-metadata }
]
"""
CLIENT_CONFIG = """
context.modules = [
 { name = libpipewire-module-protocol-native }
 { name = libpipewire-module-metadata }
]
"""
EMPTY = {"available": True, "captures": [], "screenSharingCaptures": []}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source", type=Path, default=Path(__file__).resolve().parents[1] / "shell/bingux/camera-status.py"
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    detection = runpy.run_path(str(args.source))
    processes, monitor_pids, events = [], set(), []
    selector = selectors.DefaultSelector()
    pending = bytearray()
    records = []

    def spawn(command, **kwargs):
        process = subprocess.Popen(command, **kwargs)
        processes.append(process)
        return process

    def stop(process):
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=4)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
        for stream in (process.stdin, process.stdout):
            if stream:
                stream.close()

    with tempfile.TemporaryDirectory(prefix="camera-native-", dir="/var/tmp") as directory:
        root = Path(directory)
        config = root / "pipewire.conf"
        config.write_text(CONFIG)
        (root / "client.conf").write_text(CLIENT_CONFIG)
        env = os.environ | {
            "PIPEWIRE_RUNTIME_DIR": str(root),
            "XDG_RUNTIME_DIR": str(root),
            "PIPEWIRE_REMOTE": "camera-test",
            "PIPEWIRE_CONFIG_DIR": str(root),
        }
        with (args.output / "native.stderr").open("w") as errors:

            def server():
                process = spawn(["pipewire", "-c", str(config)], env=env, stdout=subprocess.DEVNULL, stderr=errors)
                deadline = time.monotonic() + 3
                while not (root / "camera-test").exists():
                    assert process.poll() is None and time.monotonic() < deadline, "private server did not start"
                    time.sleep(0.01)
                return process

            def controller():
                return spawn(
                    ["pw-cli"], env=env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=errors, text=True
                )

            def command(client, text):
                client.stdin.write(text + "\n")
                client.stdin.flush()

            def create_camera(client, name):
                command(
                    client,
                    "create-node spa-node-factory { factory.name = support.node.driver "
                    f'node.name = Test-Camera media.role = Camera application.name = "{name}" '
                    'node.description = "Test camera" node.always-process = true }',
                )

            def snapshot():
                objects = json.loads(subprocess.check_output(["pw-dump", "-N"], env=env, timeout=3))
                return objects, dict(
                    available=True,
                    captures=detection["captures"](objects),
                    screenSharingCaptures=detection["screen_sharing_captures"](objects),
                )

            def expect(expected, label, timeout=5):
                start = time.monotonic()
                while time.monotonic() - start < timeout:
                    while records:
                        state = records.pop(0)
                        events.append(dict(label=label, seconds=time.monotonic() - start, state=state))
                        assert state == expected, f"{label}: {state!r} != {expected!r}"
                        monitor_pids.update(
                            int(v) for v in Path(f"/proc/{helper.pid}/task/{helper.pid}/children").read_text().split()
                        )
                        return
                    for key, _ in selector.select(0.05):
                        chunk = os.read(key.fd, 65536)
                        assert chunk, "helper exited"
                        pending.extend(chunk)
                        while b"\n" in pending:
                            line, _, tail = pending.partition(b"\n")
                            pending[:] = tail
                            records.append(json.loads(line))
                raise AssertionError(f"{label}: no state within {timeout}s")

            try:
                daemon = server()
                client = controller()
                create_camera(client, "123")
                # Wait for pw-cli's asynchronous creation, then use the real snapshot as the oracle.
                deadline = time.monotonic() + 3
                while True:
                    graph, initial = snapshot()
                    if initial["captures"]:
                        break
                    assert time.monotonic() < deadline, "test node did not run"
                    time.sleep(0.02)
                assert initial["captures"] == [{"app": 123, "appId": "", "device": "Test camera"}]
                helper = spawn([sys.executable, str(args.source)], env=env, stdout=subprocess.PIPE, stderr=errors)
                selector.register(helper.stdout, selectors.EVENT_READ)
                expect(initial, "initial running numeric-name owner")
                for cycle in range(2):
                    identity = next(
                        o["id"] for o in graph if o.get("info", {}).get("props", {}).get("media.role") == "Camera"
                    )
                    command(client, f"destroy {identity}")
                    expect(EMPTY, f"removal {cycle}")
                    create_camera(client, "Camera owner")
                    active = dict(EMPTY, captures=[{"app": "Camera owner", "appId": "", "device": "Test camera"}])
                    expect(active, f"running owner {cycle}")
                    graph, actual = snapshot()
                    assert actual == active
                    stop(daemon)
                    expect(dict(EMPTY, available=False), f"disconnect {cycle}")
                    stop(client)
                    daemon = server()
                    client = controller()
                    create_camera(client, "123")
                    expect(initial, f"reconnected fresh owner {cycle}")
                    graph, actual = snapshot()
                    assert actual == initial
                assert not records and not selector.select(4), "healthy idle emitted state or timed out"
                stop(helper)
                assert all(not Path(f"/proc/{pid}").exists() for pid in monitor_pids), "monitor not reaped"
                result = dict(
                    status="PASS",
                    source=str(args.source),
                    sha256=hashlib.sha256(args.source.read_bytes()).hexdigest(),
                    pipewire_version=subprocess.check_output(["pw-dump", "--version"], text=True).strip(),
                    events=events,
                    monitor_pids=sorted(monitor_pids),
                    all_monitors_reaped=True,
                    physical_camera_opened=False,
                    host_service_changed=False,
                    scope="Real PipeWire driver nodes marked Camera; no physical video or shell UI",
                )
                (args.output / "native.json").write_text(json.dumps(result, indent=2) + "\n")
                print(json.dumps(result, indent=2))
            finally:
                selector.close()
                for process in reversed(processes):
                    stop(process)


if __name__ == "__main__":
    main()
