"""Pure analysis for tests/bsm-boot.py: scenes in a frame series, tracer and kernel logs.

No VM and no QEMU here; tests/bsm-boot.test.py runs all of it on synthetic input.

Frames. A recorded frame is a `BootFrame` (time, size, class, mean luma, 1/8 scale RGB
thumbnail). `scene_of` names what it shows:

    black        no light: a disabled scanout, or the kernel console with nothing on it
    placeholder  QEMU's 640x480 "display output is not active" surface (scanout disabled)
    size-change  any other size than the session's 1280x800
    splash       Plymouth: the near-black field of the theme with the mark and the spinner
    flat         one colour over the whole screen (bsm's cover, or the splash's field without
                 its mark); `flat-splash` / `flat-cover` when the colour is one of those two
    content      anything with structure: the greeter, a desktop

The handover is the stretch of frames between the last `splash` frame and the first
`content` frame after it. A seamless handover has none of `black`, `placeholder`,
`size-change` in it (and ideally nothing at all: the greeter replaces the splash in one
flip). The VNC server delivers a frame when the picture changes, so a still scene has
one frame; a scene lasts from its first frame to the next frame of another scene.

Logs. `parse_trace` reads tests/bsm-boot-trace.py's log (DRM clients with the master
flag, CRTC state, who owns the scanout framebuffer, VT mode), `parse_dmesg` and
`drm_events` read the kernel's DRM debug lines.
"""

from __future__ import annotations

from dataclasses import dataclass
import re

import numpy as np

# Plymouth's theme (distro/packages/bingux-os/bingux.script): 0.043, 0.043, 0.051 over 255.
SPLASH_BG = (10.0, 10.0, 13.0)
# bsm's built-in cover colour (bsm/core/src/config.rs): #0d0f17.
COVER_BG = (13.0, 15.0, 23.0)
SESSION_SIZE = (1280, 800)
PLACEHOLDER_SIZE = (640, 480)
COLOUR_TOLERANCE = 3.0
FLAT_STD = 3.0
MARK_LUMA = 60.0


# ---- frames ----------------------------------------------------------------------


@dataclass
class BootFrame:
    t: float  # seconds since power-on (host)
    size: tuple  # (width, height)
    kind: str  # distro_capture.classify(): black, content, placeholder, size-change
    mean: float  # mean luma 0..255
    thumb: np.ndarray  # 1/8 scale RGB
    scene: str = ""


def thumb_luma(thumb):
    pixels = thumb.astype(np.float32)
    return pixels[..., 0] * 0.299 + pixels[..., 1] * 0.587 + pixels[..., 2] * 0.114


def corner_colour(thumb):
    """Mean RGB of the four 8x8 corner blocks of a thumbnail."""
    h, w = thumb.shape[:2]
    blocks = [thumb[:8, :8], thumb[:8, w - 8 :], thumb[h - 8 :, :8], thumb[h - 8 :, w - 8 :]]
    return np.mean([b.reshape(-1, 3).mean(axis=0) for b in blocks], axis=0)


def near(colour, reference, tolerance=COLOUR_TOLERANCE):
    return all(abs(float(c) - r) <= tolerance for c, r in zip(colour, reference))


def scene_of(size, mean, thumb, *, cover=COVER_BG, splash=SPLASH_BG):
    """Name the scene of one frame (see the module docstring)."""
    if tuple(size) == PLACEHOLDER_SIZE:
        return "placeholder"
    if tuple(size) != SESSION_SIZE:
        return "size-change"
    luma = thumb_luma(thumb)
    if float(luma.max()) < 12.0 and mean < 3.0:
        return "black"
    corners = corner_colour(thumb)
    if near(corners, splash) and float(luma.max()) >= MARK_LUMA:
        return "splash"
    if float(luma.std()) < FLAT_STD:
        if near(corners, splash):
            return "flat-splash"
        if near(corners, cover):
            return "flat-cover"
        return "flat"
    return "content"


def classify_frames(frames, **colours):
    for frame in frames:
        frame.scene = scene_of(frame.size, frame.mean, frame.thumb, **colours)
    return frames


def scene_runs(frames):
    """[(scene, first_t, last_t, count)] for consecutive frames of the same scene."""
    runs = []
    for frame in frames:
        if runs and runs[-1][0] == frame.scene:
            runs[-1] = (frame.scene, runs[-1][1], frame.t, runs[-1][3] + 1)
        else:
            runs.append((frame.scene, frame.t, frame.t, 1))
    return runs


BAD_SCENES = ("black", "placeholder", "size-change")


def interlude(frames):
    """The frames between the last splash frame and the first content frame after it.

    Returns None when no splash frame is followed by a content frame (the recording did not
    see the handover). Otherwise a dict:

      splash_last        time of the last splash frame
      content_first      time of the first content frame after it
      entries            [(t, scene, seconds)] each frame of the interlude with the time its
                         scene lasted (until the next frame)
      seconds            content_first - first interlude frame (0 when the greeter replaced the
                         splash in one step)
      bad_frames         number of black, placeholder and size-change frames
      bad_seconds        how long those scenes lasted (a lower bound: a flash between two VNC
                         updates is not seen)
      by_scene           {scene: seconds}
    """
    content_at = None
    splash_at = None
    for i, frame in enumerate(frames):
        if frame.scene == "splash":
            splash_at = i
        elif frame.scene == "content" and splash_at is not None:
            content_at = i
            break
    if content_at is None or splash_at is None:
        return None
    between = frames[splash_at + 1 : content_at]
    entries = []
    for index, frame in enumerate(between):
        end = between[index + 1].t if index + 1 < len(between) else frames[content_at].t
        entries.append((frame.t, frame.scene, max(0.0, end - frame.t)))
    by_scene = {}
    for _, scene, seconds in entries:
        by_scene[scene] = by_scene.get(scene, 0.0) + seconds
    bad = [e for e in entries if e[1] in BAD_SCENES]
    return {
        "splash_last": frames[splash_at].t,
        "content_first": frames[content_at].t,
        "entries": entries,
        "seconds": frames[content_at].t - between[0].t if between else 0.0,
        "bad_frames": len(bad),
        "bad_seconds": sum(e[2] for e in bad),
        "by_scene": by_scene,
    }


def frame_gaps(frames, after, before):
    """Longest time between consecutive frames inside [after, before]: the frame rate floor of the recording."""
    window = [f.t for f in frames if after <= f.t <= before]
    if len(window) < 2:
        return 0.0
    return max(b - a for a, b in zip(window, window[1:]))


# ---- tracer log ------------------------------------------------------------------

TRACE_LINE = re.compile(r"^(\d+\.\d+) (\S+) ?(.*)$")


def parse_trace(text):
    """[(guest_monotonic, kind, value)] from the tracer's log."""
    events = []
    for line in text.splitlines():
        match = TRACE_LINE.match(line.strip())
        if match:
            events.append((float(match.group(1)), match.group(2), match.group(3)))
    return events


def trace_series(events, kind):
    """[(t, value)] for one kind of tracer event."""
    return [(t, value) for t, k, value in events if k == kind]


def master_holder(clients):
    """The `command:pid` that has the master flag in a tracer `clients` value, or None."""
    for entry in clients.split():
        parts = entry.rsplit(":", 2)
        if len(parts) == 3 and parts[2] == "M":
            return f"{parts[0]}:{parts[1]}"
    return None


def master_timeline(events, card):
    """[(t, holder or None)] each time the master (the client with the master flag) changes on `card`."""
    timeline = []
    for t, value in trace_series(events, f"clients-{card}"):
        holder = master_holder(value)
        if not timeline or timeline[-1][1] != holder:
            timeline.append((t, holder))
    return timeline


def first_time(series, predicate, after=None):
    for t, value in series:
        if (after is None or t >= after) and predicate(value):
            return t
    return None


def summarise_trace(events, card="1"):
    """What the tracer saw of the display handover, in guest monotonic seconds.

    splash_master_lost  Plymouth stopped being DRM master (deactivate or quit)
    splash_gone         plymouthd no longer exists (its pid file is gone)
    next_master         (t, holder) of the first master after Plymouth
    crtc_disabled       [(t, crtc)] a CRTC that was enabled went to enable=0 after the splash
    scanout_off         [(t, plane)] a plane that scanned something went off after the splash
    fbcon_planes        [(t, plane)] a plane whose framebuffer belongs to fbcon after the splash
    modes               distinct "enable/active/mode" strings per CRTC, in order
    bsm_first_flip      first time a plane is scanned out of a framebuffer allocated by bsm
    vt_modes            [(t, mode)] of the VT (process, auto)
    """
    masters = master_timeline(events, card)
    lost = None
    holder_after = None
    for index, (t, holder) in enumerate(masters):
        if holder and holder.startswith("plymouthd"):
            for t2, h2 in masters[index + 1 :]:
                if h2 != holder:
                    lost = t2
                    break
            break
    if lost is not None:
        holder_after = next(((t, h) for t, h in masters if t >= lost and h and not h.startswith("plymouthd")), None)
    splash_start = next((t for t, h in masters if h and h.startswith("plymouthd")), None)
    gone = first_time(trace_series(events, "plymouth"), lambda v: v == "no", after=splash_start)
    crtc_disabled, scanout_off, fbcon_planes, modes = [], [], [], {}
    after = lost if lost is not None else splash_start
    for t, kind, value in events:
        if kind.startswith("crtc-"):
            name = kind[5:]
            history = modes.setdefault(name, [])
            if not history or history[-1] != value:
                history.append(value)
            if after is not None and t >= after and "en=0" in value:
                crtc_disabled.append((t, name))
        elif kind.startswith("plane-") and after is not None and t >= after:
            if value == "off":
                scanout_off.append((t, kind[6:]))
            elif "by=[fbcon]" in value:
                fbcon_planes.append((t, kind[6:]))
    bsm_first = next((t for t, k, v in events if k.startswith("plane-") and "by=bsm" in v), None)
    return {
        "splash_start": splash_start,
        "splash_master_lost": lost,
        "splash_gone": gone,
        "next_master": holder_after,
        "masters": masters,
        "crtc_disabled": crtc_disabled,
        "scanout_off": scanout_off,
        "fbcon_planes": fbcon_planes,
        "modes": modes,
        "bsm_first_flip": bsm_first,
        "vt_modes": trace_series(events, "vtmode"),
    }


# ---- kernel log ------------------------------------------------------------------

DMESG_LINE = re.compile(r"^\[\s*(\d+\.\d+)\]\s?(.*)$")
MARK = re.compile(r"bsm-boot-trace: mono=(\d+\.\d+)")


def parse_dmesg(text):
    """[(kernel_seconds, line)] from `dmesg --time-format=raw`."""
    rows = []
    for line in text.splitlines():
        match = DMESG_LINE.match(line)
        if match:
            rows.append((float(match.group(1)), match.group(2)))
    return rows


def kernel_to_monotonic(rows):
    """Seconds to add to a kernel (printk) time to get CLOCK_MONOTONIC, from the tracer's kmsg marks; None without any."""
    offsets = []
    for stamp, line in rows:
        match = MARK.search(line)
        if match:
            offsets.append(float(match.group(1)) - stamp)
    if not offsets:
        return None
    offsets.sort()
    return offsets[len(offsets) // 2]


DRM_PATTERNS = [
    ("crtc-disable", re.compile(r"drm_atomic_helper_commit_crtc_disable\]\s+disabling \[CRTC:(\d+):(\S+?)\]")),
    ("crtc-enable", re.compile(r"drm_atomic_helper_commit_crtc_enable\]\s+enabling \[CRTC:(\d+):(\S+?)\]")),
    ("crtc-setmode", re.compile(r"drm_atomic_helper_commit_crtc_set_mode\]\s+modeset on \[CRTC:(\d+):(\S+?)\]")),
    ("setcrtc", re.compile(r"drm_mode_setcrtc\]\s+\[CRTC:(\d+):(\S+?)\]")),
    ("set-master", re.compile(r'comm="(\S+)" pid=(\d+), dev=\S+, auth=\d, DRM_IOCTL_SET_MASTER')),
    ("drop-master", re.compile(r'comm="(\S+)" pid=(\d+), dev=\S+, auth=\d, DRM_IOCTL_DROP_MASTER')),
    ("open", re.compile(r'drm_open_helper\] comm="(\S+)", pid=(\d+)')),
    ("close", re.compile(r'drm_file_free\] comm="(\S+)", pid=(\d+)')),
    ("fbdev-restore", re.compile(r"drm_client_dev_restore\] fbdev")),
]
MODESET_KINDS = ("crtc-disable", "crtc-enable", "crtc-setmode")


def drm_events(rows):
    """[(kernel_seconds, kind, detail)] for the modeset, master and open/close lines of the DRM debug log."""
    events = []
    for stamp, line in rows:
        for kind, pattern in DRM_PATTERNS:
            match = pattern.search(line)
            if match:
                events.append((stamp, kind, " ".join(match.groups())))
                break
    return events


def modesets_after_splash(events, margin=0.5):
    """Modesets (CRTC disable, enable, set mode) that came after the splash's own.

    The splash sets its mode with the legacy SETCRTC ioctl when it starts. Everything the
    kernel logs as a CRTC disable, enable or set mode more than `margin` seconds after the
    last SETCRTC is a modeset of the handover or later. Returns (splash_setcrtc_time or None,
    [(t, kind, detail)]).
    """
    setcrtcs = [t for t, kind, _ in events if kind == "setcrtc"]
    cutoff = (max(setcrtcs) + margin) if setcrtcs else None
    later = [e for e in events if e[1] in MODESET_KINDS and (cutoff is None or e[0] > cutoff)]
    return (max(setcrtcs) if setcrtcs else None), later


def fbdev_restores_after(events, after):
    return [e for e in events if e[1] == "fbdev-restore" and e[0] >= after]


def journal_rows(text):
    """[(seconds, line)] from `journalctl -o short-monotonic`."""
    rows = []
    for line in text.splitlines():
        match = re.match(r"^\[\s*(\d+\.\d+)\]\s+(.*)$", line)
        if match:
            rows.append((float(match.group(1)), match.group(2)))
    return rows


def first_row(rows, needle, after=0.0):
    """The first (seconds, line) whose line contains `needle`, at or after `after`."""
    for t, line in rows:
        if t >= after and needle in line:
            return t, line
    return None
