#!/usr/bin/env python3
"""Run the production greeter and lock shells from a staged package install.

This is the path a package takes: `install-shell.py --destdir` stages the build
under a scratch root, then the installed `bingux-greeter` and `bingux-lock`
symlinks start `bingux-shell`, which unpacks the shell archive into the scratch
cache and runs GreeterShell.qml and LockShell.qml from there, inside a headless
Sway session with the fake greetd from greeter-ui.py.

The check is that each shell stays up, loads without QML errors (a TypeError or
a missing-signal warning in the log fails the run) and paints more than the
plain background. The log, a screenshot and a checks.json for each shell are
written to BUILD_DIR/shell-archive/ so a failed run can be inspected.
"""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import zlib

ROOT = Path(__file__).resolve().parent.parent


def load_fake_greetd():
    spec = importlib.util.spec_from_file_location("greeter_ui", ROOT / "tests/greeter-ui.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.FakeGreetd


def png_is_flat(path):
    """True when every pixel of a grim PNG has the same colour (nothing drawn)."""
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return True
    pos = 8
    width = height = 0
    idat = b""
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            width, height = struct.unpack(">II", body[:8])
            if body[8] != 8 or body[9] not in (2, 6):
                return True
            channels = 3 if body[9] == 2 else 4
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    raw = zlib.decompress(idat)
    stride = width * channels
    first = None
    previous = bytearray(stride)
    offset = 0
    for _ in range(height):
        kind = raw[offset]
        line = bytearray(raw[offset + 1 : offset + 1 + stride])
        offset += 1 + stride
        if kind == 1:
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif kind == 2:
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 0xFF
        elif kind == 3:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif kind == 4:
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = previous[i]
                c = previous[i - channels] if i >= channels else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else b if pb <= pc else c
                line[i] = (line[i] + pred) & 0xFF
        for x in range(0, stride, channels):
            pixel = bytes(line[x : x + 3])
            if first is None:
                first = pixel
            elif pixel != first:
                return False
        previous = line
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-dir", default="build")
    parser.add_argument("--seconds", type=float, default=6.0, help="how long each shell runs before the screenshot")
    args = parser.parse_args()
    build = Path(args.build_dir).resolve()
    for tool in ("sway", "grim"):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    binary = build / "bingux-shell/quickshell/src/bingux-shell"
    if not binary.is_file():
        print("build bingux-shell first (make shell); missing " + str(binary))
        return 1
    artifacts = build / "shell-archive"
    shutil.rmtree(artifacts, ignore_errors=True)
    artifacts.mkdir(parents=True)

    home = Path(tempfile.mkdtemp(prefix="bingux-shell-archive-"))
    try:
        stage = home / "stage"
        subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/install-shell.py"),
                "--prefix",
                "/usr",
                "--destdir",
                str(stage),
                "--build-dir",
                str(build),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        prefix = stage / "usr"
        runtime = home / "run"
        runtime.mkdir(mode=0o700)
        state = home / "state"
        (state / "bingux").mkdir(parents=True)
        (state / "bingux/greeter.json").write_text(json.dumps({"user": "luna", "session": "gnoblin"}))
        # A passwd-free account list, as in greeter-ui.py. bingux-shell puts
        # its own bin directory first on PATH, so the stand-in replaces the
        # staged bingux-session and hands everything else to the real one.
        session = prefix / "bin/bingux-session"
        session.rename(prefix / "bin/bingux-session.real")
        (home / "users.json").write_text(
            json.dumps({"users": [{"name": "luna", "displayName": "Luna Lovegood", "icon": "", "wallpaper": ""}]})
        )
        session.write_text(
            f'#!/bin/sh\ncase "$1" in\n    users) cat {home / "users.json"};;\n'
            f'    *) exec {prefix}/bin/bingux-session.real "$@";;\nesac\n'
        )
        session.chmod(0o755)
        sessions = home / "share/wayland-sessions"
        sessions.mkdir(parents=True)
        (sessions / "gnoblin.desktop").write_text(
            "[Desktop Entry]\nName=Gnoblin\nExec=/bin/true\nDesktopNames=Gnoblin;\n"
        )
        sock = str(runtime / "greetd.sock")
        greetd = load_fake_greetd()(sock)
        greetd.start()

        (home / "sway.conf").write_text("output HEADLESS-1 resolution 1280x800\n")
        environment = os.environ | {
            "HOME": str(home),
            "XDG_RUNTIME_DIR": str(runtime),
            "XDG_STATE_HOME": str(state),
            "XDG_CACHE_HOME": str(home / "cache"),
            "XDG_DATA_DIRS": str(home / "share"),
            "WLR_BACKENDS": "headless",
            "WLR_RENDERER": "pixman",
            "WLR_LIBINPUT_NO_DEVICES": "1",
            "GREETD_SOCK": sock,
            "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
            "QT_QPA_PLATFORM": "wayland",
        }
        # The binary derives these from its install prefix; a developer shell
        # must not leak its own values into the staged install.
        for name in (
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "BINGUX_CONFIG_PATH",
            "BINGUX_QUICKSHELL",
            "QML_IMPORT_PATH",
            "BINGUX_HELPER_DIR",
            "BINGUX_SHELL_ARCHIVE",
            "BINGUX_SHELL_SOURCE",
        ):
            environment.pop(name, None)
        sway = subprocess.Popen(
            ["sway", "-c", str(home / "sway.conf")],
            env=environment,
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

            # The bsm greeter talks to bsm's control socket; the fake server
            # (tests/bsm-fake-control.py) answers with the real protocol.
            bsm_assets = home / "bsm-assets"
            bsm_log = home / "bsm-fake.log"
            fake_bsm = subprocess.Popen(
                [
                    sys.executable,
                    str(ROOT / "tests/bsm-fake-control.py"),
                    "--socket", str(runtime / "bsm-control.sock"),
                    "--assets", str(bsm_assets),
                    "--log", str(bsm_log),
                ],
                stdout=subprocess.PIPE,
                text=True,
            )
            fake_bsm.stdout.readline()
            bsm_environment = environment | {
                "BSM_CONTROL_SOCKET": str(runtime / "bsm-control.sock"),
                "BSM_GREETER_CACHE": str(bsm_assets),
            }

            all_ok = True
            # The lock goes last: sway keeps the output locked (red) after a lock client ends.
            for name in ("bingux-greeter", "bingux-bsm-greeter", "bingux-lock"):
                command = prefix / "bin" / name
                shell_environment = environment
                if name == "bingux-bsm-greeter":
                    # The installed wrapper names its prefix (/usr here); run a copy
                    # that names the staged one instead.
                    staged = home / name
                    staged.write_text(command.read_text().replace("PREFIX=/usr\n", f"PREFIX={prefix}\n"))
                    staged.chmod(0o755)
                    shell_environment = bsm_environment
                    launch = [str(staged)]
                else:
                    launch = [str(command), "--no-color"]
                shell = subprocess.Popen(
                    launch,
                    env=shell_environment,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                )
                time.sleep(args.seconds)
                alive = shell.poll() is None
                shot = artifacts / f"{name}.png"
                subprocess.run(["grim", str(shot)], env=environment, check=False)
                shell.terminate()
                try:
                    output, _ = shell.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    shell.kill()
                    output, _ = shell.communicate()
                (artifacts / f"{name}.log").write_text(output)
                qml_errors = [
                    line
                    for line in output.splitlines()
                    if "TypeError" in line
                    or "ReferenceError" in line
                    or "no signal of the target" in line
                    or " ERROR scene:" in line
                    or "is not a type" in line
                ]
                if name == "bingux-bsm-greeter":
                    wrapper_ok = command.is_file() and not command.is_symlink() and os.access(command, os.X_OK)
                    first_check = ("command is an installed wrapper", wrapper_ok)
                else:
                    first_check = ("command is a bingux-shell symlink", os.readlink(command) == "bingux-shell")
                checks = {
                    first_check[0]: first_check[1],
                    "ran from the unpacked archive": "cache/bingux/shell/" in output and "Launching config" in output,
                    "alive after start": alive,
                    "no QML errors": not qml_errors,
                    "painted content": shot.is_file() and not png_is_flat(shot),
                }
                if name == "bingux-bsm-greeter":
                    checks["greeter said hello to bsm"] = bsm_log.is_file() and '"recv": "hello"' in bsm_log.read_text()
                (artifacts / f"{name}.checks.json").write_text(
                    json.dumps({"checks": checks, "qml_errors": qml_errors[:20], "screenshot": str(shot)}, indent=2)
                    + "\n"
                )
                print(name)
                for label, ok in checks.items():
                    print(("  ok   " if ok else "  FAIL ") + label)
                for line in qml_errors[:10]:
                    print("    " + line.strip())
                all_ok = all_ok and all(checks.values())
            print(f"artifacts: {artifacts}")
            return 0 if all_ok else 1
        finally:
            if "fake_bsm" in locals():
                fake_bsm.terminate()
            sway.terminate()
            sway.wait(timeout=5)
    finally:
        shutil.rmtree(home, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
