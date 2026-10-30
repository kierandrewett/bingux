#!/usr/bin/env python3
"""Fail when a Bingux window has a visual layout defect.

This is the headless run of the inspector's Layout tab. The tab and this
script share one set of rules (crates/ui/src/inspector/layout_checks.rs):
uneven padding, an item off its row's centre line, siblings of one kind at
different heights, a box with dead space under its content, clipped content,
overlapping controls, text cut off while there is room, misaligned
baselines, an icon off the centre of its slot (`icon-not-centred`: a field's
leading slot or an icon button, half a pixel at most), and a value cut off with
an ellipsis or a clip (`value-ellipsised`:
a value is shown in full; only a path or name that `path_label` shortens by
design, with its full text as the tooltip, is allowed). They are visual
defects, not accessibility problems.

Eight more rules hold the padding model of design/rules.md to the tree
(inspector/checks/, each finding starts with its rule IDs from
design/rules.json): `inset-asymmetry` (SP-01), `trailing-inset` (SP-02, AL-09,
SP-12, SZ-12), `column-aligned` (AL-01, SZ-05), `list-row-inset` (SP-09),
`field-text-start` (AL-03), `header-measure` (AL-04, SZ-02),
`radius-concentric` (BR-01, BR-02) and `off-scale-spacing` (SP-15, SP-21).
`--parity` adds a pass that compares each page's desktop and phone capture:
the named insets of every component class (written with each capture) must
scale by a token pair, at most 1.4x and never smaller, or be held equal
(PH-01, PH-03).

The apps render through `bingux_ui::snapshot` without a compositor. With
BINGUX_UI_LAYOUT_LINT=FILE set, every capture the run saves also appends the
findings of its frame to FILE, and this script reads them. It runs

  * every Bingux Settings page (the sidebar panels and a few subpages) at the
    desktop size and at 390x800,
  * the Settings popups, sheets and dialogs that can be opened by a click, at
    both sizes, and
  * every bingux-taxonomy page, which holds each component and state of
    bingux-ui, at desktop and phone size, with the states that open a menu or
    a dialog, and
  * Bingux Files: the folder list, and the Properties dialog (Basic,
    Permissions and Details) of a file, a link and a folder whose names, paths
    and link target are far too long for the window, at desktop and phone size.

    python3 scripts/layout-lint.py                  # everything
    python3 scripts/layout-lint.py --only settings  # Settings only
    python3 scripts/layout-lint.py --only taxonomy  # taxonomy only
    python3 scripts/layout-lint.py --only files     # Files only
    python3 scripts/layout-lint.py --pages network,sound --sizes 390x800
    python3 scripts/layout-lint.py --json /tmp/layout.json
    python3 scripts/layout-lint.py --parity         # also desktop vs phone insets
    python3 scripts/layout-lint.py --rtl            # also the mirror check (see below)

--rtl captures each page, popup and window a second time with BINGUX_DIRECTION=rtl. The
left-to-right run keeps the shape of its frame in a directory (BINGUX_UI_LAYOUT_MIRROR),
the right-to-left run compares its own with it, and a box that is not the reflection of
where it is (same size, same distance from the other edge) is a `direction-asymmetric`
finding. Add an ALLOWED entry for the rule where a box must not mirror (a media bar,
a chart plot), with the reason.

Exit status is 1 when there is a finding that ALLOWED does not cover. Fix the
component, or token, that causes a finding. Where a box is lopsided on
purpose, add an entry to ALLOWED with the reason: a rule, a regex for the
capture and a regex for the component path or the node.

Build first, with the usual shared target:

    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build --features snapshot \
        --manifest-path apps/settings/Cargo.toml
    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build \
        --manifest-path apps/taxonomy/Cargo.toml
    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build \
        --manifest-path apps/files/Cargo.toml

(`--build` does this.) Settings run against fakes (BINGUX_SETTINGS_FAKE_*), so
a result is the same on every machine. Needs nothing beyond the standard
library.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SETTINGS = ROOT / "apps" / "settings"
TAXONOMY = ROOT / "apps" / "taxonomy"
FILES = ROOT / "apps" / "files"
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

DESKTOP = "default"
# The name a right-to-left capture run takes after its left-to-right one's.
RTL_TAG = "-rtl"
PHONE = "390x800"

# Subpages that are laid out on their own, with the pages from the sidebar.
SUBPAGES = [
    "network/wifi",
    "network/wifi/saved",
    "network/hotspot",
    "network/mobile",
    "applications/default",
    "datetime/timezone",
    "background/font",
    "keyboard/shortcuts",
]

# Popups, sheets and dialogs that open from a click on a label. (name, page,
# the script that opens it). `save` writes the capture the lint reads.
POPUPS = [
    ("wifi-join", "network/wifi", "click-text Neighbour"),
    ("wifi-enterprise", "network/wifi", "click-text Campus"),
    ("wifi-hidden", "network/wifi", "click-text Connect to hidden network…"),
    ("datetime-set", "datetime", "click-text Set date and time"),
    ("keyboard-add-input", "keyboard", "click-text Add an input source"),
    ("about-rename", "about", "click-text Rename"),
    ("users-password", "users", "click-text Change password"),
    ("sound-test", "sound", "click-text Test speakers"),
    ("ethernet-new-profile", "network/ethernet", "click-text Add profile…"),
]
# Popups that a snapshot opens itself (BINGUX_SETTINGS_SNAPSHOT_OPEN).
OPENED = [
    ("search-add-engine", "search", "add-engine"),
    ("search-remove-engine", "search", "remove-engine"),
    ("applications-uninstall", "applications", "uninstall"),
]

# Data that comes from the machine is replaced by fixed fakes.
FAKES = {
    "BINGUX_SETTINGS_FAKE_NETWORK": "online",
    "BINGUX_SETTINGS_FAKE_MODEM": "ok",
    "BINGUX_SETTINGS_FAKE_DISPLAYS": "1",
    # What changes from one run to the next on one machine would make the left-to-right and the
    # right-to-left capture of a page differ: the About page's uptime and root disk fill, and the
    # Date and time page's clock. (The drives' free space is `MOUNTS` below.)
    "BINGUX_SETTINGS_FAKE_ABOUT": "1",
    "BINGUX_SETTINGS_FAKE_TIMEDATE": "1",
    "BINGUX_SETTINGS_FAKE_NOW": "1793284320",
}

# The drives of every capture (`BINGUX_FILES_FIXTURE_MOUNTS`, crates/ui/src/files/
# places_fixture.rs): the machine's own drives and how full they are change between two runs,
# and a "10 GB free" that becomes "9.9 GB free" is a different width.
MOUNTS = """\
local | System | / | 500/120
removable | KINGSTON | /run/media/me/KINGSTON | 64/12.5 | user
network | nas | /mnt/nas | stuck
"""

# The rules of design/rules.json that the 18 rules of layout_checks.rs without IDs of their own hold, by
# rule key, and `parity` and `direction-asymmetric`, which this script finds (read by scripts/rules-check.py;
# the eight rules of inspector/checks/ carry theirs in their Meta).
#
# `clipped`, `overlap`, `hidden-label` and `horizontal-scroll` hold TX-02 ("nothing is unreadable or
# covers other text, ... and narrow windows"): ink cut off by a box that clips it, two siblings lying on
# each other, a control whose label is out of sight, and content wider than the window are each a text
# a person cannot read in full. The same four are mapped the same way in scripts/overflow-check.py
# (tests/rules-check.test.py keeps the two in step).
RULE_IDS = {
    "uneven-padding": ["SP-01"],
    "off-centre": ["AL-06"],
    "unequal-siblings": ["SZ-02", "SZ-05"],
    "dead-space": ["SP-07"],
    "baseline": ["AL-13"],
    "wrapped": ["TX-02"],
    "overflow": ["TX-02"],
    "truncated": ["TX-02"],
    "text-collision": ["TX-02"],
    "focus-ring": ["FC-01"],
    "value-ellipsised": ["TX-01"],
    "icon-not-centred": ["AL-02"],
    "direction-asymmetric": ["AY-01"],
    "parity": ["PH-01", "PH-03"],
    "clipped": ["TX-02"],
    "overlap": ["TX-02"],
    "horizontal-scroll": ["TX-02"],
    "hidden-label": ["TX-02"],
}

# A rule the check finds that no rule of design/rules.json states. Its findings are not dropped: the report
# counts them under "Findings that hold no rule of the book" with the reason below, and they still fail
# the check. No rule is invented for them (the book's text is Kieran's).
RULE_UNMAPPED = {
    "outside-parent": "a box reaching outside the box that holds it: the book has no rule for boxes staying inside their parent (only for text, TX-02, and for padding, SP-01), so this is a layout-integrity check of the inspector without a rule ID",
}

# Findings that are right by design. Each entry: the rule's key, a regex that
# must match the capture's name, a regex that must match the component path or
# the node's description, and why the box is meant to look like this. An entry
# that matches nothing is reported, so the list cannot rot.
ALLOWED: list[dict] = [
    {
        "rule": "value-ellipsised",
        "capture": r"^(taxonomy:files_|files)",
        "where": r"PathLabel Label #",
        "why": "A file or place name cut in the middle by path_label with `.tooltip(false)`: the file list's hover card and the status line show the whole name (design/path-elision.md), so there is no tooltip for the Elided marker to name.",
    },
    {
        "rule": "value-ellipsised",
        "capture": r"^taxonomy:",
        "where": r"A (name|title) far too long for the (room|bar) it is",
        "why": "The specimen of the long-name case: it shows how a component cuts a name too long for its room, so it is cut on purpose.",
    },
    {
        "rule": "wrapped",
        "capture": r"^taxonomy:",
        "where": r"\"(Presentation|Destructive|Overlay 20|Hero 56|wrap 5|Invalid, focused|reduced motion)\"",
        "why": "A specimen caption in a tile of the width of the token it shows (a file kind, a motion, a space step): the tile is fixed and the caption is the name of the thing drawn, not copy that must fit.",
    },
    {
        "rule": "direction-asymmetric",
        "capture": r"^taxonomy-rtl:files_file-preview",
        "where": r"MediaControls",
        "why": "A player is a physical object and its bar is a tape: play, position and volume keep their places in every language (media_controls.rs sets TextDirection::Ltr; design/rtl.md, 'Does not mirror'), so the bar's children are the same in both runs and not each other's reflection.",
    },
    {
        "rule": "direction-asymmetric",
        "capture": r"^taxonomy-rtl:controls_feedback",
        "where": r"ProgressSegment",
        "why": "An indeterminate bar's segment travels along its track on a clock, so where it is when a capture is taken depends on the run and not on the direction (scripts/loop-check.py proves it moves).",
    },
    {
        "rule": "off-scale-spacing",
        "capture": r"^taxonomy:files_file-preview",
        "where": r"PreviewDocument",
        "why": "The PDF viewer centres the fitted page: the page's start margin is the room left over beside it (preview/viewers/pdf.rs, `left`), measured, not a token.",
    },
    {
        "rule": "direction-asymmetric",
        "capture": r"^taxonomy-rtl:data_charts",
        "where": r".",
        "why": "A chart's plot, its axis labels and its legend rows are drawn left to right in every language (Material: charts and graphs are not mirrored; the SVG is pixels, so the labels over it are too; design/rtl.md).",
    },
    {
        "rule": "clipped",
        "capture": r"^files-list-phone:files-(tabs|preview)",
        "where": r"TabStrip > Tab",
        "why": "A phone has room for one tab and the buttons after it: the strip scrolls the others out of its region and clips them there on purpose, with the selected tab in view (design/tabs.md, Width and Scrolling).",
    },
    {
        "rule": "direction-asymmetric",
        "capture": r"^taxonomy-rtl:controls_(selection|small-widgets)",
        "where": r"TabEnd|The page the selected tab opens|The tabs are separate from it|220 px of room|column #",
        "why": "These specimens are `directed(Direction::Rtl, ..)` in both runs (they show the right-to-left tab strip and fade), so they are the same in both and not each other's reflection.",
    },
    {
        "rule": "value-ellipsised",
        "capture": r"^settings-background_font-",
        "where": r"Row.*The quick brown fox",
        "why": "A font row draws a sample sentence in its own family on one line (Row::font): a specimen of the face, not a value, and the family's name is its subtitle.",
    },
    {
        "rule": "column-aligned",
        "capture": r"^settings-(background|desktop|mouse)-",
        "where": r"Row > Dropdown|Dropdown #\d+",
        "why": "A dropdown is min_width(Dim::Field) and grows to its longest option so a value is shown in full (value-ellipsised): a card whose options are longer than Dim::Field has dropdowns of two widths. Open design decision (SZ-05, M0232): fix the width by token and shorten the options, or keep the value whole.",
    },
    {
        "rule": "off-scale-spacing",
        "capture": r"^taxonomy:controls_text-input",
        "where": r"TextField row #\d+",
        "why": "A flat or dense field's trailing button is Dim::Box5 (20) in a field that is not on the grid, so its inset is half of what the button leaves (3 px) on all three sides: equal insets (SP-02, AL-09) take priority over the grid there (text_field.rs, `inset`).",
    },
    {
        "rule": "parity",
        "capture": r"^parity:",
        "where": r"Page/padding-(left|right)$",
        "why": "The page's edge padding is S8 (32) on a desktop window and GUTTER (S4, 18) on a phone: a phone has less room, so this inset shrinks by design (design/rules.md padding model, 'Page column').",
    },
    {
        "rule": "parity",
        "capture": r"^parity:",
        "where": r"PlaceRow\[.*\]/(icon-left|padding-left|padding-right)$",
        "why": "NAV_INSET is (8, 16): the side padding of a list row matches the space above and below its icon, which is 8 in a 40 px row and 16 in a 56 px row (tokens.rs NAV_INSET), so it doubles on a phone on purpose (SP-09).",
    },
    {
        "rule": "parity",
        "capture": r"^parity:",
        "where": r"Row\[1\|[a-z]+\|Switch\]/(padding-right|trailing-end)$",
        "why": "SWITCH_INSET is (18, 16): a trailing switch sits half the row height less the switch height from the end, which is 18 in a 56 px row with a 20 px switch and 16 in a 60 px row with a 28 px switch (tokens.rs SWITCH_INSET, SP-02).",
    },
    {
        "rule": "radius-concentric",
        "capture": r"^taxonomy:foundations_style-system",
        "where": r"PageColumn row #\d+",
        "why": "The style-system page draws a Radius::Pill specimen (a 240 by 20 capsule) to show the radius ladder: a picture of the token, not a control.",
    },
    {
        "rule": "dead-space",
        "capture": r"^popup-[a-z-]+-phone:",
        "where": r"Dialog #\d+",
        "why": "A phone popup is a sheet: its card hangs sheet_overhang() (Radius::Overlay, 20 px) below the window so only its top corners show, and its content ends sheet_foot() (that overhang plus Dim::SafeBottom) above the card's bottom edge (shell/panel.rs, window/popup.rs; SP-14). The empty strip under the last row is that foot, not dead space.",
    },
    {
        "rule": "outside-parent",
        "capture": r"^popup-[a-z-]+-phone:",
        "where": r"Dialog #\d+",
        "why": "The sheet's card hangs its overhang (20 px, Radius::Overlay) below the bottom edge of the window on purpose, so its bottom corners are off screen (shell/panel.rs sheet_overhang, window/popup.rs `a_sheet_hangs_its_overhang_below_the_window`).",
    },
    {
        "rule": "wrapped",
        "capture": r"^taxonomy:files_file-icons",
        "where": r"text #\d+ \"",
        "why": "A specimen caption under an icon in the 16 and 20 px rows of the file-icons page: the tile is as wide as the icon it shows, so a kind's name (\"Data\") breaks in it. The caption names the specimen and is not copy that must fit.",
    },
    {
        "rule": "wrapped",
        "capture": r"^taxonomy:foundations_motion",
        "where": r"\"Row\"",
        "why": "A specimen caption in a tile of the width of the token it shows: the motion page's tiles are sized by the duration they stand for.",
    },
    {
        "rule": "value-ellipsised",
        "capture": r"^taxonomy:foundations_motion",
        "where": r"\"MOVE_MS\"",
        "why": "A specimen caption in a tile of the width of the token it shows: the tile is fixed and the caption names the token.",
    },
    {
        "rule": "outside-parent",
        "capture": r"^taxonomy:foundations_motion",
        "where": r"\"reduced motion\"",
        "why": "A specimen caption in a tile of the width of the token it shows (the same tile as the wrapped caption above): the caption is wider than the fixed tile.",
    },
    {
        "rule": "value-ellipsised",
        "capture": r"^taxonomy:data_charts",
        "where": r"FlameGraph",
        "why": "A flame graph writes each frame's name inside its bar and cuts it to the bar's width (charts/flame.rs): a bar of a few pixels cannot hold its name, as in every flame graph. Open: the graph has no hover card that names the frame, so a cut name cannot be read (BH/CT-03 gap).",
    },
]


def allowed(finding: dict, capture: str) -> dict | None:
    for entry in ALLOWED:
        if (
            entry["rule"] == finding["rule"]
            and re.search(entry["capture"], capture)
            and re.search(entry["where"], f"{finding['path']} {finding['target']}")
        ):
            return entry
    return None


# Parity: the same page at desktop and at 390x800 puts its insets by the same
# logic (design/rules.md PH-01, PH-03, SP-13). The Rust side writes the named
# insets of each component class with every capture ("Row/trailing-end": the
# values measured and how often); this compares the most common value of each
# between the two sizes. Every inset scales with the density by a token pair
# (compact at least as big as regular, at most PARITY_SCALE_MAX times), except
# the ones PARITY_HELD says are held equal. A difference that is intended is an
# ALLOWED entry for the rule "parity", matched on the pages and on "Class/name".
PARITY_SCALE_MAX = 1.4
PARITY_HELD: list[tuple[str, str, str]] = [
    ("FileRow", r"^(padding-(left|right)|icon-left)$", "FILE_ROW_INSET is (8, 8): a file's icon sits the same distance from the list's edge at every width, only the rows' height changes (PH-03)."),
    ("AlertDialog", r"^padding-", "DIALOG_INSET is 12 at every density, so a button's radius is the card's less the inset (SP-18, BR-01)."),
    (r".*", r"^radius$", "Radii are held equal on phone and desktop: nothing is rounder by accident (design/rules.md, 'Held equal on phone and desktop')."),
    ("HeaderBar", r"^padding-right$", "The header rail's end inset is S2 (8 px) at every density; its start inset depends on whether a leading control (Back) is there."),
]
# The insets compared: the ones that are one token, not a width.
PARITY_METRICS = re.compile(
    r"^(padding-(top|right|bottom|left)|gap|radius|icon-left|label-start|trailing-end|content-(top|bottom|start))$"
)
SIZE_TAG = re.compile(r"(desktop|phone)")


def modal(values: dict[str, int]) -> float:
    """The value measured most often, the smaller on a tie."""
    best = max(values.items(), key=lambda item: (item[1], -float(item[0])))
    return float(best[0])


def parity_findings(results: list[dict]) -> list[dict]:
    """Compare each page's desktop capture with its phone capture."""
    pages: dict[str, dict[str, dict[str, dict[str, int]]]] = {}
    for result in results:
        for shot in result["shots"]:
            stem = Path(shot["capture"]).stem
            name = f"{result['name']}:{stem}"
            if "metrics" not in shot:
                continue
            if result["name"] == "taxonomy":
                # One run writes `page.png` (desktop) and `page.phone.png`.
                size = "phone" if stem.endswith(".phone") else "desktop"
                key = name[: -len(".phone")] + "@" if size == "phone" else name + "@"
            else:
                match = SIZE_TAG.search(name)
                if match is None:
                    continue
                size = match.group(1)
                key = SIZE_TAG.sub("@", name, count=1)
            pages.setdefault(key, {})[size] = shot["metrics"]
    grouped: dict[tuple[str, float, float, str], list[str]] = {}
    for key, sizes in pages.items():
        if "desktop" not in sizes or "phone" not in sizes:
            continue
        for metric in sorted(set(sizes["desktop"]) & set(sizes["phone"])):
            cls, _, name = metric.partition("/")
            if not PARITY_METRICS.match(name):
                continue
            desktop = modal(sizes["desktop"][metric])
            phone = modal(sizes["phone"][metric])
            if desktop < 0 or phone < 0:
                continue  # content that leaves its box is the overflow rules' business
            held = next((why for c, n, why in PARITY_HELD if re.fullmatch(c, cls) and re.search(n, name)), None)
            if held and abs(desktop - phone) > 0.05:
                bad = f"[PH-03] held equal at both densities but {desktop:g} px on desktop and {phone:g} px on phone ({held})"
            elif not held and (phone < desktop - 0.05 or phone > desktop * PARITY_SCALE_MAX + 0.05):
                bad = f"[PH-01] {desktop:g} px on desktop and {phone:g} px on phone: an inset scales up with the density by a token pair, at most {PARITY_SCALE_MAX:g}x"
            else:
                continue
            grouped.setdefault((metric, desktop, phone, bad), []).append(key)
    findings = []
    for (metric, desktop, phone, bad), keys in sorted(grouped.items()):
        findings.append(
            {
                "rule": "parity",
                "node": 0,
                "target": metric,
                "path": "",
                "what": bad + f"; on {len(keys)} page(s): " + ", ".join(sorted(keys)[:4]) + (" ..." if len(keys) > 4 else ""),
                "numbers": {"desktop": desktop, "phone": phone},
                "capture": "parity:" + ",".join(sorted(keys)),
            }
        )
    return findings


def build() -> None:
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    subprocess.run(
        ["cargo", "build", "--features", "snapshot", "--manifest-path", str(SETTINGS / "Cargo.toml")],
        check=True,
        env=env,
    )
    subprocess.run(
        ["cargo", "build", "--manifest-path", str(TAXONOMY / "Cargo.toml")], check=True, env=env
    )
    subprocess.run(
        ["cargo", "build", "--features", "snapshot", "--manifest-path", str(FILES / "Cargo.toml")],
        check=True,
        env=env,
    )


def panels() -> list[str]:
    """The sidebar panel names, read from PageId::panel_name."""
    source = (SETTINGS / "src" / "pages" / "mod.rs").read_text()
    match = re.search(r"fn panel_name\(self\).*?\n    \}\n", source, re.S)
    if match is None:
        sys.exit("cannot find PageId::panel_name in src/pages/mod.rs")
    return re.findall(r'=> "([a-z]+)"', match.group(0))


def read_lint(path: Path) -> list[dict]:
    if not path.exists():
        return []
    shots = []
    for line in path.read_text().splitlines():
        if line.strip():
            shots.append(json.loads(line))
    return shots


def run_job(job: dict, workdir: Path, timeout: int) -> dict:
    """Run one capture and return {name, status, shots}."""
    lint = workdir / f"{job['name']}.jsonl"
    lint.unlink(missing_ok=True)
    env = dict(os.environ)
    env.update(FAKES)
    env.update(job.get("env", {}))
    # A snapshot renders in software and needs no compositor: never touch the live desktop,
    # and never write the person's dconf (scripts/app_env.py).
    env = isolated(env)
    env["BINGUX_UI_LAYOUT_LINT"] = str(lint)
    env["BINGUX_FILES_FIXTURE_MOUNTS"] = str(workdir / "mounts.txt")
    if job.get("mirror"):
        env["BINGUX_UI_LAYOUT_MIRROR"] = job["mirror"]
    env["XDG_CACHE_HOME"] = str(workdir / "cache" / job["name"])
    if job.get("size") and job["size"] != DESKTOP:
        env["BINGUX_SETTINGS_SNAPSHOT_SIZE"] = job["size"]
    if job.get("script"):
        # `{out}` is the job's capture directory: a `save` of a bare name would write into the
        # directory the script runs in.
        env["BINGUX_SETTINGS_SNAPSHOT_SCRIPT"] = job["script"].replace("{out}", str(workdir / "png" / job["name"]))
    env.setdefault("BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS", "1500")
    out = workdir / "png" / job["name"]
    out.mkdir(parents=True, exist_ok=True)
    log = workdir / "logs" / f"{job['name']}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as sink:
        try:
            proc = subprocess.run(
                [*job["argv"], str(out), *job.get("pages", [])],
                env=env,
                stdout=sink,
                stderr=subprocess.STDOUT,
                timeout=timeout,
                check=False,
            )
            status = "ok" if proc.returncode == 0 else f"exit {proc.returncode}"
        except subprocess.TimeoutExpired:
            status = f"timed out after {timeout}s"
    return {"name": job["name"], "status": status, "shots": read_lint(lint), "log": str(log)}


def settings_jobs(pages: list[str], sizes: list[str]) -> list[dict]:
    binary = str(TARGET / "debug" / "bingux-settings")
    jobs = []
    for size in sizes:
        tag = "phone" if size == PHONE else "desktop"
        for page in pages:
            jobs.append(
                {
                    "name": f"settings-{page.replace('/', '_')}-{tag}",
                    "argv": [binary, "--snapshot"],
                    "pages": [page],
                    "size": size,
                }
            )
        for name, page, click in POPUPS:
            jobs.append(
                {
                    "name": f"popup-{name}-{tag}",
                    "argv": [binary, "--snapshot"],
                    "pages": [page],
                    "size": size,
                    "script": f"wait 1200; {click}; wait 1200; save {{out}}/popup.png",
                }
            )
        for name, page, key in OPENED:
            jobs.append(
                {
                    "name": f"popup-{name}-{tag}",
                    "argv": [binary, "--snapshot"],
                    "pages": [page],
                    "size": size,
                    "env": {"BINGUX_SETTINGS_SNAPSHOT_OPEN": key},
                }
            )
    return jobs


def taxonomy_jobs(pages: list[str] | None = None) -> list[dict]:
    # `--snapshot` writes desktop, phone and state captures for every page (or for `pages`,
    # `category/page` as the taxonomy names them).
    return [
        {
            "name": "taxonomy",
            "argv": [str(TARGET / "debug" / "bingux-taxonomy"), "--snapshot"],
            "pages": pages or [],
        }
    ]


# A file, a link and a folder whose names, paths and link target are far too
# long for a phone and for most desktop windows, so a Properties value that
# cannot be shown in full is shown cut off. Each sits alone in its folder, so
# Ctrl+A selects it and Alt+Enter opens its Properties.
LONG_NAME = "Quarterly-financial-statements-and-board-report-for-the-2026-financial-year-final-v3-reviewed.pdf"
LONG_DIR = "Photographs-from-the-summer-expedition-to-the-northern-coast-with-the-whole-family"
LONG_LINK = "shortcut-to-the-shared-archive-of-scanned-documents-and-correspondence-2019-2026"
LONG_TARGET = "/srv/archive/shared/departments/finance/audited-accounts/2026/quarter-three/scanned-originals/final-signed"
PROPERTIES = "key ctrl+a; wait 300; key alt+Return; wait 1400"


def files_fixture(root: Path) -> dict[str, Path]:
    base = root / "files-fixture" / "Projects" / "Annual-reports-and-statutory-filings-2026" / "Quarter-three-final"
    folders = {name: base / name for name in ("file", "link", "folder")}
    for folder in folders.values():
        folder.mkdir(parents=True, exist_ok=True)
    (folders["file"] / LONG_NAME).write_text("%PDF-1.4 fixture\n" * 40)
    (folders["folder"] / LONG_DIR).mkdir(exist_ok=True)
    (folders["folder"] / LONG_DIR / "note.txt").write_text("a note\n")
    link = folders["link"] / LONG_LINK
    if not link.is_symlink():
        link.symlink_to(LONG_TARGET)
    return folders


def files_jobs(workdir: Path) -> list[dict]:
    binary = str(TARGET / "debug" / "bingux-files")
    folders = files_fixture(workdir)
    home = workdir / "files-home"
    state = {
        "HOME": str(home),
        "XDG_STATE_HOME": str(home / "state"),
        "XDG_CONFIG_HOME": str(home / "config"),
        "XDG_DATA_HOME": str(home / "data"),
    }
    jobs = []
    for size, tag in (("1100x760", "desktop"), ("390x800", "phone")):
        env = dict(state, BINGUX_FILES_SNAPSHOT_SIZE=size)
        jobs.append(
            {
                "name": f"files-list-{tag}",
                "argv": [binary, "--snapshot"],
                "pages": [str(folders["file"])],
                "env": env,
            }
        )
        for kind in ("file", "link", "folder"):
            for tab in ("Basic", "Permissions", "Details"):
                script = PROPERTIES if tab == "Basic" else f"{PROPERTIES}; click-text {tab}; wait 900"
                jobs.append(
                    {
                        "name": f"files-properties-{kind}-{tab.lower()}-{tag}",
                        "argv": [binary, "--snapshot"],
                        "pages": [str(folders[kind])],
                        "env": dict(env, BINGUX_FILES_SNAPSHOT_SCRIPT=script),
                    }
                )
    return jobs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", choices=["settings", "taxonomy", "files"])
    parser.add_argument("--pages", default="", help="comma-separated Settings pages")
    parser.add_argument("--sizes", default=f"{DESKTOP},{PHONE}", help="Settings sizes, `default` is desktop")
    parser.add_argument("--jobs", type=int, default=2, help="captures to run at once")
    parser.add_argument("--timeout", type=int, default=240, help="seconds per capture run")
    parser.add_argument("--json", help="write every finding to this file")
    parser.add_argument("--keep", help="keep captures and logs in this directory")
    parser.add_argument("--build", action="store_true", help="build the three binaries first")
    parser.add_argument("--rule", default="", help="comma-separated rule keys: report only these")
    parser.add_argument("--show-allowed", action="store_true", help="list the findings ALLOWED covers")
    parser.add_argument("--parity", action="store_true", help="also compare each page's desktop and phone insets")
    parser.add_argument(
        "--taxonomy-pages",
        default="",
        help="one taxonomy page (`controls/selection`) instead of all of them: the taxonomy binary renders only the first page it is given",
    )
    parser.add_argument(
        "--rtl",
        action="store_true",
        help="also capture everything right to left (BINGUX_DIRECTION=rtl) and report a box that is not the reflection of where it is left to right",
    )
    args = parser.parse_args()

    if args.build:
        build()
    settings = TARGET / "debug" / "bingux-settings"
    taxonomy = TARGET / "debug" / "bingux-taxonomy"
    files = TARGET / "debug" / "bingux-files"
    jobs: list[dict] = []
    if args.only in (None, "settings"):
        if not settings.exists():
            sys.exit(f"{settings} is missing: build with --build")
        pages = [page for page in args.pages.split(",") if page] or panels() + SUBPAGES
        jobs += settings_jobs(pages, [size for size in args.sizes.split(",") if size])
    if args.only in (None, "taxonomy") and not args.pages:
        if not taxonomy.exists():
            sys.exit(f"{taxonomy} is missing: build with --build")
        jobs += taxonomy_jobs([page for page in args.taxonomy_pages.split(",") if page])
    if args.only in (None, "files") and not args.pages and not files.exists():
        sys.exit(f"{files} is missing: build with --build")

    workdir = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix="layout-lint-"))
    workdir.mkdir(parents=True, exist_ok=True)
    (workdir / "mounts.txt").write_text(MOUNTS)
    if args.only in (None, "files") and not args.pages:
        jobs += files_jobs(workdir)
    results = []

    def run_all(batch: list[dict]) -> None:
        with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
            futures = [pool.submit(run_job, job, workdir, args.timeout * (30 if job["name"].startswith("taxonomy") else 1)) for job in batch]
            for future in concurrent.futures.as_completed(futures):
                result = future.result()
                results.append(result)
                print(f"{result['status']:>10}  {result['name']}  ({len(result['shots'])} captures)", flush=True)

    if args.rtl:
        # Each capture is made twice: left to right, which keeps the shape of its
        # frame in a directory of its own, and right to left, which compares its
        # frame with that (BINGUX_UI_LAYOUT_MIRROR, crates/ui/src/snapshot.rs).
        mirrored = []
        for job in jobs:
            kept = workdir / "mirror" / job["name"]
            shutil.rmtree(kept, ignore_errors=True)
            job["mirror"] = str(kept)
            mirrored.append(dict(job, name=job["name"] + RTL_TAG, env=dict(job.get("env", {}), BINGUX_DIRECTION="rtl")))
        run_all(jobs)
        run_all(mirrored)
    else:
        run_all(jobs)

    findings = []
    broken = []
    wanted = {rule for rule in args.rule.split(",") if rule}
    for result in sorted(results, key=lambda r: r["name"]):
        if result["status"] != "ok" or not result["shots"]:
            broken.append(result)
        for shot in result["shots"]:
            capture = Path(shot["capture"]).stem
            where = f"{result['name']}:{capture}"
            for finding in shot["findings"]:
                if wanted and finding["rule"] not in wanted:
                    continue
                # A right-to-left capture is only read for the mirror check: its other
                # findings are the left-to-right capture's, in the other direction.
                if result["name"].endswith(RTL_TAG) and finding["rule"] != "direction-asymmetric":
                    continue
                finding = dict(finding, capture=where)
                finding["allowed"] = (allowed(finding, where) or {}).get("why")
                findings.append(finding)
    if args.parity and (not wanted or "parity" in wanted):
        for finding in parity_findings([r for r in results if not r["name"].endswith(RTL_TAG)]):
            finding["allowed"] = (allowed(finding, finding["capture"]) or {}).get("why")
            findings.append(finding)

    used = {entry["why"] for finding in findings for entry in [allowed(finding, finding["capture"])] if entry}
    # An entry for the mirror check is only expected to match when it ran.
    unused = [entry for entry in ALLOWED if entry["why"] not in used and (args.rtl or entry["rule"] != "direction-asymmetric")]
    failures = [finding for finding in findings if not finding["allowed"]]

    by_rule = Counter(finding["rule"] for finding in failures)
    print()
    for finding in sorted(failures, key=lambda f: (f["rule"], f["capture"], f["node"])):
        numbers = ", ".join(f"{name} {value:g}" for name, value in finding["numbers"].items())
        print(f"[{finding['rule']}] {finding['capture']}")
        print(f"    {finding['target']}  in {finding['path'] or '(no component)'}")
        print(f"    {finding['what']}  ({numbers})")
    if args.show_allowed:
        for finding in findings:
            if finding["allowed"]:
                print(f"[allowed {finding['rule']}] {finding['capture']}: {finding['target']} -- {finding['allowed']}")
    for entry in unused:
        print(f"unused ALLOWED entry: {entry['rule']} {entry['capture']} {entry['where']}: {entry['why']}")
    for result in broken:
        print(f"no captures: {result['name']} ({result['status']}), see {result['log']}")

    if args.json:
        Path(args.json).write_text(json.dumps(findings, indent=2))
    captures = sum(len(result["shots"]) for result in results)
    print(
        f"\n{captures} captures, {len(failures)} findings"
        + (": " + ", ".join(f"{rule} {n}" for rule, n in sorted(by_rule.items())) if by_rule else "")
        + f", {len(findings) - len(failures)} allowed"
    )
    if not args.keep:
        shutil.rmtree(workdir, ignore_errors=True)
    return 1 if failures or broken or unused else 0


if __name__ == "__main__":
    sys.exit(main())
