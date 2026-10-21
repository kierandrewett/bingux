#!/usr/bin/env python3
"""Boot the Bingux OS live image in QEMU and check that the login screen is up.

usage: tests/distro-boot.py [ISO]   (default: newest distro/out/bingux-os-*.iso)

The kernel and initramfs are taken from the ISO so the kernel command line
can add a serial console. The live image's serial getty logs root in without
a password; over it the test checks that greetd is active, that the Gnoblin
greeter compositor and bingux-greeter are running, and takes a screenshot
through the QEMU monitor. Artifacts go to BINGUX_DISTRO_OUT (default
distro/out/boot-test/): serial.log, screen.png and checks.json.
"""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def find_iso(argument):
    if argument:
        return Path(argument)
    candidates = sorted((ROOT / "distro/out").glob("bingux-os-*.iso"))
    return candidates[-1] if candidates else None


def extract(iso, member, destination):
    subprocess.run(["bsdtar", "-xf", str(iso), "-C", str(destination), member], check=True)
    return destination / member


def monitor(path, command):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.connect(str(path))
        sock.settimeout(5)
        sock.recv(4096)
        sock.sendall((command + "\n").encode())
        time.sleep(1)
        try:
            sock.recv(4096)
        except TimeoutError:
            pass


class Serial:
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        for _ in range(100):
            try:
                self.sock.connect(str(path))
                break
            except OSError:
                time.sleep(0.2)
        self.sock.settimeout(0.5)
        self.log = b""

    def read(self, deadline):
        while time.monotonic() < deadline:
            try:
                chunk = self.sock.recv(4096)
            except TimeoutError:
                continue
            if not chunk:
                break
            self.log += chunk
            yield self.log

    def wait_for(self, needle, timeout):
        deadline = time.monotonic() + timeout
        for log in self.read(deadline):
            if needle.encode() in log:
                return True
        return needle.encode() in self.log

    def run(self, command, timeout=30):
        marker = f"__DONE_{int(time.time() * 1000)}__"
        start = len(self.log)
        self.sock.sendall((command + f"; echo {marker}\n").encode())
        self.wait_for(marker, timeout)
        return self.log[start:].decode(errors="replace")


def main():
    iso = find_iso(sys.argv[1] if len(sys.argv) > 1 else None)
    if iso is None or not iso.is_file():
        print("no live image; run distro/build.sh first")
        return 1
    for tool in ("qemu-system-x86_64", "bsdtar"):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    out = Path(os.environ.get("BINGUX_DISTRO_OUT", ROOT / "distro/out/boot-test"))
    out.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="bingux-os-boot-"))
    try:
        kernel = extract(iso, "arch/boot/x86_64/vmlinuz-linux", work)
        initrd = extract(iso, "arch/boot/x86_64/initramfs-linux.img", work)
        label = subprocess.run(
            ["blkid", "-o", "value", "-s", "LABEL", str(iso)], capture_output=True, text=True, check=False
        ).stdout.strip()
        serial_path = work / "serial.sock"
        monitor_path = work / "monitor.sock"
        qemu = subprocess.Popen(
            [
                "qemu-system-x86_64",
                "-m",
                os.environ.get("BINGUX_DISTRO_MEMORY", "4G"),
                "-smp",
                "4",
                *(["-enable-kvm"] if os.access("/dev/kvm", os.W_OK) else []),
                "-display",
                "none",
                "-device",
                "virtio-vga",
                "-cdrom",
                str(iso),
                "-kernel",
                str(kernel),
                "-initrd",
                str(initrd),
                "-append",
                f"archisobasedir=arch archisolabel={label} console=ttyS0,115200 console=tty1",
                "-serial",
                f"unix:{serial_path},server,nowait",
                "-monitor",
                f"unix:{monitor_path},server,nowait",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        serial = Serial(serial_path)
        booted = serial.wait_for("login:", 300) or serial.wait_for("archiso", 1)
        checks = {"booted to a shell": False}
        if booted or serial.wait_for("#", 120):
            checks["booted to a shell"] = True
            # The greeter can take a moment after multi-user.target.
            for _ in range(60):
                active = serial.run("systemctl is-active greetd")
                if "\nactive" in active or active.strip().endswith("active"):
                    break
                time.sleep(2)
            report = serial.run(
                "systemctl is-active greetd; pgrep -a -f 'gnoblin --greeter' | head -n 1; "
                "pgrep -a -f bingux-greeter | head -n 3; id live; ls /var/lib/bingux/greeter/live"
            )
            checks["greetd active"] = "\nactive" in report
            checks["gnoblin greeter compositor running"] = "gnoblin --greeter" in report
            checks["bingux-greeter running"] = "bingux-greeter" in report
            checks["live account exists"] = "uid=" in report
            checks["greeter wallpaper snapshot"] = "wallpaper" in report
            time.sleep(5)
            monitor(monitor_path, f"screendump {work / 'screen.ppm'}")
            if (work / "screen.ppm").exists():
                subprocess.run(["magick", str(work / "screen.ppm"), str(out / "screen.png")], check=False)
                checks["screenshot captured"] = (out / "screen.png").exists()
        (out / "serial.log").write_bytes(serial.log)
        (out / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
        qemu.terminate()
        try:
            qemu.wait(timeout=10)
        except subprocess.TimeoutExpired:
            qemu.kill()
        for name, ok in checks.items():
            print(("ok   " if ok else "FAIL ") + name)
        print("artifacts:", out)
        return 0 if checks and all(checks.values()) else 1
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
