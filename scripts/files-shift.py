#!/usr/bin/env python3
"""Find every layout shift when the Files app switches, opens or closes a tab.

Runs `bingux-files` in one private headless sway session (the session helper of
the bingux-settings-session-harness skill, /var/tmp/InspectorHarness/session.sh)
on a fixture HOME, performs each tab action with real keys and pointer input
while `wf-recorder` records every frame the compositor showed (lossless), and
compares consecutive frames by structure, not by pixels:

  * the toolbar buttons and the crumb bar (its edges and the crumbs' x
    positions),
  * every sidebar row (icon and text bands),
  * the list's column header (Name, Size, Modified: bands and x extents),
  * the first rows of the list (y of each row, so the row height),
  * the status pill (right and bottom edges, height),
  * the hairlines between the parts,
  * the tab strip (reported on its own: it animates by design).

The contract (docs/files.md, "Tabs"): switching to a tab or opening one swaps
the file view's state, never the window around it. So between the frame before
the action and the settled frame after it, none of the persistent elements may
move, in any frame, by any amount. This script lists each one that does: the
action, the element, how far in px, and in which frame after the action (frame
0 is the first frame that differs from the one before the action). It also
lists the frames in which the list has no header or only some of its rows.

    python3 scripts/files-shift.py                          # all scenarios
    python3 scripts/files-shift.py --scenarios new-tab,switch-key
    python3 scripts/files-shift.py --binary /path/to/bingux-files --out OUT

Output: OUT/report.md (default /tmp/FilesShift), OUT/<scenario>.png (a strip of
the frames after the action, shifted elements boxed in red) and OUT/shifts.json.
Exit status 1 when any persistent element shifted. Build first:
`cargo build` in apps/files with CARGO_TARGET_DIR set as usual.
Needs sway, grim-free tools (wf-recorder, ffmpeg, wtype), PIL and numpy; one
session runs, and it is stopped at the end.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
SESSION_SH = Path("/var/tmp/InspectorHarness/session.sh")
SESSIONS = SESSION_SH.parent / "sessions"
DEFAULT_BINARY = (
    Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))
    / "debug"
    / "bingux-files"
)
W, H = 1400, 900
BG = 250  # the list's background luminance
INK = 40  # luminance distance from the background that is ink
PARK = (700, 870)  # where the pointer rests: empty list space

Box = tuple[int, int, int, int]  # x0, y0, x1, y1 (exclusive)

# ---- fixture ----------------------------------------------------------------------


def make_home(home: Path, big: Path | None) -> None:
    """A HOME with folders of different sizes and a 20,000 file folder."""
    for name in ("Desktop", "Documents", "Downloads", "Music", "Pictures", "Projects"):
        (home / name).mkdir(parents=True, exist_ok=True)
    for sub in (".local/state", ".cache", ".config"):
        (home / sub).mkdir(parents=True, exist_ok=True)
    documents = home / "Documents"
    if not (documents / "Notes.txt").exists():
        for name in ("Notes", "Budget", "Report", "Todo", "Letter", "Draft", "Plan", "Minutes"):
            (documents / f"{name}.txt").write_text(name)
        for name in ("Budget", "Forecast"):
            (documents / f"{name}.xlsx").write_text("x")
        (documents / "Thesis.pdf").write_text("x")
        (documents / "Archive").mkdir(exist_ok=True)
        (documents / "Invoices").mkdir(exist_ok=True)
    downloads = home / "Downloads"
    if not (downloads / "download-1.pdf").exists():
        for i in range(1, 61):
            ext = ("zip", "pdf", "txt", "iso")[i % 4]
            (downloads / f"download-{i}.{ext}").write_text(str(i))
    projects = home / "Projects"
    if not (projects / "note-1.md").exists():
        for name in ("alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"):
            (projects / name).mkdir(exist_ok=True)
            for i in range(1, 7):
                (projects / name / f"file-{i}.rs").write_text(str(i))
        for i in range(1, 151):
            (projects / f"note-{i}.md").write_text(str(i))
    if not any((home / "Pictures").iterdir()):
        subprocess.run(
            [sys.executable, str(REPO / "scripts" / "files-fixture.py"), str(home / "_fx"), "10"],
            check=True,
            capture_output=True,
        )
        for png in (home / "_fx" / "Pictures").glob("*.png"):
            os.link(png, home / "Pictures" / png.name)
        shutil.rmtree(home / "_fx")
    if not (home / "Big").exists():
        if big is not None and big.is_dir():
            (home / "Big").symlink_to(big)
        else:
            subprocess.run([sys.executable, str(REPO / "scripts" / "files-fixture.py"), str(home)], check=True)


def app_env(home: Path) -> dict[str, str]:
    env = isolated()  # the sway session sets its own display; no write reaches the person's dconf
    env.update(
        HOME=str(home),
        XDG_STATE_HOME=str(home / ".local/state"),
        XDG_CACHE_HOME=str(home / ".cache"),
        XDG_CONFIG_HOME=str(home / ".config"),
        XDG_DATA_HOME=str(home / ".local/share"),
        BINGUX_FILES_STATE=str(home / ".local/state/bingux-files-view.json"),
    )
    return env


# ---- session ----------------------------------------------------------------------


class Session:
    def __init__(self, name: str, binary: Path, home: Path) -> None:
        self.name = name
        self.dir = SESSIONS / name
        self.env = app_env(home)
        self.env.update(BIN=str(binary), INSPECT="0")
        self.home = home
        self.recorder: subprocess.Popen | None = None
        self.tabs = 1  # how many tabs are open
        self.tab_x: list[int] = []  # where each tab can be pressed, left to right

    def start(self, address: Path) -> None:
        subprocess.run([str(SESSION_SH), "stop", self.name], capture_output=True)
        subprocess.run([str(SESSION_SH), "start", self.name, str(address)], env=self.env, check=True)
        for line in (self.dir / "env").read_text().splitlines():
            key, _, value = line.partition("=")
            self.env[key] = value
        time.sleep(2.0)
        self.pointer(f"move {PARK[0]} {PARK[1]}")

    def stop(self) -> None:
        subprocess.run([str(SESSION_SH), "stop", self.name], capture_output=True)

    # input
    def pointer(self, *commands: str) -> None:
        with open(self.dir / "input", "w") as fifo:
            for command in commands:
                fifo.write(command + "\n")

    def keys(self, *args: str) -> None:
        subprocess.run(["wtype", "-s", "250", *args], env=self.env, check=False)

    def ctrl(self, key: str, shift: bool = False) -> None:
        mods = ["-M", "ctrl"] + (["-M", "shift"] if shift else [])
        unmods = (["-m", "shift"] if shift else []) + ["-m", "ctrl"]
        self.keys(*mods, "-k", key, *unmods)

    def go(self, path: str) -> None:
        """Type a path in the address bar and enter it."""
        self.ctrl("l")
        time.sleep(0.3)
        self.keys("-d", "15", path)
        self.keys("-k", "Return")

    def shot(self, path: Path) -> None:
        subprocess.run(["grim", str(path)], env=self.env, check=True)

    # recording
    def record_start(self, path: Path) -> None:
        self.recorder = subprocess.Popen(
            ["wf-recorder", "-f", str(path), "-c", "ffv1", "-x", "bgr0"],
            env=self.env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(1.2)

    def record_stop(self) -> None:
        if self.recorder is not None:
            recorder, self.recorder = self.recorder, None
            # SIGTERM: the container is readable without its trailer, and
            # wf-recorder does not always leave on SIGINT when it has no tty.
            recorder.send_signal(signal.SIGTERM)
            try:
                recorder.wait(timeout=10)
            except subprocess.TimeoutExpired:
                recorder.kill()
                recorder.wait()


# ---- frames -----------------------------------------------------------------------


@dataclass
class Frame:
    index: int
    t_ms: int
    rgb: np.ndarray
    gray: np.ndarray = field(init=False, repr=False)

    def __post_init__(self) -> None:
        self.gray = (
            self.rgb[:, :, 0] * 0.299 + self.rgb[:, :, 1] * 0.587 + self.rgb[:, :, 2] * 0.114
        ).astype(np.int16)


def decode(recording: Path) -> list[Frame]:
    """Every frame of the recording (the compositor only emits changed frames)."""
    probe = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v", "-show_entries", "packet=pts_time", "-of", "csv=p=0", str(recording)],
        capture_output=True,
        text=True,
        check=True,
    )
    times = [float(line) for line in probe.stdout.split() if line.strip()]
    proc = subprocess.Popen(
        ["ffmpeg", "-v", "error", "-i", str(recording), "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
        stdout=subprocess.PIPE,
    )
    frames: list[Frame] = []
    size = W * H * 3
    while True:
        data = proc.stdout.read(size)
        if len(data) < size:
            break
        index = len(frames)
        t = times[index] if index < len(times) else 0.0
        rgb = np.frombuffer(data, dtype=np.uint8).reshape(H, W, 3)
        frames.append(Frame(index, round(t * 1000), rgb))
    proc.wait()
    return frames


# ---- structure --------------------------------------------------------------------


def runs_1d(mask: np.ndarray, gap: int = 0) -> list[tuple[int, int]]:
    """Runs of True in a 1-D mask, merged across gaps of at most `gap`."""
    idx = np.flatnonzero(mask)
    if idx.size == 0:
        return []
    out: list[tuple[int, int]] = []
    start = prev = int(idx[0])
    for i in idx[1:]:
        i = int(i)
        if i - prev > gap + 1:
            out.append((start, prev + 1))
            start = i
        prev = i
    out.append((start, prev + 1))
    return out


def ink_boxes(gray: np.ndarray, region: Box, row_gap: int, col_gap: int, background: int = BG, delta: int = INK) -> list[Box]:
    """Ink in `region`, as bands along y, each split into runs along x."""
    x0, y0, x1, y1 = region
    sub = gray[y0:y1, x0:x1]
    ink = np.abs(sub - background) > delta
    boxes: list[Box] = []
    for ry0, ry1 in runs_1d(ink.any(axis=1), row_gap):
        for cx0, cx1 in runs_1d(ink[ry0:ry1].any(axis=0), col_gap):
            boxes.append((x0 + cx0, y0 + ry0, x0 + cx1, y0 + ry1))
    return boxes


def hairlines(gray: np.ndarray) -> dict[str, list[Box]]:
    """The one-pixel rules: under the tab strip, under the toolbar, beside the sidebar."""
    out: dict[str, list[Box]] = {"rule.toolbar": [], "rule.sidebar": []}
    for y in range(40, 140):
        row = gray[y, 300:1100]
        if np.mean(np.abs(row - 225) <= 3) > 0.9 and np.mean(np.abs(gray[y - 1, 300:1100] - 225) <= 3) < 0.5:
            out["rule.toolbar"].append((0, y, W, y + 1))
    for x in range(150, 260):
        col = gray[300:800, x]
        if np.mean(np.abs(col - 225) <= 3) > 0.9:
            out["rule.sidebar"].append((x, 100, x + 1, H))
    return out


def status_pill(gray: np.ndarray) -> list[Box]:
    """The dark status pill: rows with a solid dark run of 30 px or more."""
    region = gray[700:H, 900:W]
    dark = region < 70
    rows = []
    for y in range(dark.shape[0]):
        for a, b in runs_1d(dark[y]):
            if b - a >= 30:
                rows.append((y, a, b))
    if not rows:
        return []
    ys = [r[0] for r in rows]
    box = (900 + min(r[1] for r in rows), 700 + min(ys), 900 + max(r[2] for r in rows), 700 + max(ys) + 1)
    # The pill is pinned to the corner: dark ink anywhere else is not it.
    if not (box[3] >= H - 16 and box[2] >= W - 16 and box[3] - box[1] <= 48):
        return []
    # Its width is its text (the tab's); the corner it is pinned to is not.
    return [(box[2] - 1, box[1], box[2], box[3])]


def structure(gray: np.ndarray) -> dict[str, list[Box]]:
    """Named lists of boxes: what is where in one frame."""
    s: dict[str, list[Box]] = {}
    s["toolbar.left"] = ink_boxes(gray, (0, 52, 184, 98), 1, 6)
    s["toolbar.right"] = ink_boxes(gray, (1236, 52, W, 98), 1, 6)
    # The crumb bar: the rounded field is a slightly darker fill than the window.
    pill = gray[50:98, 184:1236] < gray[50:98, 184:1236].max() - 3
    pill_rows = np.flatnonzero(pill.mean(axis=1) > 0.5)
    pill_cols = np.flatnonzero(pill.mean(axis=0) > 0.5)
    s["crumb.field"] = (
        [(184 + int(pill_cols[0]), 50 + int(pill_rows[0]), 184 + int(pill_cols[-1]) + 1, 50 + int(pill_rows[-1]) + 1)]
        if pill_rows.size and pill_cols.size
        else []
    )
    s["crumb.items"] = ink_boxes(gray, (190, 58, 1230, 92), 1, 26, background=int(np.median(gray[58:92, 190:1230])))
    s["sidebar.rows"] = ink_boxes(gray, (0, 104, 199, H - 4), 2, 60)
    # The selected place: a soft fill (about 20 under the background).
    sel = (np.abs(gray[104:H, 0:199] - 230) <= 3)
    sel_rows = np.flatnonzero(sel.mean(axis=1) > 0.5)
    s["sidebar.selected"] = [(0, 104 + int(sel_rows[0]), 199, 104 + int(sel_rows[-1]) + 1)] if sel_rows.size else []
    # The Name cell: the others come and go with the sort arrow and the window.
    s["header.cells"] = ink_boxes(gray, (202, 104, W - 4, 138), 1, 24)[:1]
    # The scrollbar thumb (a 5 px bar at the right edge, as tall as its share of the
    # list) is ink in every row's y range: left in, it fuses the rows into one band
    # and a folder with a scrollbar reads as having no rows. The list's own columns
    # end well before it.
    s["list.rows"] = ink_boxes(gray, (202, 140, W - 28, 860), 2, 40)
    s["status.pill"] = status_pill(gray)
    s.update(hairlines(gray))
    s["tabs"] = ink_boxes(gray, (0, 10, 1190, 44), 1, 30, background=235, delta=30)
    return s


# Elements that must not move when a tab is switched or opened, with what an
# element is called in the report.
STABLE = [
    "toolbar.left",
    "toolbar.right",
    "crumb.field",
    "sidebar.rows",
    "header.cells",
    "status.pill",
    "rule.toolbar",
    "rule.sidebar",
]
# Elements whose content may differ between tabs, so only their size and
# position as a whole are compared: the first rows of the list (the row height).
ROW_PITCH = "list.rows"


def compare(a: list[Box], b: list[Box], tolerance: int = 0) -> list[tuple[int, str, Box, Box]]:
    """(px, description, before, after) per box that moved or changed size.

    Boxes are compared by index when there are as many; otherwise by the number
    that appeared or went away (a box that was not there is a shift of its size).
    """
    out: list[tuple[int, str, Box, Box]] = []
    if len(a) != len(b):
        out.append((0, f"count {len(a)} -> {len(b)}", a[0] if a else (0, 0, 0, 0), b[0] if b else (0, 0, 0, 0)))
        return out
    for before, after in zip(a, b):
        if before == after or (tolerance and max(abs(after[i] - before[i]) for i in range(4)) <= tolerance):
            continue
        dx0, dy0, dx1, dy1 = (after[i] - before[i] for i in range(4))
        px = max(abs(dx0), abs(dy0), abs(dx1), abs(dy1))
        parts = []
        if dx0 or dx1:
            parts.append(f"x {dx0:+d}" if dx0 == dx1 else f"x {dx0:+d}/{dx1:+d}")
        if dy0 or dy1:
            parts.append(f"y {dy0:+d}" if dy0 == dy1 else f"y {dy0:+d}/{dy1:+d}")
        out.append((px, ", ".join(parts), before, after))
    return out


def list_state(s: dict[str, list[Box]]) -> str:
    header = len(s["header.cells"])
    rows = s["list.rows"]
    if not header and not rows:
        return "blank"
    if not header:
        return f"no header, {len(rows)} row bands"
    if not rows:
        return "header only"
    return f"header, {len(rows)} row bands"


# ---- scenarios --------------------------------------------------------------------


@dataclass
class Scenario:
    name: str
    description: str
    tabs: list  # (folder, extras) per tab, left to right; extras: "scroll", "grid"
    active: int  # the tab on show before the action
    act: object  # callable(session), recorded
    hover: int | None = None  # a tab to rest the pointer on before the action
    settle: float = 2.2


def tab_centres(frame: Frame) -> list[int]:
    """Where the tabs are on the strip (x of each tab's label)."""
    boxes = ink_boxes(frame.gray, (0, 14, 1190, 46), 1, 22, background=235, delta=30)
    return [(b[0] + min(b[2], b[0] + 80)) // 2 for b in boxes if b[2] - b[0] >= 40]


def park(s: Session) -> None:
    s.pointer(f"move {PARK[0]} {PARK[1]}")
    time.sleep(0.4)


def ensure_tabs(s: Session, spec: list, active: int) -> None:
    """Leave exactly the tabs of `spec` open, each on its folder, `active` on show."""
    while s.tabs > 1:
        s.keys("-M", "alt", "-k", "9", "-m", "alt")
        time.sleep(0.3)
        s.ctrl("w")
        s.tabs -= 1
        time.sleep(0.5)
    s.keys("-M", "alt", "-k", "1", "-m", "alt")
    for index, (folder, extras) in enumerate(spec):
        if index > 0:
            s.ctrl("t")
            s.tabs += 1
            time.sleep(0.8)
        s.go(f"~/{folder}")
        time.sleep(3.0 if folder in ("Big", "Photos") else 1.4)
        if "grid" in extras:
            s.pointer("click 1345 74")
            time.sleep(1.0)
        if "scroll" in extras:
            s.pointer("wheel 700 500 700")
            time.sleep(1.2)
            s.pointer("click 700 330")  # select a row
            time.sleep(0.5)
        park(s)
    s.keys("-M", "alt", "-k", str(active + 1), "-m", "alt")
    time.sleep(1.0)
    park(s)


def build_scenarios() -> list[Scenario]:
    def new_tab(s: Session) -> None:
        s.ctrl("t")
        s.tabs += 1

    def next_tab(s: Session) -> None:
        s.ctrl("Tab")

    def prev_tab(s: Session) -> None:
        s.ctrl("Tab", shift=True)

    def close_tab(s: Session) -> None:
        s.ctrl("w")
        s.tabs -= 1

    def click_tab(index: int):
        def run(s: Session) -> None:
            s.pointer(f"click {s.tab_x[index]} 28")

        return run

    def middle_folder(s: Session) -> None:
        s.pointer("click 280 158 middle")
        s.tabs += 1

    docs, downs, big = ("Documents", ""), ("Downloads", ""), ("Big", "")
    return [
        Scenario("new-tab", "Ctrl+T on Documents", [docs], 0, new_tab),
        Scenario("new-tab-big", "Ctrl+T on the 20,000 file folder", [big], 0, new_tab, settle=4.0),
        Scenario("switch-key", "Ctrl+Tab, Downloads to Documents", [docs, downs], 1, next_tab),
        Scenario("switch-key-prev", "Ctrl+Shift+Tab, Documents to Downloads", [docs, downs], 0, prev_tab),
        Scenario("switch-click", "click the other tab", [docs, downs], 1, click_tab(0), hover=0),
        Scenario("switch-to-big", "Ctrl+Tab, Documents to the 20,000 file tab", [docs, big], 0, next_tab, settle=4.0),
        Scenario("switch-from-big", "Ctrl+Tab, the 20,000 file tab to Documents", [big, docs], 0, next_tab, settle=3.0),
        Scenario(
            "switch-scrolled",
            "to the Big tab, scrolled with a row selected",
            [("Big", "scroll"), docs],
            1,
            prev_tab,
            settle=4.0,
        ),
        Scenario("switch-grid", "to a Pictures tab in grid view", [("Pictures", "grid"), docs], 1, prev_tab, settle=3.0),
        Scenario("middle-click", "middle click a folder", [("Projects", "")], 0, middle_folder),
        Scenario("close-tab", "Ctrl+W", [docs, downs], 1, close_tab),
    ]


# ---- run --------------------------------------------------------------------------


def first_change(frames: list[Frame]) -> int | None:
    """The first frame whose tab strip differs from the first frame's.

    Every tab action changes the strip at once (a tab comes, goes or turns
    active); nothing else does while the pointer rests, so this is the action's
    frame 0, and frames before it are leftovers of the setup.
    """
    base = frames[0].gray[0:46, 0:1190]
    for frame in frames[1:]:
        if np.count_nonzero(np.abs(frame.gray[0:46, 0:1190] - base) > 8) > 150:
            return frame.index
    return None


@dataclass
class Shift:
    scenario: str
    frame: int
    t_ms: int
    element: str
    px: int
    what: str
    before: Box
    after: Box
    transient: bool


def analyse(name: str, frames: list[Frame]) -> dict:
    start = first_change(frames)
    result: dict = {"scenario": name, "frames": len(frames), "shifts": [], "list_states": [], "tabs": []}
    if start is None:
        result["error"] = "no frame changed: the action did nothing"
        return result
    structures = [structure(f.gray) for f in frames]
    before_i = max(0, start - 1)
    settled = len(frames) - 1
    t0 = frames[start].t_ms
    shifts: list[Shift] = []
    for k in range(start, len(frames)):
        prev, cur = structures[k - 1], structures[k]
        for element in STABLE:
            for px, what, a, b in compare(prev[element], cur[element], 1 if element.startswith(("toolbar", "sidebar")) else 0):
                # A move is a shift unless this frame is the last: every element must hold
                # still from the first frame, so the final frame is judged against the
                # frame before the action too.
                shifts.append(
                    Shift(
                        name,
                        k - start,
                        frames[k].t_ms - t0,
                        element,
                        px,
                        what,
                        a,
                        b,
                        transient=False,
                    )
                )
        # Row height: the distance between the first rows.
        for (pa, pb) in ((prev[ROW_PITCH], cur[ROW_PITCH]),):
            if len(pa) >= 3 and len(pb) >= 3:
                pitch_a = pa[2][1] - pa[1][1]
                pitch_b = pb[2][1] - pb[1][1]
                if abs(pitch_a - pitch_b) >= 2:
                    shifts.append(
                        Shift(name, k - start, frames[k].t_ms - t0, "list row pitch", abs(pitch_b - pitch_a), f"{pitch_a} -> {pitch_b} px", pa[1], pb[1], False)
                    )
            if len(pa) >= 3 and len(pb) >= 3 and pa[0][1] != pb[0][1]:
                shifts.append(
                    Shift(name, k - start, frames[k].t_ms - t0, "first list row", abs(pb[0][1] - pa[0][1]), f"y {pb[0][1] - pa[0][1]:+d}", pa[0], pb[0], False)
                )
    # Mark a shift that the settled frame does not keep as a transient (a glitch).
    final = structures[settled]
    pre = structures[before_i]
    for shift in shifts:
        if shift.element in STABLE:
            shift.transient = final[shift.element] == pre[shift.element]
    result["shifts"] = [s.__dict__ for s in shifts]
    result["list_states"] = [(frames[k].index - start, frames[k].t_ms - t0, list_state(structures[k])) for k in range(max(0, start - 1), len(frames))]
    result["tab_boxes"] = [(frames[k].index - start, [b[0] for b in structures[k]["tabs"]]) for k in range(max(0, start - 1), len(frames))]
    result["start"] = start
    return result


def strip(name: str, frames: list[Frame], result: dict, path: Path, limit: int = 10) -> None:
    """The frames around the action, half size, 2 per row, shifted elements boxed."""
    if "start" not in result:
        return
    start = result["start"]
    picks = frames[max(0, start - 1) : start - 1 + limit] if start else frames[:limit]
    by_frame = {}
    for shift in result["shifts"]:
        by_frame.setdefault(shift["frame"], []).append(shift)
    cols = 2
    rows = (len(picks) + cols - 1) // cols
    cell_w, cell_h = W // 2, H // 2 + 16
    sheet = Image.new("RGB", (cell_w * cols, cell_h * rows), (255, 255, 255))
    draw = ImageDraw.Draw(sheet)
    for i, frame in enumerate(picks):
        rel = frame.index - start
        image = Image.fromarray(frame.rgb)
        d = ImageDraw.Draw(image)
        for shift in by_frame.get(rel, []):
            for box, colour in ((shift["before"], (0, 120, 255)), (shift["after"], (220, 0, 0))):
                d.rectangle([box[0] - 1, box[1] - 1, box[2], box[3]], outline=colour, width=2)
        image = image.resize((cell_w, cell_h - 16))
        x, y = (i % cols) * cell_w, (i // cols) * cell_h
        sheet.paste(image, (x, y + 16))
        draw.text((x + 4, y + 2), f"{name}: frame {rel}  t={frame.t_ms - frames[start].t_ms:+d} ms  {result['list_states'][min(i, len(result['list_states']) - 1)][2]}", fill=(0, 0, 0))
    sheet.save(path)


def render(results: list[dict], binary: Path) -> str:
    lines = [f"# Files tab layout shifts", "", f"Binary: `{binary}`", ""]
    total = 0
    for result in results:
        lines.append(f"## {result['scenario']}: {result.get('description', '')}")
        if result.get("error"):
            lines += ["", result["error"], ""]
            continue
        shifts = result["shifts"]
        total += len(shifts)
        lines += ["", f"{result['frames']} frames recorded, action first visible in frame {result['start']}.", ""]
        states = [s for s in result["list_states"] if not s[2].startswith("header, ") or s[0] < 0]
        bad = [s for s in result["list_states"] if s[0] >= 0 and (s[2] == "blank" or s[2].startswith("no header") or s[2] == "header only")]
        if bad:
            lines.append("List not drawn whole: " + "; ".join(f"frame {f} ({t:+d} ms): {text}" for f, t, text in bad))
            lines.append("")
        if shifts:
            lines += ["| frame | t (ms) | element | px | what | kept at the end |", "| ---: | ---: | --- | ---: | --- | --- |"]
            for shift in shifts:
                lines.append(
                    f"| {shift['frame']} | {shift['t_ms']:+d} | {shift['element']} | {shift['px']} | {shift['what']} | {'no (glitch)' if shift['transient'] else 'yes'} |"
                )
        else:
            lines.append("No persistent element moved.")
        lines.append("")
    lines += ["", f"**{total} shifts of persistent elements in {len(results)} scenarios.**", ""]
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--out", type=Path, default=Path("/tmp/FilesShift"))
    parser.add_argument("--home", type=Path, default=Path("/var/tmp/FilesShift-home"))
    parser.add_argument("--big", type=Path, default=None, help="an existing folder of 20,000 files (default: one of the other fixtures' Big/)")
    parser.add_argument("--scenarios", default="", help="comma separated names; default all")
    parser.add_argument("--session", default="FilesShift")
    parser.add_argument("--keep", action="store_true", help="keep the recordings")
    parser.add_argument("--reanalyse", type=Path, default=None, help="analyse the .mkv recordings of an earlier --keep run instead of running")
    args = parser.parse_args()
    if args.reanalyse:
        args.out.mkdir(parents=True, exist_ok=True)
        results = []
        for recording in sorted(args.reanalyse.glob("*.mkv")):
            frames = decode(recording)
            result = analyse(recording.stem, frames)
            result["description"] = recording.stem
            strip(recording.stem, frames, result, args.out / f"{recording.stem}.png")
            results.append(result)
        report = render(results, args.binary)
        (args.out / "report.md").write_text(report)
        (args.out / "shifts.json").write_text(json.dumps(results, indent=1))
        print(report)
        sys.exit(1 if any(r["shifts"] for r in results) else 0)

    big = args.big
    if big is None:
        for candidate in ("/var/tmp/FileTips-home/Big", "/var/tmp/FileScroll-home/Big"):
            if Path(candidate).is_dir():
                big = Path(candidate)
                break
    make_home(args.home, big)
    args.out.mkdir(parents=True, exist_ok=True)
    scenarios = build_scenarios()
    wanted = [n for n in args.scenarios.split(",") if n]
    if wanted:
        scenarios = [s for s in scenarios if s.name in wanted]
    monitors = subprocess.run(["pgrep", "-c", "-f", "gsettings monitor"], capture_output=True, text=True).stdout.strip()

    session = Session(args.session, args.binary, args.home)
    results: list[dict] = []
    try:
        session.start(args.home / "Documents")
        for scenario in scenarios:
            print(f"[files-shift] {scenario.name}", flush=True)
            ensure_tabs(session, scenario.tabs, scenario.active)
            session.shot(args.out / "_strip.png")
            strip_frame = Frame(0, 0, np.array(Image.open(args.out / "_strip.png").convert("RGB")))
            session.tab_x = tab_centres(strip_frame)
            if scenario.hover is not None:
                session.pointer(f"move {session.tab_x[scenario.hover]} 28")
                time.sleep(0.5)
            time.sleep(0.6)
            recording = args.out / f"{scenario.name}.mkv"
            session.record_start(recording)
            scenario.act(session)
            time.sleep(scenario.settle)
            session.record_stop()
            frames = decode(recording)
            result = analyse(scenario.name, frames)
            result["description"] = scenario.description
            strip(scenario.name, frames, result, args.out / f"{scenario.name}.png")
            results.append(result)
            if not args.keep:
                recording.unlink(missing_ok=True)
    finally:
        session.record_stop()
        session.stop()
    after = subprocess.run(["pgrep", "-c", "-f", "gsettings monitor"], capture_output=True, text=True).stdout.strip()
    report = render(results, args.binary)
    (args.out / "report.md").write_text(report)
    (args.out / "shifts.json").write_text(json.dumps(results, indent=1))
    print(report)
    print(f"[files-shift] gsettings monitors before {monitors}, after {after}")
    sys.exit(1 if any(r["shifts"] for r in results) else 0)


if __name__ == "__main__":
    main()
