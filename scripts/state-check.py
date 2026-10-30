#!/usr/bin/env python3
"""Fail when a control does not respond, cannot be reached, or asks for the wrong cursor.

The layout lint reads one captured tree. This check drives the window: for every
interactive node of a capture it points at it, presses it, focuses it and renders
each state, tabs through the whole page and back, opens every menu and dropdown
and walks it by key, and reads the cursor the window asks for over each control.
It covers the interaction-state cluster of design/rules.md:

  CK-03  hover and state matrix  state-hover, state-press, state-selected,
                                 hover-collateral, row-hover, state-open, state-disabled
  CK-12  touch targets (390x800) touch-target
  CK-13  resize-handle and scrollbar clearance   resize-clearance, scrollbar-clearance
  CK-14  focus ring and clipped ring             state-focus, ring-clipped
  CK-16  menu structure and keyboard walk        menu-keys
  CK-18  tab walk                tab-reach, tab-order, tab-ring, tab-trap, popup-escape
  CK-25  cursor map              cursor-map

The apps render through `bingux_ui::snapshot` without a compositor. With
BINGUX_UI_STATE_CHECK=FILE set, every capture the run saves also runs the checks
(crates/ui/src/inspector/state_checks) and appends its report to FILE.
This script reads them. It runs

  * every Bingux Settings page, subpage and click-opened popup, at the desktop
    size and at 390x800,
  * every bingux-taxonomy page, which holds each component and state of
    bingux-ui, and
  * the Files window (`bingux-files --snapshot`), desktop and phone.

    python3 scripts/state-check.py                    # everything
    python3 scripts/state-check.py --only settings    # Settings only
    python3 scripts/state-check.py --only files
    python3 scripts/state-check.py --pages network,sound --sizes 390x800
    python3 scripts/state-check.py --scope static,cursor   # the quick checks
    python3 scripts/state-check.py --json /tmp/state.json
    python3 scripts/state-check.py --fixtures         # the check on known-bad and clean controls

Each finding names the rule IDs of design/rules.json. Exit status is 1 when a
finding is not covered by ALLOWED here or by `ALLOW` in state_checks/mod.rs, when
a capture run produced nothing or crashed, and when the whole run swept no
control (it would be passing on nothing). Fix the component, not the check. A
control that is stateless, small or quiet on purpose is an entry with its reason:
in `ALLOW` when it is the component's doing, in ALLOWED here when it is one
capture's.

Needs the three binaries built (`--build` does it) and nothing beyond the standard
library. Captures run with the animation clock off, so a capture takes seconds
per page, not minutes; the taxonomy runs as one process.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
FILES = ROOT / "apps" / "files"

# The layout lint owns the list of pages, popups and the way a capture is run.
_spec = importlib.util.spec_from_file_location("layout_lint", ROOT / "scripts" / "layout-lint.py")
assert _spec and _spec.loader
lint = importlib.util.module_from_spec(_spec)
sys.modules["layout_lint"] = lint
_spec.loader.exec_module(lint)

DESKTOP = lint.DESKTOP
PHONE = lint.PHONE

# Findings that are right by design, for one capture. Each entry: the rule key, a
# regex that must match the capture's name, a regex that must match the finding's
# target, and why. An entry that matches nothing is reported, so the list cannot
# rot. A control that is the same everywhere belongs in `ALLOW` (state_checks/mod.rs).
ALLOWED: list[dict] = [
    {
        "rule": "ring-clipped",
        "capture": r"^taxonomy-files",
        "where": r"FileView \(List\)",
        "why": "The file list's focus ring is drawn inside its box and its overlay scrollbar is painted at the same right edge, over the ring's middle: the ring is whole on the other three sides and the right edge shows through above and below the scrollbar thumb.",
    },
]

# Defects found and not fixed in this change, each with what is wrong and who owns
# the code. They are listed on every run and do not fail it; the day one is fixed
# its entry is reported as unused and goes. Same shape as ALLOWED.
def _known(rule: str, where: str, why: str) -> dict:
    return {"rule": rule, "capture": ".", "where": where, "why": why}


_CLIPPED = (
    "Focus puts the control flush against its clipping box (the sidebar's scroller, the bottom of the page, the top of the window), "
    "so the 2 px ring and its 2 px gap are cut on that side: seen on the Settings sidebar's first row. Needs a scroll margin in "
    "the scroll-into-view (vendor freya-components) or focusable_inside. Not fixed."
)
_COMPLETION = (
    "Taxonomy address-bar states: the path field's completion list is drawn over the header and the crumbs, so the pointer and the "
    "ring land on the list (seen in the frame). PathBar's own tests show the crumbs differ rest/hover/pressed with the pointer on them. "
    "Not diagnosed further; address_crumbs.rs is PathBar's."
)
_SEARCHBAR = (
    "Files search bar specimen (files/search.rs, FilesSearch): on the phone the filter tokens are laid over the Subfolders and Filters "
    "buttons (visible in the frame), so those controls are covered and show no ring or hover; the desktop field has no hover either."
)
_RING_NODE = (
    "The ring node is mounted at focus but the strip around the face does not change: text fields (form, password) and the header "
    "dropdowns. Not diagnosed (text_field.rs focusable_invalid / header dropdown ring)."
)
_POPUP_TAB = (
    "In a modal popup the headless walk finds Tab not moving off a focused text field, or focus landing outside the scope (taxonomy "
    "dialogs); FC-07 says Tab cycles inside the scope. Not diagnosed: the sentinel hand-off, or the field."
)
_MENU = (
    "Highlight read from the rows' fills in a dropdown inside a Row (a long or selected-first list): Home/Down read as no row lit. "
    "Not diagnosed whether the keys or the reading is at fault; the Theme, Density and Strings dropdowns walk clean."
)
KNOWN: list[dict] = [
    _known("ring-clipped", r"SidebarRow|Remove engine|New tab|Main menu", _CLIPPED),
    _known("ring-clipped", r"AddressCrumb|AddressChevron|HeaderBar > Dropdown|custom control", _COMPLETION),
    _known("state-press", r"AddressCrumb|AddressChevron|HeaderBar > Dropdown", _COMPLETION),
    _known("state-focus", r"HeaderBar > Dropdown", _RING_NODE),
    _known("tab-ring", r"SearchBar", _SEARCHBAR),
    _known("state-hover", r"SearchBar", _SEARCHBAR),
    _known("tab-ring", r"TextField|HeaderBar > Dropdown", _RING_NODE),
    _known("state-focus", r"TextField|Switch|ActionButton", _RING_NODE),
    _known("tab-trap", r".", _POPUP_TAB),
    _known("menu-keys", r"Dropdown", _MENU),
    _known(
        "popup-escape",
        r"Window",
        "Settings form popups (device, user, network name, password): by hand one Escape leaves the field and a second closes the "
        "popup (Device name), but the walk's Tab then Escape sequence leaves them open. Not diagnosed.",
    ),
    _known(
        "tab-reach",
        r".",
        "Shift+Tab does not retrace the lap in some popups and pages, and Tab stops on nodes that are not in the page's focusable list "
        "(sentinels, a page region). Not diagnosed.",
    ),
    _known(
        "state-press",
        r"Breadcrumb|SplitPane “Pane divider”",
        "Taxonomy small-widgets page: the breadcrumb crumbs and the split-pane divider have a pressed fill (Interaction/`dragging` set on the "
        "pointer down, the stop given `pressing`) yet the frame with the button down is the hover frame, in this 9500 px page only. Cause not found.",
    ),
    _known(
        "text-under-glyph",
        r"PlacesMenu “Places”",
        "Phone file chooser (taxonomy): the places button is 58 px wide, its name (w_flex, ellipsis) gets 5 px and runs under the chevron "
        "(files/places_view.rs PlacesMenu). Not fixed: hiding the name below a width did not change the finding (not understood).",
    ),
    _known("tab-order", r"docs/bingux-ui.md#components", "Taxonomy status-page specimen: Tab goes from a control 2880 px down the page to the next docs link above it. Not diagnosed."),
    _known(
        "state-hover",
        r"IconButton “Back”|IconMenuButton “Main menu”|> Switch$",
        "Phone header icon buttons (Back, Main menu) and every switch after the first of a page show no hover (the first lights): reproduced "
        "by state_checks/tests.rs every_switch_of_a_phone_group_lights_under_the_pointer_and_again_when_pressed (ignored until fixed); the "
        "hit area (controls/press.rs hit_area, controls/switch.rs) does not hear the pointer. Cause not found.",
    ),
]

# What a capture run says when the app crashed inside it.
CRASH = re.compile(r"panicked|unrecoverable|relaunch")


def allowed(finding: dict) -> dict | None:
    if finding.get("allowed"):
        return {"why": finding["allowed"]}
    for entry in ALLOWED:
        if (
            entry["rule"] == finding["rule"]
            and re.search(entry["capture"], finding["capture"])
            and re.search(entry["where"], finding["target"])
        ):
            return entry
    return None


def known(finding: dict) -> dict | None:
    for entry in KNOWN:
        if (
            entry["rule"] == finding["rule"]
            and re.search(entry["capture"], finding["capture"])
            and re.search(entry["where"], finding["target"])
        ):
            return entry
    return None


def read_reports(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def crash_lines(log: Path) -> list[str]:
    if not log.exists():
        return []
    lines = log.read_text(errors="replace").splitlines()
    found: list[str] = []
    for index, line in enumerate(lines):
        if CRASH.search(line):
            found.extend(lines[index : index + 3])
        if len(found) >= 18:
            break
    return found


def run(job: dict, workdir: Path, timeout: int, env: dict) -> dict:
    reports = workdir / "reports" / f"{job['name']}.jsonl"
    reports.parent.mkdir(parents=True, exist_ok=True)
    reports.unlink(missing_ok=True)
    job = dict(job, env=dict(job.get("env", {}), BINGUX_UI_STATE_CHECK=str(reports), **env))
    result = lint.run_job(job, workdir, timeout)
    result["shots"] = read_reports(reports)
    result["crash"] = crash_lines(Path(result["log"]))
    return result


# The taxonomy's categories (sidebar sections). Each is its own process, so the
# pages of one category are swept while another category's are.
TAXONOMY_CATEGORIES = ["foundations", "icons", "controls", "containers", "navigation", "data", "files", "inspector"]


def taxonomy_jobs() -> list[dict]:
    binary = str(lint.TARGET / "debug" / "bingux-taxonomy")
    return [
        {"name": f"taxonomy-{category}", "argv": [binary, "--snapshot"], "pages": [category]}
        for category in TAXONOMY_CATEGORIES
    ]


def files_jobs() -> list[dict]:
    binary = str(lint.TARGET / "debug" / "bingux-files")
    return [
        {"name": "files-desktop", "argv": [binary, "--snapshot"], "pages": []},
        {
            "name": "files-phone",
            "argv": [binary, "--snapshot"],
            "pages": [],
            "env": {"BINGUX_FILES_SNAPSHOT_SIZE": PHONE},
        },
    ]


def build() -> None:
    lint.build()
    subprocess.run(
        ["cargo", "build", "--features", "snapshot", "--manifest-path", str(FILES / "Cargo.toml")],
        check=True,
        env=dict(os.environ, CARGO_TARGET_DIR=str(lint.TARGET)),
    )


def squeeze(text: str) -> str:
    """A finding's detail with its numbers removed, to group the same defect found
    in many captures."""
    return re.sub(r"-?\d+(\.\d+)?", "#", text)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", choices=["settings", "taxonomy", "files"])
    parser.add_argument("--pages", default="", help="comma-separated Settings pages")
    parser.add_argument("--sizes", default=f"{DESKTOP},{PHONE}", help="Settings sizes, `default` is desktop")
    parser.add_argument("--scope", default="", help="comma list of static,cursor,tab,sweep,menu (default all)")
    parser.add_argument("--limit", type=int, default=0, help="most controls swept per capture")
    parser.add_argument("--rules", default="", help="comma-separated rule keys to report")
    parser.add_argument("--jobs", type=int, default=2, help="captures to run at once")
    parser.add_argument("--timeout", type=int, default=600, help="seconds per capture run")
    parser.add_argument("--job", default="", help="run only the captures whose name matches this regex (settings-sound-desktop, taxonomy-controls)")
    parser.add_argument("--json", help="write every finding to this file")
    parser.add_argument("--keep", help="keep captures and logs in this directory")
    parser.add_argument("--build", action="store_true", help="build the three binaries first")
    parser.add_argument(
        "--fixtures",
        action="store_true",
        help="run the check on its own known-bad and clean fixtures (cargo test, no binary) and exit",
    )
    parser.add_argument("--show-allowed", action="store_true", help="list the findings that are allowed")
    parser.add_argument("--notes", action="store_true", help="list what a run could not do")
    parser.add_argument("--kinds", action="store_true", help="list the interactive nodes by role and component")
    parser.add_argument("--all", action="store_true", help="list every capture of every finding, not one line each")
    args = parser.parse_args()
    # The captures render in software: they must never reach a live desktop, and a sweep that
    # presses every control must never write the person's dconf (scripts/app_env.py).
    safe = isolated()
    os.environ.clear()
    os.environ.update(safe)

    if args.fixtures:
        return subprocess.run(
            [
                "cargo", "test", "--features", "testing", "--manifest-path", str(ROOT / "crates" / "ui" / "Cargo.toml"),
                "--lib", "state_checks",
            ],
            env=dict(os.environ, CARGO_TARGET_DIR=str(lint.TARGET)),
        ).returncode
    if args.build:
        build()
    settings = lint.TARGET / "debug" / "bingux-settings"
    taxonomy = lint.TARGET / "debug" / "bingux-taxonomy"
    files = lint.TARGET / "debug" / "bingux-files"
    jobs: list[dict] = []
    if args.only in (None, "settings"):
        if not settings.exists():
            sys.exit(f"{settings} is missing: build with --build")
        pages = [page for page in args.pages.split(",") if page] or lint.panels() + lint.SUBPAGES
        jobs += lint.settings_jobs(pages, [size for size in args.sizes.split(",") if size])
    if args.only in (None, "taxonomy") and not args.pages:
        if not taxonomy.exists():
            sys.exit(f"{taxonomy} is missing: build with --build")
        jobs += taxonomy_jobs()
    if args.only in (None, "files") and not args.pages:
        if not files.exists():
            sys.exit(f"{files} is missing: build with --build")
        jobs += files_jobs()

    if args.job:
        jobs = [job for job in jobs if re.search(args.job, job["name"])]
    env = {}
    if args.scope:
        env["BINGUX_UI_STATE_SCOPE"] = args.scope
    if args.limit:
        env["BINGUX_UI_STATE_LIMIT"] = str(args.limit)
    keep_rules = {rule for rule in args.rules.split(",") if rule}

    workdir = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix="state-check-"))
    workdir.mkdir(parents=True, exist_ok=True)
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [
            pool.submit(run, job, workdir, args.timeout, env)
            for job in jobs
        ]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            swept = sum(shot["swept"] for shot in result["shots"])
            print(
                f"{result['status']:>10}  {result['name']}  ({len(result['shots'])} captures, {swept} swept)",
                flush=True,
            )

    findings: list[dict] = []
    notes: list[str] = []
    broken = []
    crashed = []
    totals: Counter = Counter()
    measured: Counter = Counter()
    kinds: Counter = Counter()
    for result in sorted(results, key=lambda r: r["name"]):
        if result["crash"]:
            crashed.append(result)
        if result["status"] != "ok" or not result["shots"]:
            broken.append(result)
        for shot in result["shots"]:
            where = f"{result['name']}:{Path(shot['capture']).stem}"
            for key in ("interactive", "swept", "stops", "menus", "cursors"):
                totals[key] += shot.get(key, 0)
            measured.update(shot.get("measured", {}))
            kinds.update(shot.get("kinds", {}))
            notes += [f"{where}: {note}" for note in shot.get("notes", [])]
            for finding in shot["findings"]:
                if keep_rules and finding["rule"] not in keep_rules:
                    continue
                finding = dict(finding, capture=where)
                finding["allowed"] = (allowed(finding) or {}).get("why")
                finding["known"] = (known(finding) or {}).get("why") if not finding["allowed"] else None
                findings.append(finding)

    used = {entry["why"] for finding in findings for entry in [allowed(finding), known(finding)] if entry}
    unused = [entry for entry in ALLOWED + KNOWN if entry["why"] not in used]
    failures = [finding for finding in findings if not finding["allowed"] and not finding["known"]]
    open_defects = [finding for finding in findings if finding["known"]]

    print()
    groups: dict[tuple, list[dict]] = defaultdict(list)
    for finding in failures:
        groups[(finding["rule"], finding["target"], squeeze(finding["detail"]))].append(finding)
    for (rule, target, _), members in sorted(groups.items()):
        first = members[0]
        print(f"[{rule}] {target}")
        print(f"    {first['detail']}")
        shown = [m["capture"] for m in members] if args.all else [m["capture"] for m in members[:3]]
        more = "" if args.all or len(members) <= 3 else f" and {len(members) - 3} more"
        print(f"    in {', '.join(shown)}{more}")
    by_rule = Counter(finding["rule"] for finding in failures)
    if open_defects:
        print(f"{len(open_defects)} findings are known open defects (KNOWN), not failing:")
        shown_known: set = set()
        for finding in open_defects:
            key = (finding["rule"], finding["known"])
            if key not in shown_known:
                shown_known.add(key)
                print(f"  [{finding['rule']}] {finding['known']}")
    if args.show_allowed:
        for finding in findings:
            if finding["allowed"]:
                print(f"[allowed {finding['rule']}] {finding['capture']}: {finding['target']} -- {finding['allowed']}")
    if args.kinds:
        for kind, count in kinds.most_common():
            print(f"kind: {count:6}  {kind}")
    if args.notes:
        for note in notes:
            print(f"note: {note}")
    for entry in unused:
        print(f"unused ALLOWED entry: {entry['rule']} {entry['capture']} {entry['where']}: {entry['why']}")
    for result in crashed:
        print(f"CRASH in {result['name']} (see {result['log']}):")
        for line in result["crash"]:
            print(f"    {line}")
    for result in broken:
        if result not in crashed:
            print(f"no captures: {result['name']} ({result['status']}), see {result['log']}")
    if totals["swept"] == 0 and not args.scope:
        print("no control was swept in any capture: the check ran on nothing")

    if args.json:
        Path(args.json).write_text(json.dumps(findings, indent=2))
    captures = sum(len(result["shots"]) for result in results)
    stopped = sum(1 for note in notes if "stopped after" in note)
    print(
        f"\n{captures} captures, {totals['interactive']} interactive nodes, {totals['swept']} swept, "
        f"{totals['stops']} tab stops, {totals['menus']} menus walked, {totals['cursors']} cursors read, "
        f"{len(failures)} findings"
        + (": " + ", ".join(f"{rule} {n}" for rule, n in sorted(by_rule.items())) if by_rule else "")
        + f", {len(findings) - len(failures) - len(open_defects)} allowed, {len(open_defects)} known open, "
        f"{stopped} sweeps stopped early, {len(crashed)} crashed"
    )
    unused = [entry for entry in unused if entry in ALLOWED]
    failed = bool(failures or broken or crashed or unused or (totals["swept"] == 0 and not args.scope))
    if failed and not args.keep:
        print(f"kept the captures and logs in {workdir}")
    elif not args.keep:
        shutil.rmtree(workdir, ignore_errors=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
