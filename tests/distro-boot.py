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
        """Run a shell command over the console and return its output only.

        The console echoes the command line, so the marker is split in the
        command (`__DO''NE_...`) and only the shell's own echo of the finished
        command prints it whole; the text before that echo line is dropped.
        """
        marker = f"__{int(time.time() * 1000) % 100000}__"
        start = len(self.log)
        self.sock.sendall((command + f"; echo {marker[:3]}''{marker[3:]}\n").encode())
        self.wait_for(marker, timeout)
        text = self.log[start:].decode(errors="replace").split(marker, 1)[0]
        # The line editor echoes the command as it is typed and once more when
        # it redraws the prompt; the output follows the last echo.
        echoed = text.rfind(f"{marker[:3]}''{marker[3:]}")
        if echoed >= 0:
            text = text[text.find("\n", echoed) + 1 :]
        return text


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
                if serial.run("systemctl is-active greetd").strip() == "active":
                    break
                time.sleep(2)
            # The greeter client follows the compositor; on a software-rendered
            # VM that can take a while after greetd reports active.
            for _ in range(45):
                if "GREETER_UP" in serial.run("pgrep -f bingux-greeter >/dev/null && echo GREETER_UP"):
                    break
                time.sleep(2)
            # One short command per probe: the console wraps long lines, which
            # would put echoed fragments into the captured output.
            probes = {
                "greetd active": "systemctl is-active greetd",
                "gnoblin greeter compositor running": "pgrep -c -f 'gnoblin --greeter'",
                "bingux-greeter running": "pgrep -c -f bingux-greeter",
                "live account exists": "id -u live",
                "greeter wallpaper snapshot": "ls /var/lib/bingux/greeter/live",
            }
            report = ""
            for label, probe in probes.items():
                output = serial.run(probe).strip()
                report += f"$ {probe}\n{output}\n"
                if label == "greetd active":
                    checks[label] = output == "active"
                elif label == "greeter wallpaper snapshot":
                    checks[label] = "wallpaper" in output
                elif label == "live account exists":
                    checks[label] = output.isdigit()
                else:
                    checks[label] = output.isdigit() and int(output) > 0
            # Diagnostics for a failed boot, next to the screenshot.
            report += "$ journalctl -b -u greetd\n" + serial.run(
                "journalctl -b -u greetd --no-pager -o cat | tail -n 40", 60
            )
            report += "$ journalctl -b -p err\n" + serial.run("journalctl -b -p err --no-pager -o cat | tail -n 40", 60)
            report += "$ journalctl -b (greeter session)\n" + serial.run(
                "journalctl -b --no-pager -o short-iso _UID=$(id -u greeter) | tail -n 80", 60
            )
            (out / "report.txt").write_text(report)
            # Give the greeter time to paint its first frame.
            time.sleep(10)
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
