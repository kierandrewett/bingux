"""Recording and frame checks for the distro VM tests.

Two views of the guest screen come from QEMU, both on sockets the Vm opens:

- QMP `screendump` on a second QMP socket, sampled by a capture thread on a
  fixed 10 fps clock. Slot k is due at t0 + k / 10. Each dump is read, streamed
  into ffmpeg as raw RGB and deleted, so frames never pile up in /tmp. A slot
  that a slow dump covers late gets the same frame, and the count is reported.
- The VNC server (`-vnc unix:`) sends a framebuffer update whenever the scanout
  changes, at QEMU's display refresh (about 30 Hz). Each update that changes the
  framebuffer is logged to frame-events.jsonl with a black/size classification,
  so a one-frame flash between 10 fps samples is still visible.

Timeline events come from frame predicates: a test registers a name and a
function on a frame (`Recorder.expect`), or marks a point itself
(`Recorder.mark`). The first time each one holds, its time since power-on is
stored in timeline.json.

The frame functions at the bottom work on numpy RGB arrays (H x W x 3, uint8)
and take regions as `WxH+X+Y` geometry strings.
"""

from __future__ import annotations

import json
import os
import socket
import struct
import subprocess
import threading
import time
from pathlib import Path

import numpy as np

FPS = 10
# ffmpeg here has no libx264; libopenh264 is the H.264 encoder that is present.
ENCODER = ["-c:v", "libopenh264", "-b:v", "6M", "-pix_fmt", "yuv420p"]


# ---- frame sources -------------------------------------------------------------


def read_ppm(path):
    """Decode a binary PPM (P6) file into an H x W x 3 uint8 array."""
    data = Path(path).read_bytes()
    fields, i = [], 2
    while len(fields) < 3:
        while data[i : i + 1].isspace():
            i += 1
        j = i
        while not data[j : j + 1].isspace():
            j += 1
        fields.append(int(data[i:j]))
        i = j
    width, height, _ = fields
    i += 1
    return np.frombuffer(data, np.uint8, count=width * height * 3, offset=i).reshape(height, width, 3)


class Qmp:
    """A QMP connection; events are skipped and only the command's reply is kept."""

    def __init__(self, path, timeout=5):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.sock.connect(str(path))
        self.file = self.sock.makefile("rwb")
        self.file.readline()
        self.command("qmp_capabilities")

    def command(self, name, **arguments):
        message = {"execute": name}
        if arguments:
            message["arguments"] = arguments
        self.file.write(json.dumps(message).encode() + b"\n")
        self.file.flush()
        while True:
            line = self.file.readline()
            if not line:
                raise RuntimeError("QMP connection closed")
            reply = json.loads(line)
            if "event" in reply:
                continue
            if "error" in reply:
                raise RuntimeError(f"QMP {name}: {reply['error']}")
            return reply.get("return")

    def screendump(self, filename):
        self.command("screendump", filename=str(filename), format="ppm")

    def close(self):
        self.file.close()
        self.sock.close()


# ---- frame checks --------------------------------------------------------------


def parse_geometry(geometry):
    """`WxH+X+Y` to (width, height, x, y)."""
    size, x, y = geometry.split("+", 2)[0], *geometry.split("+", 2)[1:]
    width, height = (int(v) for v in size.split("x"))
    return width, height, int(x), int(y)


def crop(frame, geometry):
    width, height, x, y = parse_geometry(geometry)
    return frame[y : y + height, x : x + width]


def luma(pixels):
    pixels = pixels.astype(np.float32)
    return pixels[..., 0] * 0.299 + pixels[..., 1] * 0.587 + pixels[..., 2] * 0.114


def bright_count(frame, geometry, level=229):
    """Pixels at or above `level` (about 90% white) in a region: the greeter and bar clock text."""
    return int((luma(crop(frame, geometry)) >= level).sum())


def blue_lead(frame, geometry):
    """Mean of blue minus red over a region, 0 to 1: the default wallpaper is a blue gradient."""
    pixels = crop(frame, geometry).astype(np.float32) / 255.0
    return float(pixels[..., 2].mean() - pixels[..., 0].mean())


def region_mean_luma(frame, geometry):
    return float(luma(crop(frame, geometry)).mean())


def is_black(frame):
    """A frame with no light in it: a black scanout, or a blank fade. Looks at every fourth pixel."""
    sample = luma(frame[::4, ::4])
    return float(sample.mean()) < 6.0 and float(sample.std()) < 4.0


def classify(frame, reference_size=None):
    """'black', 'size-change', 'placeholder' or 'content' for one frame.

    QEMU shows a small 'Display output is not active' surface while the
    scanout has no mode, so a 640x480 frame that is not the session size counts
    as a placeholder, and a frame of any other size counts as a size change.
    """
    height, width = frame.shape[:2]
    if reference_size and (width, height) != reference_size:
        if (width, height) == (640, 480):
            return "placeholder"
        return "size-change"
    if is_black(frame):
        return "black"
    return "content"


def wallpaper_difference(first, second, geometry):
    """Mean absolute difference (0 to 255) of a wallpaper-only region between two frames.

    Continuity between stages means the wallpaper did not jump, flash or
    re-decode: the same region should match within a few levels.
    """
    a = crop(first, geometry)[::4, ::4].astype(np.int16)
    b = crop(second, geometry)[::4, ::4].astype(np.int16)
    return float(np.abs(a - b).mean())


def bar_clock_box(frame, geometry="520x40+400+0", level=229):
    """Bounding box (x0, y0, x1, y1) of the white text in the top bar's clock area, or None."""
    width, height, x, y = parse_geometry(geometry)
    mask = luma(frame[y : y + height, x : x + width]) >= level
    rows = np.flatnonzero(mask.any(axis=1))
    cols = np.flatnonzero(mask.any(axis=0))
    if rows.size == 0:
        return None
    return (int(cols[0]) + x, int(rows[0]) + y, int(cols[-1]) + x, int(rows[-1]) + y)


def boxes_match(a, b, tolerance=2):
    if a is None or b is None:
        return a is b
    return all(abs(p - q) <= tolerance for p, q in zip(a, b))


def session_painted(frame):
    """True on a Bingux session: the bar's clock text on a dark strip and the blue wallpaper below it."""
    return bright_count(frame, "160x24+560+4") > 100 and blue_lead(frame, "1280x700+0+60") > 0.05


# ---- recording -----------------------------------------------------------------


class Recorder:
    """Records one VM run into `out_dir`: flow.mp4, timeline.json, frame-events.jsonl.

    Start it right after QEMU starts, with the QMP and VNC socket paths.
    """

    def __init__(self, out_dir, scratch_dir, qmp_path, vnc_path, reference_size=(1280, 800)):
        self.out = Path(out_dir)
        self.out.mkdir(parents=True, exist_ok=True)
        self.scratch = Path(scratch_dir)
        self.scratch.mkdir(parents=True, exist_ok=True)
        self.qmp_path, self.vnc_path = Path(qmp_path), Path(vnc_path)
        self.reference_size = reference_size
        self.t0 = None
        self.events = {}
        self.sources = {}
        self.expectations = {}
        self.stats = {"dumps": 0, "dump_errors": 0, "scanout_fallbacks": 0, "slots": 0, "duplicate_slots": 0, "size_changes": 0, "errors": []}
        self.frame_times = []
        self.dump_times = []
        self.last_slot_time = None
        self.latest = None
        self.scanout_frame = None
        self.latest_time = None
        self.scanout_events = []
        self._lock = threading.Condition()
        self._stop = threading.Event()
        self._ffmpeg = None
        self._threads = []
        self._frame_events_file = None

    # Lifecycle.

    def start(self, t0):
        self.t0 = t0
        self._ffmpeg = subprocess.Popen(
            [
                "ffmpeg", "-y", "-loglevel", "error",
                "-f", "rawvideo", "-pix_fmt", "rgb24",
                "-s", f"{self.reference_size[0]}x{self.reference_size[1]}", "-r", str(FPS), "-i", "-",
                "-an", "-r", str(FPS), *ENCODER, "-movflags", "+faststart",
                str(self.out / "flow.mp4"),
            ],
            stdin=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self._frame_events_file = (self.out / "frame-events.jsonl").open("w")
        self._threads = [
            threading.Thread(target=self._sample_loop, name="capture-10fps", daemon=True),
            threading.Thread(target=self._scanout_loop, name="capture-scanout", daemon=True),
        ]
        for thread in self._threads:
            thread.start()

    def stop(self):
        """Stop both threads, finish the video and write timeline.json; returns the stats."""
        self._stop.set()
        for thread in self._threads:
            thread.join(timeout=20)
        if self._ffmpeg is not None:
            try:
                self._ffmpeg.stdin.close()
            except OSError:
                pass
            err = self._ffmpeg.stderr.read().decode(errors="replace")
            self._ffmpeg.wait(timeout=120)
            if self._ffmpeg.returncode != 0 or err.strip():
                self.stats["errors"].append("ffmpeg: " + err.strip()[-400:])
        if self._frame_events_file is not None:
            self._frame_events_file.close()
        self.stats.update(self._rate_stats())
        timeline = {
            "fps": FPS,
            "clock": "seconds since power-on (QEMU start), monotonic",
            "events": sorted(
                ({"name": name, "t": round(t, 3), "source": self.sources.get(name, "frame")} for name, t in self.events.items()),
                key=lambda e: e["t"],
            ),
            "capture": self.stats,
        }
        (self.out / "timeline.json").write_text(json.dumps(timeline, indent=2) + "\n")
        return self.stats

    def _rate_stats(self):
        """Achieved rate and jitter. Slot k is due at t0 + k / FPS, so a steady run has gaps near 100 ms."""
        if self.t0 is None or len(self.frame_times) < 2:
            return {}
        gaps = np.diff(np.array(self.frame_times))
        span = self.last_slot_time - self.t0 + 1 / FPS
        return {
            "achieved_fps": round(self.stats["slots"] / span, 2),
            "video_frames": self.stats["slots"],
            "video_seconds": round(self.stats["slots"] / FPS, 2),
            "gap_ms_median": round(float(np.median(gaps)) * 1000, 1),
            "gap_ms_max": round(float(gaps.max()) * 1000, 1),
            "dump_ms_median": round(float(np.median(self.dump_times)) * 1000, 1),
        }

    # Timeline.

    def expect(self, name, predicate):
        """Record the first time `predicate(frame)` holds as the event `name`."""
        with self._lock:
            self.expectations[name] = predicate

    def mark(self, name):
        """Record `name` at this moment (a test step, not a frame)."""
        with self._lock:
            if name not in self.events and self.t0 is not None:
                self.events[name] = time.monotonic() - self.t0
                self.sources[name] = "step"
                self._lock.notify_all()

    def wait_event(self, name, timeout):
        deadline = time.monotonic() + timeout
        with self._lock:
            while name not in self.events:
                remaining = deadline - time.monotonic()
                if remaining <= 0 or self._stop.is_set():
                    return False
                self._lock.wait(min(remaining, 0.5))
            return True

    def event_time(self, name):
        return self.events.get(name)

    def frame(self):
        """The most recent sample (numpy RGB), or None before the first one."""
        with self._lock:
            return self.latest

    # Capture loops.

    def _sample_loop(self):
        scratch = self.scratch / "current.ppm"
        qmp = None
        slot = 0
        try:
            while not self._stop.is_set():
                due = self.t0 + slot / FPS
                wait = due - time.monotonic()
                if wait > 0 and self._stop.wait(wait):
                    break
                started = time.monotonic()
                frame = None
                source = None
                try:
                    if qmp is None:
                        qmp = Qmp(self.qmp_path)
                    qmp.screendump(scratch)
                    frame = read_ppm(scratch)
                    source = "qmp"
                except (OSError, RuntimeError, ValueError) as error:
                    # QEMU has no display surface during some mode sets ("no surface").
                    # Take the picture from the VNC scanout instead; if the scanout has
                    # not produced a frame yet, the slot repeats the last picture.
                    self._dump_failed(error, started)
                    qmp = self._close_quietly(qmp)
                    frame = self._scanout_snapshot()
                    if frame is not None:
                        source = "scanout"
                finally:
                    scratch.unlink(missing_ok=True)
                done = time.monotonic()
                if source == "qmp":
                    self.dump_times.append(done - started)
                    self.stats["dumps"] += 1
                    self.frame_times.append(started)
                elif source == "scanout":
                    self.stats["scanout_fallbacks"] += 1
                    self.frame_times.append(started)
                if frame is not None:
                    picture = frame
                else:
                    picture = self.latest if self.latest is not None else np.zeros(
                        (self.reference_size[1], self.reference_size[0], 3), np.uint8
                    )
                # Every slot whose time has passed by the end of this dump gets this frame.
                last_slot = max(slot, int((done - self.t0) * FPS))
                self._emit(picture, (started + done) / 2, slot_count=last_slot - slot + 1, check=frame is not None)
                self.stats["duplicate_slots"] += last_slot - slot
                slot = last_slot + 1
                self.last_slot_time = self.t0 + last_slot / FPS
        finally:
            self._close_quietly(qmp)
            scratch.unlink(missing_ok=True)

    def _scanout_snapshot(self):
        with self._lock:
            frame = self.scanout_frame
        return frame

    def _close_quietly(self, qmp):
        if qmp is not None:
            try:
                qmp.close()
            except OSError:
                pass
        return None

    def _dump_failed(self, error, when):
        self.stats["dump_errors"] += 1
        if len(self.stats["errors"]) < 20:
            self.stats["errors"].append(f"capture: {error}")
        record = {
            "t": round(when - self.t0, 4),
            "kind": "dump-error",
            "message": str(error),
        }
        self._frame_events_file.write(json.dumps(record) + "\n")


    def _emit(self, frame, when, slot_count, check=True):
        """Write `slot_count` copies of `frame` into the video and run the frame expectations on it."""
        width, height = self.reference_size
        canvas = fit(frame, width, height)
        if frame.shape[:2] != (height, width):
            self.stats["size_changes"] += 1
            self._record_size_change(frame, when)
        data = canvas.tobytes()
        for _ in range(slot_count):
            self._ffmpeg.stdin.write(data)
            self.stats["slots"] += 1
        if check:
            self._check_expectations(frame, when)
        with self._lock:
            self.latest = frame
            self.latest_time = when

    def _check_expectations(self, frame, when):
        with self._lock:
            pending = list(self.expectations.items())
        for name, predicate in pending:
            if predicate(frame):
                with self._lock:
                    if name not in self.events:
                        self.events[name] = when - self.t0
                        self.sources[name] = "frame"
                        self._lock.notify_all()
                    self.expectations.pop(name, None)

    def _record_size_change(self, frame, when):
        name = f"screen {frame.shape[1]}x{frame.shape[0]}"
        with self._lock:
            self.events.setdefault(name, when - self.t0)
            self.sources.setdefault(name, "frame")

    def _scanout_loop(self):
        """Log each VNC framebuffer update that changes the picture."""
        try:
            conn = connect_vnc(self.vnc_path)
        except (OSError, RuntimeError) as error:
            self.stats["errors"].append(f"scanout: {error}")
            return
        try:
            conn.settimeout(10)
            framebuffer = np.zeros((conn.size[1], conn.size[0], 4), np.uint8)
            conn.request(incremental=False)
            while not self._stop.is_set():
                try:
                    kind = conn.recv_exact(1)[0]
                except TimeoutError:
                    continue
                if kind != 0:
                    raise RuntimeError(f"unexpected VNC message {kind}")
                conn.recv_exact(1)
                rects = struct.unpack(">H", conn.recv_exact(2))[0]
                changed_area = 0
                resized = False
                for _ in range(rects):
                    x, y, w, h, encoding = struct.unpack(">HHHHi", conn.recv_exact(12))
                    if encoding == -223:  # DesktopSize: a new mode, its pixels follow in a full update.
                        framebuffer = np.zeros((h, w, 4), np.uint8)
                        resized = True
                        self._scanout_event(framebuffer, "resize", rects=rects, area=0)
                        continue
                    if encoding != 0:
                        raise RuntimeError(f"unsupported VNC encoding {encoding}")
                    pixels = conn.recv_exact(w * h * 4)
                    framebuffer[y : y + h, x : x + w] = np.frombuffer(pixels, np.uint8).reshape(h, w, 4)
                    changed_area += w * h
                if not resized:
                    self._scanout_event(framebuffer, "update", rects=rects, area=changed_area)
                conn.request(incremental=True, size=(framebuffer.shape[1], framebuffer.shape[0]))
        except (OSError, RuntimeError, ValueError) as error:
            if not self._stop.is_set():
                self.stats["errors"].append(f"scanout: {error}")
        finally:
            conn.close()

    def _scanout_event(self, framebuffer, kind, rects, area):
        rgb = framebuffer[..., [2, 1, 0]]
        with self._lock:
            self.scanout_frame = rgb
        height, width = framebuffer.shape[:2]
        if (width, height) == self.reference_size and not is_black(rgb):
            label = "content"
        elif (width, height) == (640, 480):
            label = "placeholder"
        elif is_black(rgb):
            label = "black"
        else:
            label = "size-change"
        record = {
            "t": round(time.monotonic() - self.t0, 4),
            "kind": kind,
            "size": [width, height],
            "rects": rects,
            "area": area,
            "class": label,
            "mean": round(float(luma(rgb[::4, ::4]).mean()), 1),
        }
        self._frame_events_file.write(json.dumps(record) + "\n")
        self.scanout_events.append(record)


class VncConnection:
    """Just enough RFB 3.8 for raw framebuffer updates over a unix socket, with no auth."""

    def __init__(self, sock):
        self.sock = sock
        self.size = None

    def settimeout(self, seconds):
        self.sock.settimeout(seconds)

    def recv_exact(self, count):
        chunks, remaining = [], count
        while remaining:
            chunk = self.sock.recv(min(remaining, 1 << 20))
            if not chunk:
                raise RuntimeError("VNC connection closed")
            chunks.append(chunk)
            remaining -= len(chunk)
        return b"".join(chunks)

    def send(self, data):
        self.sock.sendall(data)

    def request(self, incremental, size=None):
        width, height = size or self.size
        self.send(struct.pack(">BBHHHH", 3, 1 if incremental else 0, 0, 0, width, height))

    def close(self):
        self.sock.close()


def connect_vnc(path):
    sock = None
    for _ in range(150):
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.settimeout(10)
            sock.connect(str(path))
            break
        except OSError:
            sock.close()
            sock = None
            time.sleep(0.2)
    if sock is None:
        raise RuntimeError(f"no VNC socket at {path}")
    conn = VncConnection(sock)
    conn.recv_exact(12)
    conn.send(b"RFB 003.008\n")
    types = conn.recv_exact(conn.recv_exact(1)[0])
    if 1 not in types:
        raise RuntimeError("VNC server offers no security-free login")
    conn.send(bytes([1]))
    if struct.unpack(">I", conn.recv_exact(4))[0] != 0:
        raise RuntimeError("VNC security failed")
    conn.send(bytes([1]))  # shared session
    width, height = struct.unpack(">HH", conn.recv_exact(4))
    conn.size = (width, height)
    conn.recv_exact(16)  # pixel format, echoed back as the 32-bit layout we ask for below
    name_length = struct.unpack(">I", conn.recv_exact(4))[0]
    conn.recv_exact(name_length)
    # 32 bits per pixel, little-endian, true colour: bytes are B, G, R, unused.
    conn.send(struct.pack(">BxxxBBBBHHHBBBxxx", 0, 32, 24, 0, 1, 255, 255, 255, 16, 8, 0))
    conn.send(struct.pack(">BxHii", 2, 2, 0, -223))
    conn.settimeout(10)
    return conn


def fit(frame, width, height):
    """The frame on a width x height black canvas: cropped or padded, never scaled."""
    if frame.shape[:2] == (height, width):
        return frame
    canvas = np.zeros((height, width, 3), np.uint8)
    h = min(height, frame.shape[0])
    w = min(width, frame.shape[1])
    canvas[:h, :w] = frame[:h, :w]
    return canvas
