"""QEMU harness for the bsm tests: a two-display VM, frame sampling, guest helpers.

Built on tests/distro_vm.py (the Vm class: serial root shell, monitor keys, VNC
recorder) and tests/distro_capture.py. What this adds:

- `BsmVm`: the guest gets one virtio-gpu with two heads (`max_outputs=2`), so the
  guest's DRM device has two connectors and bsm drives two monitors. Each head
  has its own VNC server (QEMU `-vnc ...,display=gpu0,head=N`) and its own
  Recorder. The kernel command line gets `bingux.bsm` through SMBIOS type 11
  (`io.systemd.boot.kernel-cmdline-extra`, read by systemd-boot), so the image's
  own boot entry is used and nothing is extracted from the ISO. Optionally a
  9p share (`-virtfs`, tag `bsminject`) carries tests/bsm-vm-inject.py's tree.
- `Sampler`: a thread that reads the newest frame of every head about 40 times a
  second and keeps decimated luma planes with host timestamps. The video slots
  run at 10 fps; a 420 ms fade needs more samples than that.
- Guest helpers: `bsmctl_json`, process listing and ancestry, the monitor tap
  (`bsmctl monitor --json` with a guest timestamp on every line), clock offset.

Nothing here knows what the scenario expects; tests/bsm-qemu.py does.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import struct
import threading
import time

import numpy as np

import distro_vm
from distro_capture import Recorder, connect_vnc
from distro_vm import Vm, RENDER_NODE
from bsm_frames import luma_plane

HEAD_COUNT = 2
LOCK_PATH = os.environ.get("BSM_VM_LOCK", "/tmp/bsm-vm.lock")


class VmLock:
    """Global lock: one bsm VM at a time on this host (`flock /tmp/bsm-vm.lock` is the same lock).

    Parallel guests starve each other and time out at start-up. Held for the VM's whole life
    and released by `release()` or process exit. Records the load average at both ends.
    """

    def __init__(self):
        import fcntl

        self._fcntl = fcntl
        self.handle = open(LOCK_PATH, "a+")
        self.load_before = None
        self.load_after = None

    def acquire(self):
        started = time.monotonic()
        self._fcntl.flock(self.handle, self._fcntl.LOCK_EX)
        self.waited = time.monotonic() - started
        self.load_before = os.getloadavg()
        return self

    def release(self):
        if self.handle is not None:
            self.load_after = os.getloadavg()
            self._fcntl.flock(self.handle, self._fcntl.LOCK_UN)
            self.handle.close()
            self.handle = None

MONITOR_LOG = "/run/bsm-test/monitor.log"


def enable_head(vnc_path, width, height):
    """Switch on a virtio-gpu head by asking its VNC server for a desktop size.

    virtio-gpu reports a head beyond the first as `disconnected` until the display
    side hands it a size (`dpy_set_ui_info` with a non-zero width and height);
    the VNC server does that when a client sends SetDesktopSize after announcing
    the ExtendedDesktopSize encoding. Returns the open connection: closing it
    does not undo the size, but keeping it open costs nothing.
    """
    conn = connect_vnc(vnc_path)
    # SetEncodings: ExtendedDesktopSize (-308) and DesktopSize (-223).
    conn.send(struct.pack(">BxHii", 2, 2, -308, -223))
    # SetDesktopSize: one screen with id 0 covering the whole desktop.
    conn.send(struct.pack(">BxHHBx", 251, width, height, 1) + struct.pack(">IHHHHI", 0, 0, 0, width, height, 0))
    return conn


class Sample:
    __slots__ = ("t", "head", "plane")

    def __init__(self, t, head, plane):
        self.t = t
        self.head = head
        self.plane = plane


class Sampler:
    """Decimated luma planes of every head, with host monotonic timestamps."""

    def __init__(self, sources, interval=0.025, keep=40000):
        self.sources = sources  # head -> callable returning the newest frame or None
        self.interval = interval
        self.keep = keep
        self.samples = []
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._thread = None

    def start(self):
        self._thread = threading.Thread(target=self._loop, name="bsm-sampler", daemon=True)
        self._thread.start()

    def stop(self):
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=5)

    def _loop(self):
        last = {}
        while not self._stop.is_set():
            started = time.monotonic()
            for head, source in self.sources.items():
                try:
                    frame = source()
                except Exception:  # noqa: BLE001 - a recorder that is shutting down
                    frame = None
                if frame is None:
                    continue
                plane = luma_plane(frame)
                previous = last.get(head)
                if previous is not None and previous.shape == plane.shape and np.array_equal(previous, plane):
                    # Unchanged picture: keep one sample per 0.25 s so a static scene is still timestamped.
                    with self._lock:
                        tail = self._latest(head)
                    if tail is not None and started - tail.t < 0.25:
                        continue
                last[head] = plane
                with self._lock:
                    self.samples.append(Sample(started, head, plane))
                    if len(self.samples) > self.keep:
                        del self.samples[: len(self.samples) - self.keep]
            delay = self.interval - (time.monotonic() - started)
            if delay > 0:
                time.sleep(delay)

    def _latest(self, head):
        for sample in reversed(self.samples):
            if sample.head == head:
                return sample
        return None

    def window(self, head, start, end):
        with self._lock:
            return [s for s in self.samples if s.head == head and start <= s.t <= end]

    def latest_plane(self, head):
        with self._lock:
            sample = self._latest(head)
        return None if sample is None else sample.plane


class BsmVm(Vm):
    """A Vm with two display heads, `bingux.bsm` on the kernel command line and an optional 9p share."""

    def __init__(self, work, *, iso, record, inject=None, bsm_param=True, cmdline_extra="", **kwargs):
        super().__init__(work, iso=iso, serial=True, record=record, pointer=True, **kwargs)
        self.inject = Path(inject) if inject else None
        self.bsm_param = bsm_param
        self.cmdline_extra = cmdline_extra
        self.head_recorders = {}
        self.head_connections = []
        self.sampler = None
        self.shutdown_seen = None

    def head_sources(self):
        sources = {0: lambda: self.recorder.frame()}
        for head, recorder in self.head_recorders.items():
            sources[head] = recorder.frame
        return sources

    def display_args(self):
        args = [
            "-device",
            f"virtio-vga-gl,id=gpu0,max_outputs={HEAD_COUNT},xres=1280,yres=800",
            "-display",
            f"egl-headless,rendernode={RENDER_NODE}",
        ]
        if not os.access(RENDER_NODE, os.R_OK | os.W_OK) or os.environ.get("BINGUX_DISTRO_DISPLAY") == "sw":
            args = ["-device", f"virtio-vga,id=gpu0,max_outputs={HEAD_COUNT}", "-display", "none"]
        for head in range(1, HEAD_COUNT):
            args += ["-vnc", f"unix:{self.work / f'vnc{head}.sock'},display=gpu0,head={head}"]
        extra = " ".join(part for part in ("bingux.bsm" if self.bsm_param else "", "cow_spacesize=2G", self.cmdline_extra) if part)
        args += ["-smbios", f"type=11,value=io.systemd.boot.kernel-cmdline-extra={extra}"]
        if self.inject is not None:
            args += [
                "-virtfs",
                f"local,path={self.inject},mount_tag=bsminject,security_model=none,readonly=on,id=bsminject",
            ]
        return args

    def start(self):
        original = distro_vm.display_args
        distro_vm.display_args = self.display_args
        try:
            super().start()
        finally:
            distro_vm.display_args = original
        for head in range(1, HEAD_COUNT):
            recorder = Recorder(self.record_dir / f"head{head}", self.work / f"vnc{head}.sock")
            recorder.start(self.t0)
            self.head_recorders[head] = recorder
            self.head_connections.append(enable_head(self.work / f"vnc{head}.sock", 1280, 800))
        self.sampler = Sampler(self.head_sources())
        self.sampler.start()

    def stop(self):
        if self.sampler is not None:
            self.sampler.stop()
        for conn in self.head_connections:
            conn.close()
        for recorder in self.head_recorders.values():
            try:
                recorder.stop()
            except Exception:  # noqa: BLE001
                pass
        super().stop()

    # QMP events.

    def wait_shutdown(self, since, timeout):
        return self._qmp.wait_event("SHUTDOWN", since, timeout)

    # Guest helpers.

    def guest_epoch(self):
        """Seconds since the epoch on the guest, by one serial round trip."""
        before = time.time()
        out = self.run("date +%s.%N", 10).strip().splitlines()
        after = time.time()
        return float(out[-1]), (before + after) / 2

    def clock_offset(self):
        """guest_clock - host_clock, accurate to the serial round trip (tens of ms)."""
        guest, host = self.guest_epoch()
        return guest - host

    def guest_has(self, path):
        return self.run(f"test -e {path} && echo yes || echo no", 10).strip().endswith("yes")

    def processes(self):
        """[(pid, ppid, user, comm, args)] of every process in the guest."""
        out = self.run("ps -eo pid=,ppid=,user:20=,comm=,args= | cut -c1-200", 20)
        rows = []
        for line in out.splitlines():
            parts = line.split(None, 4)
            if len(parts) >= 4 and parts[0].isdigit() and parts[1].isdigit():
                rows.append((int(parts[0]), int(parts[1]), parts[2], parts[3], parts[4] if len(parts) > 4 else ""))
        return rows

    def pids(self, comm, user=None):
        return sorted(p[0] for p in self.processes() if p[3] == comm and (user is None or p[2] == user))

    def descendants(self, roots, rows=None):
        rows = rows if rows is not None else self.processes()
        children = {}
        for pid, ppid, user, comm, args in rows:
            children.setdefault(ppid, []).append((pid, comm))
        found, stack = [], list(roots)
        while stack:
            pid = stack.pop()
            for child, comm in children.get(pid, []):
                found.append((child, comm))
                stack.append(child)
        return found

    def bsmctl_json(self, arguments, timeout=30, as_user=None):
        """Run `bsmctl ARGS --json` in the guest; (parsed JSON or None, raw output)."""
        command = f"bsmctl {arguments} --json 2>&1"
        out = self.run_as(as_user, command, timeout) if as_user else self.run(command, timeout)
        text = out.strip()
        try:
            return json.loads(text), text
        except ValueError:
            for line in reversed(text.splitlines()):
                try:
                    return json.loads(line), text
                except ValueError:
                    continue
        return None, text

    # The monitor tap: `bsmctl monitor --json`, each line prefixed by the guest's epoch.

    def ensure_monitor(self):
        if getattr(self, "_monitor_started", False):
            return
        self._monitor_started = True
        self.start_monitor()

    def start_monitor(self):
        self.run("mkdir -p /run/bsm-test && : > " + MONITOR_LOG, 10)
        self.run(
            "nohup sh -c 'bsmctl monitor --json 2>&1 | while IFS= read -r line; do "
            f'printf "%s %s\\n" "$(date +%s.%N)" "$line"; done\' >>{MONITOR_LOG} 2>&1 &',
            10,
        )
        self._offset = self.clock_offset()

    def monitor_events(self):
        """[(host_monotonic_time, event dict)] for every line of the monitor log so far."""
        offset = getattr(self, "_offset", 0.0)
        # Host wall clock to host monotonic, evaluated now.
        wall_to_mono = time.monotonic() - time.time()
        events = []
        for line in self.run(f"cat {MONITOR_LOG}", 20).splitlines():
            stamp, _, body = line.partition(" ")
            try:
                event = json.loads(body)
                when = float(stamp) - offset + wall_to_mono
            except ValueError:
                continue
            if isinstance(event, dict):
                events.append((when, event))
        return events

    def event_count(self):
        """Number of monitor events so far: pass it as `since` to wait only for newer ones."""
        return len(self.monitor_events())

    def wait_monitor(self, predicate, timeout, poll=1.0, since=0):
        """First (time, event) matching predicate(event) among events with index >= since, or None after `timeout`."""
        deadline = time.monotonic() + timeout
        seen = since
        while True:
            events = self.monitor_events()
            for when, event in events[seen:]:
                if predicate(event):
                    return when, event
            seen = len(events)
            if time.monotonic() >= deadline:
                return None
            time.sleep(poll)
