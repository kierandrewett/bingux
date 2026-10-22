"""NDJSON wire protocol, error codes and event builders.

Pure: no I/O. The daemon and the CLI both use these functions.
"""

from __future__ import annotations

import json
from collections.abc import Iterable, Mapping
from typing import Any

PROTOCOL_VERSION = 1
MAX_LINE_BYTES = 64 * 1024

OPS = ("probe", "version", "install", "cancel")

ERROR_CODES = frozenset({
    "bad_request",
    "not_permitted",
    "busy",
    "invalid_plan",
    "unsupported",
    "disk_not_found",
    "disk_is_live_medium",
    "disk_too_small",
    "no_free_space",
    "not_gpt",
    "cannot_shrink",
    "other_os_present",
    "missing_tool",
    "cancelled",
    "cancel_refused_after_commit",
    "no_install",
    "storage_failed",
    "payload_failed",
    "bootloader_failed",
    "config_failed",
    "internal",
})

REDACTED = "***"


class InstallerError(Exception):
    """An error with a protocol code. The message is human text, safe to send."""

    def __init__(self, code: str, message: str) -> None:
        if code not in ERROR_CODES:
            raise ValueError(f"unknown error code: {code}")
        super().__init__(message)
        self.code = code
        self.message = message


def encode(obj: Mapping[str, Any]) -> bytes:
    """Encode one protocol object as a UTF-8 line terminated by a newline."""
    text = json.dumps(obj, separators=(",", ":"), ensure_ascii=False, sort_keys=False)
    if "\n" in text:  # json.dumps escapes newlines, so this is a safety net only
        raise ValueError("encoded object contains a newline")
    return text.encode("utf-8") + b"\n"


def decode(line: bytes | str) -> dict[str, Any]:
    """Decode one request or event line. Raises InstallerError(bad_request)."""
    if isinstance(line, bytes):
        if len(line) > MAX_LINE_BYTES:
            raise InstallerError("bad_request", "request line is too long")
        try:
            line = line.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise InstallerError("bad_request", "request is not valid UTF-8") from exc
    text = line.strip()
    if not text:
        raise InstallerError("bad_request", "empty request")
    try:
        obj = json.loads(text)
    except json.JSONDecodeError as exc:
        raise InstallerError("bad_request", f"request is not JSON: {exc.msg}") from exc
    if not isinstance(obj, dict):
        raise InstallerError("bad_request", "request must be a JSON object")
    return obj


def parse_request(line: bytes | str) -> tuple[str, dict[str, Any]]:
    """Return (op, request) after checking the op name. Raises bad_request."""
    obj = decode(line)
    op = obj.get("op")
    if not isinstance(op, str):
        raise InstallerError("bad_request", "request has no op")
    if op not in OPS:
        raise InstallerError("bad_request", f"unknown op: {op}")
    return op, obj


def progress(phase: str, percent: float | int | None, message: str) -> dict[str, Any]:
    if percent is not None:
        percent = max(0, min(100, int(percent)))
    return {"event": "progress", "phase": phase, "percent": percent, "message": message}


def log(message: str) -> dict[str, Any]:
    return {"event": "log", "message": message}


def result(data: Mapping[str, Any]) -> dict[str, Any]:
    return {"event": "result", "ok": True, "data": dict(data)}


def error(code: str, message: str) -> dict[str, Any]:
    if code not in ERROR_CODES:
        raise ValueError(f"unknown error code: {code}")
    return {"event": "error", "code": code, "message": message}


def error_from(exc: InstallerError) -> dict[str, Any]:
    return error(exc.code, exc.message)


def is_terminal(event: Mapping[str, Any]) -> bool:
    return event.get("event") in ("result", "error")


def redact(text: str, secrets: Iterable[str]) -> str:
    """Replace every non-empty secret in text with REDACTED."""
    out = text
    for secret in sorted({s for s in secrets if s}, key=len, reverse=True):
        out = out.replace(secret, REDACTED)
    return out
