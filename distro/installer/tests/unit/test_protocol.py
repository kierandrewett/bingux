"""NDJSON codec and event builders."""

from __future__ import annotations

import json

import pytest

from bingux_installer import protocol
from bingux_installer.protocol import InstallerError


def test_encode_is_compact_and_newline_terminated() -> None:
    line = protocol.encode({"event": "log", "message": "hi"})
    assert line.endswith(b"\n")
    assert line.count(b"\n") == 1
    assert b", " not in line
    assert json.loads(line) == {"event": "log", "message": "hi"}


def test_decode_rejects_empty_non_json_and_non_object() -> None:
    for raw in (b"", b"   \n", b"{not json", b"[1,2]"):
        with pytest.raises(InstallerError) as exc:
            protocol.decode(raw)
        assert exc.value.code == "bad_request"


def test_decode_rejects_invalid_utf8() -> None:
    with pytest.raises(InstallerError) as exc:
        protocol.decode(b"\xff\xfe")
    assert exc.value.code == "bad_request"


def test_parse_request_checks_op_name() -> None:
    op, obj = protocol.parse_request(b'{"op":"version"}\n')
    assert op == "version" and obj["op"] == "version"
    for raw in (b'{"plan":{}}', b'{"op":"reboot"}', b'{"op":5}'):
        with pytest.raises(InstallerError) as exc:
            protocol.parse_request(raw)
        assert exc.value.code == "bad_request"


def test_progress_clamps_percent_and_allows_null() -> None:
    assert protocol.progress("copy", 150, "x")["percent"] == 100
    assert protocol.progress("copy", -3, "x")["percent"] == 0
    assert protocol.progress("copy", None, "x")["percent"] is None


def test_error_rejects_unknown_codes_and_terminal_detection() -> None:
    event = protocol.error("busy", "another install is running")
    assert protocol.is_terminal(event)
    assert not protocol.is_terminal(protocol.log("x"))
    with pytest.raises(ValueError):
        protocol.error("exploded", "nope")


def test_redact_removes_longest_secret_first() -> None:
    text = "pw=hunter22 and hunter2"
    assert protocol.redact(text, ["hunter2", "hunter22", ""]) == "pw=*** and ***"
