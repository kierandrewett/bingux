#!/usr/bin/env python3
"""Boot the Bingux OS live image from its firmware and exercise the whole session.

usage: tests/distro-boot.py [ISO]   (default: newest distro/out/bingux-os-*.iso)

The image boots the way a machine does: OVMF runs the ISO's boot loader, and
the boot menu, initramfs and splash are all on the recording. The run covers
what a user does on the live image:

1. the greeter comes up (greetd, Gnoblin in greeter mode, bingux-greeter);
2. typing the live password starts a Gnoblin session with the Bingux shell and
   the wallpaper;
3. `gnoblinctl session lock` shows the lock screen, and the password unlocks it;
4. `gnoblinctl session logout` returns to the greeter.

Keys go through the QEMU monitor, commands through the serial root shell.
Checks read the captured frames, not files.

Artifacts land in BINGUX_DISTRO_OUT (default distro/out/boot-test/):
checks.json, report.txt, serial.log, timeline.json, frame-events.jsonl,
flow.mp4 (10 fps) and one PNG per stage (greeter, session, locked, unlocked,
logged-out).
"""

import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time

from distro_capture import bright_count, blue_lead, is_black, session_painted
from distro_vm import ROOT, Vm, find_iso, missing_tools

# bingux-os-firstboot creates the live account with this password.
LIVE_USER = "live"
LIVE_PASSWORD = "live"
# The date and clock at the top of the greeter and the lock screen; nothing
# else on either screen, and nothing on the desktop, is near white.
CLOCK_REGION = "400x160+440+90"
CLOCK_PIXELS = 500
# The bar's clock text, and a wallpaper-only strip below the bar.
BAR_REGION = "160x24+560+4"
WALLPAPER_REGION = "1280x700+0+60"
SHELL_PATTERN = "'^/usr/bin/bingux( |$)'"
LOCK_STATE = 'gnoblinctl status | grep -o \'"lock_state":"[a-z]*"\''


def clock_shown(frame):
    return bright_count(frame, CLOCK_REGION) > CLOCK_PIXELS


def bar_visible(frame):
    return bright_count(frame, BAR_REGION) > 100


def wallpaper_visible(frame):
    return blue_lead(frame, WALLPAPER_REGION) > 0.05


def main():
    iso = find_iso(sys.argv[1] if len(sys.argv) > 1 else None)
    if iso is None or not iso.is_file():
        print("no live image; run distro/build.sh first")
        return 1
    missing = missing_tools("qemu-system-x86_64", "ffmpeg", "ffprobe")
    if missing:
        print("missing tool: " + ", ".join(missing))
        return 1
    out = Path(os.environ.get("BINGUX_DISTRO_OUT", ROOT / "distro/out/boot-test"))
    out.mkdir(parents=True, exist_ok=True)
    for stale in out.glob("*.png"):
        stale.unlink()
    work = Path(tempfile.mkdtemp(prefix="bingux-os-boot-"))
    vm = Vm(work, iso=iso, serial=True, record=out)
    checks = {}
    report = []

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

    try:
        vm.start()
        # Timeline hooks: the first frame with light on it, then each stage of the session.
        vm.recorder.expect("first picture", lambda frame: not is_black(frame))
        vm.recorder.expect("first greeter frame", clock_shown)
        vm.recorder.expect("first session bar", bar_visible)
        vm.recorder.expect("first session wallpaper", wallpaper_visible)
        checks["booted to a shell"] = vm.start_shell()
        if checks["booted to a shell"]:
            probe("greetd active", "systemctl is-active greetd", lambda o: o == "active", attempts=60, delay=2)
            # The greeter client follows the compositor; on a software-rendered
            # VM that can take a while after greetd reports active.
            probe(
                "gnoblin greeter compositor running",
                "pgrep -c -f 'gnoblin --greeter'",
                count,
                attempts=45,
                delay=2,
            )
            probe("bingux-greeter running", "pgrep -c -f bingux-greeter", count, attempts=45, delay=2)
            probe("live account exists", f"id -u {LIVE_USER}", lambda o: o.isdigit())
            probe(
                "greeter wallpaper snapshot",
                f"ls /var/lib/bingux/greeter/{LIVE_USER}",
                lambda o: "wallpaper" in o,
            )
            # Keys sent before the greeter is on screen are lost (baseline run 2 typed
            # into the bare wallpaper), so wait for its clock rather than a timer.
            frame = vm.wait_frame(clock_shown, attempts=60)
            checks["greeter shows the clock"] = frame is not None
            shot("greeter", frame)
            vm.recorder.mark("greeter interactive")
            # Give the greeter a moment to take keyboard focus after its first paint.
            time.sleep(3)

            # Sign in the way a user would: type into the focused field.
            vm.recorder.expect("first session frame", session_painted)
            vm.type_text(LIVE_PASSWORD)
            vm.key("ret")
            vm.recorder.mark("password submitted")
            # Still on the greeter after the wait: the keys were lost, so type again.
            if vm.wait_frame(session_painted, attempts=20) is None:
                greeter = vm.frame()
                if greeter is not None and clock_shown(greeter):
                    vm.type_text(LIVE_PASSWORD)
                    vm.key("ret")
            probe("session shell runs as the user", f"pgrep -c -u {LIVE_USER} -f {SHELL_PATTERN}", count, attempts=60)
            probe("session compositor runs as the user", f"pgrep -c -u {LIVE_USER} -x gnoblin", count)
            probe("session wallpaper service runs", f"pgrep -c -u {LIVE_USER} -f bingux-wallpaper", count)
            probe("logind session exists", f"loginctl list-sessions --no-legend | grep -c {LIVE_USER}", count)
            probe(
                "status service runs in the session",
                f"pgrep -c -u {LIVE_USER} -x bingux-statusd",
                count,
                attempts=20,
            )
            time.sleep(10)
            report.append("$ ps -u live\n" + vm.run(f"ps -u {LIVE_USER} -o pid=,args= | cut -c1-120") + "\n")
            frame = shot("session")
            checks["desktop shows the bar and wallpaper"] = frame is not None and session_painted(frame)

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
            # The lock client takes the lock first and paints its screen after.
            frame = vm.wait_frame(clock_shown, attempts=40)
            checks["lock screen shows the clock"] = frame is not None
            if frame is not None:
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
                shot("unlocked", frame)

            # Log out: back to the greeter, with a single statusd and no error.
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
            probe("greeter returns after log out", "pgrep -c -f 'gnoblin --greeter'", count, attempts=45, delay=2)
            probe("bingux-greeter returns after log out", "pgrep -c -f bingux-greeter", count, attempts=45, delay=2)
            time.sleep(10)
            probe("one statusd in the greeter", "pgrep -c -u greeter -x bingux-statusd", lambda o: o == "1")
            frame = shot("logged-out")
            checks["greeter shows the clock after log out"] = frame is not None and clock_shown(frame)
            report.append(
                "$ journalctl -b -u greetd\n"
                + vm.run("journalctl -b -u greetd --no-pager -o cat | tail -n 30", 60)
                + "\n"
            )
            report.append(
                "$ journalctl -b -p err\n" + vm.run("journalctl -b -p err --no-pager -o cat | tail -n 40", 60) + "\n"
            )
            report.append(
                "$ journalctl -b (user session)\n"
                + vm.run(f"journalctl -b --no-pager -o short-iso _UID=$(id -u {LIVE_USER}) | tail -n 80", 60)
                + "\n"
            )
            report.append(
                "$ journalctl -b (greeter)\n"
                + vm.run("journalctl -b --no-pager -o short-iso _UID=$(id -u greeter) | tail -n 80", 60)
                + "\n"
            )
        (out / "report.txt").write_text("\n".join(report))
        (out / "serial.log").write_bytes(vm.log)
        (out / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
    finally:
        vm.stop()
        shutil.rmtree(work, ignore_errors=True)
    for name, ok in checks.items():
        print(("ok   " if ok else "FAIL ") + name)
    print("artifacts:", out)
    return 0 if checks and all(checks.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
