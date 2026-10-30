#!/usr/bin/env python3
"""Stand-in for the bsm bridge's $XDG_RUNTIME_DIR/bsm/ready.sock.

Accepts the JSON lines that Bingux's BsmReadiness.qml writes and checks every
one against the wire format in docs/bsm/shell-integration.md:

- one JSON object per line, at most 4096 bytes, UTF-8;
- generation is the expected decimal string, component is wallpaper or desktop,
  event is launched, surface or ready;
- surface: output is a non-empty string, role belongs to the component
  (wallpaper: wallpaper; desktop: bar or dock), mapped is a boolean;
- launched: required lists roles of the component;
- order per connection and component: nothing before launched, and ready only
  while every required role has a mapped surface and none is unmapped.

Violations are collected, never raised in the server thread.

As a library (tests/bsm-shell-integration.py):
    server = FakeReadySocket(path, "7"); server.start(); ...; server.stop()
As a command: bsm-fake-ready.py --socket PATH --generation 7 [--seconds 30]
prints each accepted event as a JSON line and exits 1 on any violation.
"""

import argparse
import json
from pathlib import Path
import socket
import sys
import threading
import time

COMPONENT_ROLES = {"wallpaper": {"wallpaper"}, "desktop": {"bar", "dock"}}
EVENTS = {"launched", "surface", "ready"}
MAX_LINE = 4096


class FakeReadySocket:
    def __init__(self, path, generation):
        self.path = Path(path)
        self.generation = generation
        self.events = []  # (monotonic time, connection number, event)
        self.violations = []
        self.connections = 0
        self._lock = threading.Lock()
        self._open = []
        self._server = None
        self._stopping = False

    def start(self):
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        if self.path.exists():
            self.path.unlink()
        self._server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._server.bind(str(self.path))
        self._server.listen(8)
        threading.Thread(target=self._accept, daemon=True).start()

    def stop(self):
        self._stopping = True
        self.drop_connections()
        if self._server:
            self._server.close()
        if self.path.exists():
            self.path.unlink()

    def drop_connections(self):
        with self._lock:
            for connection in self._open:
                try:
                    connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                connection.close()
            self._open = []

    def snapshot(self):
        with self._lock:
            return list(self.events)

    def wait_for(self, predicate, timeout):
        """Poll until predicate(events) is true; returns the events or None on timeout."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            events = self.snapshot()
            if predicate(events):
                return events
            time.sleep(0.05)
        return None

    def _accept(self):
        while not self._stopping:
            try:
                connection, _ = self._server.accept()
            except OSError:
                return
            with self._lock:
                self.connections += 1
                number = self.connections
                self._open.append(connection)
            threading.Thread(target=self._serve, args=(connection, number), daemon=True).start()

    def _serve(self, connection, number):
        state = {}  # component -> {"launched": roles or None, "mapped": {(output, role): bool}}
        buffer = b""
        while True:
            try:
                data = connection.recv(65536)
            except OSError:
                return
            if not data:
                return
            buffer += data
            while b"\n" in buffer:
                line, buffer = buffer.split(b"\n", 1)
                self._line(line, number, state)
            if len(buffer) > MAX_LINE:
                self._violation(number, "line longer than %d bytes without a newline" % MAX_LINE)
                buffer = b""

    def _violation(self, number, message):
        with self._lock:
            self.violations.append("connection %d: %s" % (number, message))

    def _line(self, raw, number, state):
        if len(raw) > MAX_LINE:
            self._violation(number, "line of %d bytes" % len(raw))
            return
        try:
            event = json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            self._violation(number, "not JSON: %r" % raw[:80])
            return
        if not isinstance(event, dict):
            self._violation(number, "not an object: %r" % raw[:80])
            return
        problem = self._check(event, state)
        if problem:
            self._violation(number, "%s: %s" % (problem, raw.decode("utf-8", "replace")))
        with self._lock:
            self.events.append((time.monotonic(), number, event))

    def _check(self, event, state):
        if event.get("generation") != self.generation or not isinstance(event.get("generation"), str):
            return "generation is not the expected decimal string %r" % self.generation
        component = event.get("component")
        if component not in COMPONENT_ROLES:
            return "unknown component"
        kind = event.get("event")
        if kind not in EVENTS:
            return "unknown event"
        known = state.setdefault(component, {"launched": None, "mapped": {}})
        if kind == "launched":
            required = event.get("required")
            if not isinstance(required, list) or not required or not set(required) <= COMPONENT_ROLES[component]:
                return "launched.required is not a list of this component's roles"
            known["launched"] = set(required)
            return None
        if known["launched"] is None:
            return "%s before launched" % kind
        if kind == "surface":
            output, role, mapped = event.get("output"), event.get("role"), event.get("mapped")
            if not isinstance(output, str) or not output:
                return "surface.output is not a non-empty string"
            if role not in COMPONENT_ROLES[component]:
                return "surface.role %r does not belong to %s" % (role, component)
            if not isinstance(mapped, bool):
                return "surface.mapped is not a boolean"
            known["mapped"][(output, role)] = mapped
            return None
        for role in known["launched"]:
            if not any(m for (_, r), m in known["mapped"].items() if r == role):
                return "ready while required role %s has no mapped surface" % role
        if not all(known["mapped"].values()):
            return "ready while a surface is unmapped"
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--socket", required=True)
    parser.add_argument("--generation", required=True)
    parser.add_argument("--seconds", type=float, default=30)
    arguments = parser.parse_args()
    server = FakeReadySocket(arguments.socket, arguments.generation)
    server.start()
    shown = 0
    deadline = time.monotonic() + arguments.seconds
    try:
        while time.monotonic() < deadline:
            events = server.snapshot()
            for _, number, event in events[shown:]:
                print(json.dumps({"connection": number, **event}), flush=True)
            shown = len(events)
            time.sleep(0.05)
    finally:
        server.stop()
    for violation in server.violations:
        print("VIOLATION " + violation, file=sys.stderr)
    return 1 if server.violations else 0


if __name__ == "__main__":
    sys.exit(main())
