"""Daemon: auth decision, request handling over a socketpair, cancel state machine."""

from __future__ import annotations

import json
import socket

import pytest

from bingux_installer import daemon
from bingux_installer.install import Env, InstallState
from bingux_installer.protocol import InstallerError


class NoRunner:
    """Fails loudly if any phase reaches the system; these tests never should."""

    def run(self, argv, **kwargs):  # noqa: ANN001, ANN003
        raise AssertionError(f"unexpected command: {argv}")

    def stream(self, argv, on_line, **kwargs):  # noqa: ANN001, ANN003
        raise AssertionError(f"unexpected command: {argv}")


def roundtrip(line: bytes, env: Env, *, uid: int = 0, gid: int = 0) -> list[dict]:
    server, client = socket.socketpair()
    try:
        client.sendall(line)
        client.shutdown(socket.SHUT_WR)
        daemon.handle_connection(server, env, uid=uid, gid=gid)
        server.close()
        data = b""
        while True:
            chunk = client.recv(4096)
            if not chunk:
                break
            data += chunk
    finally:
        client.close()
    return [json.loads(part) for part in data.splitlines() if part.strip()]


def fresh_env() -> Env:
    return Env(runner=NoRunner())


def test_decide_auth_allows_root() -> None:
    assert daemon.decide_auth(0, []) is True


def test_decide_auth_allows_group_member() -> None:
    assert daemon.decide_auth(4242, ["users", "bingux-install"]) is True


def test_decide_auth_refuses_other_uid() -> None:
    assert daemon.decide_auth(1000, ["users", "wheel"]) is False


def test_version_request_returns_result() -> None:
    (event,) = roundtrip(b'{"op":"version"}\n', fresh_env())
    assert event["event"] == "result"
    assert event["ok"] is True
    assert event["data"]["protocol"] == 1


def test_unauthorised_peer_gets_not_permitted(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(daemon, "peer_groups", lambda uid, gid: ["users"])
    (event,) = roundtrip(b'{"op":"version"}\n', fresh_env(), uid=1000, gid=1000)
    assert event == {"event": "error", "code": "not_permitted",
                     "message": "peer is not root or a member of bingux-install"}


def test_group_member_uid_is_served(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(daemon, "peer_groups", lambda uid, gid: ["bingux-install"])
    (event,) = roundtrip(b'{"op":"version"}\n', fresh_env(), uid=4242, gid=4242)
    assert event["event"] == "result"


def test_empty_request_is_bad_request() -> None:
    (event,) = roundtrip(b"", fresh_env())
    assert event["code"] == "bad_request"


def test_non_json_request_is_bad_request() -> None:
    (event,) = roundtrip(b"not json\n", fresh_env())
    assert event["code"] == "bad_request"


def test_unknown_op_is_bad_request() -> None:
    (event,) = roundtrip(b'{"op":"reboot"}\n', fresh_env())
    assert event["code"] == "bad_request"


def test_cancel_without_install_is_no_install() -> None:
    (event,) = roundtrip(b'{"op":"cancel"}\n', fresh_env())
    assert event["code"] == "no_install"


def test_cancel_while_running_is_accepted() -> None:
    env = fresh_env()
    env.state.acquire()
    (event,) = roundtrip(b'{"op":"cancel"}\n', env)
    assert event["ok"] is True
    with pytest.raises(InstallerError) as exc:
        env.state.check_cancelled()
    assert exc.value.code == "cancelled"


def test_second_install_while_running_is_busy() -> None:
    env = fresh_env()
    env.state.acquire()
    events = roundtrip(b'{"op":"install","plan":{"version":1}}\n', env)
    assert events[-1]["code"] == "busy"


def test_install_with_bad_plan_ends_with_invalid_plan() -> None:
    events = roundtrip(b'{"op":"install","plan":{"version":2}}\n', fresh_env())
    assert events[0]["event"] == "progress"
    assert events[-1]["event"] == "error"
    assert events[-1]["code"] == "invalid_plan"


def test_install_releases_lock_after_failure() -> None:
    env = fresh_env()
    roundtrip(b'{"op":"install","plan":"nope"}\n', env)
    assert env.state.running is False


def test_cancel_refused_after_commit_point() -> None:
    state = InstallState()
    state.acquire()
    state.begin_commit()
    with pytest.raises(InstallerError) as exc:
        state.request_cancel()
    assert exc.value.code == "cancel_refused_after_commit"


def test_cancel_before_commit_stops_the_install() -> None:
    state = InstallState()
    state.acquire()
    state.request_cancel()
    with pytest.raises(InstallerError) as exc:
        state.begin_commit()
    assert exc.value.code == "cancelled"


def test_listening_socket_refuses_without_activation() -> None:
    with pytest.raises(InstallerError) as exc:
        daemon.listening_socket(environ={}, pid=123)
    assert exc.value.code == "internal"


def test_listening_socket_refuses_wrong_pid() -> None:
    with pytest.raises(InstallerError):
        daemon.listening_socket(environ={"LISTEN_PID": "5", "LISTEN_FDS": "1"}, pid=123)
