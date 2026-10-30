#!/usr/bin/env python3
"""Fail when a window's structure changes between its first frame and its settled one.

CK-02 of design/rules.md, LS-01, LS-02, LS-03, LS-06 and TX-16: a page draws its
final layout on its first frame and only values change when the data lands; a tab
switch or a new tab, a popup opening and a banner appearing while a display is
dragged do not move the rest of the window, not even for one frame.

`scripts/layout-stability.py` holds that for the Settings pages by pixels: it
records every 16 ms frame and matches the edges between frames. This holds it for
everything else too, by the layout tree, which every capture can give: each scenario
captures one frame as early as the harness allows and one after the window has
settled (BINGUX_UI_PROBE, crates/ui/src/inspector/probe_checks), and the
two trees are compared node by node. There is no list of pages that are exempt.

Scenarios:

  * every Settings page and subpage, at the desktop size and at 390x800, with the
    result of every background job held back for 1.5 s (BINGUX_UI_RESULT_DELAY_MS), so
    the first frame is the loading state;
  * every Settings popup, dialog and sheet that a click opens: the frame after the
    click against the frame 1.5 s later, at both sizes;
  * every bingux-taxonomy page: the first frame against the settled one;
  * Bingux Files: the folder list, a new tab (Ctrl+T), switching back to the first
    tab, the Properties dialog and the preview pane: the frame the key press gives
    against the settled one;
  * Displays: a display held mid-drag against the page before the drag, where a banner
    that appears must not push what is below it (LS-03).

    python3 scripts/structure-stability.py                 # everything
    python3 scripts/structure-stability.py --only files
    python3 scripts/structure-stability.py --pages network,sound --sizes 390x800
    python3 scripts/structure-stability.py --fixtures      # the comparison on known-bad and clean trees only
    python3 scripts/structure-stability.py --show-allowed

How two trees are compared. Nodes are paired by their path from the root (each
step is the node's title and how many of that title came before it under its
parent). A paired node is a shift when its position inside its parent, or its size,
changed by more than a pixel: what moved with its parent did not move. A text node
is judged on its vertical position and its height only (a value that changes length
moves its own left or right edge, which is not layout). Only the outermost node
of a shift is reported. A node that is only in one tree is `appears` or
`disappears` when it is a component of 16 px or more that takes part in the layout
(an overlay does not push anything), and a page title block that is not on the first
frame is `title-late` (TX-16). Everything that is in a scroll container compares its
position inside it, so a list that is scrolled by the same amount is not a shift.

Exit status is 1 on a shift that ALLOWED does not cover, an ALLOWED entry that matches
nothing, a run that produced nothing, and a fixture that is not judged as it should be.
Fix the page: give the late rows a placeholder of their height (docs/settings.md,
"Loading"). ALLOWED is for what is meant to change size, each with its reason.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import probe_lib as lib  # noqa: E402
from probe_lib import Allow, Finding, Probe  # noqa: E402

lint = lib.lint
TOLERANCE = 1.0
MIN_SIZE = 16.0
RESULT_DELAY_MS = 1500
SETTLE_AFTER_MS = 3500

PLAIN = {"row", "column", "element", "text"}

# The Network subpages that frame-time.py walks with the fake network, plus the pages
# that load their own data.
EXTRA_SUBPAGES = [
    "network/proxy",
    "network/usage",
    "network/add-vpn",
    "network/mobile/advanced",
    "network/mobile/usage",
    "network/ethernet/enp42s0",
    "network/ethernet/enp42s0/profile",
    "network/details/Wired%20connection%201",
]

# What is meant to change size between the first frame and the settled one: rule key,
# scenario regex, node regex, and why.
ALLOWED: list[dict] = [
    {
        "rule": "layout-shift",
        "scenario": r"^files-new-tab-desktop$",
        "node": r"^HeaderBar/TabStrip ",
        "why": "A tab opens by growing its width over MOVE_MS (instant under reduced motion): design/tabs.md, 'Width animation on add/close'. Only the strip's own tabs move; the page below the strip does not (the check reports the outermost shift and found none there).",
    },
    {
        "rule": "layout-shift",
        "scenario": r"^files-preview-desktop$",
        "node": r"^FileChooser/FileView ",
        "why": "Alt+P opens the preview pane, which takes 360 px from the file view over MOVE_MS: a change the user asked for, not one the loading makes (docs/files.md, 'Preview'). The first frame after the key still has the view at full width.",
    },
    {
        "rule": "appears",
        "scenario": r"^files-list-phone$",
        "node": r"^FileView/ScrollView ",
        "why": "A folder's rows arrive with its listing, which the loader reads on a worker (docs/files.md, the loader); the view box is on the first frame and only the rows are not. A placeholder row count cannot be known before the listing. Open: the phone list has no scroll container until then, so its scrollbar and rows appear together.",
    },
]


# ---- the comparison ----------------------------------------------------------------


def signatures(probe: Probe) -> dict[int, tuple]:
    """Each node's path from the root as (title, index among same-titled siblings) steps."""
    sigs: dict[int, tuple] = {}
    counts: dict[tuple[int, str], int] = {}
    for index in range(len(probe.nodes)):
        parent, title = probe.parent(index), probe.title(index)
        at = counts.get((parent, title), 0)
        counts[(parent, title)] = at + 1
        sigs[index] = (sigs[parent] if parent >= 0 else ()) + ((title, at),)
    return sigs


def in_layout(probe: Probe, index: int) -> bool:
    """Whether a node is on screen and in the flow: visible, not inert, no overlay above it."""
    flags = probe.flags(index)
    if not flags & lib.VISIBLE or flags & lib.INERT:
        return False
    return not any(probe.flags(up) & lib.OUT_OF_FLOW for up in [index, *probe.ancestors(index)])


def comparable(probe: Probe, index: int) -> bool:
    flags = probe.flags(index)
    title = probe.title(index)
    return bool(flags & lib.TEXT or flags & lib.PAINTS or title not in PLAIN)


def label_of(probe: Probe, index: int) -> str:
    """A name for a node in a finding: its component and the first text inside it."""
    kids = probe.children()
    stack = [index]
    text = ""
    while stack and not text:
        cur = stack.pop(0)
        if probe.flags(cur) & lib.TEXT:
            text = probe.text(cur).strip()
        stack += kids.get(cur, [])
    where = probe.path(index).split("/")[-2:]
    name = "/".join(where) if where != [""] else probe.title(index)
    return f'{name} "{text[:36]}"' if text else name


def relative(probe: Probe, index: int) -> tuple[float, float, float, float]:
    """A node's box with its origin taken from its parent's, and the scroll offset left in."""
    x, y, w, h = probe.box(index)
    parent = probe.parent(index)
    if parent < 0:
        return x, y, w, h
    px, py, _, _ = probe.box(parent)
    return x - px, y - py, w, h


def diff(first: Probe, last: Probe, scenario: str) -> list[Finding]:
    """The shifts, appearances and disappearances between two captures of one window."""
    found: list[Finding] = []
    first_sigs, last_sigs = signatures(first), signatures(last)
    by_sig_first = {sig: index for index, sig in first_sigs.items()}
    by_sig_last = {sig: index for index, sig in last_sigs.items()}
    reported: set[int] = set()  # indexes in `last` already reported, for the cascade
    for index in range(len(last.nodes)):
        if not in_layout(last, index) or not comparable(last, index):
            continue
        sig = last_sigs[index]
        before = by_sig_first.get(sig)
        if before is None:
            continue
        if not in_layout(first, before):
            continue
        if any(up in reported for up in last.ancestors(index)):
            continue
        a, b = relative(first, before), relative(last, index)
        is_text = bool(last.flags(index) & lib.TEXT)
        if is_text:
            deltas = {"y": b[1] - a[1], "height": b[3] - a[3]}
        else:
            deltas = {"x": b[0] - a[0], "y": b[1] - a[1], "width": b[2] - a[2], "height": b[3] - a[3]}
        worst = max(abs(v) for v in deltas.values())
        if worst <= TOLERANCE:
            continue
        reported.add(index)
        moved = ", ".join(f"{k} {v:+.1f}" for k, v in deltas.items() if abs(v) > TOLERANCE)
        found.append(
            Finding(
                "layout-shift",
                ["LS-01", "LS-02"],
                scenario,
                label_of(last, index),
                f"moved or resized between the first frame and the settled one ({moved} px)",
                {k: round(v, 1) for k, v in deltas.items() if abs(v) > TOLERANCE},
            )
        )
    for sig, index in by_sig_last.items():
        if sig in by_sig_first or not in_layout(last, index):
            continue
        if any(up in reported for up in last.ancestors(index)):
            continue
        title = last.title(index)
        _, _, w, h = last.box(index)
        if title in PLAIN or min(w, h) < MIN_SIZE and not last.flags(index) & lib.TEXT:
            continue
        if last.flags(index) & lib.TEXT and not last.text(index).strip():
            continue
        # A node under one that appeared is the appearing node's own content.
        if any(up in reported or last_sigs[up] not in by_sig_first for up in last.ancestors(index)):
            continue
        late_title = title == "PageHead"
        reported.add(index)
        found.append(
            Finding(
                "title-late" if late_title else "appears",
                ["TX-16", "LS-01"] if late_title else ["LS-01"],
                scenario,
                label_of(last, index),
                "the page title block is not on the first frame" if late_title else f"is not on the first frame ({w:.0f} x {h:.0f} px): it appears after the data lands",
                {"width": round(w, 1), "height": round(h, 1)},
            )
        )
    for sig, index in by_sig_first.items():
        if sig in by_sig_last or not in_layout(first, index):
            continue
        title = first.title(index)
        _, _, w, h = first.box(index)
        if title in PLAIN or min(w, h) < MIN_SIZE:
            continue
        if any(first_sigs[up] not in by_sig_last for up in first.ancestors(index)):
            continue
        found.append(
            Finding(
                "disappears",
                ["LS-01"],
                scenario,
                label_of(first, index),
                f"is on the first frame ({w:.0f} x {h:.0f} px) and gone when settled: a placeholder that is replaced by something else",
                {"width": round(w, 1), "height": round(h, 1)},
            )
        )
    return found


# ---- scenarios -----------------------------------------------------------------------


class Scenario:
    def __init__(self, name: str, job: dict, first: str, extra_env: dict[str, str] | None = None):
        self.name = name
        self.job = job
        self.first = first  # the stem of the first capture; the last capture of the job is the settled one
        self.extra_env = extra_env or {}


def settings_scenarios(pages: list[str], sizes: list[str], popups: bool = True) -> list[Scenario]:
    binary = str(lib.TARGET / "debug" / "bingux-settings")
    out: list[Scenario] = []
    delay = {"BINGUX_UI_RESULT_DELAY_MS": str(RESULT_DELAY_MS), "BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS": "16"}
    for size in sizes:
        tag = "phone" if size == lib.PHONE else "desktop"
        for page in pages:
            name = f"settings-{page.replace('/', '_')}-{tag}"
            job = {
                "name": name,
                "argv": [binary, "--snapshot"],
                "pages": [page],
                "size": size,
                "script": f"save first.png; wait {RESULT_DELAY_MS + SETTLE_AFTER_MS}",
            }
            out.append(Scenario(name, job, "first", delay))
        for popup, page, click in lint.POPUPS if popups else []:
            name = f"popup-{popup}-{tag}"
            job = {
                "name": name,
                "argv": [binary, "--snapshot"],
                "pages": [page],
                "size": size,
                "script": f"wait {RESULT_DELAY_MS + 1500}; {click}; save popup-first.png; wait 1800; save popup-last.png",
            }
            out.append(Scenario(name, job, "popup-first", {"BINGUX_UI_RESULT_DELAY_MS": str(RESULT_DELAY_MS)}))
    return out


def displays_scenario() -> list[Scenario]:
    """Displays with two screens, one held mid-drag: the banner that offers Apply must not push the page."""
    binary = str(lib.TARGET / "debug" / "bingux-settings")
    job = {
        "name": "displays-drag-desktop",
        "argv": [binary, "--snapshot"],
        "pages": ["display"],
        "script": "wait 1500; save before.png; drag 380,330,330,330; wait 800; save dragged.png",
        "env": {"BINGUX_SETTINGS_FAKE_DISPLAYS": "2"},
    }
    return [Scenario("displays-drag-desktop", job, "before")]


def taxonomy_scenario() -> list[Scenario]:
    binary = str(lib.TARGET / "debug" / "bingux-taxonomy")
    job = {"name": "taxonomy-walk", "argv": [binary, "--snapshot"], "pages": []}
    return [
        Scenario(
            "taxonomy",
            job,
            "first",
            {
                "BINGUX_SNAPSHOT_SETTLE_MS": "16",
                "BINGUX_SNAPSHOT_SCRIPT": f"save {{out}}.first.png; wait {SETTLE_AFTER_MS}; save {{out}}.last.png",
            },
        )
    ]


def files_scenarios(workdir: Path) -> list[Scenario]:
    binary = str(lib.TARGET / "debug" / "bingux-files")
    folders = lint.files_fixture(workdir)
    home = workdir / "files-home"
    state = {
        "HOME": str(home),
        "XDG_STATE_HOME": str(home / "state"),
        "XDG_CONFIG_HOME": str(home / "config"),
        "XDG_DATA_HOME": str(home / "data"),
        "BINGUX_FILES_SNAPSHOT_SETTLE_MS": "16",
    }
    walks = {
        "list": "save first.png; wait 3000",
        "new-tab": "wait 1500; key ctrl+t; save first.png; wait 1500",
        "switch-tab": "wait 1500; key ctrl+t; wait 1200; key alt+1; save first.png; wait 1500",
        "properties": "wait 1500; key ctrl+a; wait 300; key alt+return; save first.png; wait 1500",
        "preview": "wait 1500; key alt+p; save first.png; wait 1500",
    }
    out = []
    for size, tag in (("1100x760", "desktop"), ("390x800", "phone")):
        for name, script in walks.items():
            job = {
                "name": f"files-{name}-{tag}",
                "argv": [binary, "--snapshot"],
                "pages": [str(folders["file"])],
                "env": dict(state, BINGUX_FILES_SNAPSHOT_SIZE=size, BINGUX_FILES_SNAPSHOT_SCRIPT=script),
            }
            out.append(Scenario(job["name"], job, "first"))
    return out


def run_scenario(scenario: Scenario, workdir: Path, timeout: int, variant: lib.Variant) -> dict:
    job = dict(scenario.job)
    result = lib.run_job(job, workdir, timeout * (30 if scenario.name == "taxonomy" else 1), variant, scenario.extra_env)
    result["scenario"] = scenario
    return result


def judged(result: dict) -> list[Finding]:
    """The findings of one run: pair the first capture with the settled one."""
    scenario: Scenario = result["scenario"]
    probes: list[Probe] = result["probes"]
    findings: list[Finding] = []
    if scenario.name == "taxonomy":
        firsts = {Path(p.capture).name.removesuffix(".first.png"): p for p in probes if p.capture.endswith(".first.png")}
        lasts = {Path(p.capture).name.removesuffix(".last.png"): p for p in probes if p.capture.endswith(".last.png")}
        for page, first in sorted(firsts.items()):
            if page in lasts:
                findings += diff(first, lasts[page], f"taxonomy:{page}")
        result["pairs"] = len(firsts)
        return findings
    first = next((p for p in probes if Path(p.capture).stem == scenario.first), None)
    last = probes[-1] if probes else None
    if first is None or last is None or first is last:
        result["probes"] = []  # the run did not capture both frames: reported as no captures
        return findings
    result["pairs"] = 1
    return diff(first, last, scenario.name)


# ---- fixtures --------------------------------------------------------------------------


def fixture(rows: list[tuple[int, str, float, float, float, float, int, int]], strings: list[str] | None = None) -> Probe:
    titles = sorted({row[1] for row in rows} | {"Window"})
    nodes = [(row[0], titles.index(row[1]), row[2], row[3], row[4], row[5], row[6], row[7]) for row in rows]
    return Probe("fixture", 1.0, (400.0, 300.0), titles, strings or ["Wi-Fi"], nodes, [], [], [], "fixture")


def run_fixtures() -> list[str]:
    problems = []
    V = lib.VISIBLE
    page = (-1, "Window", 0, 0, 400, 300, V, -1)
    # A page with a header and one row.
    clean_a = fixture([page, (0, "Row", 10, 100, 380, 56, V | lib.PAINTS, -1), (0, "Row", 10, 160, 380, 56, V | lib.PAINTS, -1)])
    same = fixture([page, (0, "Row", 10, 100, 380, 56, V | lib.PAINTS, -1), (0, "Row", 10, 160, 380, 56, V | lib.PAINTS, -1)])
    if diff(clean_a, same, "fixture-same"):
        problems.append("fixture: identical trees were reported as different")
    # The second row grows when its data lands: a shift.
    taller = fixture([page, (0, "Row", 10, 100, 380, 56, V | lib.PAINTS, -1), (0, "Row", 10, 160, 380, 80, V | lib.PAINTS, -1)])
    got = {f.rule for f in diff(clean_a, taller, "fixture-taller")}
    if "layout-shift" not in got:
        problems.append(f"fixture: a row that grew was not found (found {sorted(got)})")
    # A third row appears after the load.
    third = fixture([page, (0, "Row", 10, 100, 380, 56, V | lib.PAINTS, -1), (0, "Row", 10, 160, 380, 56, V | lib.PAINTS, -1), (0, "Row", 10, 220, 380, 56, V | lib.PAINTS, -1)])
    got = {f.rule for f in diff(clean_a, third, "fixture-third")}
    if "appears" not in got:
        problems.append(f"fixture: a row that appeared late was not found (found {sorted(got)})")
    # A page title block that arrives late.
    titled = fixture([page, (0, "PageHead", 10, 20, 380, 60, V, -1), (0, "Row", 10, 100, 380, 56, V | lib.PAINTS, -1), (0, "Row", 10, 160, 380, 56, V | lib.PAINTS, -1)])
    got = {f.rule for f in diff(clean_a, titled, "fixture-title")}
    if "title-late" not in got:
        problems.append(f"fixture: a late page title was not found (found {sorted(got)})")
    # A banner in the flow pushes the rows below it: found; the same banner as an overlay does not.
    pushed = fixture([page, (0, "Banner", 10, 40, 380, 50, V | lib.PAINTS, -1), (0, "Row", 10, 150, 380, 56, V | lib.PAINTS, -1), (0, "Row", 10, 210, 380, 56, V | lib.PAINTS, -1)])
    got = {f.rule for f in diff(clean_a, pushed, "fixture-banner")}
    if "layout-shift" not in got:
        problems.append(f"fixture: rows pushed down by a banner were not found (found {sorted(got)})")
    overlay = fixture([page, (0, "Banner", 10, 40, 380, 50, V | lib.PAINTS | lib.OUT_OF_FLOW, -1), (0, "Row", 10, 100, 380, 56, V | lib.PAINTS, -1), (0, "Row", 10, 160, 380, 56, V | lib.PAINTS, -1)])
    if diff(clean_a, overlay, "fixture-overlay"):
        problems.append("fixture: a banner that floats over the page was reported as a shift")
    # A value that changes length does not move its row.
    texty_a = fixture([page, (0, "Row", 10, 100, 380, 56, V | lib.PAINTS, -1), (1, "text", 300, 120, 40, 18, V | lib.TEXT, 0)])
    texty_b = fixture([page, (0, "Row", 10, 100, 380, 56, V | lib.PAINTS, -1), (1, "text", 250, 120, 90, 18, V | lib.TEXT, 0)])
    if diff(texty_a, texty_b, "fixture-text"):
        problems.append("fixture: a text that grew wider was reported as a shift")
    return problems


# ---- main -----------------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    lib.add_sweep_args(parser, variants="dark")
    parser.add_argument("--fixtures", action="store_true", help="only check the comparison on its own fixtures")
    args = parser.parse_args()

    problems = run_fixtures()
    for problem in problems:
        print(problem)
    print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
    if args.fixtures or problems:
        return 1 if problems else 0

    if args.build:
        lint.build()
    workdir = lib.workdir_of(args, "structure-stability-")
    sizes = [size for size in args.sizes.split(",") if size]
    scenarios: list[Scenario] = []
    if args.only in (None, "settings"):
        pages = [page for page in args.pages.split(",") if page] or lint.panels() + lint.SUBPAGES + EXTRA_SUBPAGES
        scenarios += settings_scenarios(pages, sizes, popups=not args.pages)
        if not args.pages:
            scenarios += displays_scenario()
    if args.only in (None, "taxonomy") and not args.pages:
        scenarios += taxonomy_scenario()
    if args.only in (None, "files") and not args.pages:
        scenarios += files_scenarios(workdir)
    for scenario in scenarios:
        binary = Path(scenario.job["argv"][0])
        if not binary.exists():
            sys.exit(f"{binary} is missing: build with --build")

    import concurrent.futures

    variant = lib.parse_variants(args.variants)[0]
    results: list[dict] = []
    findings: list[Finding] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [pool.submit(run_scenario, scenario, workdir, args.timeout, variant) for scenario in scenarios]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            found = judged(result)
            findings += found
            results.append(result)
            print(f"{result['status']:>10}  {result['scenario'].name}  ({len(found)} findings)", flush=True)
    pairs = sum(result.get("pairs", 0) for result in results)
    print(f"{pairs} first/settled pairs compared")
    allow = Allow([{"rule": e["rule"], "capture": e["scenario"], "where": e["node"], "why": e["why"]} for e in ALLOWED])
    return lib.finish("structure-stability", findings, allow, args, results, pairs, workdir, "pairs")


if __name__ == "__main__":
    sys.exit(main())
