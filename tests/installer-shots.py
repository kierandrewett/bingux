#!/usr/bin/env python3
"""Captures a settled frame of every installer page under headless sway.

Packs the installer tree with installer-ui.py's pack_tree, runs the driver in
tests/installer-shots.qml against the fake daemon, and grabs each page when the
driver prints "INSTALLER_SHOT <name> <count>". The harness then writes <count>
to the ack file, which is what lets the driver advance. Shots go to
BINGUX_INSTALLER_SHOTS (default build/installer-shots).

usage: installer-shots.py [--shell BINARY]
"""

import argparse
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
SCRATCH_BASE = Path.home() / ".cache" / "bingux-InstallerUI"
ICON_DIR = ROOT / "subprojects/adwaita-icon-theme/Adwaita"
EXPECTED = [
    "welcome", "keyboard", "network", "timezone", "hostname", "disk", "storage",
    "account", "summary", "progress", "finished", "error",
]


def load_installer_ui():
    spec = importlib.util.spec_from_file_location("installer_ui", ROOT / "tests/installer-ui.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def grab_ppm(environment):
    """Returns a raw PPM frame from grim, or None if grim failed."""
    result = subprocess.run(["grim", "-t", "ppm", "-"], env=environment, capture_output=True, check=False)
    return result.stdout if result.returncode == 0 and result.stdout else None


def is_blank(frame):
    """True when a PPM frame has no non-zero pixel bytes; the header alone is well under 100 bytes."""
    return len(frame.translate(None, b"\x00")) < 100


def grab_settled(target, environment):
    """Grabs until two consecutive frames match and show content, so the shot is neither mid-layout nor blank."""
    previous = None
    for _ in range(40):
        time.sleep(0.3)
        current = grab_ppm(environment)
        if current is not None and not is_blank(current) and current == previous:
            return subprocess.run(["grim", str(target)], env=environment, check=False).returncode == 0
        previous = current
    return False


def make_wallpaper(scratch):
    """A gradient test wallpaper standing in for /usr/share/backgrounds/bingux-os/default.jpg."""
    wallpaper = scratch / "wallpaper.png"
    subprocess.run(
        ["magick", "-size", "1280x800", "plasma:#2b5a9e-#e8a35c", str(wallpaper)],
        check=True,
        capture_output=True,
    )
    return wallpaper


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--shell", default=str(ROOT / "build/bingux-shell/quickshell/src/bingux-shell"))
    arguments = parser.parse_args()
    installer_ui = load_installer_ui()
    shots_dir = Path(os.environ.get("BINGUX_INSTALLER_SHOTS", ROOT / "build/installer-shots"))
    shots_dir.mkdir(parents=True, exist_ok=True)
    SCRATCH_BASE.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="installer-shots-", dir=SCRATCH_BASE))
    sway = None
    try:
        tree = installer_ui.pack_tree(scratch)
        shutil.copy(ROOT / "tests/installer-shots.qml", tree / "InstallerShotsDriver.qml")
        wallpaper = make_wallpaper(scratch)
        daemon_socket = str(scratch / "shots.sock")
        daemon = installer_ui.FakeDaemon(daemon_socket)
        daemon.start()

        home = scratch / "home"
        runtime = scratch / "run"
        runtime.mkdir(parents=True, mode=0o700)
        home.mkdir()
        sway_config = scratch / "sway.conf"
        sway_config.write_text("output HEADLESS-1 resolution 1280x800\n")
        ack = scratch / "shots.ack"
        ack.write_text("0")
        environment = {k: v for k, v in os.environ.items() if k not in ("DISPLAY", "WAYLAND_DISPLAY")} | {
            "HOME": str(home),
            "XDG_RUNTIME_DIR": str(runtime),
            "XDG_STATE_HOME": str(home / "state"),
            "XDG_DATA_DIRS": str(home / "share"),
            "WLR_BACKENDS": "headless",
            "WLR_RENDERER": "pixman",
            "WLR_LIBINPUT_NO_DEVICES": "1",
            "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
            "BINGUX_ADWAITA_ICON_DIR": str(ICON_DIR),
            "BINGUX_INSTALLER_SOCKET": daemon_socket,
            "INSTALLER_SHOTS_ACK": str(ack),
            "INSTALLER_SHOTS_WALLPAPER": wallpaper.as_uri(),
            "QT_QPA_PLATFORM": "wayland",
        }
        sway = subprocess.Popen(
            ["sway", "-c", str(sway_config)],
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(100):
            sockets = [p for p in runtime.glob("wayland-*") if not p.name.endswith(".lock")]
            if sockets:
                environment["WAYLAND_DISPLAY"] = sockets[0].name
                break
            time.sleep(0.1)
        else:
            print("headless sway did not start")
            return 1

        shell = subprocess.Popen(
            [arguments.shell, "-p", str(tree / "InstallerShotsDriver.qml")],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        lines = []
        captured = []
        finished = threading.Event()

        def read_output():
            for line in shell.stdout:
                lines.append(line)
                if "INSTALLER_SHOTS_DONE" in line:
                    finished.set()
                if "INSTALLER_SHOT " in line:
                    name, number = line.split("INSTALLER_SHOT ", 1)[1].split()[:2]
                    if grab_settled(shots_dir / (name + ".png"), environment):
                        captured.append(name)
                    ack.write_text(number)

        reader = threading.Thread(target=read_output, daemon=True)
        reader.start()
        deadline = time.monotonic() + 240
        while time.monotonic() < deadline and not finished.is_set() and shell.poll() is None:
            time.sleep(0.1)
        shell.kill()
        shell.wait()
        reader.join(timeout=10)
        done = finished.is_set()
        errors = [line for line in lines if "ERROR" in line and "pipewire" not in line and "INSTALLER_SHOT" not in line]
        print("".join(errors) if errors else "no errors logged")
        print("shots:", ", ".join(captured))
        print("shots directory:", shots_dir)
        ok = done and captured == EXPECTED
        if not ok:
            print("FAIL shots missing:", [name for name in EXPECTED if name not in captured], "done:", done)
        return 0 if ok else 1
    finally:
        if sway is not None:
            sway.terminate()
            sway.wait(timeout=5)
        shutil.rmtree(scratch, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
