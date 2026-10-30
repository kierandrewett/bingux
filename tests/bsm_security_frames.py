"""Frame analysis for the bsm security and failure scenarios (tests/bsm-qemu-security.py).

The walkthrough in tests/bsm-qemu.py proves a desktop is hidden by comparing frames with a
reference picture of that desktop. That cannot work on a display where the desktop and the
greeter are almost the same picture (the second display of the test VM shows only the
wallpaper in both). These scenarios instead put a *marker* in each user's desktop: a
full-screen window of one saturated colour on every monitor (tests/bsm_guest_probe.py
`marker`). No greeter, cover or transition frame contains that colour, so any frame that
does is a frame in which a user's desktop buffer reached the screen.

The marker colour is found by chroma, not by exact value: a pixel counts when its two
"high" channels exceed its "low" channel by more than `CHROMA_MIN`. That also catches a
desktop blended into a fade at about a quarter opacity or more, which an exact colour
match would not. The greeter's avatar discs are the only saturated pixels the greeter
draws; they cover well under one percent of a display, far below `LEAK_FRACTION`.

Black frames are counted separately: a frame whose brightest pixels are still nearly black.
The bsm cover is dark blue-grey (#0d1117), not black, so a black frame is a frame in
which nothing was drawn at all.
"""

from __future__ import annotations

import threading
import time

import numpy as np

# (high channels, low channel) of the marker colours. Channels: 0 red, 1 green, 2 blue.
MARKER_CHANNELS = {
    "alice": ((0, 2), 1),  # magenta, #ff00ff
    "bob": ((0, 1), 2),  # yellow, #ffe000
}
MARKER_COLOURS = {"alice": "#ff00ff", "bob": "#ffe000"}
MARKER_NAMES = tuple(MARKER_CHANNELS)

CHROMA_MIN = 60
# A frame shows a desktop when this fraction of its pixels carries the marker's chroma.
LEAK_FRACTION = 0.03
# A desktop is "fully visible" when the marker covers at least this much of the frame.
VISIBLE_FRACTION = 0.5
STEP = 4  # decimation of the analysed frame


def marker_fraction(frame, name, step=STEP):
    """Fraction of the (decimated) frame whose chroma matches the marker `name`."""
    high, low = MARKER_CHANNELS[name]
    view = frame[::step, ::step].astype(np.int16)
    score = np.minimum(view[..., high[0]], view[..., high[1]]) - view[..., low]
    return float((score > CHROMA_MIN).mean())


def luma_stats(frame, step=STEP):
    """(mean luma, 99th percentile luma) of the decimated frame."""
    view = frame[::step, ::step].astype(np.float32)
    luma = view[..., 0] * 0.299 + view[..., 1] * 0.587 + view[..., 2] * 0.114
    return float(luma.mean()), float(np.percentile(luma, 99))


def is_black(mean, p99):
    """Nothing was drawn: even the brightest one percent of the frame is nearly black."""
    return mean < 5.0 and p99 < 12.0


class Sample:
    __slots__ = ("t", "head", "width", "height", "mean", "p99", "fractions")

    def __init__(self, t, head, width, height, mean, p99, fractions):
        self.t = t
        self.head = head
        self.width = width
        self.height = height
        self.mean = mean
        self.p99 = p99
        self.fractions = fractions  # name -> marker fraction

    @property
    def black(self):
        return is_black(self.mean, self.p99)


def analyse_frame(frame, t, head):
    height, width = frame.shape[:2]
    mean, p99 = luma_stats(frame)
    return Sample(t, head, width, height, mean, p99, {name: marker_fraction(frame, name) for name in MARKER_NAMES})


class MarkerSampler:
    """Reads the newest frame of every display about 20 times a second and keeps its marker numbers.

    `sources`: head -> callable returning the newest RGB frame or None (BsmVm.head_sources()).
    Full frames are kept only for a bounded number of leaks, so a run can show what leaked.
    """

    def __init__(self, sources, interval=0.05, keep=200_000, max_leak_frames=24):
        self.sources = sources
        self.interval = interval
        self.keep = keep
        self.max_leak_frames = max_leak_frames
        self.samples = []
        self.leak_frames = []  # (t, head, name, frame copy)
        self.watching_leaks = True
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._thread = None
        self._last = {}

    def start(self):
        self._thread = threading.Thread(target=self._loop, name="bsm-marker-sampler", daemon=True)
        self._thread.start()

    def stop(self):
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=5)

    def _loop(self):
        while not self._stop.is_set():
            started = time.monotonic()
            for head, source in self.sources.items():
                try:
                    frame = source()
                except Exception:  # noqa: BLE001 - a recorder that is shutting down
                    frame = None
                if frame is None:
                    continue
                sample = analyse_frame(frame, started, head)
                previous = self._last.get(head)
                self._last[head] = sample
                with self._lock:
                    self.samples.append(sample)
                    if len(self.samples) > self.keep:
                        del self.samples[: len(self.samples) - self.keep]
                if self.watching_leaks and len(self.leak_frames) < self.max_leak_frames:
                    for name, fraction in sample.fractions.items():
                        if fraction > LEAK_FRACTION and (previous is None or previous.fractions[name] <= LEAK_FRACTION):
                            self.leak_frames.append((started, head, name, np.array(frame)))
            delay = self.interval - (time.monotonic() - started)
            if delay > 0:
                time.sleep(delay)

    # Queries.

    def window(self, start, end, head=None):
        with self._lock:
            return [s for s in self.samples if start <= s.t <= end and (head is None or s.head == head)]

    def latest(self, head):
        with self._lock:
            for sample in reversed(self.samples):
                if sample.head == head:
                    return sample
        return None

    def leaks(self, start, end, name, head=None, threshold=LEAK_FRACTION):
        """Samples in the window where the desktop marked `name` is on screen."""
        return [s for s in self.window(start, end, head) if s.fractions[name] > threshold]

    def last_leak_time(self, start, end, name, head=None):
        found = self.leaks(start, end, name, head)
        return max((s.t for s in found), default=None)

    def visible(self, start, end, name, head=None):
        """Samples in the window where the desktop `name` fills the display (the positive control)."""
        return [s for s in self.window(start, end, head) if s.fractions[name] >= VISIBLE_FRACTION]

    def black(self, start, end, head=None):
        return [s for s in self.window(start, end, head) if s.black]

    def summary(self, start, end, heads):
        """Per head: sample count, the largest marker fraction per user, black frame count, sizes seen."""
        out = {}
        for head in heads:
            window = self.window(start, end, head)
            out[head] = {
                "samples": len(window),
                "max_fraction": {name: round(max((s.fractions[name] for s in window), default=0.0), 4) for name in MARKER_NAMES},
                "leak_samples": {name: sum(1 for s in window if s.fractions[name] > LEAK_FRACTION) for name in MARKER_NAMES},
                "black_samples": sum(1 for s in window if s.black),
                "sizes": sorted({(s.width, s.height) for s in window}),
            }
        return out

    def save_csv(self, path, start, end, base):
        """One row per sample: seconds since `base`, head, size, mean luma, marker fractions."""
        with open(path, "w") as handle:
            handle.write("t_s,head,width,height,mean_luma,alice,bob,black\n")
            for s in self.window(start, end):
                handle.write(
                    f"{s.t - base:.3f},{s.head},{s.width},{s.height},{s.mean:.1f},"
                    f"{s.fractions['alice']:.4f},{s.fractions['bob']:.4f},{int(s.black)}\n"
                )
