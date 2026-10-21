"""Shared runtime support for the Python helpers the shell starts.

Every helper that the shell spawns should use this module so that:

- replies are one JSON object per line and errors are structured, so the QML
  side can show a message instead of hanging on an empty stream;
- failures reach the journal as `bingux[<tag>]: ...` lines on stderr;
- optional dependencies such as PyGObject are reported, not tracebacks;
- subprocesses always have a timeout and a readable failure;
- files are written atomically with a private mode.

The module has no dependencies beyond the standard library so it can be
imported before any optional module is checked.
"""

from __future__ import annotations

import functools
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import traceback

MAX_LINE_BYTES = 1_048_576


class HelperError(Exception):
    """A failure with a message that is safe to show to the user."""

    def __init__(self, message, code="error"):
        super().__init__(message)
        self.code = code


def log(tag, message):
    """Write one journal-visible line to stderr."""
    try:
        sys.stderr.write(f"bingux[{tag}]: {message}\n")
        sys.stderr.flush()
    except OSError:
        pass


def reply(payload):
    """Write one JSON object as a single line and flush it."""
    try:
        sys.stdout.write(json.dumps(payload, separators=(",", ":")) + "\n")
        sys.stdout.flush()
    except BrokenPipeError:
        # The shell went away; there is nobody left to tell.
        raise SystemExit(0) from None


def fail(tag, message, code="error", exit_code=1):
    """Report a structured error on stdout, log it, and exit."""
    log(tag, message)
    reply({"ok": False, "code": code, "error": str(message)})
    raise SystemExit(exit_code)


def require_gi(tag, **versions):
    """Import `gi.repository` modules or fail with a readable message.

    `require_gi("calendar", Gio="2.0", GLib="2.0")` returns a tuple of the
    requested modules in order. A missing PyGObject or typelib produces a
    structured `unavailable` error instead of an ImportError traceback.
    """
    try:
        import gi
    except ImportError:
        fail(tag, "PyGObject (python3-gobject) is not installed.", code="unavailable")
    from importlib import import_module

    try:
        for name, version in versions.items():
            gi.require_version(name, version)
        # gi.repository loads each namespace on first import, so attribute
        # access alone is not enough.
        return tuple(import_module(f"gi.repository.{name}") for name in versions)
    except (ValueError, ImportError) as error:
        fail(tag, f"GObject introspection data is missing: {error}", code="unavailable")


def run(command, timeout=10, *, input=None, check=False, env=None):
    """Run a command with a timeout and turn failures into HelperError."""
    try:
        result = subprocess.run(
            command,
            input=input,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
            env=env,
        )
    except FileNotFoundError:
        raise HelperError(f"{command[0]} is not installed.", code="unavailable") from None
    except subprocess.TimeoutExpired:
        raise HelperError(f"{command[0]} did not answer within {timeout} seconds.", code="timeout") from None
    except OSError as error:
        raise HelperError(f"Could not run {command[0]}: {error}") from None
    if check and result.returncode != 0:
        detail = (result.stderr or result.stdout or "").strip().splitlines()
        suffix = f": {detail[-1]}" if detail else ""
        raise HelperError(f"{command[0]} failed{suffix}")
    return result


def read_lines(stream=None, max_bytes=MAX_LINE_BYTES):
    """Yield stripped lines from a stream; stop cleanly at EOF.

    Lines longer than `max_bytes` raise HelperError so a misbehaving peer
    cannot grow memory without bound.
    """
    stream = stream or sys.stdin
    while True:
        line = stream.readline(max_bytes + 1)
        if not line:
            return
        if len(line) > max_bytes:
            raise HelperError("Request line is too long.", code="protocol")
        line = line.rstrip("\n")
        if line:
            yield line


def atomic_write(path, data, mode=0o600):
    """Write a file through a temporary sibling, fsync, then rename."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w" if isinstance(data, str) else "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise


def runtime_dir():
    """The private per-user runtime directory, or HelperError when absent."""
    value = os.environ.get("XDG_RUNTIME_DIR")
    if not value:
        raise HelperError("XDG_RUNTIME_DIR is not set; start the helper from a login session.")
    path = Path(value)
    if not path.is_dir():
        raise HelperError(f"XDG_RUNTIME_DIR {path} does not exist.")
    return path


def cache_dir(name):
    """`$XDG_CACHE_HOME/bingux/<name>`, created with a private mode."""
    base = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "bingux" / name
    base.mkdir(parents=True, exist_ok=True, mode=0o700)
    return base


def main(tag):
    """Decorate a helper's entry point with uniform failure handling.

    HelperError becomes a structured reply; any other exception is logged
    with its traceback and reported as an internal error. The decorated
    function's return value is the exit status.
    """

    def decorator(function):
        @functools.wraps(function)
        def wrapper(*args, **kwargs):
            try:
                status = function(*args, **kwargs)
            except HelperError as error:
                fail(tag, error, code=error.code)
            except KeyboardInterrupt:
                raise SystemExit(130) from None
            except SystemExit:
                raise
            except Exception:
                log(tag, traceback.format_exc().rstrip())
                fail(tag, "The helper hit an internal error; see the journal.", code="internal")
            raise SystemExit(0 if status is None else status)

        return wrapper

    return decorator
