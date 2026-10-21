"""QEMU driver for the Bingux live ISO: serial root shell, QMP screendumps, logs.

The guest is not modified. The serial getty is started with an SMBIOS type 11
unit drop-in (the same mechanism as tests/distro_vm.py), and the image's own
autologin drop-in then gives a root shell on ttyS0.
"""

GETTY_DROPIN = "[Unit]\nWants=serial-getty@ttyS0.service\n"

import base64
import json
import os
import re
import shutil
import socket
import struct
import subprocess
import tempfile
import time
import uuid
import zlib
from pathlib import Path

GETTY_DROPIN = "[Unit]\nWants=serial-getty@ttyS0.service\n"
ANSI = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[()][0-9A-Za-z]|\x1b[=>78]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
OVMF_PATHS = {
    "code": ("/usr/share/edk2/ovmf/OVMF_CODE.fd", "/usr/share/OVMF/OVMF_CODE.fd"),
    "vars": ("/usr/share/edk2/ovmf/OVMF_VARS.fd", "/usr/share/OVMF/OVMF_VARS.fd"),
}


class CommandFailed(RuntimeError):
    pass


def ovmf(kind):
    """OVMF `code` or `vars` image: BINGUX_OVMF_CODE / BINGUX_OVMF_VARS, else the usual system paths."""
    override = os.environ.get(f"BINGUX_OVMF_{kind.upper()}")
    if override:
        return Path(override)
    for candidate in OVMF_PATHS[kind]:
        if Path(candidate).exists():
            return Path(candidate)
    raise FileNotFoundError(f"OVMF {kind} image not found; set BINGUX_OVMF_{kind.upper()}")


def make_disk(path, size_bytes, fmt="raw"):
    subprocess.run(["qemu-img", "create", "-f", fmt, str(path), str(size_bytes)], check=True, stdout=subprocess.DEVNULL)
    return Path(path)


def write_png(path, width, height, rgb):
    """Write 8-bit RGB pixels (`rgb`, row-major, 3 bytes per pixel) as a PNG."""

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    stride = width * 3
    raw = b"".join(b"\x00" + rgb[y * stride : (y + 1) * stride] for y in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    Path(path).write_bytes(
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
    )


def read_ppm(path):
    """Parse a binary P6 PPM (QEMU's screendump format) into (width, height, rgb bytes)."""
    data = Path(path).read_bytes()
    match = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", data)
    if not match or int(match.group(3)) != 255:
        raise ValueError(f"{path}: not an 8-bit binary PPM")
    width, height = int(match.group(1)), int(match.group(2))
    return width, height, data[match.end() : match.end() + width * height * 3]


class LiveVm:
    """One QEMU guest. Logs go to `artifacts`; sockets and scratch files go to `scratch` (under /tmp)."""

    def __init__(self, artifacts, scratch=None):
        self.artifacts = Path(artifacts)
        self.artifacts.mkdir(parents=True, exist_ok=True)
        self.scratch = Path(scratch) if scratch else Path(tempfile.mkdtemp(prefix="vmdrive-", dir="/tmp"))
        self.scratch.mkdir(parents=True, exist_ok=True)
        self._transcript = (self.artifacts / "transcript.log").open("w", encoding="utf-8")
        self._commands = (self.artifacts / "commands.jsonl").open("w", encoding="utf-8")
        self.proc = self.serial = self.qmp = None
        self._serial_log = bytearray()
        self._qmp_buf = b""
        self._runs = 0

    # Lifecycle.

    def start(self, iso=None, disks=(), firmware="uefi", ram_mb=4096, cpus=4, cdrom=None,
              extra_disks_format="qcow2", name="bingux-live"):
        """Boot the guest. `iso` is the boot medium (-cdrom); `cdrom` is an extra read-only data CD.

        `disks` is a list of (path, size_bytes); a missing disk is created with qemu-img in
        `extra_disks_format`, an existing one is attached as it is. Each disk is a virtio-blk
        device (/dev/vda, /dev/vdb, ...).
        """
        if firmware not in ("uefi", "bios"):
            raise ValueError(f"firmware must be 'uefi' or 'bios', not {firmware!r}")
        if self.alive():
            raise RuntimeError("guest already running")
        serial_sock, qmp_sock = self.scratch / "serial.sock", self.scratch / "qmp.sock"
        args = [
            "qemu-system-x86_64", "-name", name, "-machine", "q35" if firmware == "uefi" else "pc",
            "-m", str(ram_mb), "-smp", str(cpus), "-vga", "std", "-display", "none", "-no-reboot",
        ]
        if os.access("/dev/kvm", os.W_OK):
            args.append("-enable-kvm")
        if firmware == "uefi":
            vars_copy = self.scratch / "OVMF_VARS.fd"
            shutil.copyfile(ovmf("vars"), vars_copy)  # fresh NVRAM for every boot
            args += ["-drive", f"if=pflash,format=raw,readonly=on,file={ovmf('code')}",
                     "-drive", f"if=pflash,format=raw,file={vars_copy}"]
        if iso:
            args += ["-cdrom", str(iso), "-boot", "d"]
        for index, (path, size) in enumerate(disks):
            if not Path(path).exists():
                make_disk(path, size, extra_disks_format)
            args += ["-drive", f"file={path},format={extra_disks_format},if=none,id=disk{index}",
                     "-device", f"virtio-blk-pci,drive=disk{index},serial=disk{index}"]
        if cdrom:
            args += ["-device", "virtio-scsi-pci,id=scsi0",
                     "-drive", f"file={cdrom},format=raw,media=cdrom,readonly=on,if=none,id=cd1",
                     "-device", "scsi-cd,bus=scsi0.0,drive=cd1"]
        credential = base64.b64encode(GETTY_DROPIN.encode()).decode()
        args += [
            "-smbios", f"type=11,value=io.systemd.credential.binary:systemd.unit-dropin.multi-user.target={credential}",
            "-serial", f"unix:{serial_sock},server,nowait",
            "-qmp", f"unix:{qmp_sock},server,nowait",
            "-netdev", "user,id=net0", "-device", "virtio-net-pci,netdev=net0",
        ]
        self.event(f"power on: {firmware} iso={iso} cdrom={cdrom} disks={[str(p) for p, _ in disks]}")
        self.proc = subprocess.Popen(args, stdout=subprocess.DEVNULL,
                                     stderr=(self.scratch / "qemu.log").open("w"))
        self.serial = self._connect(serial_sock)
        self.qmp = self._connect(qmp_sock)
        self.qmp.settimeout(30)
        self._qmp("qmp_capabilities")

    def alive(self):
        return self.proc is not None and self.proc.poll() is None

    def wait_for_shell(self, timeout=300, boot_delay=30):
        """Wait for the autologin root shell, then turn off the line editor and tty echo.

        Returns True once a probe prints its result without echoing the typed command.
        Keystrokes sent before the getty is up are lost, so the sentinel is resent every 3 s.
        """
        # Keys typed into the bootloader menu pick entries (a newline boots or powers off), so stay silent
        # until its 15 second auto-boot has run.
        self._pump_for(boot_delay)
        mark = len(self._serial_log)
        ready = b"BINGUX-READY-2"
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline and not self._seen(ready, mark):
            self._send(b"echo BINGUX-READY-$((1+1))\n")
            self._pump_for(3, until=lambda: self._seen(ready, mark))
        if not self._seen(ready, mark):
            self.event("shell not reached")
            return False
        setup = (b"unsetopt zle prompt_sp prompt_cr 2>/dev/null; stty -echo; PS1=''; PROMPT=''; RPROMPT=''; "
                 b"export PAGER=cat SYSTEMD_PAGER=cat SYSTEMD_COLORS=0\n")
        for _ in range(10):
            self._send(setup)
            self._pump_for(1)
            mark = len(self._serial_log)
            # The typed text prints as 42 only once the shell evaluates it; the typed form must not show.
            self._send(b"echo BINGUX-PROBE-$((40+2))\n")
            self._pump_for(2)
            seen = self._serial_log[mark:].decode("utf-8", "replace")
            if "BINGUX-PROBE-42" in seen and "$((40+2))" not in seen:
                self.event("shell ready (echo off)")
                return True
        self.event("shell up but echo could not be turned off")
        return False

    def run(self, command, timeout=600, check=False, on_line=None):
        """Run `command` in the guest shell. Returns (rc, output); rc is None on timeout.

        The command is followed by `__rc=$?; echo; echo __RC_<nonce>_$__rc__`, so the exit code and
        the end of output are unambiguous. `on_line(line)` is called for each output line as it arrives.
        """
        self._runs += 1
        nonce = uuid.uuid4().hex[:12]
        marker_re = re.compile(rf"__RC_{nonce}_(\d+)__")
        echo_token = f"__RC_{nonce}_${{__rc}}__"
        started_iso = time.strftime("%Y-%m-%dT%H:%M:%S")
        start = len(self._serial_log)
        t0 = time.monotonic()
        self.event(f"run {self._runs}: {command}")
        self._send(f"{command}\n__rc=$?; echo; echo {echo_token}\n".encode())
        emitted = 0
        rc, output, timed_out = None, "", False
        while True:
            text = self._text_since(start, echo_token)
            found = marker_re.search(text)
            if found:
                rc = int(found.group(1))
                output = text[: found.start()].lstrip("\n")  # the prompt after the last marker leaves a stray newline
                output = output[:-1] if output.endswith("\n") else output
                break
            if time.monotonic() - t0 > timeout:
                timed_out = True
                self._send(b"\x03")  # interrupt the runaway command
                output = text
                break
            if on_line:
                lines = text.lstrip("\n").split("\n")
                for line in lines[emitted:-1]:
                    on_line(line)
                emitted = max(emitted, len(lines) - 1)
            self._pump(0.2)
        if on_line:
            for line in output.split("\n")[emitted:]:
                on_line(line)
        seconds = time.monotonic() - t0
        self.event(f"run {self._runs} done: rc={rc} seconds={seconds:.2f}" + (" TIMEOUT" if timed_out else ""))
        self._commands.write(json.dumps({
            "n": self._runs, "started": started_iso, "cmd": command, "rc": rc,
            "seconds": round(seconds, 3), "timed_out": timed_out, "output": output,
        }) + "\n")
        self._commands.flush()
        if check and rc != 0:
            raise CommandFailed(f"{command!r} exited with rc={rc}")
        return rc, output

    def send(self, text):
        """Send raw text to the serial console. No newline is added."""
        self._send(text.encode())

    def screenshot(self, path):
        """Save the guest screen to `path` as PNG (QMP screendump to a PPM in scratch, converted, PPM removed)."""
        ppm = self.scratch / "screen.ppm"
        self._qmp("screendump", filename=str(ppm))
        deadline = time.monotonic() + 10
        while not ppm.exists() and time.monotonic() < deadline:
            time.sleep(0.1)
        width, height, rgb = read_ppm(ppm)
        write_png(path, width, height, rgb)
        ppm.unlink()
        self.event(f"screenshot {path} ({width}x{height})")
        return Path(path)

    def poweroff(self, timeout=120):
        """ACPI power button through QMP; kill the process if it has not exited within `timeout`."""
        if self.alive():
            try:
                self._qmp("system_powerdown")
            except (OSError, RuntimeError) as error:
                self.event(f"system_powerdown failed: {error}")
            deadline = time.monotonic() + timeout
            while self.alive() and time.monotonic() < deadline:
                try:
                    self._pump(0.5)
                except EOFError:
                    break
        self.kill()

    def kill(self):
        if self.proc is not None and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=10)
        self.event(f"guest stopped rc={self.proc.returncode if self.proc else None}")
        for sock in (self.serial, self.qmp):
            if sock is not None:
                sock.close()
        self.serial = self.qmp = None

    def close(self):
        """Stop the guest, close the logs and remove the scratch directory."""
        self.kill()
        for name in ("transcript", "commands"):
            handle = getattr(self, f"_{name}")
            if not handle.closed:
                handle.close()
        qemu_log = self.scratch / "qemu.log"
        if qemu_log.exists() and qemu_log.stat().st_size:
            shutil.copyfile(qemu_log, self.artifacts / "qemu.log")
        shutil.rmtree(self.scratch, ignore_errors=True)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        if self.alive():
            self.poweroff()
        self.close()

    # Internals.

    def event(self, message):
        self._transcript.write(f"## {time.strftime('%H:%M:%S')} {message}\n")
        self._transcript.flush()

    def _connect(self, path, timeout=60):
        deadline = time.monotonic() + timeout
        while True:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            try:
                sock.connect(str(path))
                sock.settimeout(0.2)
                return sock
            except OSError:
                sock.close()
                if not self.alive() or time.monotonic() > deadline:
                    raise RuntimeError(f"could not connect to {path}; see {self.scratch / 'qemu.log'}")
                time.sleep(0.1)

    def _send(self, data):
        self.serial.sendall(data)

    def _pump(self, seconds):
        """Read serial output for up to `seconds`. EOFError when the guest closed the console."""
        self.serial.settimeout(seconds)
        try:
            chunk = self.serial.recv(65536)
        except socket.timeout:
            return 0
        if not chunk:
            raise EOFError("serial console closed")
        self._serial_log += chunk
        self._transcript.write(ANSI.sub("", chunk.decode("utf-8", "replace")).replace("\r", ""))
        self._transcript.flush()
        return len(chunk)

    def _pump_for(self, seconds, until=None):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if until is not None and until():
                return
            self._pump(0.2)

    def _seen(self, needle, mark):
        return needle in bytes(self._serial_log[mark:])

    def _text_since(self, start, echo_token):
        """Output since `start` as text, ANSI-free, with the echoed `run` line removed."""
        text = ANSI.sub("", self._serial_log[start:].decode("utf-8", "replace")).replace("\r", "")
        at = text.find(echo_token)
        if at >= 0:
            end = text.find("\n", at)
            text = "" if end < 0 else text[end + 1 :]
        return text

    def _qmp(self, execute, **arguments):
        message = {"execute": execute}
        if arguments:
            message["arguments"] = arguments
        self.qmp.sendall(json.dumps(message).encode() + b"\n")
        while True:
            reply = self._qmp_line()
            if "error" in reply:
                raise RuntimeError(f"QMP {execute}: {reply['error']}")
            if "return" in reply:
                return reply["return"]
            # Greeting and events are skipped.

    def _qmp_line(self):
        while b"\n" not in self._qmp_buf:
            chunk = self.qmp.recv(65536)
            if not chunk:
                raise OSError("QMP socket closed")
            self._qmp_buf += chunk
        line, _, self._qmp_buf = self._qmp_buf.partition(b"\n")
        return json.loads(line)
