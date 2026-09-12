#!/usr/bin/env python3
"""Reconnectable capture transport. The encoder outlives disposable shell UIs."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import selectors
import socket
import subprocess
import sys
import time


def endpoint():
    identity = str(Path(__file__).resolve().parent) + os.environ.get("WAYLAND_DISPLAY", "")
    root = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp")) / f"bingux-capture-{os.getuid()}"
    root.mkdir(mode=0o700, exist_ok=True)
    if root.stat().st_uid != os.getuid() or root.stat().st_mode & 0o077:
        raise RuntimeError("Capture runtime directory must be private")
    return root / (hashlib.sha256(identity.encode()).hexdigest()[:16] + ".sock")


def serve(path):
    path.unlink(missing_ok=True)
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(path))
    listener.listen(4)
    selector = selectors.DefaultSelector()
    selector.register(listener, selectors.EVENT_READ)
    backend = subprocess.Popen(
        [sys.executable, "-u", str(Path(__file__).with_name("capture_backend.py"))],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
    )
    selector.register(backend.stdout, selectors.EVENT_READ)
    clients = {}
    pending = bytearray()
    ready = None
    state = None
    active = False
    idle_since = time.monotonic()

    def drop(client):
        selector.unregister(client)
        clients.pop(client, None)
        client.close()

    def publish(client, payload):
        try:
            client.sendall(payload)
        except (OSError, TimeoutError):
            drop(client)

    try:
        while True:
            for key, _ in selector.select(1):
                if key.fileobj is listener:
                    client, _ = listener.accept()
                    client.settimeout(0.1)
                    clients[client] = bytearray()
                    selector.register(client, selectors.EVENT_READ)
                    replay = state if active or (state and json.loads(state).get("event") == "error") else None
                    for event in (ready, replay):
                        if event and client in clients:
                            publish(client, event)
                elif key.fileobj is backend.stdout:
                    chunk = os.read(backend.stdout.fileno(), 65536)
                    if not chunk:
                        error = (
                            json.dumps(
                                {
                                    "event": "error",
                                    "message": "The recording worker stopped unexpectedly. Check the capture service log.",
                                }
                            ).encode()
                            + b"\n"
                        )
                        for client in list(clients):
                            publish(client, error)
                        return
                    pending.extend(chunk)
                    while b"\n" in pending:
                        line, _, rest = pending.partition(b"\n")
                        pending[:] = rest
                        event = json.loads(line)
                        payload = bytes(line) + b"\n"
                        kind = event.get("event")
                        if kind == "ready":
                            ready = payload
                        elif kind in (
                            "countdown",
                            "starting",
                            "recording",
                            "finalizing",
                            "saved",
                            "error",
                            "cancelled",
                        ):
                            state = payload
                            active = kind in ("countdown", "starting", "recording", "finalizing")
                        for client in list(clients):
                            publish(client, payload)
                else:
                    client = key.fileobj
                    try:
                        chunk = client.recv(65536)
                    except OSError:
                        chunk = b""
                    if not chunk:
                        drop(client)
                        continue
                    clients[client].extend(chunk)
                    if len(clients[client]) > 32768:
                        drop(client)
                        continue
                    while b"\n" in clients[client]:
                        line, _, rest = clients[client].partition(b"\n")
                        clients[client][:] = rest
                        backend.stdin.write(line + b"\n")
                        backend.stdin.flush()
            if clients or active:
                idle_since = time.monotonic()
            elif time.monotonic() - idle_since > 60:
                return
    finally:
        for client in list(clients):
            drop(client)
        listener.close()
        selector.close()
        path.unlink(missing_ok=True)
        backend.terminate()
        try:
            backend.wait(timeout=18)
        except subprocess.TimeoutExpired:
            backend.kill()
            backend.wait()


def connect(path):
    client = socket.socket(socket.AF_UNIX)
    try:
        client.connect(str(path))
        return client
    except OSError:
        client.close()
        raise


def relay():
    path = endpoint()
    with path.with_suffix(".lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            client = connect(path)
        except OSError:
            with path.with_suffix(".log").open("ab") as log:
                subprocess.Popen(
                    [sys.executable, "-u", __file__, "--serve", str(path)],
                    stdin=subprocess.DEVNULL,
                    stdout=log,
                    stderr=log,
                    start_new_session=True,
                )
            deadline = time.monotonic() + 10
            while True:
                try:
                    client = connect(path)
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"Capture service did not start; see {path.with_suffix('.log')}")
                    time.sleep(0.05)
    selector = selectors.DefaultSelector()
    selector.register(sys.stdin, selectors.EVENT_READ)
    selector.register(client, selectors.EVENT_READ)
    try:
        while True:
            for key, _ in selector.select():
                if key.fileobj is client:
                    data = client.recv(65536)
                    if not data:
                        return
                    sys.stdout.buffer.write(data)
                    sys.stdout.buffer.flush()
                else:
                    data = os.read(sys.stdin.fileno(), 65536)
                    if not data:
                        return
                    client.sendall(data)
    finally:
        client.close()
        selector.close()


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--serve":
        serve(Path(sys.argv[2]))
    else:
        relay()
