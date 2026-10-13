#!/usr/bin/env python3
"""Start ordinary commands from dedicated application-launcher processes."""

import os
import subprocess
import sys


def reset_inherited_priority():
    """Drop only inherited negative nice; never call from the shell/shared worker."""
    try:
        if os.getpriority(os.PRIO_PROCESS, 0) < 0:
            os.setpriority(os.PRIO_PROCESS, 0, 0)
    except OSError as error:
        print(f"Could not reset inherited application priority: {error}", file=sys.stderr)


def spawn_detached(argv):
    """Launch from a dedicated helper, preserving Calendar's detached quiet child."""
    # Repeat for each command: an external daemon may have boosted this helper
    # again while it waited for the application's D-Bus name.
    reset_inherited_priority()
    return subprocess.Popen(
        argv,
        start_new_session=True,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def main(argv):
    if argv[:1] == ["--"]:
        argv = argv[1:]
    if not argv:
        print("Usage: application_spawn.py -- COMMAND [ARG ...]", file=sys.stderr)
        return 2
    reset_inherited_priority()
    try:
        os.execvp(argv[0], argv)
    except OSError as error:
        print(f"Could not launch {argv[0]}: {error}", file=sys.stderr)
        return 127 if isinstance(error, FileNotFoundError) else 126


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
