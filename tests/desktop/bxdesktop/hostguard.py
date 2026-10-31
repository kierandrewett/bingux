"""What a test run must not change on the host, and how to check it.

`HostSnapshot.take()` records the host processes that belong to the user's
real desktop and the user's session variables. After a run `compare` reports
what changed. The check is deliberately narrow, because a developer machine
(and a CI runner) has other processes starting and stopping all the time:

* every host process of the kinds the stack starts that belongs to the user's real session (same PID
  namespace as the test run, `XDG_RUNTIME_DIR` is `/run/user/UID`; other sessions, such as another
  developer tool's private Gnoblin, start and stop independently and are not evidence) (the host's own
  compositor, shell, portals, session bus) must still be running, with the same
  start time. A teardown that killed a host Quickshell would show here;
* no process may remain in the session's PID namespace (`Session.leftover`).

It does not compare the whole process table: a process that appeared or
vanished for another reason is not evidence about this run.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path

# Process names (the `comm` of /proc/PID/stat) of the programs the session starts.
WATCHED = (
    "gnoblin",
    "mutter-devkit",
    "quickshell",
    "qs",
    "Xwayland",
    "bingux-statusd",
    "bingux-searchd",
    "xdg-desktop-portal",
    "gnome-shell",
)

# Variables that name the host's desktop. A session must not see any of them.
HOST_SESSION_VARIABLES = (
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "HOME",
    "XDG_SESSION_ID",
    "XDG_SESSION_TYPE",
    "SWAYSOCK",
    "SSH_AUTH_SOCK",
    "GNOBLIN_COMPOSITOR_SOCKET",
    "QS_CONFIG_PATH",
)


@dataclass(frozen=True)
class HostProcess:
    pid: int
    start: int  # field 22 of /proc/PID/stat, in clock ticks since boot: with the PID it names one process
    comm: str


@dataclass
class HostSnapshot:
    processes: dict[int, HostProcess] = field(default_factory=dict)
    session_variables: dict[str, str] = field(default_factory=dict)

    @classmethod
    def take(cls) -> HostSnapshot:
        processes = {}
        uid = os.getuid()
        runtime = f"XDG_RUNTIME_DIR=/run/user/{uid}".encode()
        own_namespace = os.readlink("/proc/self/ns/pid")
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            try:
                if entry.stat().st_uid != uid:
                    continue
                stat = (entry / "stat").read_text()
                comm = stat[stat.index("(") + 1 : stat.rindex(")")]
                start = int(stat[stat.rindex(")") + 2 :].split()[19])
                if comm not in WATCHED:
                    continue
                if os.readlink(entry / "ns" / "pid") != own_namespace:
                    continue
                if runtime not in (entry / "environ").read_bytes().split(b"\0"):
                    continue
            except (OSError, ValueError, IndexError):
                continue
            if comm in WATCHED:
                processes[int(entry.name)] = HostProcess(int(entry.name), start, comm)
        return cls(processes, {name: os.environ[name] for name in HOST_SESSION_VARIABLES if name in os.environ})


def compare(before: HostSnapshot, after: HostSnapshot) -> list[str]:
    """Problems with the host after a run; empty when it is as it was."""
    problems = []
    for pid, process in sorted(before.processes.items()):
        now = after.processes.get(pid)
        if now is None:
            problems.append(f"host process {process.comm} (pid {pid}) is gone")
        elif now.start != process.start:
            problems.append(f"host pid {pid} ({process.comm}) was replaced by a different process")
    if before.session_variables != after.session_variables:
        problems.append("the test process's own session variables changed")
    return problems
