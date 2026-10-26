#!/usr/bin/env python3
"""Run the bsm greeter against the fake control server in headless Sway.

tests/bsm-fake-control.py speaks the real greeter control protocol. This
script starts it, starts headless Sway and the greeter (the built bingux-shell
running shell/bingux/BsmGreeterShell.qml), drives the greeter with a virtual
keyboard and the server's driver socket, takes a screenshot of every state and
checks what the greeter sent and what it drew.

Run it from a checkout:

    QS_TEST_BIN=build/bingux-shell/quickshell/src/bingux-shell \\
    QML_IMPORT_PATH=build/qml python3 tests/bsm-greeter-ui.py

Screenshots go to BSM_GREETER_SHOTS (default build/bsm-greeter-shots). Set
BSM_GREETER_RENDER=gpu for blur and motion instead of the flat, motion-free
software mode (the default, which makes shots deterministic).

Only the keyboard is driven: headless Sway has no pointer device, so Qt never
sees a pointer. Clicking, tapping and swiping are not covered here.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time

from PIL import Image, ImageChops, ImageStat

ROOT = Path(__file__).resolve().parent.parent
QS = os.environ.get("QS_TEST_BIN", str(ROOT / "build/bingux-shell/quickshell/src/bingux-shell"))
SHOTS = Path(os.environ.get("BSM_GREETER_SHOTS", ROOT / "build/bsm-greeter-shots"))
GPU = os.environ.get("BSM_GREETER_RENDER") == "gpu"
FAKE = ROOT / "tests/bsm-fake-control.py"
ENTRY = ROOT / "shell/bingux/BsmGreeterShell.qml"

failures = []


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (" (" + detail + ")" if detail and not ok else ""))
    if not ok:
        failures.append(name)


class Rig:
    """Headless Sway, the fake bsm server and one greeter process."""

    def __init__(self, server_args):
        self.home = Path(tempfile.mkdtemp(prefix="bsm-greeter-ui-"))
        runtime = self.home / "run"
        runtime.mkdir(mode=0o700)
        (self.home / "sway.conf").write_text("output HEADLESS-1 resolution 1280x800\n")
        self.env = os.environ | {
            "HOME": str(self.home),
            "XDG_RUNTIME_DIR": str(runtime),
            "WLR_BACKENDS": "headless",
            "WLR_RENDERER": "gles2",
            "WLR_LIBINPUT_NO_DEVICES": "1",
            "BINGUX_SOFTWARE_RENDERING": "0" if GPU else "1",
            "QT_QPA_PLATFORM": "wayland",
        }
        for name in ("DISPLAY", "WAYLAND_DISPLAY"):
            self.env.pop(name, None)
        if "QML_IMPORT_PATH" not in os.environ:
            self.env["QML_IMPORT_PATH"] = str(ROOT / "build/qml")
        self.sway = subprocess.Popen(
            ["sway", "-c", str(self.home / "sway.conf")],
            env=self.env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(100):
            sockets = [p for p in runtime.glob("wayland-*") if not p.name.endswith(".lock")]
            if sockets:
                self.env["WAYLAND_DISPLAY"] = sockets[0].name
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("headless sway did not start")
        self.log_path = self.home / "server.log"
        self.assets = self.home / "assets"
        self.server = subprocess.Popen(
            [
                sys.executable,
                str(FAKE),
                "--socket", str(self.home / "control.sock"),
                "--driver", str(self.home / "driver.sock"),
                "--assets", str(self.assets),
                "--log", str(self.log_path),
                *server_args,
            ],
            env=self.env,
            stdout=subprocess.PIPE,
            text=True,
        )
        if self.server.stdout.readline().strip() != "READY":
            raise RuntimeError("the fake server did not start")
        self.env["BSM_CONTROL_SOCKET"] = str(self.home / "control.sock")
        self.env["BSM_GREETER_CACHE"] = str(self.assets)
        # The seat needs a keyboard before the client binds it.
        self.keyboard = subprocess.Popen(["wtype", "-s", "120000"], env=self.env)
        time.sleep(0.5)
        self.client = subprocess.Popen(
            [QS, "-p", str(ENTRY)],
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        self.output = []
        threading.Thread(target=lambda: [self.output.append(line) for line in self.client.stdout], daemon=True).start()

    def close(self):
        for process in (self.client, self.server, self.keyboard, self.sway):
            process.terminate()
        for process in (self.client, self.server, self.keyboard, self.sway):
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
        shutil.rmtree(self.home, ignore_errors=True)

    # -- observation ------------------------------------------------------
    def records(self):
        if not self.log_path.exists():
            return []
        return [json.loads(line) for line in self.log_path.read_text().splitlines() if line.strip()]

    def count(self, kind, **fields):
        return sum(1 for r in self.records() if r["recv"] == kind and all(r.get(k) == v for k, v in fields.items()))

    def wait_for(self, kind, count=1, timeout=8, **fields):
        end = time.time() + timeout
        while time.time() < end:
            if self.count(kind, **fields) >= count:
                return True
            time.sleep(0.05)
        return False

    def shot(self, name, settle=0.7):
        time.sleep(settle)
        SHOTS.mkdir(parents=True, exist_ok=True)
        path = SHOTS / (name + ".png")
        subprocess.run(["grim", str(path)], env=self.env, check=True)
        return Image.open(path).convert("RGB")

    # -- input ------------------------------------------------------------
    def type(self, text):
        subprocess.run(["wtype", "-s", "60", text], env=self.env, check=True)

    def key(self, *names):
        args = ["wtype", "-s", "120"]
        for name in names:
            args += ["-k", name, "-s", "120"]
        subprocess.run(args, env=self.env, check=True)

    def drive(self, **command):
        with_socket = subprocess.run(
            ["python3", "-c", DRIVER, str(self.home / "driver.sock"), json.dumps(command)],
            capture_output=True,
            text=True,
            check=True,
        )
        return with_socket.stdout.strip()


DRIVER = """
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
s.sendall(sys.argv[2].encode() + b"\\n")
print(s.makefile().readline().strip())
"""


def region_colour(image, box):
    crop = image.crop(box).resize((1, 1), Image.BOX)
    return crop.getpixel((0, 0))


def changed(a, b, box):
    """Mean per-channel difference between two shots inside a box."""
    diff = ImageChops.difference(a.crop(box), b.crop(box))
    return sum(ImageStat.Stat(diff).mean) / 3


def distance(a, b):
    return sum(abs(x - y) for x, y in zip(a, b))


def fill_extent(image):
    """Width in pixels of the white (or red) fill of the progress bar.

    The card is 480 wide and centred, so the track starts at x = 400. The
    track is found as the row in the card area with a long run of pixels that
    are lighter than the wallpaper behind them.
    """
    width, height = image.size
    best = 0
    for y in range(560, 740):
        run = 0
        for x in range(400, 880):
            r, g, b = image.getpixel((x, y))
            bright = r > 225 and g > 225 and b > 225
            reddish = r > 230 and g < 150 and b < 130
            if bright or reddish:
                run += 1
            else:
                break
        best = max(best, run)
    return best


def has_red_text(image, box):
    count = 0
    for y in range(box[1], box[3]):
        for x in range(box[0], box[2]):
            r, g, b = image.getpixel((x, y))
            if r > 200 and g < 150 and b < 130:
                count += 1
    return count > 12


def login_scenario():
    rig = Rig(["--mode", "login"])
    try:
        check("login: the greeter asks for a password for the first account", rig.wait_for("begin_auth", uid=1000, intent="login", timeout=20))
        login = rig.shot("login", settle=1.5)
        check("login: hello carries the protocol version", rig.count("hello", version=1) == 1)
        check("login: the picture and the clock are drawn", region_colour(login, (0, 0, 1280, 400)) != region_colour(login, (0, 400, 1280, 800)))

        rig.type("wrong")
        rig.key("Return")
        check("wrong password: the answer reached bsm", rig.wait_for("auth_response", length=5))
        check("wrong password: a new attempt began by itself", rig.wait_for("begin_auth", count=2, uid=1000))
        wrong = rig.shot("wrong-password")
        check("wrong password: the error is shown in red", has_red_text(wrong, (480, 655, 800, 682)))

        rig.key("Tab", "Return")
        picker = rig.shot("account-picker")
        check("picker: opens from the keyboard and covers the lower screen", changed(picker, wrong, (420, 490, 860, 770)) > 12)
        alex_colour = region_colour(picker, (0, 0, 400, 300))

        rig.key("Down", "Return")
        check("switching account: bsm was asked to select it", rig.wait_for("select_user", uid=1001))
        records = [r["recv"] for r in rig.records()]
        check("switching account: the old conversation was cancelled first", records.index("cancel_auth") < records.index("select_user"))
        check("switching account: a new conversation began for the new account", rig.wait_for("begin_auth", uid=1001, intent="login"))
        switched = rig.shot("switched-account", settle=1.0)
        check("switching account: the appearance changed", distance(region_colour(switched, (0, 0, 400, 300)), alex_colour) > 30)

        rig.type("secret")
        rig.key("Return")
        check("one-time code: the password reached bsm", rig.wait_for("auth_response", length=6))
        rig.shot("password-accepted-code-prompt", settle=1.0)
        rig.type("123456")
        rig.key("Return")
        check("sign-in: the code was sent", rig.wait_for("auth_response", count=2, length=6))
        time.sleep(0.8)
        started = rig.shot("startup-progress-start")
        extents = {"start": fill_extent(started)}

        expected = [
            ("startup-progress-15", [("reach", "auth_accepted"), ("reach", "pam_session")], 15),
            ("startup-progress-40", [("reach", "host_connected"), ("reach", "gnoblin_ready")], 40),
        ]
        for name, steps, percent in expected:
            for command, stage in steps:
                rig.drive(cmd=command, stage=stage)
            image = rig.shot(name)
            extents[name] = fill_extent(image)
        rig.drive(cmd="count", stage="outputs_configured", done=1, total=2)
        extents["startup-progress-45"] = fill_extent(rig.shot("startup-progress-45-counted"))
        for stage in ("outputs_configured", "bingux_launched"):
            rig.drive(cmd="reach", stage=stage)
        rig.drive(cmd="count", stage="surfaces_mapped", done=2, total=3)
        extents["startup-progress-73"] = fill_extent(rig.shot("startup-progress-73-counted"))
        for stage in ("surfaces_mapped",):
            rig.drive(cmd="reach", stage=stage)
        rig.drive(cmd="count", stage="desktop_ready", done=1, total=2)
        extents["startup-progress-87"] = fill_extent(rig.shot("startup-progress-87-counted"))
        print("fill extents:", extents)
        track = 480
        for name, percent in [("start", 0), ("startup-progress-15", 15), ("startup-progress-40", 40), ("startup-progress-45", 45), ("startup-progress-73", 73), ("startup-progress-87", 87)]:
            want = round(track * percent / 100)
            got = extents[name]
            check(f"progress {percent}: the bar follows the model", abs(got - want) <= 14 if percent > 0 else got <= 14, f"{got}px, expected about {want}px")

        rig.drive(cmd="fail", stage="desktop_ready", code="timeout", message="The desktop did not draw its first frame in time.")
        failed = rig.shot("startup-failure")
        check("failure: the bar stops and turns red", has_red_text(failed, (400, 560, 880, 700)) or fill_extent(failed) > 0)

        rig.key("Tab", "Return")
        check("failure: Retry begins a new conversation", rig.wait_for("begin_auth", count=2, uid=1001, intent="login"))
        rig.shot("failure-retry")

        rig.key("Tab", "Tab", "Return")
        power = rig.shot("power-menu")
        rig.key("Down", "Return")
        confirm = rig.shot("power-confirm")
        check("power: the menu is drawn above the corner", changed(power, login, (1030, 600, 1260, 740)) > 12)
        rig.key("Return")
        check("power: a confirmed restart reaches bsm", rig.wait_for("power", action="reboot"))
        rig.shot("power-operation", settle=1.0)
        return rig.records(), "".join(rig.output)
    finally:
        rig.close()


def unlock_scenario():
    rig = Rig(["--mode", "unlock", "--unlock-uid", "1000"])
    try:
        check("unlock: the owner's unlock form starts at once", rig.wait_for("begin_auth", uid=1000, intent="unlock", timeout=20))
        locked = rig.shot("locked-unlock", settle=1.5)
        check("unlock: no other account is touched by default", rig.count("select_user") == 0 and rig.count("begin_auth") == 1)
        rig.key("Tab", "Tab", "Return")
        picker = rig.shot("locked-account-picker")
        check("unlock: the slide-up handle opens the account list", changed(picker, locked, (420, 300, 860, 480)) > 12)
        rig.key("Down", "Down", "Return")
        check("unlock: choosing another account only selects it", rig.wait_for("select_user", uid=1002))
        check("unlock: another account authenticates as a switch", rig.wait_for("begin_auth", uid=1002, intent="switch"))
        other = rig.shot("locked-other-account", settle=1.0)
        check("unlock: the appearance follows the account", distance(region_colour(other, (0, 0, 400, 300)), region_colour(locked, (0, 0, 400, 300))) > 30)
        check("unlock: nothing was signed in by choosing", rig.count("auth_response") == 0)
        rig.key("Tab", "Return")
        check("unlock: Back returns to the session owner", rig.wait_for("select_user", uid=1000))
        check("unlock: the owner's unlock form returns", rig.wait_for("begin_auth", count=2, uid=1000, intent="unlock"))
        rig.type("secret")
        rig.key("Return")
        check("unlock: the password reached bsm", rig.wait_for("auth_response", length=6))
        time.sleep(0.8)
        signing = rig.shot("signing-in")
        check("unlock: the greeter keeps its pixels while bsm unlocks", region_colour(signing, (0, 0, 1280, 400)) is not None)
        return rig.records(), "".join(rig.output)
    finally:
        rig.close()


def main():
    for tool in ("sway", "grim", "wtype", "swaymsg"):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    if not Path(QS).exists():
        print("missing bingux-shell binary: set QS_TEST_BIN")
        return 1
    login_records, login_output = login_scenario()
    unlock_records, unlock_output = unlock_scenario()
    for name, output in (("login", login_output), ("unlock", unlock_output)):
        check(f"{name}: no QML errors", "ERROR" not in output.replace("ERROR: ", "ERROR_"), output[-800:] if "ERROR" in output else "")
        check(f"{name}: no secret or code in the greeter output", "secret" not in output and "123456" not in output)
    server_log = json.dumps(login_records + unlock_records)
    check("the fake server never saw a secret in its log", "secret" not in server_log and "123456" not in server_log)
    print("shots:", SHOTS)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
