"""The bubblewrap command line that isolates a session from the host.

`bwrap_argv` is a pure function, so the isolation rules are unit-tested
(tests/desktop/fast/test_sandbox.py) without starting anything.

What the sandbox gives a session:

* new user, mount, PID, IPC, UTS and network namespaces. The network
  namespace has only a down loopback, so abstract D-Bus sockets, X11 sockets
  and the host's services are unreachable, not merely unused;
* a root made of the host's `/usr` (read-only) and a tmpfs: nothing else of the
  host is visible. `/etc` is read-only and `/etc/xdg` is an empty tmpfs, so no
  host autostart entry (blueman, solaar, a VM agent) can start;
* `/run/user/UID`, `$HOME`, `/tmp` and `/logs` are directories of the run's
  temporary root, mounted read-write. These are the only places a session
  writes, so tearing the root down removes everything it made;
* the checkout, Gnoblin's prefix, the Bingux prefix and Quickshell's directory,
  mounted read-only at their own paths;
* a cleared environment. Only variables the session sets are present.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path


@dataclass(frozen=True)
class SandboxSpec:
    root: Path  # the run's temporary root; holds run/, home/, tmp/, logs/
    uid: int
    home: str  # the path $HOME has inside the sandbox
    env: dict[str, str]
    read_only: tuple[Path, ...] = ()  # mounted at their own path, read-only
    overlays: tuple[tuple[Path, Path], ...] = ()  # (host source, sandbox target), read-only
    dri: bool = False  # expose /dev/dri (GPU rendering); off by default
    command: tuple[str, ...] = ("true",)
    extra_args: tuple[str, ...] = field(default=())


def bwrap_argv(spec: SandboxSpec, bwrap: str = "bwrap") -> list[str]:
    runtime = f"/run/user/{spec.uid}"
    argv = [
        bwrap,
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--clearenv",
        "--ro-bind", "/usr", "/usr",
        "--ro-bind", "/etc", "/etc",
        "--tmpfs", "/etc/xdg",
        "--symlink", "usr/lib", "/lib",
        "--symlink", "usr/lib64", "/lib64",
        "--symlink", "usr/bin", "/bin",
        "--symlink", "usr/sbin", "/sbin",
        "--dev", "/dev",
        "--proc", "/proc",
        "--tmpfs", "/var",
        "--tmpfs", "/run",
        "--dir", runtime,
        "--bind", str(spec.root / "run"), runtime,
        "--bind", str(spec.root / "tmp"), "/tmp",
        "--bind", str(spec.root / "logs"), "/logs",
        "--bind", str(spec.root / "home"), spec.home,
    ]  # fmt: skip
    if spec.dri:
        argv += ["--dev-bind", "/dev/dri", "/dev/dri"]
    for path in spec.read_only:
        argv += ["--ro-bind", str(path), str(path)]
    for source, target in spec.overlays:
        argv += ["--ro-bind", str(source), str(target)]
    for name, value in sorted(spec.env.items()):
        argv += ["--setenv", name, value]
    argv += list(spec.extra_args)
    argv += ["--", *spec.command]
    return argv


def nsenter_argv(target_pid: int, command: list[str], cwd: str = "/", nsenter: str = "nsenter") -> list[str]:
    """Run `command` inside the namespaces of a process in the sandbox.

    `--preserve-credentials` is required: without it nsenter calls setgroups,
    which an unprivileged user namespace forbids. `-r` sets the root to the
    target's root and the working directory to `/`; `--wd` does not, and leaves
    the process with a working directory from the host's mount (getcwd fails).
    The environment of the new process is the caller's, so callers pass an
    explicit one.
    """
    if cwd != "/":
        command = ["/bin/sh", "-c", 'cd "$1" && shift && exec "$@"', "sh", cwd, *command]
    return [
        nsenter,
        "--target", str(target_pid),
        "--user", "--preserve-credentials",
        "--mount", "--pid", "--net", "--ipc", "--uts",
        "-r",
        "--", *command,
    ]  # fmt: skip
