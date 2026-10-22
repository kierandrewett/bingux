"""QEMU harness shared by tests/distro-boot.py and tests/distro-install.py.

One Vm wraps one QEMU process and boots it the way a machine does: the ISO
goes on as `-cdrom` and the guest's own boot loader runs, with OVMF for UEFI
(the default) or SeaBIOS with `uefi=False`. Nothing is extracted from the ISO
and no kernel command line is written, so the menu, initramfs and splash all
show on the screen.

Sockets under the VM's work directory:

- monitor.sock: HMP, for key presses.
- vnc.sock: VNC, the only frame source for distro_capture.Recorder. QMP
  screendump is not used: it fails with "no surface" under virtio-vga-gl.
- serial.sock: the serial console, when `serial=True`.

Serial console without `console=ttyS0`: the live image's serial-getty autologin
is wired by a unit drop-in that `multi-user.target` wants. The harness passes
that drop-in as a systemd credential through SMBIOS type 11 on its own QEMU
command line. The image is unchanged and real hardware never sees it.

Screens are read from the recorder's newest frame (numpy RGB) and never
through a file on the disk. Nothing here knows about Bingux; the tests do.
"""

import base64
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time

from distro_capture import Recorder

ROOT = Path(__file__).resolve().parent.parent
OVMF_CODE = Path(os.environ.get("BINGUX_DISTRO_OVMF_CODE", "/usr/share/edk2/ovmf/OVMF_CODE.fd"))
OVMF_VARS = Path(os.environ.get("BINGUX_DISTRO_OVMF_VARS", "/usr/share/edk2/ovmf/OVMF_VARS.fd"))
RENDER_NODE = Path(os.environ.get("BINGUX_DISTRO_RENDER_NODE", "/dev/dri/renderD128"))

# QEMU key names for characters that are not their own name.
KEY_NAMES = {" ": "spc", "-": "minus", ".": "dot", "_": "shift-minus", "/": "slash", "@": "shift-2"}

# Makes multi-user.target want the serial getty on ttyS0; the image's own
# autologin drop-in for that getty then gives the root shell.
SERIAL_GETTY_DROPIN = "[Unit]\nWants=serial-getty@ttyS0.service\n"


def find_iso(argument=None):
    if argument:
        return Path(argument)
    candidates = sorted((ROOT / "distro/out").glob("bingux-os-*.iso"))
    return candidates[-1] if candidates else None


def missing_tools(*tools):
    return [tool for tool in tools if shutil.which(tool) is None]


def display_args():
    """GPU and display for the guest.

    `egl-headless` with virtio-vga-gl gives the guest a virgl 3D device, so the
    compositor's blur and motion run on the GPU path as they do on hardware.
    It needs the host render node; without one, or with
    BINGUX_DISTRO_DISPLAY=sw, the guest gets the software VGA device.
    """
    if os.environ.get("BINGUX_DISTRO_DISPLAY", "gl") == "gl" and os.access(RENDER_NODE, os.R_OK | os.W_OK):
        return ["-device", "virtio-vga-gl", "-display", f"egl-headless,rendernode={RENDER_NODE}"]
    return ["-device", "virtio-vga", "-display", "none"]


class Vm:
    def __init__(self, work, *, iso=None, disk=None, uefi=True, memory=None, serial=False, vars_path=None, record=None):
        """`record`: directory for flow.mp4, timeline.json and frame-events.jsonl.

        Without it the recording goes to the work directory, which the caller removes.
        """
        self.work = Path(work)
        self.work.mkdir(parents=True, exist_ok=True)
        self.iso = iso
        self.disk = disk
        self.uefi = uefi
        self.memory = memory or os.environ.get("BINGUX_DISTRO_MEMORY", "4G")
        self.serial_enabled = serial
        self.record_dir = Path(record) if record else self.work / "record"
        # A second boot of the same disk must see the first boot's boot entry.
        self.vars_path = Path(vars_path) if vars_path else self.work / "OVMF_VARS.fd"
        self.recorder = None
        self.stopped = False
        self.process = None
        self.t0 = None
        self.log = b""
        self._serial = None
        self._monitor = None

    def start(self):
        args = [
            "qemu-system-x86_64",
            "-machine",
            "q35",
            "-m",
            self.memory,
            "-smp",
            "4",
            *(["-enable-kvm"] if os.access("/dev/kvm", os.W_OK) else []),
            *display_args(),
            "-vnc",
            f"unix:{self.work / 'vnc.sock'}",
            "-monitor",
            f"unix:{self.work / 'monitor.sock'},server,nowait",
            "-netdev",
            "user,id=net0",
            "-device",
            "virtio-net-pci,netdev=net0",
        ]
        if self.uefi:
            if not self.vars_path.exists():
                shutil.copy(OVMF_VARS, self.vars_path)
            args += [
                "-drive",
                f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
                "-drive",
                f"if=pflash,format=raw,file={self.vars_path}",
            ]
        if self.disk:
            args += ["-drive", f"file={self.disk},if=virtio,format=qcow2"]
        if self.iso:
            args += ["-cdrom", str(self.iso)]
        if self.serial_enabled:
            credential = base64.b64encode(SERIAL_GETTY_DROPIN.encode()).decode()
            args += [
                "-smbios",
                f"type=11,value=io.systemd.credential.binary:systemd.unit-dropin.multi-user.target={credential}",
                "-serial",
                f"unix:{self.work / 'serial.sock'},server,nowait",
            ]
        self.t0 = time.monotonic()
        self.process = subprocess.Popen(
            args, stdout=subprocess.DEVNULL, stderr=(self.work / "qemu.log").open("w"), text=True
        )
        if self.serial_enabled:
            self._serial = self._connect("serial.sock")
            self._serial.settimeout(0.5)
        self._monitor = self._connect("monitor.sock")
        self._monitor.settimeout(2)
        self._monitor_read()
        self.recorder = Recorder(self.record_dir, self.work / "vnc.sock")
        self.recorder.start(self.t0)
        self.recorder.mark("power on")

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
        if self.process is None or self.stopped:
            return
        self.stopped = True
        if self.recorder is not None:
            self.recorder.stop()
        self.process.terminate()
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.kill()
        for sock in (self._serial, self._monitor):
            if sock is not None:
                sock.close()

    # Monitor: key presses.

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

    # Screen.

    def frame(self):
        """The newest captured frame (H x W x 3 uint8 numpy array), or None before the first sample."""
        return self.recorder.frame()

    def still(self, path, frame=None, timeout=5):
        """Save `frame` (default: the newest captured frame) as a PNG and return it, or None when none arrives in time."""
        from PIL import Image

        deadline = time.monotonic() + timeout
        while frame is None and time.monotonic() < deadline:
            frame = self.frame()
            if frame is None:
                time.sleep(0.1)
        if frame is not None:
            Image.fromarray(frame).save(path)
        return frame

    def wait_frame(self, accept, attempts=60, delay=3):
        """Poll frames until accept(frame) holds; returns that frame, or None when the VM ended or time ran out."""
        for _ in range(attempts):
            if not self.alive():
                return None
            frame = self.frame()
            if frame is not None and accept(frame):
                return frame
            time.sleep(delay)
        return None

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
        self.recorder.mark("serial shell")
        setup = (
            b"\nunsetopt zle prompt_sp prompt_cr 2>/dev/null; stty -echo; PS1=''; PROMPT=''; RPROMPT=''; "
            b"export PAGER=cat SYSTEMD_PAGER=cat SYSTEMD_COLORS=0\n"
        )
        for _ in range(30):
            self._serial.sendall(setup)
            time.sleep(2)
            self._drain(1)
            mark = len(self.log)
            # The typed text is not what it prints, so the printed form alone proves no echo.
            self._serial.sendall(b"echo READY_$((40+2))\n")
            self._drain(3)
            seen = self.log[mark:]
            if b"READY_42" in seen and b"$((40+2))" not in seen:
                return True
        return False

    def run(self, command, timeout=30):
        """Run a shell command on the console and return its output."""
        self._runs = getattr(self, "_runs", 0) + 1
        marker = f"BXDONE{self._runs}"
        start = len(self.log)
        # `BX''DONE` is typed but prints as the marker, so the marker only shows in the output.
        self._serial.sendall((command + f"\necho BX''DONE{self._runs}\n").encode())
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
