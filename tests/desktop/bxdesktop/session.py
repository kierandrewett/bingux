"""`Session`: the isolated Gnoblin + Bingux desktop, started and torn down for a test run.

    with Session(stack, artefacts) as desktop:
        desktop.screenshot("boot")
        client = desktop.launch(["foot"])
        desktop.key("Return")

What starts, in order, inside one bubblewrap sandbox (`sandbox.py`):

1. a private `dbus-daemon` whose socket is in the session's runtime directory;
2. `gnoblin --devkit --wayland-display NAME`, with `GNOBLIN_DEVKIT_KEEP_SESSION=1`.
   Gnoblin's devkit mode asks Mutter to start a viewer window on a parent
   Wayland display and ends the session when the viewer exits. There is no
   parent display here, the viewer fails to open one, and the keep-session
   variable makes Mutter carry on, with its 1280x720 virtual monitor still
   rendered and still readable by `grim`. No second compositor is involved.
   (`GNOME_DEVKIT_HEADLESS`, which two Gnoblin scripts mention, is not read by
   any code in Mutter or Gnoblin.)
3. Gnoblin's autostart list from this checkout's `gnoblin/bingux.lua`: the
   wallpaper, the shell (`shell/bingux`) and the popouts;
4. `bingux-statusd` (CPU/RAM, network, notifications), when it is built;
5. an input helper (`inputd.py`) holding one RemoteDesktop session.

Commands are run inside the sandbox with `nsenter` (`run`, `spawn`), so a test
sees what a program in the session sees. Everything the session writes is in
`root` (a directory under `/dev/shm`, or `BINGUX_TEST_TMP`), which `stop`
removes; logs and screenshots go to the artefact directory.
"""

from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path

from . import imaging, keysyms
from .logs import LogReport, scan_logs
from .sandbox import SandboxSpec, bwrap_argv, nsenter_argv
from .stack import Stack

DISPLAY_NAME = "bx-0"
MONITOR = (1280, 720)

# Services of the real session that a test sandbox may activate on the bus.
# Anything else on the host's /usr/share/dbus-1/services (mail, calendar and
# contact servers, a sound server) stays unreachable.
DBUS_SERVICES = (
    "org.freedesktop.portal.Desktop",
    "org.freedesktop.impl.portal.PermissionStore",
    "org.freedesktop.impl.portal.desktop.gtk",
)


class SessionError(RuntimeError):
    pass


class CommandFailed(SessionError):
    """A command run in the session exited non-zero. The message carries its output."""

    def __init__(self, argv: list[str], done: subprocess.CompletedProcess):
        self.argv = argv
        self.returncode = done.returncode
        self.stdout = done.stdout
        self.stderr = done.stderr
        super().__init__(
            f"{' '.join(argv[:6])} exited with {done.returncode}\nstdout: {done.stdout.strip()[-800:]}\n"
            f"stderr: {done.stderr.strip()[-800:]}"
        )


def scratch_parent() -> Path:
    """Where a run's temporary root lives: BINGUX_TEST_TMP, else /dev/shm, else the system temp."""
    configured = os.environ.get("BINGUX_TEST_TMP")
    for candidate in (configured, "/dev/shm", tempfile.gettempdir()):
        if candidate and Path(candidate).is_dir() and os.access(candidate, os.W_OK):
            return Path(candidate)
    raise SessionError("no writable directory for the session root; set BINGUX_TEST_TMP")


def descendants(pid: int) -> set[int]:
    """Every live process below `pid`, read from /proc (host view)."""
    children: dict[int, list[int]] = {}
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
            children.setdefault(int(fields[1]), []).append(int(entry.name))
        except (OSError, IndexError):
            continue
    found: set[int] = set()
    todo = [pid]
    while todo:
        for child in children.get(todo.pop(), []):
            if child not in found:
                found.add(child)
                todo.append(child)
    return found


def processes_in_namespace(pid_namespace: str | None) -> list[int]:
    """Host PIDs of every live process in a PID namespace, given as the text of /proc/PID/ns/pid."""
    if not pid_namespace:
        return []
    found = []
    for entry in Path("/proc").iterdir():
        if entry.name.isdigit():
            try:
                if os.readlink(entry / "ns" / "pid") == pid_namespace and process_alive(int(entry.name)):
                    found.append(int(entry.name))
            except OSError:
                continue
    return found


def process_alive(pid: int) -> bool:
    try:
        state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
    except (OSError, IndexError):
        return False
    return state != "Z"


@dataclass
class Client:
    """A program started inside the session."""

    session: Session
    argv: list[str]
    process: subprocess.Popen
    log: Path
    window: dict | None = None
    service: bool = False  # started by the session itself (statusd), not by a test

    def close(self, timeout: float = 5.0) -> None:
        if self.window and self.session.alive:
            self.session.gnoblinctl("window", "close", str(self.window.get("id", "")), check=False)
        try:
            self.process.wait(timeout)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(3)
            except subprocess.TimeoutExpired:
                self.process.kill()


@dataclass
class Session:
    stack: Stack
    artefacts: Path
    name: str = "session"
    gpu: bool = False
    shell: bool = True
    services: tuple[str, ...] = ("bingux-statusd",)
    env: dict[str, str] = field(default_factory=dict)  # extra variables for everything in the session

    root: Path = field(init=False)
    _bwrap: subprocess.Popen | None = field(init=False, default=None)
    _inner_pid: int = field(init=False, default=0)
    _spawned: list[Client] = field(init=False, default_factory=list)
    _input: subprocess.Popen | None = field(init=False, default=None)
    _tree: set[int] = field(init=False, default_factory=set)
    _shots: int = field(init=False, default=0)
    started_at: float = field(init=False, default=0.0)
    namespaces: dict[str, str] = field(init=False, default_factory=dict)
    boot: dict = field(init=False, default_factory=dict)  # facts recorded when the desktop became ready
    leftover: list[int] = field(init=False, default_factory=list)

    # -- layout --------------------------------------------------------------

    @property
    def uid(self) -> int:
        return os.getuid()

    @property
    def runtime(self) -> str:
        return f"/run/user/{self.uid}"

    @property
    def home(self) -> str:
        return os.environ.get("HOME") or f"/home/{os.environ.get('USER', 'bingux')}"

    @property
    def alive(self) -> bool:
        return self._bwrap is not None and self._bwrap.poll() is None

    def sandbox_env(self) -> dict[str, str]:
        """The complete environment of every process in the session. Nothing is inherited."""
        stack = self.stack
        home = self.home
        bingux = stack.bingux_prefix
        data_dirs = [stack.gnoblin_prefix / "share"]
        if bingux:
            data_dirs.append(bingux / "share")
        data_dirs += [Path("/usr/local/share"), Path("/usr/share")]
        path = [str(stack.gnoblin_prefix / "bin"), "/tmp/bin"]
        if bingux:
            path.append(str(bingux / "bin"))
        path += ["/usr/local/bin", "/usr/bin"]
        env = {
            "HOME": home,
            "USER": os.environ.get("USER", "bingux"),
            "LANG": "C.UTF-8",
            "TZ": "UTC",
            "PATH": ":".join(path),
            "XDG_RUNTIME_DIR": self.runtime,
            "XDG_CONFIG_HOME": f"{home}/.config",
            "XDG_DATA_HOME": f"{home}/.local/share",
            "XDG_CACHE_HOME": f"{home}/.cache",
            "XDG_STATE_HOME": f"{home}/.local/state",
            "XDG_CONFIG_DIRS": "/etc/xdg",  # an empty tmpfs in the sandbox: no host autostart
            "XDG_DATA_DIRS": ":".join(str(d) for d in data_dirs),
            "XDG_CURRENT_DESKTOP": "Gnoblin",
            "XDG_SESSION_DESKTOP": "gnoblin",
            "XDG_SESSION_TYPE": "wayland",
            "DBUS_SESSION_BUS_ADDRESS": f"unix:path={self.runtime}/bus",
            "GSETTINGS_BACKEND": "keyfile",
            "GIO_USE_VFS": "local",
            "GVFS_DISABLE_FUSE": "1",
            "GTK_A11Y": "none",
            "NO_AT_BRIDGE": "1",
            "GDK_BACKEND": "wayland",
            "QT_QPA_PLATFORM": "wayland",
            "CLUTTER_BACKEND": "wayland",
            "GNOBLIN_DEVKIT_KEEP_SESSION": "1",
            "QT_LOGGING_RULES": "qt.qpa.services=false",
        }
        env.update({k: v for k, v in stack.gnoblin_env.items() if k not in ("XDG_DATA_DIRS", "GNOBLIN_LIBDIR")})
        env["GNOBLIN_LIBDIR"] = stack.gnoblin_env.get("GNOBLIN_LIBDIR", "lib64")
        if not self.gpu:
            env["LIBGL_ALWAYS_SOFTWARE"] = "1"
        if bingux:
            env.update(
                {
                    "QML_IMPORT_PATH": str(bingux / "lib" / "bingux" / "qml"),
                    "BINGUX_ADWAITA_ICON_DIR": str(bingux / "share" / "bingux" / "icons" / "Adwaita"),
                    "BINGUX_AUDIO_METER": str(bingux / "bin" / "bingux-audio-meter"),
                }
            )
        env.update(
            {
                "BINGUX_QUICKSHELL": str(stack.quickshell),
                "BINGUX_CONFIG_PATH": str(stack.shell_dir),
                "BINGUX_PYTHON": "/usr/bin/python3",
            }
        )
        for program, variable in (
            ("bingux-config", "BINGUX_CONFIG_BIN"),
            ("bingux-settings", "BINGUX_SETTINGS_BIN"),
            ("bingux-statusd", "BINGUX_STATUSD_BIN"),
        ):
            found = stack.binary(program)
            if found:
                env[variable] = str(found)
        env.update(self.env)
        return env

    def client_env(self) -> dict[str, str]:
        env = self.sandbox_env()
        env["WAYLAND_DISPLAY"] = DISPLAY_NAME
        return env

    # -- life cycle ----------------------------------------------------------

    def start(self, timeout: float = 90.0) -> Session:
        if self._bwrap is not None:
            raise SessionError("session already started")
        self.started_at = time.monotonic()
        self.artefacts.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="bingux-desktop-", dir=scratch_parent()))
        for sub in ("run", "tmp", "home", "logs", "tmp/bin"):
            (self.root / sub).mkdir(mode=0o700)
        try:
            self._prepare()
            self._launch(timeout)
        except BaseException:
            self.stop()
            raise
        return self

    def _prepare(self) -> None:
        stack = self.stack
        home = self.root / "home"
        config = home / ".config" / "gnoblin"
        config.mkdir(parents=True)
        (config / "init.lua").write_text(
            "-- Written by tests/desktop: the checkout's Bingux module, nothing else.\n"
            f'gnoblin.load("{stack.repo}/gnoblin/bingux.lua")\n'
        )
        # The commands gnoblin/bingux.lua autostarts, as the packaged launchers run them.
        for command, entry in (
            ("bingux", ""),
            ("bingux-wallpaper", "/WallpaperShell.qml"),
            ("bingux-popouts", "/PopoutsShell.qml"),
        ):
            launcher = self.root / "tmp" / "bin" / command
            launcher.write_text(f'#!/bin/sh\nexec "$BINGUX_QUICKSHELL" -p "$BINGUX_CONFIG_PATH{entry}" "$@"\n')
            launcher.chmod(0o755)
        services = self.root / "dbus-services"
        services.mkdir()
        host_services = Path("/usr/share/dbus-1/services")
        for service in DBUS_SERVICES:
            source = host_services / f"{service}.service"
            if source.is_file():
                shutil.copy2(source, services / source.name)
        (self.root / "tmp" / "dbus.conf").write_text(self._dbus_config(services))
        (self.root / "tmp" / "init.sh").write_text(self._init_script())

    def _dbus_config(self, services: Path) -> str:
        gnoblin_services = self.stack.gnoblin_prefix / "share" / "dbus-1" / "services"
        extra = f"  <servicedir>{gnoblin_services}</servicedir>\n" if gnoblin_services.is_dir() else ""
        return (
            '<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"\n'
            ' "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">\n'
            "<busconfig>\n  <type>session</type>\n  <keep_umask/>\n"
            f"  <listen>unix:path={self.runtime}/bus</listen>\n"
            "  <auth>EXTERNAL</auth>\n"
            f"  <servicedir>{services}</servicedir>\n{extra}"
            '  <policy context="default">\n    <allow send_destination="*" eavesdrop="true"/>\n'
            '    <allow eavesdrop="true"/>\n    <allow own="*"/>\n  </policy>\n'
            '  <limit name="max_incoming_unix_fds">250000000</limit>\n'
            '  <limit name="max_outgoing_unix_fds">250000000</limit>\n'
            '  <limit name="service_start_timeout">120000</limit>\n'
            "</busconfig>\n"
        )

    def _init_script(self) -> str:
        # The sandbox's first process. gnoblin is exec'd so that it, not a shell,
        # is the session leader: when it exits, the sandbox ends.
        return (
            "set -u\n"
            'mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"\n'
            "dbus-daemon --config-file=/tmp/dbus.conf --nofork --print-address=1 >/logs/dbus.log 2>&1 &\n"
            f'for _ in $(seq 1 100); do [ -S "{self.runtime}/bus" ] && break; sleep 0.1; done\n'
            "unset WAYLAND_DISPLAY DISPLAY\n"
            f"exec gnoblin --devkit --wayland-display {DISPLAY_NAME} >/logs/gnoblin.log 2>&1\n"
        )

    def _launch(self, timeout: float) -> None:
        stack = self.stack
        read_only = [stack.repo, stack.gnoblin_prefix, stack.quickshell.parent]
        if stack.bingux_prefix:
            read_only.append(stack.bingux_prefix)
        if stack.cargo_target:
            read_only.append(stack.cargo_target)
        read_only = sorted({p.resolve() for p in read_only}, key=lambda p: len(p.parts))
        # Quickshell's directory is only needed when it lives outside /usr.
        read_only = [p for p in read_only if not str(p).startswith(("/usr/bin", "/usr/local/bin"))]
        status_read, status_write = os.pipe()
        spec = SandboxSpec(
            root=self.root,
            uid=self.uid,
            home=self.home,
            env=self.sandbox_env(),
            read_only=tuple(read_only),
            dri=self.gpu,
            command=("bash", "/tmp/init.sh"),
            extra_args=("--json-status-fd", str(status_write)),
        )
        argv = bwrap_argv(spec, str(stack.tools["bwrap"]))
        (self.artefacts / "bwrap-command.txt").write_text(" \\\n  ".join(argv) + "\n")
        self._bwrap = subprocess.Popen(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=(self.artefacts / "bwrap.log").open("wb"),
            stderr=subprocess.STDOUT,
            pass_fds=(status_write,),
            start_new_session=True,
            env={"PATH": "/usr/bin:/bin"},
        )
        os.close(status_write)
        with os.fdopen(status_read) as status:
            try:
                self._inner_pid = int(json.loads(status.readline())["child-pid"])
            except (ValueError, KeyError) as error:
                raise SessionError(f"bwrap did not report its child ({error}); see bwrap.log") from error
        self._wait_for(self._compositor_ready, timeout, "Gnoblin to answer on its control socket")
        if self.shell:
            self._wait_for(self._shell_ready, timeout, "the Bingux shell's bar and wallpaper")
            self._wait_for(lambda: bool(self.shell_ipc("shell", "status")), timeout, "the shell's IPC")
        for service in self.services:
            self._start_service(service)
        self._start_input()
        self._tree = descendants(self._bwrap.pid)
        self.namespaces = {
            kind: os.readlink(f"/proc/{self._inner_pid}/ns/{kind}") for kind in ("pid", "mnt", "net", "user")
        }

    def _wait_for(self, check, timeout: float, what: str) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self._bwrap is None or self._bwrap.poll() is not None:
                raise SessionError(f"the sandbox exited while waiting for {what}\n{self.tail('gnoblin.log')}")
            try:
                if check():
                    return
            except (OSError, SessionError, subprocess.SubprocessError, ValueError, KeyError):
                pass
            time.sleep(0.25)
        raise SessionError(f"timed out after {timeout:.0f}s waiting for {what}\n{self.tail('gnoblin.log')}")

    def _compositor_ready(self) -> bool:
        return (self.root / "run" / DISPLAY_NAME).exists() and "pong" in self.gnoblinctl("ping", check=False)

    def _shell_ready(self) -> bool:
        namespaces = {layer["namespace"] for layer in self.layers() if layer.get("mapped")}
        return {"bingux-top-bar", "bingux-wallpaper"} <= namespaces

    def _start_service(self, program: str) -> None:
        binary = self.stack.binary(program)
        if binary is None:
            return
        self.spawn([str(binary)], name=program).service = True

    def _start_input(self) -> None:
        if shutil.which("python3") is None:
            return
        script = self.stack.repo / "tests" / "desktop" / "bxdesktop" / "inputd.py"
        self._input = subprocess.Popen(
            nsenter_argv(self._inner_pid, ["/usr/bin/python3", str(script)], nsenter=str(self.stack.tools["nsenter"])),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=(self.artefacts / "inputd.log").open("wb"),
            env=self.client_env(),
            text=True,
        )
        assert self._input.stdout
        ready = self._input.stdout.readline()
        if not ready or not json.loads(ready).get("ready"):
            raise SessionError("the input helper did not start; see inputd.log (it needs PyGObject)")

    def stop(self) -> None:
        """Stop everything in the session, collect its logs, remove its root. Safe to call twice."""
        if getattr(self, "root", None) is None:
            return
        if self._input is not None:
            try:
                assert self._input.stdin
                self._input.stdin.close()
                self._input.wait(3)
            except (OSError, subprocess.TimeoutExpired):
                self._input.kill()
            self._input = None
        tree = descendants(self._bwrap.pid) | self._tree if self._bwrap else set()
        for client in self._spawned:
            if client.process.poll() is None:
                client.process.terminate()
        if self._bwrap is not None and self._bwrap.poll() is None:
            # Ask the session to end (SIGTERM to everything but the init), then take the sandbox down.
            subprocess.run(
                nsenter_argv(
                    self._inner_pid, ["/usr/bin/kill", "-TERM", "-1"], nsenter=str(self.stack.tools["nsenter"])
                ),
                env={"PATH": "/usr/bin"},
                capture_output=True,
                check=False,
            )
            try:
                self._bwrap.wait(6)
            except subprocess.TimeoutExpired:
                pass
        if self._bwrap is not None:
            for sig in (signal.SIGKILL,):
                try:
                    os.killpg(self._bwrap.pid, sig)
                except ProcessLookupError:
                    pass
            self._bwrap.wait()
        self.leftover = sorted(
            {pid for pid in tree if process_alive(pid)} | set(processes_in_namespace(self.namespaces.get("pid")))
        )
        self._collect_logs()
        shutil.rmtree(self.root, ignore_errors=True)
        self.root = None  # type: ignore[assignment]
        self._bwrap = None

    def __enter__(self) -> Session:
        return self.start()

    def __exit__(self, *exc) -> None:
        self.stop()

    # -- running things in the session ---------------------------------------

    def _nsenter(self, argv: list[str], cwd: str = "/") -> list[str]:
        return nsenter_argv(self._inner_pid, argv, cwd=cwd, nsenter=str(self.stack.tools["nsenter"]))

    def run(
        self,
        argv: list[str],
        *,
        env: dict[str, str] | None = None,
        timeout: float = 30.0,
        check: bool = True,
        input: str | None = None,
        display: bool = True,
    ) -> subprocess.CompletedProcess:
        """Run a command to completion inside the session and return its output."""
        if not self.alive:
            raise SessionError("the session is not running")
        full = self.client_env() if display else self.sandbox_env()
        full.update(env or {})
        done = subprocess.run(
            self._nsenter(argv),
            env=full,
            input=input,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        if check and done.returncode != 0:
            raise CommandFailed(argv, done)
        return done

    def spawn(self, argv: list[str], *, name: str | None = None, env: dict[str, str] | None = None) -> Client:
        """Start a long-running program in the session. Its output goes to the artefact logs."""
        if not self.alive:
            raise SessionError("the session is not running")
        label = name or Path(argv[0]).name
        log = self.artefacts / f"{label}-{len(self._spawned)}.log"
        full = self.client_env()
        full.update(env or {})
        process = subprocess.Popen(
            self._nsenter(argv),
            env=full,
            stdin=subprocess.DEVNULL,
            stdout=log.open("wb"),
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        client = Client(self, argv, process, log)
        self._spawned.append(client)
        return client

    # -- Gnoblin -------------------------------------------------------------

    def gnoblinctl(self, *args: str, check: bool = True, timeout: float = 15.0) -> str:
        done = self.run(["gnoblinctl", *args], check=check, timeout=timeout)
        return done.stdout

    def _gnoblin_json(self, *args: str) -> dict:
        return json.loads(self.gnoblinctl(*args))

    def windows(self) -> list[dict]:
        return self._gnoblin_json("window", "list").get("windows", [])

    def layers(self) -> list[dict]:
        return self._gnoblin_json("layer", "list").get("layers", [])

    def layer(self, namespace: str) -> dict | None:
        return next((layer for layer in self.layers() if layer["namespace"] == namespace and layer.get("mapped")), None)

    # -- the Bingux shell ----------------------------------------------------

    def shell_ipc(self, target: str, function: str, *args: str, timeout: float = 15.0) -> str:
        """Call a function of the running shell's IpcHandler (`quickshell ipc call`)."""
        done = self.run(
            [str(self.stack.quickshell), "ipc", "-p", str(self.stack.shell_dir), "call", "--", target, function, *args],
            timeout=timeout,
        )
        return done.stdout.strip()

    def shell_json(self, target: str, function: str, *args: str) -> object:
        return json.loads(self.shell_ipc(target, function, *args))

    # -- waiting -------------------------------------------------------------

    def wait_until(self, check, timeout: float = 20.0, what: str = "the condition", interval: float = 0.25):
        """Poll `check()` until it returns something truthy; return that. Raises with `what` on timeout."""
        deadline = time.monotonic() + timeout
        last: object = None
        while time.monotonic() < deadline:
            if not self.alive:
                raise SessionError(f"the session ended while waiting for {what}")
            try:
                last = check()
            except (OSError, SessionError, subprocess.SubprocessError, ValueError, KeyError) as error:
                last = error
            else:
                if last:
                    return last
            time.sleep(interval)
        raise TimeoutError(f"timed out after {timeout:.0f}s waiting for {what} (last value: {last!r})")

    # -- clients -------------------------------------------------------------

    def launch(self, argv: list[str], *, title: str | None = None, timeout: float = 30.0, **kwargs) -> Client:
        """Start a program and wait for its window to be mapped. Returns the Client with `window` set."""
        before = {window.get("id") for window in self.windows()}
        client = self.spawn(argv, **kwargs)

        def found():
            if client.process.poll() is not None:
                raise SessionError(f"{argv[0]} exited with {client.process.returncode} before it mapped a window")
            fresh = [w for w in self.windows() if w.get("id") not in before]
            if title is not None:
                fresh = [w for w in fresh if title in (w.get("title") or "")]
            return fresh[0] if fresh else None

        client.window = self.wait_until(found, timeout, f"a window from {' '.join(argv[:2])}")
        return client

    # -- input ---------------------------------------------------------------

    def _send(self, **message) -> None:
        if self._input is None or self._input.poll() is not None:
            raise SessionError("the input helper is not running")
        assert self._input.stdin and self._input.stdout
        self._input.stdin.write(json.dumps(message) + "\n")
        self._input.stdin.flush()
        reply = json.loads(self._input.stdout.readline() or '{"ok": false, "error": "input helper closed"}')
        if not reply.get("ok"):
            raise SessionError(f"input {message['op']} failed: {reply.get('error')}")

    def key(self, *chord: str, settle: float = 0.15) -> None:
        """Press a chord: `key("Return")`, `key("Alt_L", "Tab")`. Modifiers are held until the last key is released."""
        symbols = [keysyms.keysym(name) for name in chord]
        for symbol in symbols:
            self._send(op="key", keysym=symbol, down=True)
        for symbol in reversed(symbols):
            self._send(op="key", keysym=symbol, down=False)
        time.sleep(settle)

    def type(self, text: str, settle: float = 0.15) -> None:
        for character in text:
            symbol = keysyms.keysym_for_character(character)
            self._send(op="key", keysym=symbol, down=True)
            self._send(op="key", keysym=symbol, down=False)
        time.sleep(settle)

    def move(self, x: int, y: int, settle: float = 0.1) -> None:
        """Put the pointer at stage coordinates (x, y)."""
        self._send(op="warp", x=x, y=y)
        time.sleep(settle)

    def click(
        self, x: int | None = None, y: int | None = None, button: str = "left", settle: float = 0.3, hold: float = 0.08
    ) -> None:
        if x is not None and y is not None:
            self.move(x, y)
        code = {"left": 0x110, "right": 0x111, "middle": 0x112}[button]
        self._send(op="button", button=code, down=True)
        time.sleep(hold)  # toolkits that handle press and release on separate frames drop a zero-length click
        self._send(op="button", button=code, down=False)
        time.sleep(settle)

    def hover(self, x: int, y: int, region: imaging.Box, timeout: float = 20.0, minimum: float = 0.01) -> None:
        """Point at (x, y) and wait until `region` of the screen changes: the program under the pointer reacted.

        A toolkit that is still starting up takes the first presses and drops them. Waiting for a
        hover effect shows it is taking input, as a person who sees the row light up would.
        """
        baseline = self.capture()
        flip = [0]

        def reacted() -> bool:
            flip[0] ^= 1
            self.move(x + flip[0], y, settle=0.15)  # each poll is a motion event: the pointer is not still
            return self.capture().changed_fraction(baseline, region) >= minimum

        self.wait_until(reacted, timeout, f"the screen to react to the pointer at ({x}, {y})", interval=0.1)

    def scroll(self, dy: float, settle: float = 0.2) -> None:
        self._send(op="axis", dx=0.0, dy=dy)
        time.sleep(settle)

    # -- pictures ------------------------------------------------------------

    def capture(self) -> imaging.Screenshot:
        """Capture the whole stage with grim and return it without keeping the file. Use this when polling."""
        # grim runs in the sandbox, which cannot see the artefact directory: it writes into the
        # session's /tmp (root/tmp on the host).
        staged = self.root / "tmp" / "grim-out.png"
        staged.unlink(missing_ok=True)
        done = self.run(["grim", "/tmp/grim-out.png"], check=False, timeout=30)
        if done.returncode != 0 or not staged.exists():
            raise SessionError(f"grim failed: {done.stderr.strip()}")
        shot = imaging.Screenshot.open(staged)
        staged.unlink()
        return shot

    def screenshot(self, name: str | None = None) -> imaging.Screenshot:
        """Capture the stage and keep it in the artefact directory as NNN-name.png."""
        shot = self.capture()
        self._shots += 1
        path = self.artefacts / "screenshots" / f"{self._shots:03d}-{name or 'shot'}.png"
        path.parent.mkdir(exist_ok=True)
        imaging.Image.fromarray(shot.pixels).save(path)
        shot.path = path
        return shot

    # -- logs ----------------------------------------------------------------

    def log_path(self, name: str) -> Path:
        """A live log inside the session root, or the copy in the artefact directory after `stop`."""
        live = self.root / "logs" / name if getattr(self, "root", None) else None
        return live if live and live.exists() else self.artefacts / "logs" / name

    def tail(self, name: str, lines: int = 30) -> str:
        try:
            return "".join(self.log_path(name).read_text(errors="replace").splitlines(keepends=True)[-lines:])
        except OSError:
            return ""

    def _collect_logs(self) -> None:
        destination = self.artefacts / "logs"
        destination.mkdir(exist_ok=True)
        for source in (self.root / "logs").glob("*"):
            shutil.copy2(source, destination / source.name)
        for source in (self.root / "run" / "quickshell").glob("by-id/*/log.log"):
            shutil.copy2(source, destination / f"quickshell-{source.parent.name}.log")
        (self.artefacts / "stack.json").write_text(json.dumps(self.stack.describe(), indent=2) + "\n")

    def _live_logs(self) -> dict[str, Path]:
        sources: dict[str, Path] = {}
        if getattr(self, "root", None):
            sources.update({path.name: path for path in (self.root / "logs").glob("*.log")})
        for client in self._spawned:
            sources[client.log.name] = client.log
        return sources

    def log_cursor(self) -> dict[str, int]:
        """The current end of every log, to ask later for only what was written after this point."""
        return {name: path.stat().st_size for name, path in self._live_logs().items() if path.exists()}

    def log_report(self, since: dict[str, int] | None = None) -> LogReport:
        return scan_logs(self._live_logs(), since or {})

    # -- between tests -------------------------------------------------------

    def reset(self) -> None:
        """Put the desktop back to its idle state: close every client window and every shell panel."""
        for client in list(self._spawned):
            if client.service:
                continue
            if client.process.poll() is None:
                client.process.terminate()
            self._spawned.remove(client)
        for window in self.windows():
            self.gnoblinctl("window", "close", str(window["id"]), check=False)
        for panel in ("search", "calendar", "controls", "notifications", "metrics"):
            self.shell_ipc("shell", "panel", panel, "close")
        self.wait_until(lambda: not self.windows(), 10, "every client window to close")
