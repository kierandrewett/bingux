"""A session starts and stops completely, also when the test run itself is killed."""

import os
import signal
import subprocess
import sys
import textwrap
import time
from pathlib import Path

import pytest

from bxdesktop import Session, SessionError
from bxdesktop.session import process_alive, processes_in_namespace, sweep_stale_roots

pytestmark = pytest.mark.needs_gnoblin


def test_stop_ends_every_process_and_removes_the_root(stack, artifact_root):
    session = Session(stack, artifact_root / "lifecycle").start()
    root, bwrap = session.root, session._bwrap
    in_session = processes_in_namespace(session.namespaces["pid"])
    comms = {Path(f"/proc/{pid}/comm").read_text().strip() for pid in in_session}
    assert {"gnoblin", "quickshell", "dbus-daemon", "bingux-statusd"} <= comms, comms

    session.stop()

    assert bwrap.poll() is not None
    assert session.leftover == []
    assert not [pid for pid in in_session if process_alive(pid)]
    assert not root.exists()
    for name in ("gnoblin.log", "dbus.log"):
        assert (artifact_root / "lifecycle" / "logs" / name).is_file()
    session.stop()  # a second stop is harmless


def test_a_start_that_fails_leaves_nothing_behind(stack, artifact_root):
    # With no PATH the init script cannot find `gnoblin`: the sandbox's first process exits at once.
    session = Session(stack, artifact_root / "failed", env={"PATH": "/nonexistent"})
    with pytest.raises(SessionError, match="sandbox exited"):
        session.start(timeout=20)
    assert session.alive is False
    assert getattr(session, "root", None) is None
    assert session.leftover == []


def test_killing_the_test_run_takes_the_sandbox_with_it(stack, artifact_root):
    """`--die-with-parent`: SIGKILL gives the harness no chance to clean up; the kernel must."""
    script = textwrap.dedent(
        f"""
        import sys, time
        sys.path.insert(0, {str(Path(__file__).resolve().parents[1])!r})
        from pathlib import Path
        from bxdesktop import Session, Stack
        session = Session(Stack.discover(), Path({str(artifact_root / "killed")!r}), services=()).start()
        print(session.namespaces["pid"], session.root, flush=True)
        time.sleep(600)
        """
    )
    child = subprocess.Popen(
        [sys.executable, "-c", script],
        stdout=subprocess.PIPE,
        text=True,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )
    try:
        namespace, root = child.stdout.readline().strip().split(" ", 1)
        assert processes_in_namespace(namespace), "the child's session has no processes"
        child.send_signal(signal.SIGKILL)
        child.wait()
        deadline = time.monotonic() + 15
        while processes_in_namespace(namespace) and time.monotonic() < deadline:
            time.sleep(0.2)
        assert processes_in_namespace(namespace) == [], "the sandbox outlived its parent"
        # The root could not be removed by a killed process; the next start sweeps it.
        assert Path(root).is_dir()
        assert Path(root) in sweep_stale_roots(Path(root).parent)
        assert not Path(root).exists()
    finally:
        if child.poll() is None:
            child.kill()
