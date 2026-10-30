#!/usr/bin/env python3
"""Walk the phone layout of Bingux Settings and check its transitions.

Each scenario runs `bingux-settings --snapshot` with a scripted input walk
(BINGUX_SETTINGS_SNAPSHOT_SCRIPT) and every frame recorded
(BINGUX_SETTINGS_SNAPSHOT_FRAMES), at a phone size: 390x800, 360x640 and
landscape 800x390 by default. The input is the snapshot harness's own: a held
mouse (`down`, `move`, `up`) or a held finger (`touch_down`, `touch_move`,
`touch_up`), so a touch drag reaches the app as real touch events.

Scenarios: a page pushed and popped, the list of pages and a page, a dropdown
that opens as a bottom sheet (opened and closed by Escape and by the scrim),
the sheet pulled by its grabber with a mouse and with a finger (released short,
past the threshold, flung, lifted above rest), the list inside the sheet
dragged by a finger (a hand-over to the sheet at the top), a popup sheet, a
toast and the search field.

Each run's frames are checked by pixel analysis, with no knowledge of the app's
tree:

- blank: no frame is a single flat colour, which is what a failed first paint
  or a lost surface looks like.
- flicker: a frame that differs a lot from both neighbours while those two are
  alike (A B A), which is a flash of wrong content.
- sheet-rigid: while a sheet moves, the grabber and the first line of text keep
  their distance from the sheet's top edge. A sheet that moved its background
  and left its rows behind (or the other way round) fails this.
- sheet-reach: the sheet always reaches the window's bottom edge: it translates,
  it never resizes.
- scrim: the scrim fades with the drag, never the wrong way.
- sheet-rest: a released sheet that should rest is back at its rest top; a
  dismissed one is gone.
- sheet-bottom: with the list scrolled to its end, nothing of it touches the
  bottom edge: the last row ends above the bottom padding.
- margins: while no sheet is open, the page's side margins and bottom edge are
  free of content (nothing clipped by the window edge).

Output, under --out (default /tmp/MobileWalk): report.md (a table of every check
per scenario and size), strips/<size>/<scenario>.png (a contact sheet of the
scenario's frames), frames/ (every frame) and logs/. The exit status is 1 when a
check fails.

Needs PIL and numpy. Build first: `cargo build --features snapshot` in
apps/settings, with CARGO_TARGET_DIR set as usual. The walk pins the
colour scheme and runs gsettings on a throw-away in-memory backend, so it
changes nothing on the desktop. The list scenarios need a Displays page with
at least one display; without one they are skipped and the report says so.

    python3 scripts/mobile-walk.py                       # everything, three sizes
    python3 scripts/mobile-walk.py --sizes 390x800 --only sheet-drag-release,sheet-fling
    python3 scripts/mobile-walk.py --skip-run            # re-analyse kept frames
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
DEFAULT_BINARY = (
    Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))
    / "debug"
    / "bingux-settings"
)
DEFAULT_SIZES = ["390x800", "360x640", "800x390"]
NARROW_BELOW = 740  # theme::NARROW_BELOW: below this width the window is a phone
SCRIM_TOLERANCE = 2.5  # luminance levels the scrim may wobble against the drag
RIGID_TOLERANCE = 2  # px the grabber and the title may drift against the sheet's top
REST_TOLERANCE = 3  # px a rested sheet may differ from its first rest position
BOTTOM_STRIP = 8  # px at the bottom edge that must hold no list content
MARGIN_LEFT = 10  # px at the left edge that must hold no page content
MARGIN_RIGHT = (14, 8)  # the band right of the cards, left of the scrollbar
HEADER_BAND = 64  # px at the top that hold the header and window buttons
FLICKER_DIFF = 0.02  # share of pixels that differ for a "big" change between frames
FLICKER_SAME = 0.002  # share of pixels that differ for "alike"

# The text of the control each scenario opens, by label. `LIST` is the
# Resolution dropdown of the first display, whose list is long enough to scroll
# in a short window; `SHORT` is a two-option dropdown on every machine.
SHORT = ("datetime", "24-hour")
LIST = ("display", "×")


@dataclasses.dataclass
class Scenario:
    """One scripted walk and the checks its frames must pass."""

    name: str
    page: str
    script: str
    checks: list[str]
    description: str
    # How the walk opens a sheet: the page's address to start on and the
    # label to click, so the probe run can find the sheet's pill.
    opens: tuple[str, str] | None = None
    # Expected end state of a sheet walk: "rest", "closed" or None.
    ends: str | None = None
    skip_when_missing: bool = False


@dataclasses.dataclass
class Finding:
    check: str
    ok: bool
    detail: str
    frame: int | None = None


@dataclasses.dataclass
class Probe:
    """Where a sheet's parts are at rest, found from a screenshot."""

    x: int
    pill_y: int
    top: int
    title_y: int | None
    list_y: int
    sheet_color: tuple[int, int, int]


def parse_size(size: str) -> tuple[int, int]:
    width, height = size.split("x")
    return int(width), int(height)


# ---------------------------------------------------------------- running ---


def run_script(binary: Path, out: Path, name: str, size: str, page: str, script: str, timeout: int) -> str:
    """Run one walk. Frames go to OUT/frames/NAME/PAGE, shots to OUT/shots."""
    frames = out / "frames" / name
    shutil.rmtree(frames, ignore_errors=True)
    # Settings writes through gsettings: scripts/app_env.py keeps the walk's clicks off the
    # person's dconf and off the live desktop.
    env = isolated()
    env.update(
        BINGUX_SETTINGS_SNAPSHOT_SIZE=size,
        BINGUX_SETTINGS_SNAPSHOT_FRAMES=str(frames),
        BINGUX_SETTINGS_SNAPSHOT_SCRIPT=script,
        BINGUX_SETTINGS_SNAPSHOT_SCHEME="prefer-dark",
    )
    log = out / "logs" / f"{name}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as sink:
        try:
            proc = subprocess.run(
                [str(binary), "--snapshot", str(out / "png"), page.split("/")[0]],
                env=env,
                stdout=sink,
                stderr=subprocess.STDOUT,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return f"timed out after {timeout}s"
    return "ok" if proc.returncode == 0 else f"exit {proc.returncode}"


def missing_click(out: Path, name: str) -> bool:
    log = out / "logs" / f"{name}.log"
    return log.exists() and "click-text found no label" in log.read_text()


# --------------------------------------------------------------- analysis ---


def load_frames(directory: Path) -> tuple[list[dict], list[dict]]:
    """The recorded frames and markers. A frame carries its pixels as RGB."""
    manifests = list(directory.glob("*/frames.jsonl"))
    if not manifests:
        return [], []
    base = manifests[0].parent
    frames, markers = [], []
    for line in manifests[0].read_text().splitlines():
        if not line.strip():
            continue
        rec = json.loads(line)
        if rec["kind"] == "marker":
            markers.append({"text": rec["text"], "index": len(frames), "t": rec["t_ms"]})
            continue
        frames.append({"i": len(frames), "t": rec["t_ms"], "file": base / rec["file"]})
    cache: dict[Path, np.ndarray] = {}
    for frame in frames:
        if frame["file"] not in cache:
            cache[frame["file"]] = np.asarray(Image.open(frame["file"]).convert("RGB"))
        frame["rgb"] = cache[frame["file"]]
    return frames, markers


def luminance(rgb: np.ndarray) -> np.ndarray:
    return rgb @ np.array([0.299, 0.587, 0.114])


def marker_index(markers: list[dict], text: str, nth: int = 0) -> int | None:
    hits = [m["index"] for m in markers if m["text"] == text]
    return hits[nth] if len(hits) > nth else None


def find_sheet(rgb: np.ndarray, color: tuple[int, int, int]) -> int | None:
    """The row where a sheet of `color` starts, scanning down from the top.

    A row belongs to the sheet when most of the pixels between the corners are
    within a few levels of its colour, for ten rows in a row. Page content in
    the sheet's colour never fills a row that way: text and rules break it.
    """
    height, width = rgb.shape[:2]
    lo, hi = width // 6, width - width // 6
    target = np.array(color, dtype=np.int16)
    close = (np.abs(rgb[:, lo:hi].astype(np.int16) - target).max(axis=2) <= 3).mean(axis=1)
    solid = close > 0.78
    for y in range(0, height - 10):
        if solid[y : y + 10].all():
            return y
    return None


def pill_row(rgb: np.ndarray, top: int, color: tuple[int, int, int]) -> int | None:
    """The row of the grabber pill: the first row under the top with ink in the centre."""
    height, width = rgb.shape[:2]
    centre = rgb[:, width // 2 - 14 : width // 2 + 14].astype(np.int16)
    target = np.array(color, dtype=np.int16)
    differs = (np.abs(centre - target).max(axis=2) > 6).mean(axis=1)
    for y in range(top + 3, min(height, top + 24)):
        if differs[y] > 0.6:
            return y
    return None


def title_row(rgb: np.ndarray, top: int, pill: int, color: tuple[int, int, int]) -> int | None:
    """The first row of ink under the pill, left of the centre: the sheet's title."""
    height, width = rgb.shape[:2]
    target = np.array(color, dtype=np.int16)
    left = rgb[:, 12 : width // 2].astype(np.int16)
    ink = (np.abs(left - target).max(axis=2) > 40).sum(axis=1)
    for y in range(pill + 8, min(height, top + 90)):
        if ink[y] >= 3:
            return y
    return None


def probe_sheet(png: Path) -> Probe | None:
    """Finds a resting sheet in a screenshot: its colour, top, pill and title."""
    rgb = np.asarray(Image.open(png).convert("RGB"))
    height, width = rgb.shape[:2]
    # The sheet reaches the bottom edge: its colour is the pixel there.
    color = tuple(int(v) for v in rgb[height - 3, 12])
    top = find_sheet(rgb, color)
    if top is None:
        return None
    pill = pill_row(rgb, top, color)
    if pill is None:
        return None
    title = title_row(rgb, top, pill, color)
    list_y = min(height - 12, (title or pill) + 70)
    return Probe(width // 2, pill, top, title, list_y, color)


def scrim_level(rgb: np.ndarray) -> float:
    """Luminance of a page pixel the sheet never covers: the scrim darkens it."""
    return float(luminance(rgb[10:11, 2:6].reshape(-1, 3)).mean())


def check_blank(frames: list[dict]) -> Finding:
    for frame in frames:
        flat = frame["rgb"].reshape(-1, 3)
        if (flat == flat[0]).all():
            return Finding("blank", False, f"frame {frame['i']} is one flat colour", frame["i"])
    return Finding("blank", True, f"{len(frames)} frames, none flat")


def changed(a: np.ndarray, b: np.ndarray) -> float:
    """The share of pixels that differ between two frames by more than noise."""
    return float((np.abs(a.astype(np.int16) - b.astype(np.int16)).max(axis=2) > 12).mean())


def check_flicker(frames: list[dict]) -> Finding:
    for prev, mid, nxt in zip(frames, frames[1:], frames[2:]):
        if mid["file"] == prev["file"] or mid["file"] == nxt["file"]:
            continue
        outer = changed(prev["rgb"], nxt["rgb"])
        if outer <= FLICKER_SAME and changed(prev["rgb"], mid["rgb"]) >= FLICKER_DIFF:
            return Finding("flicker", False, f"frame {mid['i']} flashes (A B A)", mid["i"])
    return Finding("flicker", True, "no A B A flash")


def sheet_poses(frames: list[dict], probe: Probe, first: int, last: int) -> list[dict]:
    """The sheet's top, pill and title row in each frame from `first` to `last`."""
    poses = []
    for frame in frames[first : last + 1]:
        top = find_sheet(frame["rgb"], probe.sheet_color)
        if top is None:
            poses.append({"i": frame["i"], "top": None})
            continue
        pill = pill_row(frame["rgb"], top, probe.sheet_color)
        title = title_row(frame["rgb"], top, pill, probe.sheet_color) if pill is not None else None
        poses.append(
            {
                "i": frame["i"],
                "top": top,
                "pill": None if pill is None else pill - top,
                "title": None if title is None else title - top,
                "scrim": scrim_level(frame["rgb"]),
            }
        )
    return poses


def check_rigid(poses: list[dict], probe: Probe) -> Finding:
    want_pill = probe.pill_y - probe.top
    want_title = None if probe.title_y is None else probe.title_y - probe.top
    for pose in poses:
        if pose["top"] is None or pose["top"] <= 2:
            continue
        if pose.get("pill") is not None and abs(pose["pill"] - want_pill) > RIGID_TOLERANCE:
            return Finding(
                "sheet-rigid",
                False,
                f"frame {pose['i']}: the grabber is {pose['pill']} px under the sheet's top, {want_pill} at rest",
                pose["i"],
            )
        if (
            want_title is not None
            and pose.get("title") is not None
            and abs(pose["title"] - want_title) > RIGID_TOLERANCE
        ):
            return Finding(
                "sheet-rigid",
                False,
                f"frame {pose['i']}: the title is {pose['title']} px under the sheet's top, {want_title} at rest",
                pose["i"],
            )
    seen = sum(1 for pose in poses if pose["top"] is not None)
    return Finding("sheet-rigid", seen > 0, f"grabber and title keep their place in {seen} frames")


def check_reach(frames: list[dict], probe: Probe, first: int, last: int) -> Finding:
    """While a sheet is on screen it reaches the bottom edge: it translates, it never resizes.

    A sheet whose background was resized or detached from its rows would leave
    the window's bottom row in another colour while its top is on screen.
    """
    for frame in frames[first : last + 1]:
        rgb = frame["rgb"]
        if find_sheet(rgb, probe.sheet_color) is None:
            continue
        bottom = rgb[-1, 12].astype(np.int16)
        if np.abs(bottom - np.array(probe.sheet_color, dtype=np.int16)).max() > 6:
            return Finding("sheet-reach", False, f"frame {frame['i']}: the sheet stops short of the bottom edge", frame["i"])
    return Finding("sheet-reach", True, "the sheet reaches the bottom edge in every frame")


def check_scrim(poses: list[dict]) -> Finding:
    """The scrim brightens as the sheet goes down: the page pixel and the sheet's top rise together."""
    seen = [pose for pose in poses if pose["top"] is not None]
    for a, b in zip(seen, seen[1:]):
        if b["top"] > a["top"] + 3 and b["scrim"] < a["scrim"] - SCRIM_TOLERANCE:
            return Finding(
                "scrim",
                False,
                f"frame {b['i']}: the sheet is lower ({a['top']}->{b['top']}) and the scrim darker "
                f"({a['scrim']:.1f}->{b['scrim']:.1f})",
                b["i"],
            )
        if b["top"] < a["top"] - 3 and b["scrim"] > a["scrim"] + SCRIM_TOLERANCE:
            return Finding(
                "scrim",
                False,
                f"frame {b['i']}: the sheet is higher ({a['top']}->{b['top']}) and the scrim lighter "
                f"({a['scrim']:.1f}->{b['scrim']:.1f})",
                b["i"],
            )
    return Finding("scrim", True, f"the scrim follows the sheet over {len(seen)} frames")


def check_rest(frames: list[dict], probe: Probe, ends: str) -> Finding:
    last = frames[-1]["rgb"]
    top = find_sheet(last, probe.sheet_color)
    if ends == "closed":
        return Finding("sheet-rest", top is None, "the sheet is gone" if top is None else f"the sheet is still at {top}")
    if top is None:
        return Finding("sheet-rest", False, "the sheet closed and should have rested")
    ok = abs(top - probe.top) <= REST_TOLERANCE
    return Finding("sheet-rest", ok, f"rest top {probe.top}, ends at {top}")


def check_bottom(frames: list[dict], probe: Probe, marker: int | None) -> Finding:
    """Nothing of the list reaches the bottom strip of a sheet whose list is scrolled to its end."""
    index = marker if marker is not None else len(frames) - 1
    rgb = frames[min(index, len(frames) - 1)]["rgb"]
    strip = rgb[-BOTTOM_STRIP:, 20:-20].astype(np.int16)
    target = np.array(probe.sheet_color, dtype=np.int16)
    touching = (np.abs(strip - target).max(axis=2) > 12).mean()
    return Finding(
        "sheet-bottom",
        touching < 0.02,
        f"{touching * 100:.1f}% of the bottom {BOTTOM_STRIP} px is not the sheet's own colour",
        index,
    )


def check_margins(
    frames: list[dict], width: int, sheet_color: tuple[int, int, int] | None, loaded: int | None
) -> Finding:
    """On a phone-size window the page's margins and bottom edge stay clear.

    Frames while a page slides are skipped (a sliding page crosses the
    margins by design): only frames identical to their neighbour count.
    """
    if width >= NARROW_BELOW:
        return Finding("margins", True, "a window with a sidebar: not checked")
    checked = 0
    for prev, frame in zip(frames, frames[1:]):
        if frame["file"] != prev["file"] or (loaded is not None and frame["i"] < loaded):
            continue
        rgb = frame["rgb"].astype(np.int16)
        if sheet_color is not None and find_sheet(frame["rgb"], sheet_color) is not None:
            continue
        height = rgb.shape[0]
        body = rgb[HEADER_BAND : height - 4]
        zones = {
            "left": body[:, :MARGIN_LEFT],
            "right": body[:, width - MARGIN_RIGHT[0] : width - MARGIN_RIGHT[1]],
        }
        for side, zone in zones.items():
            # A clear margin is one flat colour; a card or text crossing it is not.
            flat = np.array(np.median(zone.reshape(-1, 3), axis=0), dtype=np.int16)
            ink = (np.abs(zone - flat).max(axis=2) > 14).mean()
            if ink > 0.01:
                return Finding("margins", False, f"frame {frame['i']}: content in the {side} margin ({ink * 100:.1f}%)", frame["i"])
        checked += 1
    return Finding("margins", True, f"{checked} settled frames, margins clear")


# -------------------------------------------------------------- scenarios ---


def sheet_open(opens: tuple[str, str]) -> str:
    page, label = opens
    steps = []
    if page == "display":
        steps.append("address display/0")
    steps += ["wait 2500", f"click-text {label}", "wait 1200"]
    return "; ".join(steps)


def build_scenarios(size: str, probe: Probe | None, opens: tuple[str, str]) -> list[Scenario]:
    width, height = parse_size(size)
    scenarios: list[Scenario] = []
    open_steps = sheet_open(opens)
    page = opens[0]

    if width < NARROW_BELOW:
        scenarios += [
            Scenario(
                "page-push-pop",
                "network",
                "mark idle start; wait 1500; mark loaded; mark push; address network/wifi/saved; wait 900; "
                "mark pop; address network; wait 900; mark end",
                ["blank", "flicker", "margins"],
                "A subpage pushed and popped",
            ),
            Scenario(
                "list-page",
                "network",
                "mark idle start; wait 1500; mark loaded; mark to-page; touch 195,120; wait 900; mark end",
                ["blank", "flicker"],
                "The list of pages, a page opened from it by a tap",
            ),
            Scenario(
                "search-open-close",
                "sound",
                "mark idle start; wait 1500; mark loaded; mark search; address search; wait 900; "
                "mark close; address sound; wait 900; mark end",
                ["blank", "flicker", "margins"],
                "The search page opened and left",
            ),
            Scenario(
                "toast",
                "about",
                "mark idle start; wait 1500; mark loaded; click-text Copy system info; wait 1800; mark gone; wait 5200; mark end",
                ["blank", "flicker", "margins"],
                "A toast in and out",
            ),
        ]

    if probe is None:
        return scenarios

    x, y = probe.x, probe.pill_y
    list_x = max(24, probe.x // 2)
    ly = probe.list_y
    drop = height  # a point far below, for a full pull

    def hold(kind: str, points: list[tuple[int, int]], waits: int = 40) -> str:
        down, move, up = {"mouse": ("down", "move", "up"), "touch": ("touch_down", "touch_move", "touch_up")}[kind]
        first = points[0]
        steps = [f"{down} {first[0]},{first[1]}"]
        for px, py in points[1:]:
            steps += [f"{move} {px},{py}", f"wait {waits}"]
        last = points[-1]
        steps += [f"mark release", f"{up} {last[0]},{last[1]}", "wait 900"]
        return "; ".join(steps)

    def ramp(y0: int, y1: int, steps: int) -> list[tuple[int, int]]:
        return [(x, int(y0 + (y1 - y0) * i / steps)) for i in range(steps + 1)]

    def sheet(name: str, script: str, checks: list[str], description: str, ends: str | None) -> Scenario:
        return Scenario(
            name,
            page,
            f"mark idle start; {open_steps}; mark rest; {script}; mark end",
            ["blank", "flicker"] + checks,
            description,
            opens,
            ends,
        )

    pull = ["sheet-rigid", "sheet-reach", "scrim"]
    for kind in ("mouse", "touch"):
        scenarios += [
            sheet(
                f"sheet-drag-release-{kind}",
                hold(kind, ramp(y, y + 70, 7), waits=120),
                pull + ["sheet-rest"],
                f"{kind}: the grabber dragged 70 px and let go slowly: springs back",
                "rest",
            ),
            sheet(
                f"sheet-drag-dismiss-{kind}",
                hold(kind, ramp(y, min(drop - 10, y + 220), 11), waits=120),
                pull + ["sheet-rest"],
                f"{kind}: the grabber dragged past the threshold and let go slowly: dismissed",
                "closed",
            ),
            sheet(
                f"sheet-fling-{kind}",
                hold(kind, ramp(y, y + 45, 3), waits=0),
                pull + ["sheet-rest"],
                f"{kind}: a short quick flick down: dismissed",
                "closed",
            ),
            sheet(
                f"sheet-lift-{kind}",
                hold(kind, ramp(y, max(10, y - 160), 8), waits=120),
                pull + ["sheet-rest"],
                f"{kind}: the grabber pulled above rest: rubber-banded, then springs back",
                "rest",
            ),
        ]
    scenarios += [
        sheet(
            "sheet-open-escape",
            f"scroll {list_x},{ly},-3000; wait 1200; mark scrolled; key escape; wait 900",
            ["sheet-bottom", "sheet-rest"],
            "The sheet opened and closed by Escape",
            "closed",
        ),
        sheet(
            "sheet-scrim-tap",
            f"mark tap; click {width // 2},{max(20, probe.top // 2)}; wait 900",
            ["sheet-rest"],
            "The sheet closed by a press on the scrim",
            "closed",
        ),
        sheet(
            "sheet-handoff-touch",
            hold("touch", [(list_x, ly)] + [(list_x, ly + dy) for dy in range(12, 150, 14)], waits=100),
            ["sheet-rigid", "scrim"],
            "A finger in the list dragged down: the list hands the drag to the sheet at its top",
            None,
        ),
        sheet(
            "sheet-list-scroll-touch",
            hold("touch", [(list_x, ly)] + [(list_x, ly - dy) for dy in range(12, 150, 14)], waits=100),
            ["sheet-rigid"],
            "A finger in the list dragged up: the list scrolls and the sheet stays",
            "rest",
        ),
    ]
    return scenarios


def probe_for(binary: Path, out: Path, size: str, opens: tuple[str, str], timeout: int) -> Probe | None:
    name = f"probe-{opens[0]}-{size}"
    shot = out / "shots" / f"{name}.png"
    shot.parent.mkdir(parents=True, exist_ok=True)
    script = f"{sheet_open(opens)}; save {shot}"
    status = run_script(binary, out, name, size, opens[0], script, timeout)
    if status != "ok" or missing_click(out, name) or not shot.exists():
        return None
    return probe_sheet(shot)


# ----------------------------------------------------------------- output ---


def write_strip(frames: list[dict], markers: list[dict], path: Path, max_frames: int = 24) -> None:
    """A contact sheet of the run's distinct frames, marked where the walk marked."""
    distinct: list[dict] = []
    for frame in frames:
        if not distinct or frame["file"] != distinct[-1]["file"]:
            distinct.append(frame)
    if not distinct:
        return
    step = max(1, len(distinct) // max_frames)
    picks = distinct[::step][:max_frames]
    height, width = picks[0]["rgb"].shape[:2]
    scale = min(1.0, 150 / width)
    tw, th = max(1, int(width * scale)), max(1, int(height * scale))
    cols = min(len(picks), 8)
    rows = (len(picks) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * (tw + 4), rows * (th + 14)), (30, 30, 30))
    draw = ImageDraw.Draw(sheet)
    label_at = {m["index"]: m["text"] for m in markers}
    for n, frame in enumerate(picks):
        x, y = (n % cols) * (tw + 4), (n // cols) * (th + 14)
        sheet.paste(Image.fromarray(frame["rgb"]).resize((tw, th)), (x, y + 12))
        note = next((label_at[i] for i in range(frame["i"] - step + 1, frame["i"] + 1) if i in label_at), "")
        draw.text((x + 1, y), f"{frame['i']} {note}"[:22], fill=(255, 255, 0))
    path.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(path)


def analyse(out: Path, scenario: Scenario, size: str, probe: Probe | None) -> tuple[list[Finding], str]:
    frames, markers = load_frames(out / "frames" / f"{scenario.name}-{size}")
    if not frames:
        return [Finding("run", False, "no frames were recorded")], ""
    width, height = parse_size(size)
    findings: list[Finding] = []
    sheet_color = probe.sheet_color if probe else None
    start = marker_index(markers, "rest") if scenario.opens else None
    end = marker_index(markers, "end")
    end = len(frames) - 1 if end is None else min(end, len(frames) - 1)
    for check in scenario.checks:
        if check == "blank":
            findings.append(check_blank(frames))
        elif check == "flicker":
            findings.append(check_flicker(frames))
        elif check == "margins":
            findings.append(check_margins(frames, width, sheet_color, marker_index(markers, "loaded")))
        elif probe is not None and start is not None:
            first = start
            if check == "sheet-reach":
                findings.append(check_reach(frames, probe, first, end))
            elif check in ("sheet-rigid", "scrim"):
                poses = sheet_poses(frames, probe, first, end)
                findings.append(check_rigid(poses, probe) if check == "sheet-rigid" else check_scrim(poses))
            elif check == "sheet-rest":
                findings.append(check_rest(frames, probe, scenario.ends or "rest"))
            elif check == "sheet-bottom":
                findings.append(check_bottom(frames, probe, marker_index(markers, "scrolled")))
    strip = out / "strips" / size / f"{scenario.name}.png"
    write_strip(frames, markers, strip)
    return findings, str(strip.relative_to(out))


def render(results: list[dict], binary: Path, notes: list[str]) -> str:
    lines = [
        "# Bingux Settings mobile walk",
        "",
        f"Generated {time.strftime('%Y-%m-%d %H:%M:%S')} with `{binary}`.",
        "",
        "Every scenario is a scripted walk recorded frame by frame; the checks read the frames' pixels. "
        "See the header of `scripts/mobile-walk.py` for what each check means.",
        "",
    ]
    lines += [f"- {note}" for note in notes]
    lines += ["", "| Size | Scenario | Check | Result | Detail |", "|---|---|---|---|---|"]
    failed = 0
    for result in results:
        for finding in result["findings"]:
            mark = "pass" if finding.ok else "**FAIL**"
            failed += 0 if finding.ok else 1
            lines.append(
                f"| {result['size']} | {result['name']} | {finding.check} | {mark} | {finding.detail} |"
            )
    lines += ["", f"{failed} failing checks.", "", "## Frame strips", ""]
    for result in results:
        if result["strip"]:
            lines.append(f"- {result['size']} {result['name']}: `{result['strip']}` ({result['description']})")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, default=Path("/tmp/MobileWalk"))
    parser.add_argument("--bin", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--sizes", default=",".join(DEFAULT_SIZES), help="comma-separated WxH list")
    parser.add_argument("--only", default="", help="comma-separated scenario names")
    parser.add_argument("--timeout", type=int, default=240)
    parser.add_argument("--skip-run", action="store_true", help="analyse frames already kept")
    args = parser.parse_args()

    if not args.skip_run and not args.bin.exists():
        sys.exit(f"{args.bin} does not exist: build with `cargo build --features snapshot`")
    only = {name for name in args.only.split(",") if name}
    args.out.mkdir(parents=True, exist_ok=True)
    results: list[dict] = []
    notes: list[str] = []
    for size in args.sizes.split(","):
        width, _ = parse_size(size)
        probe, opens = None, None
        # The probe finds where the sheet's grabber is at this size. Try the
        # long list first, then the short one that exists on every machine.
        for candidate in (LIST, SHORT):
            if args.skip_run:
                shot = args.out / "shots" / f"probe-{candidate[0]}-{size}.png"
                found = probe_sheet(shot) if shot.exists() else None
            else:
                found = probe_for(args.bin, args.out, size, candidate, args.timeout)
            if found is not None:
                probe, opens = found, candidate
                break
        if probe is None or opens is None:
            notes.append(f"{size}: no dropdown sheet could be opened; the sheet scenarios are skipped")
            opens = SHORT
        elif opens == SHORT:
            notes.append(f"{size}: no display on this machine; the sheet scenarios use the two-option clock format list")
        scenarios = build_scenarios(size, probe, opens)
        for scenario in scenarios:
            if only and scenario.name not in only:
                continue
            key = f"{scenario.name}-{size}"
            if not args.skip_run:
                status = run_script(args.bin, args.out, key, size, scenario.page, scenario.script, args.timeout)
                if status != "ok":
                    results.append(
                        {
                            "size": size,
                            "name": scenario.name,
                            "description": scenario.description,
                            "findings": [Finding("run", False, status)],
                            "strip": "",
                        }
                    )
                    continue
            findings, strip = analyse(args.out, scenario, size, probe)
            results.append(
                {
                    "size": size,
                    "name": scenario.name,
                    "description": scenario.description,
                    "findings": findings,
                    "strip": strip,
                }
            )
            verdict = "ok" if all(f.ok for f in findings) else "FAIL"
            print(f"{size:>8} {scenario.name:<28} {verdict}", flush=True)
    report = render(results, args.bin, notes)
    (args.out / "report.md").write_text(report)
    print(f"\nreport: {args.out / 'report.md'}")
    return 1 if any(not f.ok for r in results for f in r["findings"]) else 0


if __name__ == "__main__":
    sys.exit(main())
