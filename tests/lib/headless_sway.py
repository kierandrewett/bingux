"""A private headless Sway session for QML tests that need layer-shell surfaces.

`session()` starts Sway with one headless output, yields the environment a
Wayland client needs, and always tears the compositor down. The output is
virtual: nothing appears on the host display and no host input is touched.
`pointer()`, `button()` and `screenshot()` drive and read that session only.
Sway has no pointer device without hardware, so the pointer is a client of the
compositor's virtual pointer protocol (tests/lib/virtual-pointer.c, built once
into the temporary directory).

Users: tests/dock-folders.py.
"""

from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def wait_for(predicate, timeout=10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


ROOT = Path(__file__).resolve().parents[2]


def build_virtual_pointer(directory):
    """Compile tests/lib/virtual-pointer.c into `directory`; return the program."""
    protocol = ROOT / "tests/protocols/wlr-virtual-pointer-unstable-v1.xml"
    header = directory / "wlr-virtual-pointer-unstable-v1-client-protocol.h"
    code = directory / "wlr-virtual-pointer-unstable-v1-protocol.c"
    subprocess.run(["wayland-scanner", "client-header", str(protocol), str(header)], check=True)
    subprocess.run(["wayland-scanner", "private-code", str(protocol), str(code)], check=True)
    program = directory / "virtual-pointer"
    flags = subprocess.run(
        ["pkg-config", "--cflags", "--libs", "wayland-client"], check=True, capture_output=True, text=True
    ).stdout.split()
    subprocess.run(
        [
            "cc",
            "-std=c11",
            "-O1",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-I",
            str(directory),
            str(ROOT / "tests/lib/virtual-pointer.c"),
            str(code),
            "-o",
            str(program),
            *flags,
        ],
        check=True,
    )
    return program


class Session:
    def __init__(self, environment, root, size):
        self.environment = environment
        self.root = root
        self.size = size
        self._pointer = None

    def _send(self, command):
        if self._pointer is None:
            program = build_virtual_pointer(self.root)
            self._pointer = subprocess.Popen(
                [str(program), str(self.size[0]), str(self.size[1])],
                env=self.environment,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
            )
        self._pointer.stdin.write(command + "\n")
        self._pointer.stdin.flush()
        if self._pointer.stdout.readline().strip() != "ok":
            raise RuntimeError("virtual pointer refused: " + command)

    def close(self):
        if self._pointer is not None:
            self._pointer.stdin.close()
            self._pointer.wait(timeout=5)

    def swaymsg(self, *arguments):
        result = subprocess.run(
            ["swaymsg", *arguments], env=self.environment, capture_output=True, text=True, check=True
        )
        return result.stdout

    def outputs(self):
        return json.loads(self.swaymsg("-t", "get_outputs"))

    def pointer(self, x, y):
        """Move the pointer to layout coordinates (x, y)."""
        self._send(f"move {round(x)} {round(y)}")

    def button(self, action):
        """action is press or release; the left button."""
        self._send(action)

    def screenshot(self, path):
        subprocess.run(["grim", str(path)], env=self.environment, check=True)


@contextmanager
def session(size="1280x800", scale=1, parent=None):
    """Yield a Session. `parent` chooses where the temporary directory lives
    (default: the system temporary directory; use /dev/shm on a full disk)."""
    for tool in ("sway", "swaymsg", "grim"):
        if shutil.which(tool) is None:
            raise SystemExit("missing tool: " + tool)
    root = Path(tempfile.mkdtemp(prefix="bingux-sway-", dir=parent))
    runtime = root / "run"
    runtime.mkdir(mode=0o700)
    config = root / "sway.conf"
    config.write_text(f"output HEADLESS-1 resolution {size} scale {scale}\n")
    base = os.environ | {
        "HOME": str(root),
        "XDG_RUNTIME_DIR": str(runtime),
        "WLR_BACKENDS": "headless",
        "WLR_RENDERER": "gles2",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
    }
    base.pop("DISPLAY", None)
    base.pop("WAYLAND_DISPLAY", None)
    sway = subprocess.Popen(["sway", "-c", str(config)], env=base, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:

        def sockets():
            display = [p for p in runtime.glob("wayland-*") if not p.name.endswith(".lock")]
            return display, list(runtime.glob("sway-ipc.*.sock"))

        if not wait_for(lambda: all(sockets())):
            raise SystemExit("headless sway did not start")
        display, ipc = sockets()
        environment = base | {
            "WAYLAND_DISPLAY": display[0].name,
            "SWAYSOCK": str(ipc[0]),
            "QT_QPA_PLATFORM": "wayland",
        }
        width, height = (int(part) for part in size.split("x"))
        session = Session(environment, runtime, (round(width / scale), round(height / scale)))
        try:
            # Start the pointer now, so a test's first move is not delayed by
            # the compile.
            session.pointer(0, 0)
            yield session
        finally:
            session.close()
    finally:
        sway.terminate()
        try:
            sway.wait(timeout=5)
        except subprocess.TimeoutExpired:
            sway.kill()
        shutil.rmtree(root, ignore_errors=True)
