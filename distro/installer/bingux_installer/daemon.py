"""Root installer daemon: socket-activated, NDJSON over a unix stream socket.

Peers are authorised when their uid is 0 or they belong to the bingux-install group
(SO_PEERCRED plus the group list). Each connection runs in its own thread so a cancel
request can arrive while an install is running. Journal output never contains plans
or passwords.
"""

from __future__ import annotations

import grp
import os
import pwd
import socket
import struct
import sys
import threading
from collections.abc import Callable, Iterable, Mapping
from typing import Any

from . import __version__
from .install import Env, run_install, run_probe, version_data
from .protocol import (
    MAX_LINE_BYTES,
    InstallerError,
    encode,
    error,
    error_from,
    parse_request,
    result,
)
from .runner import SubprocessRunner

GROUP = "bingux-install"
LISTEN_FD = 3
CLIENT_TIMEOUT_SECONDS = 30.0
SO_PEERCRED_FMT = "3i"
Sender = Callable[[Mapping[str, Any]], None]


def decide_auth(uid: int, group_names: Iterable[str]) -> bool:
    """Pure authorisation rule: root, or a member of bingux-install."""
    return uid == 0 or GROUP in set(group_names)


def peer_groups(uid: int, gid: int) -> list[str]:
    """Group names for a peer, from the passwd entry and the supplementary list."""
    try:
        user = pwd.getpwuid(uid).pw_name
    except KeyError:
        return []
    names: list[str] = []
    try:
        gids = os.getgrouplist(user, gid)
    except OSError:
        gids = [gid]
    for group_id in gids:
        try:
            names.append(grp.getgrgid(group_id).gr_name)
        except KeyError:
            continue
    return names


def peer_credentials(conn: socket.socket) -> tuple[int, int]:
    raw = conn.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize(SO_PEERCRED_FMT))
    _pid, uid, gid = struct.unpack(SO_PEERCRED_FMT, raw)
    return uid, gid


def listening_socket(environ: Mapping[str, str] | None = None, pid: int | None = None) -> socket.socket:
    """Adopt the socket passed by systemd on fd 3. Refuses to start without activation."""
    env = os.environ if environ is None else environ
    my_pid = os.getpid() if pid is None else pid
    try:
        listen_pid = int(env.get("LISTEN_PID", "0"))
        listen_fds = int(env.get("LISTEN_FDS", "0"))
    except ValueError as exc:
        raise InstallerError("internal", "LISTEN_PID/LISTEN_FDS are not integers") from exc
    if listen_pid != my_pid or listen_fds < 1:
        raise InstallerError("internal", "bingux-installer-daemon must be started by systemd socket activation")
    return socket.socket(fileno=LISTEN_FD)


def _read_line(conn: socket.socket) -> bytes:
    buf = bytearray()
    while b"\n" not in buf:
        chunk = conn.recv(4096)
        if not chunk:
            break
        buf.extend(chunk)
        if len(buf) > MAX_LINE_BYTES:
            raise InstallerError("bad_request", "request line is too long")
    if not buf:
        raise InstallerError("bad_request", "empty request")
    return bytes(buf.split(b"\n", 1)[0])


class _Sink:
    """Writes events to the client. A dropped client must not abort a running install."""

    def __init__(self, conn: socket.socket) -> None:
        self._conn = conn
        self._open = True

    def __call__(self, event: Mapping[str, Any]) -> None:
        if not self._open:
            return
        try:
            self._conn.sendall(encode(event))
        except OSError as exc:
            # The install keeps running without its client; say so, so a lost result is visible.
            print(f"bingux-installer-daemon: client dropped, event not sent: {exc}", file=sys.stderr)
            self._open = False


def handle_connection(conn: socket.socket, env: Env, *, uid: int, gid: int) -> None:
    """Serve one connection. Credentials are passed in so tests can inject peers."""
    send: Sender = _Sink(conn)
    try:
        if not decide_auth(uid, peer_groups(uid, gid)):
            # Drain the request first: closing with unread bytes resets the socket and drops the reply.
            try:
                _read_line(conn)
            except (InstallerError, OSError):
                pass
            send(error("not_permitted", "peer is not root or a member of bingux-install"))
            return
        op, req = parse_request(_read_line(conn))
        _dispatch(op, req, env, send)
    except InstallerError as exc:
        send(error_from(exc))
    except Exception as exc:  # noqa: BLE001 - the client always gets a terminal line
        print(f"bingux-installer-daemon: internal error: {type(exc).__name__}", file=sys.stderr)
        send(error("internal", "unexpected failure in the installer daemon"))


def _dispatch(op: str, req: Mapping[str, Any], env: Env, send: Sender) -> None:
    if op == "version":
        send(result(version_data()))
    elif op == "probe":
        send(result(run_probe(env)))
    elif op == "cancel":
        env.state.request_cancel()
        send(result({"cancel_requested": True}))
    elif op == "install":
        data = run_install(req.get("plan"), emit=send, env=env)
        send(result(data))
    else:  # parse_request already rejects unknown ops; keep the branch explicit
        raise InstallerError("bad_request", f"unknown op: {op}")


def serve(listen: socket.socket, env: Env) -> None:
    while True:
        conn, _addr = listen.accept()
        threading.Thread(target=_serve_one, args=(conn, env), daemon=True).start()


def _serve_one(conn: socket.socket, env: Env) -> None:
    with conn:
        conn.settimeout(CLIENT_TIMEOUT_SECONDS)
        try:
            uid, gid = peer_credentials(conn)
        except OSError:
            conn.sendall(encode(error("not_permitted", "could not read peer credentials")))
            return
        handle_connection(conn, env, uid=uid, gid=gid)


def main(argv: list[str] | None = None) -> int:
    del argv  # the daemon takes no arguments; systemd supplies the socket
    try:
        listen = listening_socket()
    except InstallerError as exc:
        print(f"bingux-installer-daemon: {exc.message}", file=sys.stderr)
        return 2
    print(f"bingux-installer-daemon {__version__} listening", file=sys.stderr)
    serve(listen, Env(runner=SubprocessRunner()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
