#!/usr/bin/env python3
"""Boot the Bingux OS live image from its firmware and walk the live session.

usage: tests/distro-boot.py [ISO]   (default: newest distro/out/bingux-os-*.iso)

The image boots the way a machine does: OVMF runs the ISO's boot loader, and
the boot menu, splash and installer are all on the recording. The run covers
what a user does on the live image:

1. the boot menu comes up, then the first picture after it (the splash);
2. the live session lands in the installer kiosk, whose welcome page is
   interactive (its OCR title is readable and it takes a click);
3. "Try Bingux" on the welcome page starts the live desktop, and the bar and
   wallpaper paint (`bingux-live-switch live`);
4. `gnoblinctl session lock` shows the lock screen, and the live password
   unlocks it;
5. `gnoblinctl session logout` ends the live desktop.

The greeter sign-in is not part of the live flow: it is covered by
tests/distro-install.py (installed disk) and tests/distro-installed-greeter.py.

Keys go through the QEMU monitor, commands through the serial root shell.
Checks read the captured frames, not files.

Artifacts land in BINGUX_DISTRO_OUT (default distro/out/boot-test/):
checks.json, report.txt, serial.log, timeline.json, frame-events.jsonl,
flow.mp4 (10 fps) and one PNG per stage (menu, welcome, desktop, locked,
unlocked, logged-out).
"""

import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time

from distro_capture import bright_count, blue_lead, is_black, session_painted
from distro_installer_driver import Installer, Pointer
from distro_vm import ROOT, Vm, find_iso, missing_tools

# bingux-os-firstboot creates the live account with this password.
LIVE_USER = "live"
LIVE_PASSWORD = "live"
# The boot menu's highlighted entry: a grey bar (not the blue wallpaper) across the middle of the screen.
MENU_BAR = "560x24+360+355"
# The lock screen's big clock. The desktop's bar clock is top-centre, and the
# greeter has no clock in this box, so 2000 bright pixels separates the lock
# screen (about 5800) from the desktop (about 860).
LOCK_CLOCK_REGION = "400x160+440+90"
LOCK_CLOCK_PIXELS = 2000
SHELL_PATTERN = "'^/usr/bin/bingux( |$)'"
LOCK_STATE = 'gnoblinctl status | grep -o \'"lock_state":"[a-z]*"\''
WELCOME_TITLE = "Welcome to Bingux"
TRY_BINGUX = "Try Bingux"


def lock_clock_shown(frame):
    return bright_count(frame, LOCK_CLOCK_REGION) > LOCK_CLOCK_PIXELS


def menu_shown(frame):
    """The boot menu: a grey highlight bar (bright, not blue) on the black screen."""
    return bright_count(frame, MENU_BAR, level=120) > 300 and blue_lead(frame, MENU_BAR) < 0.02


def main():
    iso = find_iso(sys.argv[1] if len(sys.argv) > 1 else None)
    if iso is None or not iso.is_file():
        print("no live image; run distro/build.sh first")
        return 1
    missing = missing_tools("qemu-system-x86_64", "ffmpeg", "ffprobe", "tesseract")
    if missing:
        print("missing tool: " + ", ".join(missing))
        return 1
    out = Path(os.environ.get("BINGUX_DISTRO_OUT", ROOT / "distro/out/boot-test"))
    out.mkdir(parents=True, exist_ok=True)
    for stale in out.glob("*.png"):
        stale.unlink()
    work = Path(tempfile.mkdtemp(prefix="bingux-os-boot-"))
    vm = Vm(work, iso=iso, serial=True, record=out, pointer=True)
    checks = {}
    report = []
    timeline = {}

    def probe(label, command, accept, *, as_user=None, attempts=1, delay=3):
        output = vm.poll(command, accept, attempts=attempts, delay=delay, as_user=as_user)
        report.append(f"$ {command}\n{output}\n")
        checks[label] = bool(accept(output))
        return output

    def shot(name, frame=None):
        """Save a stage PNG (the given frame, else the newest one); returns the frame, or None."""
        return vm.still(out / f"{name}.png", frame)

    def count(output):
        return output.isdigit() and int(output) > 0

    def stamp(name):
        timeline[name] = round(time.monotonic() - vm.t0, 2)
        vm.recorder.mark(name)
        print(f"[{timeline[name]:8.1f}s] {name}", flush=True)

    try:
        vm.start()
        vm.recorder.expect("boot menu", menu_shown)
        vm.recorder.expect("first picture", lambda frame: not is_black(frame))
        vm.recorder.expect("first session frame", session_painted)
        checks["boot menu shown"] = vm.wait_frame(lambda f: menu_shown(f), attempts=40, delay=1) is not None
        if checks["boot menu shown"]:
            shot("menu", vm.frame())
        stamp("boot menu")

        checks["booted to a shell"] = vm.start_shell()
        if checks["booted to a shell"]:
            probe("live account exists", f"id -u {LIVE_USER}", lambda o: o.isdigit())
            probe("greetd active", "systemctl is-active greetd", lambda o: o == "active", attempts=60, delay=2)

            # The installer welcome page is the first interactive state of the live image.
            pointer = Pointer(work / "vnc.sock", vm.frame)
            inst = Installer(vm, pointer, log=lambda message: print(message, flush=True))
            checks["live session lands in the installer"] = inst.wait_page(WELCOME_TITLE, 420)
            stamp("installer welcome")
            if checks["live session lands in the installer"]:
                stamp("installer interactive")
                shot("welcome", inst.frame())
                # Try Bingux leaves the installer for the live desktop.
                stamp("submit try bingux")
                checks["try bingux is offered"] = inst.click_text(TRY_BINGUX, timeout=20)
                frame = vm.wait_frame(session_painted, attempts=60)
                checks["try bingux reaches the live desktop"] = frame is not None
                if frame is not None:
                    stamp("live desktop bar")
                    shot("desktop", frame)
                pointer.close()
                time.sleep(10)
                probe(
                    "session shell runs as the user",
                    f"pgrep -c -u {LIVE_USER} -f {SHELL_PATTERN}",
                    count,
                    attempts=60,
                )
                probe("session compositor runs as the user", f"pgrep -c -u {LIVE_USER} -x gnoblin", count)
                probe("session wallpaper service runs", f"pgrep -c -u {LIVE_USER} -f bingux-wallpaper", count)
                probe(
                    "status service runs in the session",
                    f"pgrep -c -u {LIVE_USER} -x bingux-statusd",
                    count,
                    attempts=20,
                )
                report.append("$ ps -u live\n" + vm.run(f"ps -u {LIVE_USER} -o pid=,args= | cut -c1-120") + "\n")

                # Lock: the request reaches statusd, which starts the lock client.
                vm.recorder.mark("lock requested")
                report.append(
                    "$ gnoblinctl session lock\n" + vm.run_as(LIVE_USER, "gnoblinctl session lock", timeout=30) + "\n"
                )
                probe(
                    "session locks on request",
                    LOCK_STATE,
                    lambda o: '"locked"' in o,
                    as_user=LIVE_USER,
                    attempts=20,
                    delay=2,
                )
                frame = vm.wait_frame(lock_clock_shown, attempts=40)
                checks["lock screen shows the clock"] = frame is not None
                if frame is not None:
                    vm.recorder.mark("lock screen shown")
                    shot("locked", frame)
                time.sleep(2)
                vm.recorder.mark("unlock submitted")
                vm.type_text(LIVE_PASSWORD)
                vm.key("ret")
                probe(
                    "session unlocks with the password",
                    LOCK_STATE,
                    lambda o: '"unlocked"' in o,
                    as_user=LIVE_USER,
                    attempts=20,
                    delay=2,
                )
                time.sleep(3)
                frame = vm.wait_frame(session_painted, attempts=20)
                checks["desktop back after unlock"] = frame is not None
                if frame is not None:
                    vm.recorder.mark("desktop after unlock")
                    shot("unlocked", frame)

                # Log out: the live desktop ends and the session's processes are gone.
                vm.recorder.mark("logout requested")
                report.append(
                    "$ gnoblinctl session logout\n" + vm.run_as(LIVE_USER, "gnoblinctl session logout", timeout=30) + "\n"
                )
                probe(
                    "user session ends on log out",
                    f"pgrep -c -u {LIVE_USER} -x gnoblin",
                    lambda o: o == "0",
                    attempts=30,
                    delay=2,
                )
                time.sleep(10)
                shot("logged-out")
                vm.recorder.mark("logout done")
            report.append(
                "$ journalctl -b -p err\n" + vm.run("journalctl -b -p err --no-pager -o cat | tail -n 40", 60) + "\n"
            )
            report.append(
                "$ journalctl -b (user session)\n"
                + vm.run(f"journalctl -b --no-pager -o short-iso _UID=$(id -u {LIVE_USER}) | tail -n 80", 60)
                + "\n"
            )
        (out / "report.txt").write_text("\n".join(report))
        (out / "serial.log").write_bytes(vm.log)
        (out / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
    finally:
        vm.stop()
        shutil.rmtree(work, ignore_errors=True)
    (out / "stamps.json").write_text(json.dumps(timeline, indent=2) + "\n")
    for name, ok in checks.items():
        print(("ok   " if ok else "FAIL ") + name)
    print("artifacts:", out)
    return 0 if checks and all(checks.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
