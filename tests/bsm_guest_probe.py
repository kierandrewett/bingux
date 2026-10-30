#!/usr/bin/env python3
"""Guest-side probes for tests/bsm-qemu-security.py. Runs inside the Bingux OS VM, as root.

usage: bsm_guest_probe.py COMMAND [options]

Every command prints human readable lines and, last, one line `PROBE-JSON {...}` that the
host parses. Commands that act "as" an account drop to it in-process (`setgroups`, `setgid`,
`setuid`), so the probe is exactly what a process of that account can do, and nothing is
mocked: the sockets, buses and files probed are the real ones of the running system.

  connect       open a socket as an account, send the first protocol bytes, report what came back
  impostor      as the greeter account but another pid, try to take the greeter's two sockets
  watch-sockets wait for a session's brokered socket to appear, then probe it as several accounts
  access        stat and try to read paths as an account
  ptrace        try PTRACE_ATTACH to processes as an account
  drm           open the DRM card node as an account and try what a non-master can try
  marker        a solid colour window on every monitor of the session it runs in, with a key log
  publish       call org.bingux.Session1.PublishAppearance with an image file descriptor
  rd-inject     inject keys through org.gnome.Mutter.RemoteDesktop on a session bus
  screencast    open a ScreenCast session and record a monitor on a session bus
  scope-exec    run a command as an account inside that account's hosted session scope
  scope-of      print the cgroup scope of an account's hosted session

Nothing here is installed in the image: the harness copies this file to /tmp in the guest.
"""

from __future__ import annotations

import argparse
import ctypes
import errno
import json
import os
from pathlib import Path
import pwd
import grp
import socket
import struct
import sys
import time

GREETER_USER = "bingux-greeter"
GREETER_WAYLAND = "/run/bsm/greeter-wayland.sock"
GREETER_CONTROL = "/run/bsm/greeter-control.sock"
HELPER_SOCKET = "/run/bsm-helper/helper.sock"
SESSION_SOCKET_DIR = "/run/bsm/sessions"
CGROUP_ROOT = "/sys/fs/cgroup"


def emit(result):
    print("PROBE-JSON " + json.dumps(result, sort_keys=True), flush=True)


def errname(error):
    return errno.errorcode.get(error.errno, str(error.errno)) if isinstance(error, OSError) else repr(error)


# ---- identity -------------------------------------------------------------------


def uid_of(user):
    return int(user) if str(user).isdigit() else pwd.getpwnam(user).pw_uid


def drop_to(user, extra_groups=()):
    """Become `user` with its real groups (as a login would), plus `extra_groups` (names)."""
    if user in (None, "", "root"):
        return
    entry = pwd.getpwnam(user)
    if os.getuid() != entry.pw_uid:
        os.initgroups(user, entry.pw_gid)
        if extra_groups:
            os.setgroups(os.getgroups() + [grp.getgrnam(name).gr_gid for name in extra_groups])
        os.setgid(entry.pw_gid)
        os.setuid(entry.pw_uid)
    os.environ.update(HOME=entry.pw_dir, USER=user, LOGNAME=user, XDG_RUNTIME_DIR=f"/run/user/{entry.pw_uid}")


def scope_of(user):
    """cgroup directory of the hosted session of `user`: the scope of its bsm-host process."""
    uid = uid_of(user)
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if (entry / "comm").read_text().strip() != "bsm-host" or entry.stat().st_uid != uid:
                continue
            for line in (entry / "cgroup").read_text().splitlines():
                if line.startswith("0::") and line.endswith(".scope"):
                    return CGROUP_ROOT + line[3:]
        except OSError:
            continue
    return None


def enter_scope(path):
    with open(os.path.join(path, "cgroup.procs"), "w") as handle:
        handle.write(str(os.getpid()))


# ---- connect ----------------------------------------------------------------------


def wayland_hello():
    """wl_display.get_registry(new_id 2): the first request of every Wayland client."""
    return struct.pack("<III", 1, (12 << 16) | 1, 2)


KINDS = {
    "wayland": (socket.SOCK_STREAM, wayland_hello()),
    "session": (socket.SOCK_STREAM, wayland_hello()),
    "control": (socket.SOCK_STREAM, b'{"type":"hello","version":1}\n'),
    "helper": (socket.SOCK_SEQPACKET, b'{"type":"ping"}'),
}


def connect_probe(path, kind, wait=1.5):
    """Connect, send what a client sends first, and report every byte the server gives back."""
    sock_type, payload = KINDS[kind]
    result = {"path": path, "kind": kind, "uid": os.getuid(), "gids": os.getgroups()}
    started = time.monotonic()
    sock = socket.socket(socket.AF_UNIX, sock_type)
    sock.settimeout(5)
    try:
        sock.connect(path)
    except OSError as error:
        result["connect"] = errname(error)
        sock.close()
        return result
    result["connect"] = "ok"
    try:
        sock.sendall(payload)
        result["sent"] = True
    except OSError as error:
        result["sent"] = errname(error)
    received = b""
    result["closed"] = None
    sock.settimeout(wait)
    deadline = time.monotonic() + wait
    while time.monotonic() < deadline:
        try:
            chunk = sock.recv(65536)
        except socket.timeout:
            break
        except OSError as error:
            result["closed"] = "reset" if error.errno == errno.ECONNRESET else errname(error)
            break
        if not chunk:
            result["closed"] = "eof"
            break
        received += chunk
        if len(received) > 1 << 20:
            break
    result["received_bytes"] = len(received)
    result["received_head"] = received[:24].hex()
    result["seconds"] = round(time.monotonic() - started, 3)
    sock.close()
    return result


def cmd_connect(args):
    drop_to(args.user, args.groups.split(",") if args.groups else ())
    emit(connect_probe(args.path, args.kind, args.wait))


def run_as(user, function, *arguments):
    """Fork, become `user`, run `function`, return its result (None if the child died)."""
    read_end, write_end = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.close(read_end)
        try:
            drop_to(user)
            data = json.dumps(function(*arguments)).encode()
        except Exception as error:  # noqa: BLE001 - reported to the parent
            data = json.dumps({"error": repr(error)}).encode()
        os.write(write_end, data)
        os._exit(0)
    os.close(write_end)
    chunks = []
    while True:
        chunk = os.read(read_end, 65536)
        if not chunk:
            break
        chunks.append(chunk)
    os.close(read_end)
    os.waitpid(pid, 0)
    try:
        return json.loads(b"".join(chunks))
    except ValueError:
        return None


def start_as(user, function, *arguments):
    """Like run_as but returns (pid, read_fd) at once so several probes run in parallel."""
    read_end, write_end = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.close(read_end)
        try:
            drop_to(user)
            data = json.dumps(function(*arguments)).encode()
        except Exception as error:  # noqa: BLE001
            data = json.dumps({"error": repr(error)}).encode()
        os.write(write_end, data)
        os._exit(0)
    os.close(write_end)
    return pid, read_end


def finish(handle):
    pid, read_end = handle
    chunks = []
    while True:
        chunk = os.read(read_end, 65536)
        if not chunk:
            break
        chunks.append(chunk)
    os.close(read_end)
    os.waitpid(pid, 0)
    try:
        return json.loads(b"".join(chunks))
    except ValueError:
        return None


def cmd_impostor(args):
    """As the greeter account but another pid: hammer both greeter sockets, report whether any byte came back."""
    stats = {name: {"attempts": 0, "connected": 0, "bytes": 0, "errors": {}} for name in ("wayland", "control")}

    def hammer():
        end = time.monotonic() + args.seconds
        while time.monotonic() < end:
            for name, path in (("wayland", GREETER_WAYLAND), ("control", GREETER_CONTROL)):
                result = connect_probe(path, name, wait=0.25)
                entry = stats[name]
                entry["attempts"] += 1
                if result["connect"] == "ok":
                    entry["connected"] += 1
                    entry["bytes"] += result.get("received_bytes", 0)
                else:
                    entry["errors"][result["connect"]] = entry["errors"].get(result["connect"], 0) + 1
            time.sleep(0.02)
        return stats

    result = run_as(GREETER_USER, hammer)
    emit({"pid_note": "the impostor ran in a forked child of the probe", "stats": result})


def start_command_probe(template, path):
    """Run a shell command (with `{path}` filled in) in the background; its last PROBE-JSON line is the result."""
    import subprocess

    return subprocess.Popen(template.format(path=path), shell=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)


def finish_command_probe(process, timeout):
    import subprocess

    try:
        output, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        process.kill()
        return {"error": "timeout"}
    for line in reversed(output.splitlines()):
        if line.startswith("PROBE-JSON "):
            return json.loads(line[len("PROBE-JSON ") :])
    return {"error": "no PROBE-JSON line", "output": output[-300:]}


def cmd_watch_sockets(args):
    """Wait for a brokered session socket, then probe it as several accounts, early and late.

    `early` probes run the moment the socket appears: the compositor has not been told the
    bridge's pid yet, so a connection of the right uid is held and judged later. `late`
    probes run `--later` seconds on, after the manager registered the bridge's pid.
    `--cmd NAME=COMMAND` adds an identity that is a shell command (`{path}` is the socket),
    for what drop_to cannot express, such as a user namespace that maps the caller to
    another uid.
    """
    deadline = time.monotonic() + args.timeout
    found = None
    while time.monotonic() < deadline and found is None:
        try:
            names = [n for n in os.listdir(SESSION_SOCKET_DIR) if n.endswith(".sock")]
        except OSError:
            names = []
        if args.session_prefix:
            names = [n for n in names if n.startswith(args.session_prefix)]
        if names:
            found = os.path.join(SESSION_SOCKET_DIR, names[0])
            break
        time.sleep(0.004)
    result = {"socket": found, "appeared_after": round(time.monotonic() - (deadline - args.timeout), 3)}
    if found is None:
        emit(result)
        return
    result["socket_mode"] = oct(os.stat(found).st_mode & 0o7777)
    identities = [who for who in args.identities.split(",") if who]
    commands = dict(item.split("=", 1) for item in args.cmd)

    def round_of():
        accounts = {who: start_as(who, connect_probe, found, "session", args.hold) for who in identities}
        processes = {name: start_command_probe(template, found) for name, template in commands.items()}
        out = {who: finish(handle) for who, handle in accounts.items()}
        out.update({name: finish_command_probe(process, args.hold + 20) for name, process in processes.items()})
        return out

    result["early"] = round_of()
    time.sleep(max(0.0, args.later))
    result["socket_existed_before_late_probes"] = os.path.exists(found)
    result["late"] = round_of()
    # Wait for the real bridge to take the socket: single use, so the file goes away.
    end = time.monotonic() + args.consumed_within
    while os.path.exists(found) and time.monotonic() < end:
        time.sleep(0.25)
    result["socket_consumed_by_bridge"] = not os.path.exists(found)
    emit(result)


# ---- files ---------------------------------------------------------------------------


def probe_path(path):
    entry = {"path": path}
    try:
        info = os.stat(path)
    except OSError as error:
        entry["stat"] = errname(error)
        return entry
    entry.update(stat="ok", mode=oct(info.st_mode & 0o7777), uid=info.st_uid, gid=info.st_gid, kind="dir" if os.path.isdir(path) else "other")
    if os.path.isdir(path):
        try:
            entry["list"] = sorted(os.listdir(path))[:12]
        except OSError as error:
            entry["list"] = errname(error)
    else:
        try:
            with open(path, "rb") as handle:
                entry["read"] = "ok"
                entry["head"] = handle.read(16).hex()
        except OSError as error:
            entry["read"] = errname(error)
    try:
        fd = os.open(path, os.O_WRONLY)
        os.close(fd)
        entry["write"] = "ok"
    except OSError as error:
        entry["write"] = errname(error)
    return entry


def cmd_access(args):
    drop_to(args.user, args.groups.split(",") if args.groups else ())
    emit({"user": args.user, "uid": os.getuid(), "paths": [probe_path(p) for p in args.paths]})


def cmd_create(args):
    """Try to create a file (or a directory with --dir) at each path as `user`."""
    drop_to(args.user)
    out = []
    for path in args.paths:
        try:
            if args.dir:
                os.mkdir(path)
                os.rmdir(path)
            else:
                fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                os.close(fd)
                os.unlink(path)
            out.append({"path": path, "create": "ok"})
        except OSError as error:
            out.append({"path": path, "create": errname(error)})
    emit({"user": args.user, "results": out})


def cmd_ptrace(args):
    drop_to(args.user)
    libc = ctypes.CDLL(None, use_errno=True)
    libc.ptrace.restype = ctypes.c_long
    results = {}
    for pid in args.pids:
        ctypes.set_errno(0)
        rc = libc.ptrace(16, pid, 0, 0)  # PTRACE_ATTACH
        error = ctypes.get_errno()
        results[str(pid)] = "attached" if rc == 0 else errno.errorcode.get(error, str(error))
        if rc == 0:
            libc.ptrace(17, pid, 0, 0)  # PTRACE_DETACH
    emit({"user": args.user, "ptrace": results})


# ---- DRM -------------------------------------------------------------------------------

DRM_IOCTL_BASE = ord("d")


def _ioc(direction, number, size):
    return (direction << 30) | (size << 16) | (DRM_IOCTL_BASE << 8) | number


IOC_RW = 3
DRM_IOCTL_SET_MASTER = (0 << 30) | (DRM_IOCTL_BASE << 8) | 0x1E
DRM_IOCTL_MODE_GETRESOURCES = _ioc(IOC_RW, 0xA0, 64)
DRM_IOCTL_MODE_GETCRTC = _ioc(IOC_RW, 0xA1, 104)
DRM_IOCTL_MODE_GETFB = _ioc(IOC_RW, 0xAD, 28)


def cmd_drm(args):
    """What a process of `user` can do on the DRM card node: open it, list CRTCs, read the scanout, become master."""
    import fcntl

    drop_to(args.user)
    result = {"user": args.user, "uid": os.getuid(), "gids": os.getgroups(), "node": args.node}
    try:
        fd = os.open(args.node, os.O_RDWR)
    except OSError as error:
        result["open"] = errname(error)
        emit(result)
        return
    result["open"] = "ok"
    try:
        fcntl.ioctl(fd, DRM_IOCTL_SET_MASTER)
        result["set_master"] = "ok"
    except OSError as error:
        result["set_master"] = errname(error)
    # drm_mode_card_res: counts and four pointers; two passes, as libdrm does.
    counts = struct.pack("=QQQQIIII", 0, 0, 0, 0, 0, 0, 0, 0) + b"\0" * 16
    buffer = bytearray(counts)
    try:
        fcntl.ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, buffer, True)
    except OSError as error:
        result["getresources"] = errname(error)
        emit(result)
        return
    _fbs, crtcs_ptr, _conns, _encs, count_fbs, count_crtcs, count_conns, count_encs = struct.unpack("=QQQQIIII", bytes(buffer[:48]))
    result["crtc_count"] = count_crtcs
    crtc_ids = (ctypes.c_uint32 * max(count_crtcs, 1))()
    buffer = bytearray(struct.pack("=QQQQIIII", 0, ctypes.addressof(crtc_ids), 0, 0, 0, count_crtcs, 0, 0) + b"\0" * 16)
    try:
        fcntl.ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, buffer, True)
    except OSError as error:
        result["getresources_ids"] = errname(error)
        emit(result)
        return
    scanouts = []
    for crtc in list(crtc_ids)[:count_crtcs]:
        # drm_mode_crtc: set_connectors_ptr (8), count_connectors (4), crtc_id (4), fb_id (4), ...
        crtc_buffer = bytearray(struct.pack("=QII", 0, 0, crtc) + b"\0" * (104 - 16))
        try:
            fcntl.ioctl(fd, DRM_IOCTL_MODE_GETCRTC, crtc_buffer, True)
        except OSError as error:
            scanouts.append({"crtc": crtc, "getcrtc": errname(error)})
            continue
        fb_id = struct.unpack_from("=I", crtc_buffer, 16)[0]
        entry = {"crtc": crtc, "fb_id": fb_id}
        if fb_id:
            fb_buffer = bytearray(struct.pack("=IIIIIII", fb_id, 0, 0, 0, 0, 0, 0))
            try:
                fcntl.ioctl(fd, DRM_IOCTL_MODE_GETFB, fb_buffer, True)
                entry["getfb_handle"] = struct.unpack_from("=I", fb_buffer, 24)[0]
            except OSError as error:
                entry["getfb"] = errname(error)
        scanouts.append(entry)
    result["scanouts"] = scanouts
    emit(result)


# ---- appearance ----------------------------------------------------------------------


def cmd_publish(args):
    """PublishAppearance as `user` with the wallpaper given as an open file descriptor."""
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio, GLib

    drop_to(args.user)
    fd = os.open(args.image, os.O_RDONLY)
    fd_list = Gio.UnixFDList()
    index = fd_list.append(fd)
    parameters = GLib.Variant("(sssbbahah)", ("dark", "#3584e4", "24h", False, True, [index], []))
    connection = Gio.bus_get_sync(Gio.BusType.SYSTEM, None)
    result = {"user": args.user}
    try:
        reply, _out_fds = connection.call_with_unix_fd_list_sync(
            "org.bingux.Session1", "/org/bingux/Session1", "org.bingux.Session1", "PublishAppearance",
            parameters, None, Gio.DBusCallFlags.NONE, 30000, fd_list, None,
        )
        result["reply"] = "ok"
    except GLib.Error as error:
        result["reply"] = error.message[:200]
        result["error_name"] = Gio.DBusError.get_remote_error(error) or ""
    emit(result)


# ---- Mutter remote desktop and screen cast (the user's own session bus) ----------------


def session_bus(address):
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio

    return Gio.DBusConnection.new_for_address_sync(
        address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None
    )


def cmd_rd_inject(args):
    """Connect to a session bus and inject key presses through org.gnome.Mutter.RemoteDesktop."""
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio, GLib

    drop_to(args.user)
    address = args.bus or f"unix:path=/run/user/{os.getuid()}/bus"
    result = {"user": args.user, "bus": address}
    try:
        connection = session_bus(address)
    except GLib.Error as error:
        result["connect"] = error.message[:160]
        emit(result)
        return
    result["connect"] = "ok"
    interface = "org.gnome.Mutter.RemoteDesktop"

    def call(path, iface, method, parameters=None):
        return connection.call_sync(interface, path, iface, method, parameters, None, Gio.DBusCallFlags.NONE, 10000, None)

    try:
        path = call("/org/gnome/Mutter/RemoteDesktop", interface, "CreateSession").unpack()[0]
        result["session"] = path
        call(path, interface + ".Session", "Start")
        for character in args.keys:
            for state in (True, False):
                call(path, interface + ".Session", "NotifyKeyboardKeysym", GLib.Variant("(ub)", (ord(character), state)))
            time.sleep(0.05)
        call(path, interface + ".Session", "Stop")
        result["injected"] = args.keys
    except GLib.Error as error:
        result["error"] = error.message[:200]
    emit(result)


def cmd_screencast(args):
    """Create a ScreenCast session on a session bus and ask for a monitor recording."""
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio, GLib

    drop_to(args.user)
    address = args.bus or f"unix:path=/run/user/{os.getuid()}/bus"
    result = {"user": args.user, "bus": address}
    try:
        connection = session_bus(address)
    except GLib.Error as error:
        result["connect"] = error.message[:160]
        emit(result)
        return
    result["connect"] = "ok"
    interface = "org.gnome.Mutter.ScreenCast"

    def call(path, iface, method, parameters=None):
        return connection.call_sync(interface, path, iface, method, parameters, None, Gio.DBusCallFlags.NONE, 10000, None)

    try:
        path = call("/org/gnome/Mutter/ScreenCast", interface, "CreateSession", GLib.Variant("(a{sv})", ({},))).unpack()[0]
        result["session"] = path
        stream = call(path, interface + ".Session", "RecordMonitor", GLib.Variant("(sa{sv})", (args.connector, {}))).unpack()[0]
        result["stream"] = stream
        call(path, interface + ".Session", "Stop")
    except GLib.Error as error:
        result["error"] = error.message[:200]
    emit(result)


# ---- scope ----------------------------------------------------------------------------


def cmd_scope_of(args):
    emit({"user": args.user, "scope": scope_of(args.user)})


def cmd_scope_exec(args):
    """Move into the hosted session scope of `--scope-of`, become `--user`, exec the command."""
    scope = scope_of(args.scope_of)
    if scope is None:
        emit({"error": f"no hosted session scope for {args.scope_of}"})
        sys.exit(3)
    enter_scope(scope)
    drop_to(args.user)
    os.chdir(os.environ["HOME"])
    for item in args.env:
        key, _, value = item.partition("=")
        os.environ[key] = value
    uid = os.getuid()
    os.environ.setdefault("DBUS_SESSION_BUS_ADDRESS", f"unix:path=/run/user/{uid}/bus")
    os.execvp(args.command[0], args.command)


# ---- marker window --------------------------------------------------------------------


def cmd_marker(args):
    """One full-screen window of a solid colour on every monitor, rebuilt when monitors come and go.

    The colour is the test's proof of what is on screen: if the colour shows where it should
    not, a desktop buffer leaked. Every key press the windows receive is appended to --keylog,
    which proves who gets the keyboard. Runs until killed.
    """
    import gi

    gi.require_version("Gtk", "4.0")
    gi.require_version("Gdk", "4.0")
    from gi.repository import Gdk, GLib, Gtk

    drop_to(args.user)
    os.environ.setdefault("GDK_BACKEND", "wayland")
    colour = args.colour

    class Marker(Gtk.Application):
        def __init__(self):
            super().__init__(application_id="org.bingux.test.Marker" + colour.strip("#"))
            self.windows = []

        def do_activate(self):
            display = Gdk.Display.get_default()
            css = Gtk.CssProvider()
            css.load_from_string("window.marker, window.marker * { background-color: %s; background-image: none; }" % colour)
            Gtk.StyleContext.add_provider_for_display(display, css, Gtk.STYLE_PROVIDER_PRIORITY_USER)
            self.monitors = display.get_monitors()
            self.monitors.connect("items-changed", lambda *_: GLib.idle_add(self.rebuild))
            self.rebuild()
            self.hold()

        def on_key(self, _controller, keyval, _keycode, _state):
            with open(args.keylog, "a") as handle:
                handle.write(f"{time.time():.3f} {Gdk.keyval_name(keyval)}\n")
            return False

        def rebuild(self):
            for window in self.windows:
                window.destroy()
            self.windows = []
            names = []
            for index in range(self.monitors.get_n_items()):
                monitor = self.monitors.get_item(index)
                window = Gtk.ApplicationWindow(application=self)
                window.add_css_class("marker")
                window.set_decorated(False)
                controller = Gtk.EventControllerKey()
                controller.connect("key-pressed", self.on_key)
                window.add_controller(controller)
                window.fullscreen_on_monitor(monitor)
                window.present()
                self.windows.append(window)
                names.append(monitor.get_connector())
            print(f"marker {colour} windows on {names}", flush=True)
            return False

    Marker().run([])



def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="subcommand", required=True)

    p = sub.add_parser("connect")
    p.add_argument("--kind", choices=sorted(KINDS), required=True)
    p.add_argument("--path", required=True)
    p.add_argument("--user", default="root")
    p.add_argument("--groups", default="")
    p.add_argument("--wait", type=float, default=1.5)
    p.set_defaults(function=cmd_connect)

    p = sub.add_parser("impostor")
    p.add_argument("--seconds", type=float, default=12.0)
    p.set_defaults(function=cmd_impostor)

    p = sub.add_parser("watch-sockets")
    p.add_argument("--identities", default="alice,bob,root")
    p.add_argument("--cmd", action="append", default=[], help="NAME=COMMAND probe run as a shell command; {path} is the socket")
    p.add_argument("--session-prefix", default="")
    p.add_argument("--timeout", type=float, default=120.0)
    p.add_argument("--hold", type=float, default=14.0)
    p.add_argument("--later", type=float, default=3.0)
    p.add_argument("--consumed-within", type=float, default=60.0)
    p.set_defaults(function=cmd_watch_sockets)

    p = sub.add_parser("access")
    p.add_argument("--user", required=True)
    p.add_argument("--groups", default="")
    p.add_argument("paths", nargs="+")
    p.set_defaults(function=cmd_access)

    p = sub.add_parser("create")
    p.add_argument("--user", required=True)
    p.add_argument("--dir", action="store_true")
    p.add_argument("paths", nargs="+")
    p.set_defaults(function=cmd_create)

    p = sub.add_parser("ptrace")
    p.add_argument("--user", required=True)
    p.add_argument("pids", nargs="+", type=int)
    p.set_defaults(function=cmd_ptrace)

    p = sub.add_parser("drm")
    p.add_argument("--user", required=True)
    p.add_argument("--node", default="/dev/dri/card1")
    p.set_defaults(function=cmd_drm)

    p = sub.add_parser("publish")
    p.add_argument("--user", required=True)
    p.add_argument("--image", required=True)
    p.set_defaults(function=cmd_publish)

    p = sub.add_parser("rd-inject")
    p.add_argument("--user", required=True)
    p.add_argument("--bus", default="")
    p.add_argument("keys")
    p.set_defaults(function=cmd_rd_inject)

    p = sub.add_parser("screencast")
    p.add_argument("--user", required=True)
    p.add_argument("--bus", default="")
    p.add_argument("--connector", default="")
    p.set_defaults(function=cmd_screencast)

    p = sub.add_parser("scope-of")
    p.add_argument("--user", required=True)
    p.set_defaults(function=cmd_scope_of)

    p = sub.add_parser("scope-exec")
    p.add_argument("--scope-of", required=True, help="account whose hosted session scope to enter")
    p.add_argument("--user", required=True)
    p.add_argument("--env", action="append", default=[], help="KEY=VALUE for the command")
    p.add_argument("command", nargs=argparse.REMAINDER)
    p.set_defaults(function=cmd_scope_exec)

    p = sub.add_parser("marker")
    p.add_argument("--user", required=True)
    p.add_argument("--colour", required=True)
    p.add_argument("--keylog", required=True)
    p.set_defaults(function=cmd_marker)

    args = parser.parse_args()
    if args.subcommand == "scope-exec" and args.command and args.command[0] == "--":
        args.command = args.command[1:]
    args.function(args)


if __name__ == "__main__":
    main()
