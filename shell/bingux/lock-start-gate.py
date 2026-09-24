#!/usr/bin/env python3
"""Wait for Gnoblin to admit this fixed lock-client PID before exec."""

import os
import sys


def main() -> int:
    if len(sys.argv) < 2:
        print("bingux-lock: missing locker command", file=sys.stderr)
        return 2
    value = os.environ.get("GNOBLIN_LOCK_START_FD")
    if value is not None:
        try:
            fd = int(value, 10)
            if fd < 0 or str(fd) != value:
                raise ValueError
            released = os.read(fd, 1)
            os.close(fd)
        except (OSError, ValueError):
            print("bingux-lock: could not read GNOBLIN_LOCK_START_FD", file=sys.stderr)
            return 1
        if len(released) != 1:
            print("bingux-lock: Gnoblin closed the startup gate", file=sys.stderr)
            return 1
    os.execvp(sys.argv[1], sys.argv[1:])


if __name__ == "__main__":
    raise SystemExit(main())
