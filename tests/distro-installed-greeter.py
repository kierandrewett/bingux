#!/usr/bin/env python3
"""Boot an installed Bingux OS disk three times and time the greeter cycle.

usage: tests/distro-installed-greeter.py [RUNS]   (default 3)

The disk comes from tests/distro-install.py (uefi-luks): its kept install is
the base image, and each run boots a throwaway qcow2 overlay on top of it, so
the kept install is never written. Each run walks what a user does on the
installed machine:

1. power on, the firmware runs the installed loader and the LUKS passphrase
   prompt is typed (the disk is encrypted);
2. the greeter comes up; the account signs in (retyped until a session paints,
   as the install test does);
3. the desktop paints;
4. `gnoblinctl session lock` shows the lock screen, and the password unlocks it;
5. `gnoblinctl session logout` ends the session.

Lock, unlock and logout run as the account over the serial console (the
installed system's serial getty), so the desktop itself is not driven.

Inputs (from distro-install.py's kept install; run that first):
  ~/.cache/bingux-e2e/keep/uefi-luks-installed.qcow2
  ~/.cache/bingux-e2e/keep/uefi-luks-OVMF_VARS.fd

Artifacts per run in distro/out/installed-greeter-<N>/: checks.json,
report.txt, serial.log, timeline.json, frame-events.jsonl, flow.mp4 (10 fps),
PNGs of the greeter, desktop, locked and logged-out frames. Medians across
runs go to distro/out/installed-greeter/summary.json.
"""

import json
import os
import re
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

from distro_capture import bright_count, greeter_shown, session_painted
from distro_installer_driver import text_on_screen
from distro_vm import ROOT, Vm, missing_tools

KEEP = Path.home() / ".cache/bingux-e2e/keep"
KEEP_DISK = KEEP / "uefi-luks-installed.qcow2"
KEEP_VARS = KEEP / "uefi-luks-OVMF_VARS.fd"
USER = "tester"
PASSWORD = "bingux-test-2026"
PASSPHRASE = "bingux-luks-2026"
LOCK_STATE = 'gnoblinctl status | grep -o \'"lock_state":"[a-z]*"\''
# The lock screen's big clock (see tests/distro-boot.py for the same region).
LOCK_CLOCK_REGION = "400x160+440+90"
LOCK_CLOCK_PIXELS = 2000


def lock_clock_shown(frame):
    return bright_count(frame, LOCK_CLOCK_REGION) > LOCK_CLOCK_PIXELS


def wait_for_text(vm, phrase, timeout, poll=2.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frame = vm.frame()
        if frame is not None and text_on_screen(frame, phrase) is not None:
            return True
        time.sleep(poll)
    return False


def make_overlay(work):
    """A qcow2 overlay on the kept install: the kept disk stays read-only."""
    overlay = work / "installed.qcow2"
    subprocess.run(
        ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b", str(KEEP_DISK), str(overlay)],
        check=True,
        capture_output=True,
    )
    vars_copy = work / "OVMF_VARS.fd"
    shutil.copy(KEEP_VARS, vars_copy)
    return overlay, vars_copy


def serial_login(vm, timeout=300):
    """Log in as the account on the installed system's serial console; True when the shell answers as it."""
    if not vm.wait_for("login:", timeout):
        return False
    vm._serial.sendall(f"{USER}\n".encode())
    if not vm.wait_for("Password:", 30):
        return False
    time.sleep(1)
    vm._serial.sendall(f"{PASSWORD}\n".encode())
    time.sleep(3)
    # Turn the echo off so only command output is captured.
    vm._serial.sendall(b"stty -echo; export PAGER=cat XDG_RUNTIME_DIR=/run/user/$(id -u)\n")
    time.sleep(2)
    # The console echoes the prompt after the output; compare the answer lines only.
    return USER in clean_lines(vm.run("id -un", 30))


ANSI_ESCAPE = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-9;?]*[A-Za-z]|\x1b[=>]")


def clean_lines(output):
    """Console lines with terminal escapes removed, blanks dropped."""
    text = ANSI_ESCAPE.sub("", output).replace("\r", "")
    return [line.strip() for line in text.splitlines() if line.strip()]


def last_answer(output):
    """The last console line that is not the shell prompt."""
    lines = [line for line in clean_lines(output) if not line.endswith("$")]
    return lines[-1] if lines else ""


def run_once(index, out_root):
    out = out_root / f"installed-greeter-{index}"
    out.mkdir(parents=True, exist_ok=True)
    for stale in out.glob("*.png"):
        stale.unlink()
    work = Path(tempfile.mkdtemp(prefix="bingux-os-greeter-"))
    checks = {}
    report = []
    timeline = {}
    try:
        disk, vars_path = make_overlay(work)
        vm = Vm(
            work / "vm",
            disk=disk,
            uefi=True,
            serial=True,
            vars_path=vars_path,
            record=out,
        )

        def stamp(name):
            timeline[name] = round(time.monotonic() - vm.t0, 2)
            vm.recorder.mark(name)
            print(f"[{index}] [{timeline[name]:8.1f}s] {name}", flush=True)

        def still(name, frame=None):
            return vm.still(out / f"{name}.png", frame)

        vm.start()
        stamp("power on")
        checks["passphrase prompt shown at boot"] = wait_for_text(vm, "passphrase", 180)
        stamp("typing the disk passphrase")
        vm.type_text(PASSPHRASE)
        vm.key("ret")
        frame = vm.wait_frame(greeter_shown, attempts=120)
        checks["greeter shown"] = frame is not None
        if frame is not None:
            stamp("greeter shown")
            still("greeter", frame)
        frame = None
        for attempt in range(3):
            time.sleep(10)
            if attempt:
                for _ in range(len(PASSWORD) + 8):
                    vm.key("backspace")
            vm.type_text(PASSWORD)
            vm.key("ret")
            stamp(f"sign-in submitted {attempt}")
            frame = vm.wait_frame(session_painted, attempts=60)
            if frame is not None:
                break
            last = vm.frame()
            if last is not None:
                still(f"after-login-{attempt}", last)
        checks["sign-in paints the desktop"] = frame is not None
        if frame is not None:
            stamp("desktop painted")
            still("desktop", frame)

            checks["serial login as the account"] = serial_login(vm)
            stamp("lock requested")
            report.append("$ gnoblinctl session lock\n" + vm.run("gnoblinctl session lock 2>&1", 30) + "\n")
            vm.recorder.mark("lock requested done")
            lock_out = vm.poll(LOCK_STATE, lambda o: '"locked"' in o, attempts=20, delay=2)
            checks["session locks on request"] = '"locked"' in lock_out
            frame = vm.wait_frame(lock_clock_shown, attempts=40)
            checks["lock screen shows the clock"] = frame is not None
            if frame is not None:
                stamp("lock screen shown")
                still("locked", frame)
            time.sleep(2)
            stamp("unlock submitted")
            vm.type_text(PASSWORD)
            vm.key("ret")
            unlock_out = vm.poll(LOCK_STATE, lambda o: '"unlocked"' in o, attempts=20, delay=2)
            checks["session unlocks with the password"] = '"unlocked"' in unlock_out
            time.sleep(3)
            frame = vm.wait_frame(session_painted, attempts=20)
            checks["desktop back after unlock"] = frame is not None
            if frame is not None:
                stamp("desktop after unlock")
                still("unlocked", frame)

            stamp("logout requested")
            report.append("$ gnoblinctl session logout\n" + vm.run("gnoblinctl session logout 2>&1", 30) + "\n")
            left = vm.poll(
                f"pgrep -c -u {USER} -x gnoblin",
                lambda o: last_answer(o) == "0",
                attempts=30,
                delay=2,
            )
            checks["user session ends on log out"] = last_answer(left) == "0"
            time.sleep(10)
            still("logged-out")
            stamp("logout done")
        report.append("$ journalctl -b -p err\n" + vm.run("journalctl -b -p err --no-pager -o cat | tail -n 40", 60) + "\n")
        (out / "report.txt").write_text("\n".join(report))
        (out / "serial.log").write_bytes(vm.log)
        (out / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
    finally:
        vm.stop()
        shutil.rmtree(work, ignore_errors=True)
    (out / "stages.json").write_text(json.dumps(timeline, indent=2) + "\n")
    for name, ok in checks.items():
        print(f"[{index}] " + ("ok   " if ok else "FAIL ") + name)
    return timeline, checks


# Intervals between named stages, in seconds, reported as medians across runs.
SPANS = {
    "power on to greeter": ("power on", "greeter shown"),
    "greeter to desktop": ("greeter shown", "desktop painted"),
    "lock request to lock screen": ("lock requested", "lock screen shown"),
    "unlock to desktop": ("unlock submitted", "desktop after unlock"),
    "logout request to session gone": ("logout requested", "logout done"),
}


def summarise(out_root, results):
    spans = {}
    for label, (start, end) in SPANS.items():
        values = []
        for timeline, _ in results:
            if start in timeline and end in timeline:
                values.append(round(timeline[end] - timeline[start], 2))
        spans[label] = {"runs": values, "median": round(statistics.median(values), 2) if values else None}
    passed = [all(checks.values()) for _, checks in results]
    summary = {"runs": len(results), "all_checks_passed": passed, "spans": spans}
    (out_root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def main():
    runs = int(sys.argv[1]) if len(sys.argv) > 1 else 3
    for path in (KEEP_DISK, KEEP_VARS):
        if not path.is_file():
            print(f"missing {path}; run tests/distro-install.py uefi-luks first")
            return 1
    missing = missing_tools("qemu-system-x86_64", "qemu-img", "ffmpeg", "ffprobe", "tesseract")
    if missing:
        print("missing tool: " + ", ".join(missing))
        return 1
    out_root = Path(os.environ.get("BINGUX_DISTRO_OUT", ROOT / "distro/out/installed-greeter"))
    out_root.mkdir(parents=True, exist_ok=True)
    results = []
    for index in range(1, runs + 1):
        results.append(run_once(index, out_root.parent))
    summary = summarise(out_root, results)
    print(json.dumps(summary, indent=2))
    return 0 if all(summary["all_checks_passed"]) else 1


if __name__ == "__main__":
    sys.exit(main())
