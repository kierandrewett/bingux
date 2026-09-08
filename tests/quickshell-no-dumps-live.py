#!/usr/bin/env python3
"""An intentional crash must not write a core file or a Quickshell minidump."""
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import tempfile
import time


if not os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-"):
    raise SystemExit("Run through Gnoblin's private test session")
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
qs = os.environ.get("QS_TEST_BIN", "qs")
with tempfile.TemporaryDirectory(prefix="bingux-no-dumps-") as directory:
    fixture = Path(directory)
    (fixture / "shell.qml").write_text("""
import Quickshell
import Quickshell.Io
ShellRoot {
    IpcHandler {
        target: "dump-test"
        function pid(): string { return JSON.stringify(Quickshell.processId); }
    }
}
""")
    environment = os.environ | {"XDG_CACHE_HOME": str(fixture / "cache")}
    with (fixture / "runtime.log").open("w+") as log:
        process = subprocess.Popen([qs, "-p", str(fixture), "--no-color"], cwd=fixture,
                                   env=environment, stdout=log, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        try:
            deadline = time.monotonic() + 8
            pid = None
            while time.monotonic() < deadline:
                result = subprocess.run([qs, "-p", str(fixture), "ipc", "call", "dump-test", "pid"],
                                        env=environment, capture_output=True, text=True, timeout=5)
                try:
                    pid = int(json.loads(result.stdout))
                    if pid > 1:
                        break
                except (ValueError, TypeError):
                    pass
                time.sleep(0.1)
            assert pid and pid > 1, "The private crash fixture did not start"
            assert str(fixture).encode() in Path(f"/proc/{pid}/cmdline").read_bytes()
            os.kill(pid, signal.SIGABRT)
            assert process.wait(timeout=6) != 0, "The crashed process must exit"
            assert not list(fixture.glob("core*")), "A kernel core file was written"
            assert not list((fixture / "cache/quickshell/crashes").glob("**/*")), "A Quickshell crash dump was written"
            print("PASS: intentional Quickshell crash exits without kernel or application memory dumps")
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=5)
