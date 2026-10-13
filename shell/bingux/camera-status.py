#!/usr/bin/env python3
"""Publish camera and screen-sharing ownership from the PipeWire graph."""

import json
import os
import selectors
import signal
import subprocess
import time

MAX_FRAME_BYTES = 4 * 1024 * 1024
MAX_OBJECTS = 4096
MAX_GRAPH_BYTES = 4 * 1024 * 1024
FRAME_TIMEOUT = 3
RECONNECT_DELAY = 2
PROPERTIES = (
    "media.role",
    "application.name",
    "application.process.binary",
    "application.id",
    "application.desktop",
    "node.description",
    "media.name",
    "pipewire.access",
    "pipewire.access.portal.media_roles",
    "pipewire.access.portal.app_id",
)


def captures(nodes):
    result = []
    for node in nodes:
        info = node.get("info", {})
        props = info.get("props", {})
        # This is the same marker used by GNOME Shell's CameraMonitor.
        if props.get("media.role") != "Camera" or info.get("state") != "running":
            continue
        app = props.get("application.name") or props.get("application.process.binary") or "Unknown application"
        app_id = (
            props.get("application.id")
            or props.get("application.desktop")
            or props.get("application.process.binary")
            or ""
        )
        device = props.get("node.description") or props.get("media.name") or "Camera"
        entry = {"app": app, "appId": app_id, "device": device}
        if entry not in result:
            result.append(entry)
    return result


def screen_sharing_captures(objects):
    result = []
    for obj in objects:
        props = obj.get("info", {}).get("props", {})
        # ScreenCast portal connections have an explicit empty media-role set.
        # Camera connections use "Camera"; ordinary Flatpak clients have no set.
        if (
            obj.get("type") != "PipeWire:Interface:Client"
            or props.get("pipewire.access") != "portal"
            or props.get("pipewire.access.portal.media_roles") != ""
        ):
            continue
        app_id = (
            props.get("pipewire.access.portal.app_id")
            or props.get("application.id")
            or props.get("application.desktop")
            or props.get("application.process.binary")
            or ""
        )
        app = props.get("application.name") or props.get("application.process.binary") or "Unknown application"
        entry = {"app": app, "appId": app_id, "device": "Screen sharing"}
        if entry not in result:
            result.append(entry)
    return result


def monitor_frames(stream):
    """Read pw-dump's adjacent JSON arrays without assuming read boundaries.

    pw-dump line-buffers its output, but a pipe read can split UTF-8 or combine
    arrays. Only startup and incomplete frames have a deadline; idle has none.
    """
    buffer = bytearray()
    depth = 0
    quoted = escaped = False
    deadline = time.monotonic() + FRAME_TIMEOUT
    with selectors.DefaultSelector() as selector:
        selector.register(stream, selectors.EVENT_READ)
        while True:
            if deadline is not None and time.monotonic() >= deadline:
                raise ValueError("PipeWire graph frame timed out")
            timeout = None if deadline is None else max(0, deadline - time.monotonic())
            if not selector.select(timeout):
                raise ValueError("PipeWire graph frame timed out")
            chunk = os.read(stream.fileno(), 65536)
            if not chunk:
                raise OSError("PipeWire monitor closed")
            for byte in chunk:
                if not buffer:
                    if byte in b" \t\r\n":
                        continue
                    if byte != ord("["):
                        raise ValueError("PipeWire graph is not a JSON array")
                    deadline = time.monotonic() + FRAME_TIMEOUT
                buffer.append(byte)
                if len(buffer) > MAX_FRAME_BYTES:
                    raise ValueError("PipeWire graph frame exceeds limit")
                if quoted:
                    if escaped:
                        escaped = False
                    elif byte == ord("\\"):
                        escaped = True
                    elif byte == ord('"'):
                        quoted = False
                elif byte == ord('"'):
                    quoted = True
                elif byte == ord("["):
                    depth += 1
                elif byte == ord("]"):
                    depth -= 1
                    if depth == 0:
                        frame = json.loads(buffer)
                        buffer.clear()
                        deadline = None
                        yield frame


def update_graph(graph, sizes, frame):
    """pw-dump emits complete changed objects, plus info/props:null removals.

    Keep only the Node/Client fields needed for privacy, not formats, params or
    unrelated graph objects. Properties replace earlier properties, never merge.
    """
    total = sum(sizes.values())
    for obj in frame:
        if not isinstance(obj, dict) or type(obj.get("id")) is not int:
            raise ValueError("Invalid PipeWire object")
        identity = obj["id"]
        if not 0 <= identity <= 0xFFFFFFFF:
            raise ValueError("Invalid PipeWire object ID")
        total -= sizes.pop(identity, 0)
        if obj.get("info", True) is None or obj.get("props", True) is None:
            graph.pop(identity, None)
            continue
        if obj.get("type") not in ("PipeWire:Interface:Node", "PipeWire:Interface:Client"):
            graph.pop(identity, None)
            continue
        info = obj.get("info")
        if not isinstance(info, dict) or not isinstance(info.get("props", {}), dict):
            raise ValueError("Invalid PipeWire object info")
        props = {key: value for key, value in info.get("props", {}).items() if key in PROPERTIES}
        # pw-dump converts numeric/boolean-looking strings to JSON scalars.
        # Keep the same values as snapshot detection; reject only containers.
        if any(not isinstance(value, (str, int, float, bool, type(None))) for value in props.values()):
            raise ValueError("Invalid PipeWire privacy property")
        retained = {"type": obj["type"], "info": {"state": info.get("state"), "props": props}}
        size = len(json.dumps(retained).encode())
        total += size
        if (identity not in graph and len(graph) >= MAX_OBJECTS) or total > MAX_GRAPH_BYTES:
            raise ValueError("PipeWire privacy graph exceeds limit")
        # Assignment preserves owner order on updates with unchanged details.
        graph[identity] = retained
        sizes[identity] = size


def stop_monitor(process):
    if process.poll() is None:
        process.terminate()
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=1)
    finally:
        process.stdout.close()


def watch():
    previous = None
    while True:
        process = None
        try:
            process = subprocess.Popen(
                ["pw-dump", "-N", "--monitor"],
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                stdin=subprocess.DEVNULL,
            )
            graph, sizes = {}, {}
            for frame in monitor_frames(process.stdout):
                update_graph(graph, sizes, frame)
                current = {
                    "available": True,
                    "captures": captures(graph.values()),
                    "screenSharingCaptures": screen_sharing_captures(graph.values()),
                }
                if current != previous:
                    print(json.dumps(current), flush=True)
                    previous = current
        except BrokenPipeError:
            return
        except (OSError, ValueError, RecursionError, subprocess.SubprocessError):
            current = {"available": False, "captures": [], "screenSharingCaptures": []}
            if current != previous:
                print(json.dumps(current), flush=True)
                previous = current
        finally:
            if process:
                stop_monitor(process)
        time.sleep(RECONNECT_DELAY)


def stop_watcher(_signum, _frame):
    # Unwind the monitor's finally block when QML stops its helper.
    raise SystemExit(0)


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, stop_watcher)
    signal.signal(signal.SIGINT, stop_watcher)
    watch()
