#!/usr/bin/env python3
"""Run the lock screen inside a headless Sway session.

tests/lock-ui.qml locks the session with ext-session-lock-v1, rejects one
password, accepts the next, plays the exit and unlocks. Sway is started with
its GLES renderer: with the pixman renderer the Qt lock client never gets a
second frame and stalls. A screenshot of the locked state is written to
BINGUX_LOCK_SHOT when set; BINGUX_LOCK_WALLPAPER points at a real image.

The Bingux.Wayland plugin (bingux-wayland-sync) must be importable: build it
with `make native` or point QML_IMPORT_PATH at an installed copy.
"""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
QS = os.environ.get("QS_TEST_BIN", "qs")


def import_path():
    configured = os.environ.get("QML_IMPORT_PATH", "")
    candidates = [Path(p) for p in configured.split(":") if p]
    candidates.append(ROOT / "build/qml")
    data_home = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    candidates.append(data_home / "bingux/lib/bingux/qml")
    for candidate in candidates:
        if (candidate / "Bingux/Wayland/qmldir").is_file():
            return str(candidate)
    return None


def main():
    for tool in ("sway", "grim", QS):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    qml_path = import_path()
    if qml_path is None:
        print("Bingux.Wayland plugin not found; run make native and install-user, or set QML_IMPORT_PATH")
        return 1
    home = Path(tempfile.mkdtemp(prefix="bingux-lock-ui-"))
    runtime = home / "run"
    runtime.mkdir(mode=0o700)
    sway_config = home / "sway.conf"
    sway_config.write_text("output HEADLESS-1 resolution 1280x800\n")
    environment = os.environ | {
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(runtime),
        "WLR_BACKENDS": "headless",
        "WLR_RENDERER": "gles2",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
        "BINGUX_LOCK_WALLPAPER": os.environ.get("BINGUX_LOCK_WALLPAPER", ""),
        "QML_IMPORT_PATH": qml_path,
        "QT_QPA_PLATFORM": "wayland",
    }
    environment.pop("DISPLAY", None)
    sway = subprocess.Popen(
        ["sway", "-c", str(sway_config)],
        env={k: v for k, v in environment.items() if k != "WAYLAND_DISPLAY"},
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(100):
            sockets = [p for p in runtime.glob("wayland-*") if not p.name.endswith(".lock")]
            if sockets:
                environment["WAYLAND_DISPLAY"] = sockets[0].name
                break
            time.sleep(0.1)
        else:
            print("headless sway did not start")
            return 1
        qml = home / "lock-ui.qml"
        qml.write_text(
            (ROOT / "tests/lock-ui.qml").read_text().replace("../shell/bingux", (ROOT / "shell/bingux").as_uri())
        )
        client = subprocess.Popen(
            [QS, "-p", str(qml)],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        shot = os.environ.get("BINGUX_LOCK_SHOT")
        if shot:
            # The locked state holds for 1.5 s after the rejected password.
            time.sleep(2.0)
            subprocess.run(["grim", shot], env=environment, check=False)
        try:
            output, _ = client.communicate(timeout=25)
        except subprocess.TimeoutExpired:
            client.kill()
            output, _ = client.communicate()
        print(output)
        print("lock exit code:", client.returncode)
        checks = {
            "lock passed": "LOCK_UI_PASSED" in output,
            "exit code": client.returncode == 0,
            "no failure": "LOCK_UI_FAILED" not in output,
        }
        for name, ok in checks.items():
            print(("ok   " if ok else "FAIL ") + name)
        return 0 if all(checks.values()) else 1
    finally:
        sway.terminate()
        sway.wait(timeout=5)
        shutil.rmtree(home, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
