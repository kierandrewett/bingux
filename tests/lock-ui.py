#!/usr/bin/env python3
"""Run the lock screen inside a headless Sway session.

tests/lock-ui.qml locks the session with ext-session-lock-v1, rejects one
password, takes the volume, mute and microphone keys, shows a statusd OSD
record, accepts the next password, plays the exit and unlocks. The keys are
typed on a virtual keyboard with wtype, and the record is sent on a stand-in
for bingux-statusd's osd-v2.sock. Sway is started with
its GLES renderer: with the pixman renderer the Qt lock client never gets a
second frame and stalls. A screenshot of the locked state is written to
BINGUX_LOCK_SHOT when set; BINGUX_LOCK_WALLPAPER points at a real image.

The Bingux.Wayland plugin (bingux-wayland-sync) must be importable: build it
with `make native` or point QML_IMPORT_PATH at an installed copy.
"""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
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
    for tool in ("sway", "grim", "wtype", QS):
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
        # Stand-in for bingux-statusd: one client, newline-delimited records.
        osd_dir = runtime / "bingux"
        osd_dir.mkdir(mode=0o700)
        osd_server = socket.socket(socket.AF_UNIX)
        osd_server.bind(str(osd_dir / "osd-v2.sock"))
        osd_server.listen(1)
        osd_peers = []
        threading.Thread(target=lambda: osd_peers.append(osd_server.accept()[0]), daemon=True).start()

        # The seat needs a keyboard before the lock client binds it, or the
        # client never gets keyboard focus. A virtual keyboard lives as long
        # as its wtype process, so this one only sleeps.
        keyboard_holder = subprocess.Popen(["wtype", "-s", "30000"], env=environment)
        time.sleep(0.5)

        client = subprocess.Popen(
            [QS, "-p", str(qml)],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        lines = []
        shot = os.environ.get("BINGUX_LOCK_SHOT")

        def drive():
            for line in client.stdout:
                lines.append(line)
                if "LOCK_UI_AWAIT_KEYS" in line:
                    # Up, then Mute, then the microphone key, in one process so
                    # the keymap is uploaded once. The first pause lets the
                    # client apply that keymap before the first key.
                    subprocess.run(
                        [
                            "wtype", "-s", "400",
                            "-k", "XF86AudioRaiseVolume", "-s", "150",
                            "-k", "XF86AudioMute", "-s", "150",
                            "-k", "XF86AudioMicMute", "-s", "300",
                        ],
                        env=environment,
                        check=False,
                    )
                    record = {
                        "protocolVersion": 2,
                        "type": "osd",
                        "monitorIndex": 0,
                        "outputNames": ["HEADLESS-1"],
                        "icon": "audio-volume-high-symbolic",
                        "label": "Speakers",
                        "level": 0.55,
                        "maxLevel": 1.0,
                    }
                    if osd_peers:
                        osd_peers[0].sendall((json.dumps(record) + "\n").encode())
                elif "LOCK_UI_OSD_SHOWN" in line and shot:
                    subprocess.run(["grim", shot], env=environment, check=False)

        reader = threading.Thread(target=drive, daemon=True)
        reader.start()
        try:
            client.wait(timeout=25)
        except subprocess.TimeoutExpired:
            client.kill()
            client.wait()
        keyboard_holder.terminate()
        reader.join(timeout=5)
        output = "".join(lines)
        print(output)
        print("lock exit code:", client.returncode)
        checks = {
            "keys and OSD": "LOCK_UI_OSD_SHOWN" in output,
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
