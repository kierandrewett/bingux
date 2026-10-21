#!/usr/bin/env python3
"""Install Bingux OS onto a blank virtual disk, then boot and sign in to it.

usage: tests/distro-install.py [ISO]   (default: newest distro/out/bingux-os-*.iso)

1. Boot the live image in QEMU with OVMF (UEFI) and a blank 20 GiB disk.
2. Run `bingux-os-install` (archinstall) over the serial console. The Bingux
   OS packages come from the repository on the image; the base system comes
   from the host's network through QEMU user networking, so this needs the
   internet and takes a while.
3. Mount the result and check the packages, the bootloader entry, the repository
   section and the enabled units.
4. Power off, boot the disk alone through its own bootloader, and check that
   the greeter appears, that the new account signs in and that the desktop
   shows its wallpaper.

The installed system has no serial console, so step 4 judges screenshots.
Artifacts land in BINGUX_DISTRO_OUT (default distro/out/install-test/):
checks.json, report.txt, install.log and PNGs of the greeter and the session.
The disk image lives in the temporary directory, which needs about 6 GiB.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from distro_vm import ROOT, Vm, bright_pixels, find_iso, missing_tools, session_painted

USER = "tester"
PASSWORD = "bingux-test-2026"
CLOCK_REGION = "400x160+440+90"
CLOCK_PIXELS = 500
INSTALL_TIMEOUT = 3600


def main():
    iso = find_iso(sys.argv[1] if len(sys.argv) > 1 else None)
    if iso is None or not iso.is_file():
        print("no live image; run distro/build.sh first")
        return 1
    missing = missing_tools("qemu-system-x86_64", "qemu-img", "bsdtar", "magick", "blkid")
    if missing:
        print("missing tool: " + ", ".join(missing))
        return 1
    out = Path(os.environ.get("BINGUX_DISTRO_OUT", ROOT / "distro/out/install-test"))
    out.mkdir(parents=True, exist_ok=True)
    for stale in out.glob("*.png"):
        stale.unlink()
    work = Path(tempfile.mkdtemp(prefix="bingux-os-install-"))
    disk = work / "disk.qcow2"
    subprocess.run(["qemu-img", "create", "-f", "qcow2", str(disk), "20G"], check=True, capture_output=True)
    vars_file = work / "OVMF_VARS.fd"
    checks = {}
    report = []
    install_log = ""
    live = installed = None
    try:
        (work / "live").mkdir()
        live = Vm(work / "live", iso=iso, disk=disk, uefi=True, serial=True, vars_path=vars_file)
        live.start()
        checks["live image booted to a shell"] = live.start_shell()
        if checks["live image booted to a shell"]:
            checks["live image started in UEFI mode"] = live.run("test -d /sys/firmware/efi && echo yes") == "yes"
            checks["bingux repository is on the image"] = "bingux-os.db" in live.run("ls /opt/bingux-os/repo")
            live.run(f"printf '%s' '{PASSWORD}' >/root/password && chmod 600 /root/password")
            started = time.monotonic()
            # archinstall enters the new system through `systemd-run --pty`, so the
            # installer needs a terminal, as it has on a console. `script`
            # provides one and keeps the transcript.
            live.run(
                "script -qefc 'bingux-os-install --disk /dev/vda --user "
                f"{USER} --password-file /root/password --yes' /root/install.log >/dev/null 2>&1 </dev/null &"
            )
            finished = live.poll(
                "pgrep -c -f 'bingux-os-install|archinstall' || true",
                lambda output: output == "0",
                attempts=INSTALL_TIMEOUT // 20,
                delay=20,
            )
            install_log = live.run("tr '\\r' '\\n' </root/install.log | tail -n 400", 60)
            report.append(f"install took {int(time.monotonic() - started)} s, processes left: {finished}\n")
            checks["installer finished successfully"] = 'COMMAND_EXIT_CODE="0"' in install_log

            if checks["installer finished successfully"]:
                live.run("mount /dev/vda2 /mnt && mount /dev/vda1 /mnt/boot")
                inspections = {
                    "bingux-os packages installed": ("pacman -r /mnt -Q bingux-os bingux gnoblin greetd", "bingux-os "),
                    "login account created": (f"grep -c '^{USER}:' /mnt/etc/passwd", "1"),
                    "greetd runs the Bingux greeter": (
                        "grep -c bingux-greeter-session /mnt/etc/greetd/config.toml",
                        "1",
                    ),
                    "greetd is enabled": ("arch-chroot /mnt systemctl is-enabled greetd", "enabled"),
                    "installed system boots to the graphical target": (
                        "arch-chroot /mnt systemctl get-default",
                        "graphical.target",
                    ),
                    "bingux helper target is enabled for sessions": (
                        "ls /mnt/etc/systemd/user/gnoblin-session.target.wants",
                        "bingux.target",
                    ),
                    "installed pacman.conf has the Bingux repository": (
                        "grep -c '^\\[bingux-os\\]' /mnt/etc/pacman.conf",
                        "1",
                    ),
                    "installed mirrorlist names the published repository": (
                        "grep -c '^Server = https' /mnt/etc/pacman.d/bingux-os-mirrorlist",
                        "1",
                    ),
                    "bootloader entry exists": ("ls /mnt/boot/loader/entries", ".conf"),
                    "default wallpaper installed": ("ls /mnt/usr/share/backgrounds/bingux-os", "default.jpg"),
                }
                for label, (command, expected) in inspections.items():
                    output = live.run(command, 120)
                    report.append(f"$ {command}\n{output}\n")
                    checks[label] = expected in output
                live.run("umount -R /mnt")
            live.run("poweroff", 5)
            for _ in range(60):
                if not live.alive():
                    break
                time.sleep(2)
            (out / "install.log").write_text(install_log)
            (out / "serial.log").write_bytes(live.log)

        if checks.get("installer finished successfully"):
            live.stop()
            (work / "installed").mkdir()
            installed = Vm(work / "installed", disk=disk, uefi=True, vars_path=vars_file)
            installed.start()
            greeter = out / "installed-greeter.png"
            checks["installed system shows the greeter"] = installed.wait_screen(
                greeter,
                lambda path: bright_pixels(path, CLOCK_REGION) > CLOCK_PIXELS,
                attempts=120,
            )
            if checks["installed system shows the greeter"]:
                session = out / "installed-session.png"
                checks["installed account signs in to a desktop"] = False
                # Keys typed while the greeter is still settling can be lost, and the
                # greeter then says "Try again": type the password again.
                for _ in range(3):
                    time.sleep(10)
                    installed.type_text(PASSWORD)
                    installed.key("ret")
                    if installed.wait_screen(session, session_painted, attempts=25):
                        checks["installed account signs in to a desktop"] = True
                        break
        (out / "report.txt").write_text("\n".join(report))
        (out / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
    finally:
        for vm in (installed, live):
            if vm is not None:
                vm.stop()
        shutil.rmtree(work, ignore_errors=True)
    for name, ok in checks.items():
        print(("ok   " if ok else "FAIL ") + name)
    print("artifacts:", out)
    return 0 if checks and all(checks.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
