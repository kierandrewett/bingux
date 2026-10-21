"""QEMU harness shared by tests/distro-boot.py and tests/distro-install.py.

One Vm wraps one QEMU process: the HMP monitor (screenshots and key presses),
an optional serial console with a root shell (the live image has one) and the
disk or ISO to boot. Nothing here knows about Bingux; the tests do.
"""

from pathlib import Path
import os
import shutil
import socket
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
OVMF_CODE = Path(os.environ.get("BINGUX_DISTRO_OVMF_CODE", "/usr/share/edk2/ovmf/OVMF_CODE.fd"))
OVMF_VARS = Path(os.environ.get("BINGUX_DISTRO_OVMF_VARS", "/usr/share/edk2/ovmf/OVMF_VARS.fd"))

# QEMU key names for characters that are not their own name.
KEY_NAMES = {" ": "spc", "-": "minus", ".": "dot", "_": "shift-minus", "/": "slash", "@": "shift-2"}


def find_iso(argument=None):
    if argument:
        return Path(argument)
    candidates = sorted((ROOT / "distro/out").glob("bingux-os-*.iso"))
    return candidates[-1] if candidates else None


def region_brightness(image, geometry):
    """Mean brightness (0 to 1) of a crop of a screenshot, or 0 if unreadable."""
    result = subprocess.run(
        ["magick", str(image), "-crop", geometry, "+repage", "-colorspace", "Gray", "-format", "%[fx:mean]", "info:"],
        capture_output=True,
        text=True,
        check=False,
    )
    try:
        return float(result.stdout.strip())
    except ValueError:
        return 0.0


def bright_pixels(image, geometry, threshold="90%"):
    """Number of near-white pixels in a crop; the greeter's clock is the only thing that paints them."""
    result = subprocess.run(
        [
            "magick",
            str(image),
            "-crop",
            geometry,
            "+repage",
            "-colorspace",
            "Gray",
            "-threshold",
            threshold,
            "-format",
            "%[fx:mean*w*h]",
            "info:",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    try:
        return float(result.stdout.strip())
    except ValueError:
        return 0.0


def missing_tools(*tools):
    return [tool for tool in tools if shutil.which(tool) is None]


class Vm:
    def __init__(self, work, *, iso=None, disk=None, uefi=False, memory=None, serial=False, shares=(), vars_path=None):
        self.work = Path(work)
        self.iso = iso
        self.disk = disk
        self.uefi = uefi
        self.memory = memory or os.environ.get("BINGUX_DISTRO_MEMORY", "4G")
        self.serial_enabled = serial
        self.shares = list(shares)
        self.process = None
        self.log = b""
        self._serial = None
        self._monitor = None
        # A second boot of the same disk must see the first boot's boot entry.
        self.vars_path = Path(vars_path) if vars_path else self.work / "OVMF_VARS.fd"

    def start(self):
        args = [
            "qemu-system-x86_64",
            "-m",
            self.memory,
            "-smp",
            "4",
            *(["-enable-kvm"] if os.access("/dev/kvm", os.W_OK) else []),
            "-display",
            "none",
            "-device",
            "virtio-vga",
            "-netdev",
            "user,id=net0",
            "-device",
            "virtio-net-pci,netdev=net0",
            "-monitor",
            f"unix:{self.work / 'monitor.sock'},server,nowait",
        ]
        if self.uefi:
            if not self.vars_path.exists():
                shutil.copy(OVMF_VARS, self.vars_path)
            args += [
                "-machine",
                "q35",
                "-drive",
                f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
                "-drive",
                f"if=pflash,format=raw,file={self.vars_path}",
            ]
        if self.disk:
            args += ["-drive", f"file={self.disk},if=virtio,format=qcow2"]
        for number, directory in enumerate(self.shares):
            args += [
                "-virtfs",
                f"local,path={directory},mount_tag=share{number},security_model=none,id=share{number}",
            ]
        if self.iso:
            kernel = self._extract("arch/boot/x86_64/vmlinuz-linux")
            initrd = self._extract("arch/boot/x86_64/initramfs-linux.img")
            label = subprocess.run(
                ["blkid", "-o", "value", "-s", "LABEL", str(self.iso)], capture_output=True, text=True, check=False
            ).stdout.strip()
            # The kernel command line is ours so the console can be the serial port.
            args += [
                "-cdrom",
                str(self.iso),
                "-kernel",
                str(kernel),
                "-initrd",
                str(initrd),
                "-append",
                f"archisobasedir=arch archisolabel={label} console=ttyS0,115200 console=tty1",
            ]
        if self.serial_enabled:
            args += ["-serial", f"unix:{self.work / 'serial.sock'},server,nowait"]
        self.process = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        if self.serial_enabled:
            self._serial = self._connect("serial.sock")
            self._serial.settimeout(0.5)
        self._monitor = self._connect("monitor.sock")
        self._monitor.settimeout(2)
        self._monitor_read()

    def _extract(self, member):
        subprocess.run(["bsdtar", "-xf", str(self.iso), "-C", str(self.work), member], check=True)
        return self.work / member

    def _connect(self, name):
        path = self.work / name
        for _ in range(150):
            try:
                sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                sock.connect(str(path))
                return sock
            except OSError:
                time.sleep(0.2)
        raise RuntimeError(f"could not connect to {path}")

    def alive(self):
        return self.process is not None and self.process.poll() is None

    def stop(self):
        if self.process is None:
            return
        self.process.terminate()
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.kill()
        for sock in (self._serial, self._monitor):
            if sock is not None:
                sock.close()

    # Monitor: screenshots and keys.

    def _monitor_read(self):
        data = b""
        while True:
            try:
                chunk = self._monitor.recv(4096)
            except TimeoutError:
                break
            if not chunk:
                break
            data += chunk
            if data.rstrip().endswith(b"(qemu)"):
                break
        return data.decode(errors="replace")

    def monitor(self, command):
        self._monitor.sendall((command + "\n").encode())
        return self._monitor_read()

    def key(self, name):
        self.monitor(f"sendkey {name}")

    def type_text(self, text):
        for char in text:
            if char.isupper():
                self.key(f"shift-{char.lower()}")
            else:
                self.key(KEY_NAMES.get(char, char))
            time.sleep(0.05)

    def screenshot(self, path):
        """Write a PNG of the current screen; returns True when it exists."""
        ppm = self.work / "screen.ppm"
        ppm.unlink(missing_ok=True)
        self.monitor(f"screendump {ppm}")
        for _ in range(20):
            if ppm.exists() and ppm.stat().st_size > 0:
                break
            time.sleep(0.25)
        if not ppm.exists():
            return False
        subprocess.run(["magick", str(ppm), str(path)], check=False)
        return Path(path).exists()

    # Serial console: a root shell.

    def _drain(self, seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            try:
                chunk = self._serial.recv(4096)
            except TimeoutError:
                continue
            if not chunk:
                break
            self.log += chunk

    def wait_for(self, needle, timeout):
        deadline = time.monotonic() + timeout
        target = needle.encode()
        while time.monotonic() < deadline:
            if target in self.log:
                return True
            try:
                chunk = self._serial.recv(4096)
            except TimeoutError:
                continue
            if not chunk:
                break
            self.log += chunk
        return target in self.log

    def start_shell(self, timeout=300):
        """Wait for the live image's console and make its root shell quiet.

        The image logs root in on the serial console with zsh and its line
        editor, which echoes every command and redraws the prompt. Switching the
        editor off and the terminal's echo off leaves only command output in the
        capture. The setup is retried until a probe shows no echo: the first
        attempts can land while the login scripts still run and be lost.
        """
        if not (self.wait_for("login:", timeout) or self.wait_for("archiso", 1) or self.wait_for("#", 60)):
            return False
        setup = (
            b"\nunsetopt zle prompt_sp prompt_cr 2>/dev/null; stty -echo; PS1=''; PROMPT=''; RPROMPT=''; "
            b"export PAGER=cat SYSTEMD_PAGER=cat SYSTEMD_COLORS=0\n"
        )
        for _ in range(30):
            self._serial.sendall(setup)
            time.sleep(2)
            self._drain(1)
            mark = len(self.log)
            # The text of this command differs from what it prints, so seeing
            # the printed form without the typed form means nothing echoed.
            self._serial.sendall(b"echo READY_$((40+2))\n")
            self._drain(3)
            seen = self.log[mark:]
            if b"READY_42" in seen and b"$((40+2))" not in seen:
                return True
        return False

    def run(self, command, timeout=30):
        """Run a shell command on the console and return its output."""
        marker = f"__{int(time.time() * 1000) % 1000000}__"
        start = len(self.log)
        self._serial.sendall((command + f"\necho {marker[:3]}''{marker[3:]}\n").encode())
        self.wait_for(marker, timeout)
        text = self.log[start:].decode(errors="replace").replace("\r", "")
        return text.split(marker, 1)[0].strip("\n")

    def run_as(self, user, command, timeout=30):
        """Run a command as a logged-in user with that user's runtime directory."""
        quoted = command.replace("'", "'\\''")
        return self.run(
            f"runuser -u {user} -- env XDG_RUNTIME_DIR=/run/user/$(id -u {user}) sh -c '{quoted}' 2>&1", timeout
        )

    def poll(self, command, accept, attempts=60, delay=3, as_user=None):
        """Run a probe until accept(output) is true or the attempts run out; returns the last output."""
        output = ""
        for _ in range(attempts):
            output = (self.run_as(as_user, command) if as_user else self.run(command)).strip()
            if accept(output):
                return output
            time.sleep(delay)
        return output
