"""Client CLI: `bingux-installer probe|version|install|cancel`.

Exit codes: 0 success, 1 daemon or transport error, 2 usage or unreadable plan.
Progress and log events go to stderr; the terminal result goes to stdout
(human summary, or the raw terminal JSON with --json).
"""

from __future__ import annotations

import argparse
import json
import socket
import sys
from collections.abc import Callable, Mapping
from typing import Any, TextIO

from .protocol import encode, decode, is_terminal

DEFAULT_SOCKET = "/run/bingux-installer.sock"
EXIT_OK = 0
EXIT_DAEMON = 1
EXIT_USAGE = 2

Connect = Callable[[str], socket.socket]


class CliError(Exception):
    """A client-side problem with a message for stderr and an exit code."""

    def __init__(self, message: str, exit_code: int) -> None:
        super().__init__(message)
        self.exit_code = exit_code


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="bingux-installer", description="Install Bingux OS to a disk.")
    parser.add_argument("--socket", default=DEFAULT_SOCKET, help="daemon socket path")
    parser.add_argument("--json", action="store_true", help="print the raw terminal JSON line")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("probe", help="list disks and tools")
    sub.add_parser("version", help="print daemon version and protocol")
    install = sub.add_parser("install", help="install from a plan file")
    install.add_argument("--plan", required=True, metavar="FILE", help="plan JSON; '-' reads stdin")
    sub.add_parser("cancel", help="cancel a running install before the disk is modified")
    return parser


def load_plan(path: str, stdin: TextIO) -> dict[str, Any]:
    try:
        if path == "-":
            text = stdin.read()
        else:
            with open(path, encoding="utf-8") as handle:
                text = handle.read()
    except OSError as exc:
        raise CliError(f"cannot read plan {path}: {exc.strerror or exc}", EXIT_USAGE) from exc
    try:
        obj = json.loads(text)
    except json.JSONDecodeError as exc:
        raise CliError(f"plan is not valid JSON: {exc.msg}", EXIT_USAGE) from exc
    if not isinstance(obj, dict):
        raise CliError("plan must be a JSON object", EXIT_USAGE)
    return obj


def build_request(args: argparse.Namespace, stdin: TextIO) -> dict[str, Any]:
    if args.command == "install":
        return {"op": "install", "plan": load_plan(args.plan, stdin)}
    return {"op": args.command}


def _default_connect(path: str) -> socket.socket:
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.connect(path)
    except OSError:
        sock.close()
        raise
    return sock


def exchange(
    socket_path: str,
    request: Mapping[str, Any],
    on_event: Callable[[dict[str, Any]], None],
    *,
    connect: Connect = _default_connect,
) -> dict[str, Any]:
    """Send one request and return the terminal event. Raises CliError on transport failure."""
    try:
        sock = connect(socket_path)
    except OSError as exc:
        raise CliError(
            f"cannot reach the installer daemon at {socket_path}: {exc.strerror or exc}", EXIT_DAEMON
        ) from exc
    with sock:
        sock.sendall(encode(request))
        buf = b""
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            buf += chunk
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                if not line.strip():
                    continue
                event = decode(line)
                if is_terminal(event):
                    return event
                on_event(event)
    raise CliError("the daemon closed the connection before a result was sent", EXIT_DAEMON)


def format_progress(event: Mapping[str, Any]) -> str:
    if event.get("event") == "log":
        return str(event.get("message", ""))
    percent = event.get("percent")
    suffix = f" {percent}%" if percent is not None else ""
    return f"[{event.get('phase', '?')}]{suffix} {event.get('message', '')}"


def format_human(op: str, event: Mapping[str, Any]) -> str:
    if not event.get("ok"):
        return f"error: {event.get('code')}: {event.get('message')}"
    data = event.get("data", {})
    if op == "version":
        return f"bingux-installer {data.get('installer_version')} (protocol {data.get('protocol')})"
    if op == "install":
        return (f"installed Bingux OS on {data.get('disk')} "
                f"({data.get('firmware')}, {data.get('filesystem')}); reboot when ready")
    if op == "cancel":
        return "cancel requested; the install stops if it has not modified the disk yet"
    if op == "probe":
        lines = [f"firmware: {data.get('firmware')}", f"live disk: {data.get('live_disk')}"]
        for disk in data.get("disks", []):
            flags = []
            if disk.get("is_live_medium"):
                flags.append("live medium")
            if disk.get("other_os"):
                flags.append("other OS: " + ", ".join(disk["other_os"]))
            lines.append(f"{disk.get('path')}  {disk.get('size_bytes')} bytes  "
                         f"{disk.get('model')}  {' '.join(flags)}".rstrip())
        missing = data.get("tools_missing") or []
        if missing:
            lines.append("missing tools: " + ", ".join(missing))
        return "\n".join(lines)
    return json.dumps(data, sort_keys=True)


def main(
    argv: list[str] | None = None,
    *,
    stdin: TextIO = sys.stdin,
    stdout: TextIO = sys.stdout,
    stderr: TextIO = sys.stderr,
    connect: Connect = _default_connect,
) -> int:
    args = build_parser().parse_args(argv)
    try:
        request = build_request(args, stdin)
        terminal = exchange(
            args.socket, request,
            lambda event: print(format_progress(event), file=stderr, flush=True),
            connect=connect,
        )
    except CliError as exc:
        print(f"bingux-installer: {exc}", file=stderr)
        return exc.exit_code
    if args.json:
        stdout.write(json.dumps(terminal, separators=(",", ":")) + "\n")
    else:
        print(format_human(args.command, terminal), file=stdout)
    return EXIT_OK if terminal.get("ok") else EXIT_DAEMON


if __name__ == "__main__":
    raise SystemExit(main())
