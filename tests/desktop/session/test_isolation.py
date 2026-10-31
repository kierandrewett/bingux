"""The session cannot reach the host's desktop, session bus, home directory or autostart apps.

Each test states what a session might reach and shows, from inside the sandbox, that it does not.
The last two tests run their own sessions to check what happens to the host around a whole run.
"""

import os
import re
import subprocess
from pathlib import Path

import pytest

from bxdesktop import Session
from bxdesktop.hostguard import HOST_SESSION_VARIABLES, HostSnapshot, compare
from bxdesktop.session import processes_in_namespace

pytestmark = pytest.mark.needs_gnoblin

HOST_RUNTIME = Path(os.environ.get("XDG_RUNTIME_DIR") or f"/run/user/{os.getuid()}")
HOST_HOME = Path(os.environ.get("HOME", "/nonexistent"))
# Entries of the sandbox's home that exist because the session writes there, or only as the
# parents of read-only mounts (the checkout, a prefix) that live under the user's home.
HOME_ENTRIES_OF_THE_SESSION = {".config", ".cache", ".local"}


def inside(desktop, *argv, display=False) -> str:
    return desktop.run(list(argv), display=display).stdout


def mount_parents(desktop) -> set[str]:
    """Top-level names under $HOME that exist only as parents of the sandbox's read-only mounts."""
    home = Path(desktop.home)
    names = set()
    for path in (desktop.stack.repo, desktop.stack.gnoblin_prefix, desktop.stack.quickshell.parent):
        for prefix in (desktop.stack.bingux_prefix, desktop.stack.cargo_target, path):
            if prefix and home in Path(prefix).resolve().parents:
                names.add(Path(prefix).resolve().relative_to(home).parts[0])
    return names


def test_the_environment_inside_holds_only_what_the_harness_set(desktop):
    seen = dict(line.split("=", 1) for line in inside(desktop, "env").splitlines() if "=" in line)
    allowed = set(desktop.sandbox_env()) | {"PWD", "SHLVL", "_", "OLDPWD"}
    assert set(seen) <= allowed, f"variables nobody set: {sorted(set(seen) - allowed)}"
    # What names the host's desktop is either absent or points into the sandbox.
    assert "DISPLAY" not in seen and "WAYLAND_DISPLAY" not in seen  # display=False: no client display either
    assert seen["DBUS_SESSION_BUS_ADDRESS"] == f"unix:path=/run/user/{os.getuid()}/bus"
    for name in ("SSH_AUTH_SOCK", "SWAYSOCK", "XDG_SESSION_ID", "DBUS_STARTER_ADDRESS", "GNOME_SETUP_DISPLAY"):
        assert name not in seen


def test_decoy_host_variables_are_not_inherited(stack, artifact_root, monkeypatch):
    """Even a host that had these set would not leak them in: the harness never passes its environment on."""
    decoys = {
        "WAYLAND_DISPLAY": "wayland-decoy",
        "DISPLAY": ":99",
        "DBUS_SESSION_BUS_ADDRESS": "unix:path=/nonexistent/host-bus",
        "SSH_AUTH_SOCK": "/nonexistent/agent",
        "SWAYSOCK": "/nonexistent/sway.sock",
        "XDG_SESSION_ID": "424242",
        "GNOBLIN_COMPOSITOR_SOCKET": "/nonexistent/compositor.sock",
        "QS_CONFIG_PATH": "/nonexistent/qs",
        "LD_PRELOAD": "/nonexistent/preload.so",
    }
    for name, value in decoys.items():
        monkeypatch.setenv(name, value)
    with Session(stack, artifact_root / "decoy", services=()) as session:
        for display in (False, True):
            seen = dict(
                line.split("=", 1) for line in session.run(["env"], display=display).stdout.splitlines() if "=" in line
            )
            for name, value in decoys.items():
                assert seen.get(name) != value, f"{name} leaked into the session"
        assert session.run(["env"]).stdout.count("/nonexistent") == 0


def test_the_host_wayland_display_is_unreachable(desktop):
    candidates = {os.environ["WAYLAND_DISPLAY"]} if os.environ.get("WAYLAND_DISPLAY") else set()
    candidates |= {p.name for p in HOST_RUNTIME.glob("wayland-*") if not p.name.endswith(".lock")}
    candidates.add("wayland-0")
    for name in sorted(candidates):
        done = desktop.run(["grim", "/tmp/never.png"], env={"WAYLAND_DISPLAY": name}, check=False)
        assert (
            done.returncode != 0
            and "failed to create display" in done.stderr.lower()
            or "connect" in done.stderr.lower()
        ), f"grim on host display {name}: {done.stderr!r}"
    listing = set(inside(desktop, "ls", "-A", f"/run/user/{os.getuid()}").split())
    assert not (listing & {p.name for p in HOST_RUNTIME.glob("wayland-*")} - {"bx-0", "bx-0.lock"})


def test_the_runtime_directory_and_session_bus_are_the_sessions_own(desktop):
    uid = os.getuid()

    def identity(path):
        return tuple(inside(desktop, "stat", "-L", "-c", "%d:%i", path).split()[0].split(":"))

    host_runtime = os.stat(HOST_RUNTIME)
    assert identity(f"/run/user/{uid}") != (str(host_runtime.st_dev), str(host_runtime.st_ino))
    host_bus = HOST_RUNTIME / "bus"
    if host_bus.exists():
        stat = os.stat(host_bus)
        assert identity(f"/run/user/{uid}/bus") != (str(stat.st_dev), str(stat.st_ino)), "the host's bus is mounted in"
    bus_id = inside(
        desktop, "gdbus", "call", "--session", "--dest", "org.freedesktop.DBus",
        "--object-path", "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.GetId",
    )  # fmt: skip
    host = subprocess.run(
        ["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus",
         "--method", "org.freedesktop.DBus.GetId"],
        capture_output=True, text=True, check=False, timeout=10,
    )  # fmt: skip
    if host.returncode == 0:  # a developer machine with a session bus; in CI there is none
        assert bus_id != host.stdout
    names = desktop.run(["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path",
                         "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.ListNames"]).stdout  # fmt: skip
    assert "org.gnome.Mutter.RemoteDesktop" in names  # it is Gnoblin's bus
    for host_service in ("org.gnome.Shell", "org.freedesktop.NetworkManager", "org.freedesktop.login1"):
        assert host_service not in names, f"{host_service} is on the session's bus"


def test_the_host_home_is_not_visible(desktop):
    seen = set(inside(desktop, "ls", "-A", desktop.home).split())
    extra = seen - HOME_ENTRIES_OF_THE_SESSION - mount_parents(desktop)
    assert not extra, f"entries of the sandbox's home that the session did not make: {sorted(extra)}"
    for private in (".ssh", ".gnupg", ".bash_history", ".mozilla", "Documents", "Downloads", ".config/autostart"):
        assert desktop.run(["test", "-e", f"{desktop.home}/{private}"], check=False).returncode != 0, private
    if HOST_HOME.is_dir():  # whatever the host keeps in its home, none of it is here
        host_names = (
            {entry.name for entry in HOST_HOME.iterdir()} - HOME_ENTRIES_OF_THE_SESSION - mount_parents(desktop)
        )
        assert not (host_names & seen)


def autostart_commands() -> set[str]:
    """Program names the host would start at login: system and user autostart entries."""
    names = set()
    for folder in (Path("/etc/xdg/autostart"), HOST_HOME / ".config" / "autostart"):
        for entry in folder.glob("*.desktop") if folder.is_dir() else ():
            match = re.search(r"^Exec=(\S+)", entry.read_text(errors="replace"), re.MULTILINE)
            if match:
                names.add(Path(match.group(1)).name)
    return names


def test_host_autostart_entries_are_not_visible_and_not_running(desktop):
    assert inside(desktop, "ls", "-A", "/etc/xdg").split() == []
    assert desktop.sandbox_env()["XDG_CONFIG_DIRS"] == "/etc/xdg"
    running = {name.strip() for name in inside(desktop, "ps", "-eo", "comm=").splitlines()}
    started = autostart_commands() & running
    assert not started, f"host autostart programs are running in the session: {sorted(started)}"
    # What did start from autostart is Gnoblin's own list, from gnoblin/bingux.lua.
    assert {"bingux-background", "bingux", "bingux-popouts"} <= set(
        re.findall(r"gnoblin-autostart: started (\S+)", desktop.log_path("gnoblin.log").read_text())
    )


def test_the_network_is_a_private_loopback(desktop):
    interfaces = {
        line.split(":")[0].strip() for line in inside(desktop, "cat", "/proc/net/dev").splitlines()[2:] if ":" in line
    }
    assert interfaces == {"lo"}
    assert desktop.run(["sh", "-c", "exec 3<>/dev/tcp/1.1.1.1/80"], check=False, timeout=10).returncode != 0


def test_writes_land_in_the_runs_temporary_root_and_nowhere_else_on_the_host(desktop):
    marker = f"isolation-marker-{os.getpid()}"
    uid = os.getuid()
    targets = {
        "/tmp": desktop.root / "tmp",
        desktop.home: desktop.root / "home",
        f"/run/user/{uid}": desktop.root / "run",
        "/var/tmp": None,  # a tmpfs: not on the host at all
        "/": None,
    }
    for sandbox_path, host_path in targets.items():
        inside(desktop, "touch", f"{sandbox_path.rstrip('/')}/{marker}")
        if host_path is not None:
            assert (host_path / marker).exists()
    for host_path in (Path("/tmp"), Path("/var/tmp"), HOST_HOME, HOST_RUNTIME, Path("/")):
        assert not (host_path / marker).exists(), f"{host_path / marker} exists on the host"
    desktop.run(["rm", "-f", *(f"{p.rstrip('/')}/{marker}" for p in targets)], check=False)


def test_a_whole_run_leaves_the_host_as_it_was(stack, artifact_root):
    before = HostSnapshot.take()
    temporary = Path(os.environ.get("BINGUX_TEST_TMP") or "/dev/shm")
    with Session(stack, artifact_root / "host-guard") as session:
        root = session.root
        session.launch(["foot", "sleep", "600"], title="foot")
        session.screenshot("host-guard")
        namespace = session.namespaces["pid"]
        assert processes_in_namespace(namespace), "the session has no processes?"
        marker = root / "tmp" / "marker"
        marker.write_text("x")
    # Everything of the session is gone: processes, the root with its tmp, run and home.
    assert session.leftover == []
    assert processes_in_namespace(namespace) == []
    assert not root.exists() and not marker.exists()
    assert not [p for p in temporary.glob("bingux-desktop-*") if p == root]
    assert compare(before, HostSnapshot.take()) == []
    assert (artifact_root / "host-guard" / "logs" / "gnoblin.log").is_file()  # the logs survived, outside the root
    assert set(HOST_SESSION_VARIABLES) >= {"WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"}
