"""Support for tests/bsm-boot.py: a VM with a boot tracer, a full-rate frame log, log parsers.

What this adds to tests/bsm_harness.py:

- `BootVm`: a `BsmVm` whose guest also runs tests/bsm-boot-trace.py from the start of
  userspace. The tracer travels as a systemd credential in SMBIOS type 11 (an extra unit
  plus a drop-in that makes sysinit.target want it), so the image is not changed.
  `drm_debug` adds `drm.debug=` and a large kernel log buffer to the command line.
- `FrameLog`: every framebuffer message that changed pixels, with its host time, size,
  class and a 1/8 scale RGB thumbnail. The harness recorder keeps only the newest frame
  and a 10 fps video; the handover needs every frame QEMU's VNC server delivers.
- Parsers for the tracer log, `journalctl -o short-monotonic` and the kernel's DRM
  debug lines, and the arithmetic that puts the guest's monotonic clock on the host's.

No VM is needed for anything but `BootVm`; tests/bsm-boot.test.py runs the rest on
synthetic input.

Staged boot. The 2026.10.27 image predates the boot handover, and unit ordering can only be
tested at a cold boot (a `daemon-reload` after boot does not change a transaction that was
already queued). `staged_credentials` therefore feeds the manager, at its start, the unit
text that the bsm package installs (tests/bsm-vm-inject.py stage runs the PKGBUILD's own
`package()`): `bsm.service` as an extra unit and the `plymouth-quit.service` drop-in as
credentials, plus one early unit that mounts the 9p share and copies the staged binaries
and helper scripts over the live root before bsm-seatd and bsm start. Nothing in the image
changes and real hardware never sees any of it.
"""

from __future__ import annotations

import base64
from pathlib import Path
import time
import zlib

import bsm_harness
import distro_capture
import distro_vm
from bsm_boot_analysis import BootFrame, scene_of
from bsm_harness import BsmVm
from distro_capture import Recorder, classify, luma

TESTS = Path(__file__).resolve().parent
TRACE_SCRIPT = TESTS / "bsm-boot-trace.py"
TRACE_LOG = "/run/bsm-boot-trace.log"
TRACE_UNIT = "bsm-boot-trace.service"
# Core (every ioctl with its process name, so SET_MASTER and DROP_MASTER show who
# called them), KMS and atomic messages (the commits and their modeset enable and
# disable lines).
DRM_DEBUG_MASK = 0x1 | 0x4 | 0x10
INJECT_UNIT = "bsm-boot-inject.service"
# Tracer credentials are small; the staged files travel over 9p.
STAGED_UNITS = ("bsm.service", "bsm-fallback.service")
STAGED_DROPINS = {"plymouth-quit.service": "plymouth-quit.service.d/10-bsm.conf"}
# A healthy QEMU answers the recorder's full-frame request (sent every 0.2 s on a quiet
# connection) at once. The harness recorder's connection has been seen to go silent for the
# rest of a boot (one boot in four, on head 0, on a loaded host); this many silent requests in a
# row make the reader reconnect.
STALL_TIMEOUTS = 25


def _guard_connect(connect):
    if getattr(connect, "_bsm_boot_guard", False):
        return connect

    def guarded(path):
        conn = connect(path)
        raw = conn.recv_exact
        silent = [0]

        def recv_exact(count):
            try:
                data = raw(count)
            except TimeoutError:
                silent[0] += 1
                if silent[0] >= STALL_TIMEOUTS:
                    raise RuntimeError(f"VNC stalled: {silent[0]} full-frame requests unanswered") from None
                raise
            silent[0] = 0
            return data

        conn.recv_exact = recv_exact
        return conn

    guarded._bsm_boot_guard = True
    return guarded


# Every recorder in this process (the harness's own included) reconnects instead of going blind.
distro_capture.connect_vnc = _guard_connect(distro_capture.connect_vnc)


def credential(name, data):
    """An SMBIOS type 11 argument that hands `data` to the guest as the system credential `name`."""
    encoded = base64.b64encode(data).decode()
    return ["-smbios", f"type=11,value=io.systemd.credential.binary:{name}={encoded}"]


def trace_unit(script=TRACE_SCRIPT):
    """Text of the tracer's service unit: the script is compressed into the command line."""
    payload = base64.b64encode(zlib.compress(Path(script).read_bytes(), 9)).decode()
    code = f"import base64,zlib;exec(zlib.decompress(base64.b64decode('{payload}')))"
    return (
        "[Unit]\n"
        "Description=bsm boot tracer (test harness)\n"
        "DefaultDependencies=no\n"
        "[Service]\n"
        "Type=simple\n"
        "Nice=-5\n"
        f"ExecStart=/usr/bin/python3 -c \"{code}\"\n"
    )


def inject_unit():
    """Early unit that copies the staged binaries and scripts from the 9p share over the live root."""
    command = (
        "modprobe 9pnet_virtio; modprobe 9p; mkdir -p /mnt/bsm-inject; "
        "mountpoint -q /mnt/bsm-inject || mount -t 9p -o trans=virtio,version=9p2000.L,ro bsminject /mnt/bsm-inject; "
        "cd /mnt/bsm-inject/root && tar -cf - usr/bin usr/libexec | tar -C / -xpf - --no-same-owner"
    )
    return (
        "[Unit]\n"
        "Description=bsm test harness: copy the staged bsm files over the live root\n"
        "DefaultDependencies=no\n"
        "After=systemd-modules-load.service\n"
        "Before=bsm-seatd.service bsm-helper.socket bsm-helper.service bsm.service sysinit.target\n"
        "[Service]\n"
        "Type=oneshot\n"
        "RemainAfterExit=yes\n"
        f"ExecStart=/bin/sh -c '{command}'\n"
    )


def staged_credentials(stage):
    """(credentials, sysinit_wants) for a cold boot that uses the files of a tests/bsm-vm-inject.py stage."""
    root = Path(stage) / "root" / "usr/lib/systemd/system"
    credentials = [(f"systemd.extra-unit.{INJECT_UNIT}", inject_unit().encode())]
    for unit in STAGED_UNITS:
        credentials.append((f"systemd.extra-unit.{unit}", (root / unit).read_bytes()))
    for unit, relative in STAGED_DROPINS.items():
        credentials.append((f"systemd.unit-dropin.{unit}", (root / relative).read_bytes()))
    return credentials, [INJECT_UNIT]


class FrameLog(Recorder):
    """A harness recorder that also keeps every changed frame as a thumbnail and its scene.

    `frames` holds a `BootFrame` for every framebuffer message that changed pixels; `t` is
    seconds since power-on (like frame-events.jsonl). `keyframes` holds a full resolution copy of
    the first frame of every scene change, for the report's stills.
    """

    THUMB_STEP = 8
    MAX_KEYFRAMES = 80

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.frames: list[BootFrame] = []
        self.keyframes: list[tuple[BootFrame, object]] = []
        self.reconnects = 0

    def _vnc_loop(self):
        # Recorder._vnc_loop ends on any connection error; a stalled one (see _guard_connect)
        # is such an error. Connect again until the recorder is stopped.
        while not self._stop.is_set():
            super()._vnc_loop()
            if self._stop.is_set():
                break
            self.reconnects += 1
            self._note_event(f"vnc reconnect {self.reconnects}", time.monotonic())
            time.sleep(0.2)

    def _after_change(self, framebuffer, when, rects, area, resized):
        rgb = framebuffer[..., [2, 1, 0]]
        height, width = rgb.shape[:2]
        mean = float(luma(rgb[::4, ::4]).mean())
        thumb = rgb[:: self.THUMB_STEP, :: self.THUMB_STEP].copy()
        frame = BootFrame(when - self.t0, (width, height), classify(rgb, self.reference_size), mean, thumb)
        frame.scene = scene_of(frame.size, mean, thumb)
        previous = self.frames[-1].scene if self.frames else None
        self.frames.append(frame)
        if frame.scene != previous and len(self.keyframes) < self.MAX_KEYFRAMES:
            self.keyframes.append((frame, rgb.copy()))
        super()._after_change(framebuffer, when, rects, area, resized)


class BootVm(BsmVm):
    """BsmVm with the boot tracer, optional DRM debug logging and a full-rate frame log."""

    def __init__(self, work, *, drm_debug=False, trace=True, credentials=(), sysinit_wants=(), stage=None, **kwargs):
        extra = kwargs.pop("cmdline_extra", "")
        if drm_debug:
            extra = f"{extra} drm.debug={DRM_DEBUG_MASK:#x} log_buf_len=128M".strip()
        credentials, sysinit_wants = list(credentials), list(sysinit_wants)
        if stage is not None:
            more, wants = staged_credentials(stage)
            credentials += more
            sysinit_wants += wants
            kwargs["inject"] = stage
        super().__init__(work, cmdline_extra=extra, **kwargs)
        self.trace = trace
        self.credentials = credentials
        self.sysinit_wants = sysinit_wants
        self.stage = stage

    def display_args(self):
        args = super().display_args()
        wants = list(self.sysinit_wants)
        if self.trace:
            args += credential(f"systemd.extra-unit.{TRACE_UNIT}", trace_unit().encode())
            wants.append(TRACE_UNIT)
        if wants:
            args += credential("systemd.unit-dropin.sysinit.target", f"[Unit]\nWants={' '.join(wants)}\n".encode())
        for name, data in self.credentials:
            args += credential(name, data)
        return args

    def start(self):
        # The harness builds its recorders inside start(); swap in the frame log for that call.
        originals = (distro_vm.Recorder, bsm_harness.Recorder)
        distro_vm.Recorder = bsm_harness.Recorder = FrameLog
        try:
            super().start()
        finally:
            distro_vm.Recorder, bsm_harness.Recorder = originals

    def frame_logs(self):
        """head -> FrameLog"""
        logs = {0: self.recorder}
        logs.update(self.head_recorders)
        return logs

    # Clocks.

    def mono_offset(self, samples=12):
        """Seconds to add to a guest CLOCK_MONOTONIC reading to get host seconds since power-on.

        Reads the guest's /proc/uptime over the serial shell and keeps the sample with the
        shortest round trip, so the error is about half that round trip plus the 10 ms of
        /proc/uptime's resolution. Returns (offset, uncertainty).
        """
        best = None
        for _ in range(samples):
            before = time.monotonic()
            out = self.run("cat /proc/uptime", 10).strip().split()
            after = time.monotonic()
            if not out:
                continue
            try:
                uptime = float(out[0])
            except ValueError:
                continue
            rtt = after - before
            if best is None or rtt < best[2]:
                best = ((before + after) / 2 - self.t0 - uptime, uptime, rtt)
        if best is None:
            raise RuntimeError("no /proc/uptime reading")
        return best[0], best[2] / 2 + 0.01

