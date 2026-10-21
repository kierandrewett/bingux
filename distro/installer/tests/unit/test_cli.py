"""Client CLI: argument parsing, plan loading, NDJSON exchange with a fake socket, exit codes."""

from __future__ import annotations

import io
import json

import pytest

from bingux_installer import cli


class FakeSock:
    """Stands in for a connected unix socket: records what was sent, replays chunks."""

    def __init__(self, chunks: list[bytes]) -> None:
        self._chunks = list(chunks)
        self.sent = b""
        self.closed = False

    def sendall(self, data: bytes) -> None:
        self.sent += data

    def recv(self, n: int) -> bytes:
        return self._chunks.pop(0) if self._chunks else b""

    def __enter__(self) -> "FakeSock":
        return self

    def __exit__(self, *exc: object) -> None:
        self.closed = True


def connector(sock: FakeSock):  # noqa: ANN201
    return lambda path: sock


def test_parser_probe_defaults() -> None:
    args = cli.build_parser().parse_args(["probe"])
    assert args.command == "probe"
    assert args.socket == "/run/bingux-installer.sock"
    assert args.json is False


def test_parser_install_requires_plan() -> None:
    with pytest.raises(SystemExit):
        cli.build_parser().parse_args(["install"])


def test_parser_accepts_json_flag_and_socket_override() -> None:
    args = cli.build_parser().parse_args(["--json", "--socket", "/tmp/s", "version"])
    assert args.json is True
    assert args.socket == "/tmp/s"
    assert args.command == "version"


def test_parser_rejects_unknown_subcommand() -> None:
    with pytest.raises(SystemExit):
        cli.build_parser().parse_args(["reboot"])


def test_load_plan_reads_stdin_for_dash() -> None:
    assert cli.load_plan("-", io.StringIO('{"version": 1}')) == {"version": 1}


def test_load_plan_rejects_non_object() -> None:
    with pytest.raises(cli.CliError) as exc:
        cli.load_plan("-", io.StringIO("[1, 2]"))
    assert exc.value.exit_code == cli.EXIT_USAGE


def test_load_plan_rejects_invalid_json() -> None:
    with pytest.raises(cli.CliError) as exc:
        cli.load_plan("-", io.StringIO("{nope"))
    assert exc.value.exit_code == cli.EXIT_USAGE


def test_build_request_for_install_wraps_plan() -> None:
    args = cli.build_parser().parse_args(["install", "--plan", "-"])
    request = cli.build_request(args, io.StringIO('{"version": 1}'))
    assert request == {"op": "install", "plan": {"version": 1}}


def test_build_request_for_cancel_has_only_op() -> None:
    args = cli.build_parser().parse_args(["cancel"])
    assert cli.build_request(args, io.StringIO("")) == {"op": "cancel"}


def test_exchange_sends_one_line_and_returns_terminal() -> None:
    sock = FakeSock([
        b'{"event":"progress","phase":"copy","percent":5,"message":"x"}\n'
        b'{"event":"log","message":"hi"}\n',
        b'{"event":"result","ok":true,"data":{}}\n',
    ])
    seen: list[dict] = []
    terminal = cli.exchange("/s", {"op": "version"}, seen.append, connect=connector(sock))
    assert sock.sent == b'{"op":"version"}\n'
    assert [event["event"] for event in seen] == ["progress", "log"]
    assert terminal == {"event": "result", "ok": True, "data": {}}
    assert sock.closed is True


def test_exchange_handles_lines_split_across_reads() -> None:
    sock = FakeSock([b'{"event":"res', b'ult","ok":true,"data":{}}\n'])
    terminal = cli.exchange("/s", {"op": "version"}, lambda e: None, connect=connector(sock))
    assert terminal["ok"] is True


def test_exchange_without_terminal_is_daemon_error() -> None:
    sock = FakeSock([b'{"event":"log","message":"partial"}\n'])
    with pytest.raises(cli.CliError) as exc:
        cli.exchange("/s", {"op": "probe"}, lambda e: None, connect=connector(sock))
    assert exc.value.exit_code == cli.EXIT_DAEMON


def test_exchange_unreachable_socket_is_daemon_error() -> None:
    def refuse(path: str):  # noqa: ANN202
        raise ConnectionRefusedError(111, "Connection refused")

    with pytest.raises(cli.CliError) as exc:
        cli.exchange("/run/bingux-installer.sock", {"op": "probe"}, lambda e: None, connect=refuse)
    assert exc.value.exit_code == cli.EXIT_DAEMON


def test_format_progress_shows_phase_and_percent() -> None:
    line = cli.format_progress({"event": "progress", "phase": "copy", "percent": 42, "message": "copying"})
    assert line == "[copy] 42% copying"


def test_format_progress_log_is_plain_message() -> None:
    assert cli.format_progress({"event": "log", "message": "hello"}) == "hello"


def test_format_human_error_names_code() -> None:
    text = cli.format_human("install", {"ok": False, "code": "busy", "message": "already running"})
    assert text == "error: busy: already running"


def test_main_json_prints_raw_terminal_line() -> None:
    sock = FakeSock([b'{"event":"result","ok":true,"data":{"installer_version":"0.1.0","protocol":1}}\n'])
    out, err = io.StringIO(), io.StringIO()
    code = cli.main(["--json", "version"], stdout=out, stderr=err, connect=connector(sock))
    assert code == cli.EXIT_OK
    assert json.loads(out.getvalue())["data"]["protocol"] == 1


def test_main_returns_daemon_exit_code_on_error_result() -> None:
    sock = FakeSock([b'{"event":"error","code":"no_install","message":"no install is running"}\n'])
    out, err = io.StringIO(), io.StringIO()
    code = cli.main(["cancel"], stdout=out, stderr=err, connect=connector(sock))
    assert code == cli.EXIT_DAEMON
    assert "no_install" in out.getvalue()


def test_main_plan_error_is_usage_exit() -> None:
    out, err = io.StringIO(), io.StringIO()
    code = cli.main(["install", "--plan", "-"], stdin=io.StringIO("[]"), stdout=out, stderr=err,
                    connect=connector(FakeSock([])))
    assert code == cli.EXIT_USAGE
    assert "plan must be a JSON object" in err.getvalue()
