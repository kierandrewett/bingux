#!/usr/bin/env python3
"""Signal the selected process without risking a reused PID."""

import argparse
import json
import sys
import os
import signal
import subprocess


def act(pid, start_time, action):
    if action == "copy":
        subprocess.run(["wl-copy", str(pid)], check=True)
        return "PID copied"
    signals = {"pause": signal.SIGSTOP, "resume": signal.SIGCONT, "end": signal.SIGTERM, "kill": signal.SIGKILL}
    if pid <= 1 or start_time <= 0 or action not in signals:
        raise ValueError("Invalid process selection")
    fd = os.pidfd_open(pid)
    try:
        with open(f"/proc/{pid}/stat") as source:
            fields = source.read().rsplit(") ", 1)[1].split()
        if int(fields[19]) != start_time:
            raise ValueError("The selected process has exited")
        signal.pidfd_send_signal(fd, signals[action])
    finally:
        os.close(fd)
    return {
        "pause": "Process paused",
        "resume": "Process resumed",
        "end": "End requested",
        "kill": "Force quit requested",
    }[action]


def act_batch(processes, action):
    if not isinstance(processes, list) or not 1 <= len(processes) <= 8192:
        raise ValueError("Invalid process selection")
    processes = list({(int(p["pid"]), int(p["startTime"])) for p in processes})
    if action == "copy":
        subprocess.run(["wl-copy", "\n".join(str(pid) for pid, _ in processes)], check=True)
        return "PID copied" if len(processes) == 1 else f"{len(processes)} PIDs copied"
    if len(processes) == 1:
        return act(*processes[0], action)
    succeeded, errors = 0, []
    for pid, start_time in processes:
        try:
            act(pid, start_time, action)
            succeeded += 1
        except (OSError, ValueError) as error:
            errors.append(f"PID {pid}: {error}")
    label = {"pause": "Paused", "resume": "Resumed", "end": "End requested for", "kill": "Force quit requested for"}[
        action
    ]
    message = f"{label} {succeeded} processes"
    if errors:
        message += f"; {len(errors)} failed. " + "; ".join(errors[:3])
    return message


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--batch", choices=["pause", "resume", "end", "kill", "copy"])
    parser.add_argument("pid", type=int, nargs="?")
    parser.add_argument("start_time", type=int, nargs="?")
    parser.add_argument("action", choices=["pause", "resume", "end", "kill", "copy"], nargs="?")
    args = parser.parse_args()
    try:
        if args.batch:
            print(act_batch(json.loads(sys.stdin.readline(8 * 1024 * 1024)), args.batch))
        else:
            print(act(args.pid, args.start_time, args.action))
    except PermissionError:
        print("Permission denied for this process")
        raise SystemExit(1)
    except ProcessLookupError:
        print("The selected process has exited")
        raise SystemExit(1)
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as error:
        print(str(error))
        raise SystemExit(1)
