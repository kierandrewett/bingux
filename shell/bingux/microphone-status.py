#!/usr/bin/env python3
"""Publish input capture ownership, refreshed by PulseAudio/PipeWire events."""

import json
import os
import subprocess
import time


def captures(sources, outputs):
    devices = {source["index"]: source for source in sources}
    result = []
    for stream in outputs:
        props = stream.get("properties", {})
        source = devices.get(stream.get("source"))
        # Peak meters and sink monitors do not capture microphone samples.
        if any(
            str(props.get(key, "")).lower() == "true"
            for key in ("stream.monitor", "stream.capture.sink", "resample.peaks")
        ):
            continue
        if source is not None:
            monitor = source.get("monitor_of_sink", source.get("monitor_of_sink_name"))
            if source.get("name", "").endswith(".monitor") or monitor not in (None, "", 4294967295, "4294967295"):
                continue
        if stream.get("corked") is True:
            continue
        app = props.get("application.name") or props.get("application.process.binary") or "Unknown application"
        device = source.get("description", source.get("name", "Audio input")) if source else "Audio input"
        entry = {"app": app, "device": device, "muted": bool(stream.get("mute") or (source or {}).get("mute"))}
        if entry not in result:
            result.append(entry)
    return result


def snapshot():
    def read(kind):
        return json.loads(subprocess.check_output(["pactl", "-f", "json", "list", kind], timeout=3))

    return {"available": True, "captures": captures(read("sources"), read("source-outputs"))}


def affects_capture(event):
    # Queries create short-lived clients. Reacting to those events would make
    # each snapshot schedule another snapshot indefinitely.
    facility = event.strip().rsplit(" on ", 1)[-1].split(" ", 1)[0]
    return facility in ("source", "source-output", "server")


def watch():
    while True:
        process = None
        try:
            process = subprocess.Popen(
                ["pactl", "subscribe"], stdout=subprocess.PIPE, text=True, env={**os.environ, "LC_ALL": "C"}
            )
            previous = snapshot()
            print(json.dumps(previous), flush=True)
            for event in process.stdout:
                if affects_capture(event):
                    current = snapshot()
                    if current != previous:
                        print(json.dumps(current), flush=True)
                        previous = current
        except (OSError, ValueError, subprocess.SubprocessError):
            print(json.dumps({"available": False, "captures": []}), flush=True)
        finally:
            if process:
                process.terminate()
                process.wait()
        time.sleep(2)


if __name__ == "__main__":
    watch()
