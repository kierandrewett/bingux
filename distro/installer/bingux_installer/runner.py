"""Subprocess abstraction so unit tests can inject fakes and record argv.

Rules: secrets go on stdin only (never argv); callers redact stderr before showing it.
"""

from __future__ import annotations

import os
import subprocess
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Protocol


class CommandError(Exception):
    def __init__(self, argv: Sequence[str], returncode: int, stderr: str = "") -> None:
        self.argv = tuple(argv)
        self.returncode = returncode
        self.stderr = stderr
        name = argv[0] if argv else "?"
        detail = " ".join(stderr.split())[:300]
        super().__init__(f"{name} exited with status {returncode}" + (f": {detail}" if detail else ""))


@dataclass(frozen=True)
class CmdResult:
    returncode: int
    stdout: str
    stderr: str


class Runner(Protocol):
    def run(
        self,
        argv: Sequence[str],
        *,
        check: bool = True,
        stdin: str | None = None,
    ) -> CmdResult:
        """Run argv to completion. Raises CommandError when check and rc != 0."""

    def stream(
        self,
        argv: Sequence[str],
        on_line: Callable[[str], None],
        *,
        check: bool = True,
    ) -> CmdResult:
        """Run argv, calling on_line for each \\r or \\n terminated chunk of stdout+stderr."""


class SubprocessRunner:
    """Real runner. Uses a fixed minimal environment so host locale does not leak in."""

    ENV: Mapping[str, str] = {
        "PATH": "/usr/local/sbin:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
        "LC_ALL": "C.UTF-8",
        "LANG": "C.UTF-8",
    }

    def _env(self) -> dict[str, str]:
        env = dict(self.ENV)
        if "PATH" in os.environ:
            env["PATH"] = self.ENV["PATH"]
        return env

    def run(self, argv: Sequence[str], *, check: bool = True, stdin: str | None = None) -> CmdResult:
        proc = subprocess.run(
            list(argv),
            input=stdin,
            capture_output=True,
            text=True,
            env=self._env(),
            check=False,
        )
        result = CmdResult(proc.returncode, proc.stdout, proc.stderr)
        if check and proc.returncode != 0:
            raise CommandError(argv, proc.returncode, proc.stderr)
        return result

    def stream(
        self,
        argv: Sequence[str],
        on_line: Callable[[str], None],
        *,
        check: bool = True,
    ) -> CmdResult:
        proc = subprocess.Popen(
            list(argv),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=self._env(),
        )
        assert proc.stdout is not None
        tail: list[str] = []
        buf = ""
        while True:
            chunk = proc.stdout.read1(4096)
            if not chunk:
                break
            buf += chunk.decode("utf-8", errors="replace")
            parts = buf.replace("\r", "\n").split("\n")
            buf = parts.pop()
            for part in parts:
                if part:
                    on_line(part)
                    tail = (tail + [part])[-20:]
        if buf:
            on_line(buf)
            tail = (tail + [buf])[-20:]
        rc = proc.wait()
        result = CmdResult(rc, "", "\n".join(tail))
        if check and rc != 0:
            raise CommandError(argv, rc, result.stderr)
        return result
