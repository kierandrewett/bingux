"""Recording and frame checks for the distro VM tests.

The recorder reads the guest screen from the VNC server (`-vnc unix:`) and
from nothing else. QMP screendump is not used: under virtio-vga-gl with
egl-headless the scanout lives in the GPU, QEMU has no CPU surface to dump,
and screendump fails with "no surface" on nearly every call. VNC reads the
scanout through GL, so it works in that mode.

Two views of the same framebuffer:

- 10 fps slots: slot k is due at t0 + k / 10. At each slot the framebuffer is
  copied, written once to the video (ffmpeg, raw RGB) and logged to
  slots.jsonl with its age and class. A slot is `real` when QEMU sent a
  framebuffer message within MAX_FRAME_AGE. A quiet connection is refreshed by
  a full-frame request every HEARTBEAT seconds, so a static screen stays
  provably current.
- Full-rate events: every framebuffer message that changes pixels is logged to
  frame-events.jsonl with its class, and frame expectations run on it, so a
  one-frame flash between slots is still seen.

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
# Capture timing (seconds). A slot is `real` when the framebuffer is at most
# MAX_FRAME_AGE old: a quiet screen is re-requested every HEARTBEAT, so a
# still picture stays current. A slot is `late` when it is taken LATE after due.
HEARTBEAT = 0.2
MAX_FRAME_AGE = 0.5
LATE = 0.05


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
    """A frame with no light in it: a black scanout, or a blank fade with a stray glyph.

    Measured on the VT-switch black frame (mean 0.2, 0.1% of pixels above luma 16)
    versus a content frame (mean 25, 72% above 16). Looks at every fourth pixel.
    """
    sample = luma(frame[::4, ::4])
    return float(sample.mean()) < 6.0 and float((sample > 16).mean()) < 0.01


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


# Screen-state predicates shared by the boot test and installer tests. Each is
# measured on boot stills (1280x800): the greeter's card title has 171 bright
# pixels and no other state has more than 0; the session's top strip mean is
# 42.6 against 30.5 on the greeter and 45.6 on the lock screen.
GREETER_CARD_TITLE = "160x24+560+278"
SESSION_STRIP = "200x28+300+0"


def greeter_shown(frame):
    """True on the kiosk greeter: the bright title of its login card."""
    return bright_count(frame, GREETER_CARD_TITLE) > 100


def session_painted(frame):
    """True on a Bingux session (desktop or lock): the bar's clock text over a lit strip and the blue wallpaper.

    The bar clock text rules out the Plymouth splash (same blue, no text at the top). The greeter has the
    same bar and wallpaper, so a frame that still shows the greeter's login card is not a session.
    """
    return (
        bright_count(frame, "160x24+560+4") > 100
        and region_mean_luma(frame, SESSION_STRIP) >= 36.0
        and blue_lead(frame, "1280x700+0+60") > 0.05
        and not greeter_shown(frame)
    )


# ---- recording -----------------------------------------------------------------


class Recorder:
    """Records one VM run into `out_dir`: flow.mp4, timeline.json, slots.jsonl and frame-events.jsonl.

    The only frame source is the VNC framebuffer on `vnc_path`. Start it right
    after QEMU starts, with the VNC socket path.
    """

    def __init__(self, out_dir, vnc_path, reference_size=(1280, 800)):
        self.out = Path(out_dir)
        self.out.mkdir(parents=True, exist_ok=True)
        self.vnc_path = Path(vnc_path)
        self.reference_size = reference_size
        self.t0 = None
        self.events = {}
        self.sources = {}
        self.expectations = {}
        self.stats = {
            "slots": 0,
            "real_slots": 0,
            "stale_slots": 0,
            "fresh_slots": 0,
            "held_slots": 0,
            "black_slots": 0,
            "size_change_slots": 0,
            "late_slots": 0,
            "write_ms_max": 0.0,
            "updates": 0,
            "changed_updates": 0,
            "refresh_requests": 0,
            "errors": [],
        }
        self.slot_times = []
        self.last_slot_time = None
        self.max_age = 0.0
        self.max_late = 0.0
        self.framebuffer = None  # H x W x 4 (B, G, R, unused), written by the VNC thread under _lock
        self.update_time = None  # monotonic time of the last framebuffer message
        self.change_count = 0  # framebuffer messages that changed pixels
        self.seen_changes = 0  # change_count when the previous slot was written
        self._lock = threading.Condition()
        self._stop = threading.Event()
        self._ffmpeg = None
        self._threads = []
        self._slots_file = None
        self._events_file = None

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
        self._slots_file = (self.out / "slots.jsonl").open("w")
        self._events_file = (self.out / "frame-events.jsonl").open("w")
        self._threads = [
            threading.Thread(target=self._slot_loop, name="capture-10fps", daemon=True),
            threading.Thread(target=self._vnc_loop, name="capture-vnc", daemon=True),
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
        for handle in (self._slots_file, self._events_file):
            if handle is not None:
                handle.close()
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
        """Achieved rate and slot-to-slot gaps. Slot k is due at t0 + k / FPS, so a steady run has gaps near 100 ms."""
        if self.t0 is None or len(self.slot_times) < 2:
            return {}
        gaps = np.diff(np.array(self.slot_times))
        span = self.last_slot_time - self.t0 + 1 / FPS
        return {
            "achieved_fps": round(self.stats["slots"] / span, 2),
            "video_frames": self.stats["slots"],
            "video_seconds": round(self.stats["slots"] / FPS, 2),
            "gap_ms_median": round(float(np.median(gaps)) * 1000, 1),
            "gap_ms_max": round(float(gaps.max()) * 1000, 1),
            "age_ms_max": round(self.max_age * 1000, 1),
            "late_ms_max": round(self.max_late * 1000, 1),
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
        """The newest framebuffer (numpy RGB), or None before the first picture."""
        with self._lock:
            if self.update_time is None:
                return None
            return self.framebuffer[..., [2, 1, 0]]

    # Slots: one real framebuffer per 10 fps slot.

    def _slot_loop(self):
        slot = 0
        while not self._stop.is_set():
            due = self.t0 + slot / FPS
            wait = due - time.monotonic()
            if wait > 0 and self._stop.wait(wait):
                break
            taken = time.monotonic()
            with self._lock:
                rgb = None if self.update_time is None else self.framebuffer[..., [2, 1, 0]]
                updated = self.update_time
                changes = self.change_count
            self._emit_slot(slot, due, taken, rgb, updated, changes)
            slot += 1

    def _emit_slot(self, slot, due, taken, rgb, updated, changes):
        """Write one slot: one copy of the framebuffer, with its age and whether it changed since the last slot."""
        width, height = self.reference_size
        late = taken - due
        self.max_late = max(self.max_late, late)
        if late > LATE:
            self.stats["late_slots"] += 1
        if rgb is None:
            picture = np.zeros((height, width, 3), np.uint8)
            age, real, fresh, label = None, False, False, "no-picture"
        else:
            picture = rgb
            age = taken - updated
            self.max_age = max(self.max_age, age)
            real = age <= MAX_FRAME_AGE
            fresh = changes != self.seen_changes
            self.seen_changes = changes
            label = classify(rgb, self.reference_size)
        started = time.monotonic()
        self._ffmpeg.stdin.write(fit(picture, width, height).tobytes())
        self.stats["write_ms_max"] = max(self.stats["write_ms_max"], round((time.monotonic() - started) * 1000, 1))
        self.slot_times.append(taken)
        self.last_slot_time = due
        stats = self.stats
        stats["slots"] += 1
        stats["real_slots" if real else "stale_slots"] += 1
        if fresh:
            stats["fresh_slots"] += 1
        elif real:
            stats["held_slots"] += 1
        if label == "black":
            stats["black_slots"] += 1
        if label in ("size-change", "placeholder"):
            stats["size_change_slots"] += 1
        record = {
            "slot": slot,
            "due": round(due - self.t0, 3),
            "taken": round(taken - self.t0, 4),
            "age_ms": None if age is None else round(age * 1000, 1),
            "real": real,
            "fresh": fresh,
            "class": label,
        }
        self._slots_file.write(json.dumps(record) + "\n")

    # Full-rate events: every framebuffer message from QEMU.

    def _vnc_loop(self):
        try:
            conn = connect_vnc(self.vnc_path)
        except (OSError, RuntimeError) as error:
            self.stats["errors"].append(f"vnc: {error}")
            return
        try:
            width, height = conn.size
            framebuffer = np.zeros((height, width, 4), np.uint8)
            with self._lock:
                self.framebuffer = framebuffer
            conn.request(incremental=False, size=(width, height))
            while not self._stop.is_set():
                # Wait at most HEARTBEAT for the next message. On a quiet screen,
                # ask for the whole picture: it is re-sent and stays current.
                conn.settimeout(HEARTBEAT)
                try:
                    kind = conn.recv_exact(1)[0]
                except TimeoutError:
                    self.stats["refresh_requests"] += 1
                    conn.request(incremental=False, size=(framebuffer.shape[1], framebuffer.shape[0]))
                    continue
                conn.settimeout(10)  # the rest of a message must arrive whole
                if kind != 0:
                    raise RuntimeError(f"unexpected VNC message {kind}")
                conn.recv_exact(1)
                rects = struct.unpack(">H", conn.recv_exact(2))[0]
                changed = resized = False
                area = 0
                for _ in range(rects):
                    x, y, w, h, encoding = struct.unpack(">HHHHi", conn.recv_exact(12))
                    if encoding == -223:  # DesktopSize: a new mode; its pixels follow in the next update.
                        framebuffer = np.zeros((h, w, 4), np.uint8)
                        with self._lock:
                            self.framebuffer = framebuffer
                        resized = changed = True
                        self._note_event(f"screen {w}x{h}", time.monotonic())
                        continue
                    if encoding != 0:
                        raise RuntimeError(f"unsupported VNC encoding {encoding}")
                    pixels = np.frombuffer(conn.recv_exact(w * h * 4), np.uint8).reshape(h, w, 4)
                    region = framebuffer[y : y + h, x : x + w]
                    if not np.array_equal(region, pixels):
                        changed = True
                    with self._lock:
                        region[...] = pixels
                    area += w * h
                now = time.monotonic()
                with self._lock:
                    self.update_time = now
                    if changed:
                        self.change_count += 1
                self.stats["updates"] += 1
                if changed:
                    self.stats["changed_updates"] += 1
                    self._after_change(framebuffer, now, rects, area, resized)
                # One incremental request stays in flight after every update.
                conn.request(incremental=True, size=(framebuffer.shape[1], framebuffer.shape[0]))
        except (OSError, RuntimeError, ValueError) as error:
            if not self._stop.is_set():
                self.stats["errors"].append(f"vnc: {error}")
        finally:
            conn.close()

    def _after_change(self, framebuffer, when, rects, area, resized):
        """Classify a changed picture, log it and run the frame expectations on it."""
        rgb = framebuffer[..., [2, 1, 0]]
        height, width = rgb.shape[:2]
        record = {
            "t": round(when - self.t0, 4),
            "kind": "resize" if resized else "update",
            "size": [width, height],
            "rects": rects,
            "area": area,
            "class": classify(rgb, self.reference_size),
            "mean": round(float(luma(rgb[::4, ::4]).mean()), 1),
        }
        self._events_file.write(json.dumps(record) + "\n")
        self._check_expectations(rgb, when)

    def _note_event(self, name, when):
        with self._lock:
            self.events.setdefault(name, when - self.t0)
            self.sources.setdefault(name, "frame")

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


# ---- handover checks over recordings -------------------------------------------
#
# A recording is the run's out_dir: flow.mp4 (10 fps, slot k at k / 10 s after
# power-on), timeline.json (named events) and frame-events.jsonl (every
# framebuffer update with its class). Gap scans read frame-events.jsonl; the
# frame checks decode only the short windows of each handover from flow.mp4.

# Wallpaper-only corner of the 1280x800 desktop: no bar, no card, no dock.
WALLPAPER_REGION = "240x100+0+700"
WALLPAPER_LIMIT = 8.0
# A black run shorter than this is a fade, longer is a defect.
BLACK_RUN_LIMIT = 0.5


def recording_events(out_dir):
    """name -> first time (seconds since power-on) of each named event in timeline.json."""
    data = json.loads((Path(out_dir) / "timeline.json").read_text())
    events = {}
    for event in data["events"]:
        events.setdefault(event["name"], float(event["t"]))
    return events


def video_size(video):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height",
         "-of", "csv=p=0", str(video)],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    width, height = out.split(",")
    return int(width), int(height)


def decode_window(video, start, end, fps=FPS):
    """[(t, frame)] for every 10 fps slot from start to end seconds, decoded from flow.mp4."""
    width, height = video_size(video)
    count = max(1, int(round((end - start) * fps)) + 1)
    raw = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-ss", f"{start:.3f}", "-i", str(video),
         "-frames:v", str(count), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
        capture_output=True, check=True,
    ).stdout
    size = width * height * 3
    return [
        (round(start + i / fps, 3), np.frombuffer(raw, np.uint8, size, i * size).reshape(height, width, 3))
        for i in range(len(raw) // size)
    ]


def frame_at(video, t):
    """The last recorded frame at or before t (within two slots), or None.

    A stage can be stamped on the video's final frame; seeking exactly there decodes
    nothing, so the window opens two slots earlier and its last frame is taken.
    """
    frames = decode_window(video, max(0.0, t - 2 / FPS), t)
    return frames[-1][1] if frames else None


def in_any(t, spans):
    return any(lo <= t <= hi for lo, hi in spans)


def black_gaps(out_dir, exclude=()):
    """Black runs in frame-events.jsonl longer than BLACK_RUN_LIMIT, outside the excluded spans.

    A run starts at the first black update and ends at the next non-black update.
    Returns [(start, end)] in seconds since power-on.
    """
    events = [json.loads(line) for line in (Path(out_dir) / "frame-events.jsonl").read_text().splitlines() if line]
    runs = []
    start = None
    last = None
    for event in events:
        if event["class"] == "black":
            if start is None:
                start = event["t"]
            last = event["t"]
        elif start is not None:
            runs.append((start, event["t"]))
            start = None
    if start is not None and last is not None:
        runs.append((start, last))
    return [
        (round(a, 2), round(b, 2))
        for a, b in runs
        if b - a > BLACK_RUN_LIMIT and not any(a >= lo and b <= hi for lo, hi in exclude)
    ]


def handover_defects(video, start, end, exclude=()):
    """Frames in [start, end] that are not content: black, placeholder or size change. Excluded spans are skipped."""
    defects = []
    for t, frame in decode_window(video, start, end):
        if in_any(t, exclude):
            continue
        kind = classify(frame, (1280, 800))
        if kind != "content":
            defects.append((t, kind))
    return defects


def bar_matches(first, second):
    """True when the top bar's clock sits at the same box in both frames."""
    return boxes_match(bar_clock_box(first), bar_clock_box(second))


def wallpaper_jump(first, second):
    """Mean absolute difference (0 to 255) of the wallpaper-only corner between two frames."""
    return wallpaper_difference(first, second, WALLPAPER_REGION)
