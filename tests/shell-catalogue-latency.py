#!/usr/bin/env python3
"""Live regression: desktop-entry updates must not starve Bingux shell IPC.

Run in the running Bingux desktop. Creates and removes only its own hidden
launcher; does not launch or close applications. Override QS_BIN if necessary.
"""

import concurrent.futures
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def main():
    shell = Path(__file__).resolve().parents[1] / "shell/bingux"
    command = [
        os.environ.get("QS_BIN", "qs"),
        "ipc",
        "--any-display",
        "-p",
        str(shell / "CaptureShell.qml"),
        "call",
        "capture",
        "status",
    ]
    applications = Path.home() / ".local/share/applications"
    applications.mkdir(parents=True, exist_ok=True)
    # Reserve a unique name; never replace an existing user launcher.
    fd, name = tempfile.mkstemp(prefix="bingux-latency-", suffix=".desktop", dir=applications)
    os.close(fd)
    entry = Path(name)

    def mutate():
        time.sleep(1)
        for revision in range(5):
            entry.write_text(
                "[Desktop Entry]\nType=Application\n"
                f"Name=Bingux latency probe {revision}\n"
                "Exec=/usr/bin/true\nNoDisplay=true\n"
            )
            time.sleep(0.2)
        entry.unlink()

    try:
        with concurrent.futures.ThreadPoolExecutor() as pool:
            work = pool.submit(mutate)
            samples = []
            deadline = time.monotonic() + 12
            while time.monotonic() < deadline:
                start = time.monotonic()
                response = subprocess.run(command, capture_output=True, text=True, timeout=3, check=True)
                json.loads(response.stdout)  # Not-ready/error output is not a successful response.
                samples.append(time.monotonic() - start)
                time.sleep(0.05)
            work.result()
        print(
            json.dumps(
                {"samples": len(samples), "max_seconds": max(samples), "median_seconds": statistics.median(samples)}
            )
        )
        assert max(samples) < 0.5, "Desktop-entry updates stalled the shell for over 500 ms"
    finally:
        entry.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
