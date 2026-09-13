#!/usr/bin/env python3
"""Publish camera capture ownership from the PipeWire graph."""

import json
import subprocess
import time


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


def snapshot():
    nodes = json.loads(subprocess.check_output(["pw-dump", "-N"], timeout=3))
    return {"available": True, "captures": captures(nodes)}


def watch():
    previous = None
    while True:
        try:
            current = snapshot()
            if current != previous:
                print(json.dumps(current), flush=True)
                previous = current
        except (OSError, ValueError, subprocess.SubprocessError):
            current = {"available": False, "captures": []}
            if current != previous:
                print(json.dumps(current), flush=True)
                previous = current
        time.sleep(1)


if __name__ == "__main__":
    watch()
