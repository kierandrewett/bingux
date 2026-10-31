#!/usr/bin/env python3
"""Fail when a scroll position is lost on the way out and back.

CK-22 of design/rules.md, LS-05: "also im still seeing that thing where the scroll
resets when you go into a subpage. i want an actual fix not hacks". A scrolled page
keeps its offset through a push and a pop, through a tab switch, through a folder and
Back, and when the data it was waiting for lands.

Each scenario is a scripted walk of one capture run (`Snapshot::run_script`): scroll the
page, `save` its frame, do the thing, `save` again. Every capture also writes its probe
(BINGUX_UI_PROBE, crates/ui/src/inspector/probe_checks): the scroll containers
of the frame, with their offset. The check reads the page's own scroll container (the
largest one) at each save and requires the offset to be what it was before, within a pixel
(`--tolerance`), and, so that a walk that never scrolled cannot pass on zero, the first
offset to be at least 40 px.

  Settings   the Network page scrolled, a subpage (Wi-Fi, a row that is in view at that offset) pushed, then popped (Alt+Left); the page
             scrolled while its data is still on its way, then the data lands; the same push
             and pop with a wheel notch sent at EVERY frame of both slides, the subpage's own
             data held back 500 ms so it lands in the middle of the push
  Files      a long folder scrolled, then a new tab opened and the first tab returned to
             (Ctrl+T, Alt+1); the parent folder opened by its crumb and Back (Alt+Left)

    python3 scripts/scroll-continuity.py
    python3 scripts/scroll-continuity.py --only files
    python3 scripts/scroll-continuity.py --fixtures     # the comparison on known-bad and clean probes only
    python3 scripts/scroll-continuity.py --show-allowed

The slide walks (`settings-network-slide-*`) open a subpage that is in view at the scrolled offset
(Wi-Fi) and go Back, with a wheel notch sent at every 16 ms frame of the push, of the Back slide, of
both or of neither (the control); the notches stay inside the 250 ms slide, because one after it is
the person scrolling the page that arrived. Each saves a capture at every frame. They hold:

  the push happened   a text only the origin page has is gone from the settled subpage, so a click
                      that missed cannot pass
  offset kept         after Back the page is at the offset it had before the push, whatever the
                      wheel did during either slide (a notch during a slide is not delivered to
                      either page: `slide_shield` in shell/navigation.rs)
  no reflow           in every frame of both slides, each page against its own settled capture (a
                      page's own texts, never one page's boxes against the other's): a text is the
                      same distance down its page, and across by one slide offset shared by all of
                      the page's texts . A card that changes height, a row that
                      lands above others, the veil lifting into another structure: each moves a text.

Exit status is 1 when an offset is not kept, when a walk did not scroll, when a run
produced nothing, and on an ALLOWED entry that matches nothing. ALLOWED is for a page that
starts at the top on purpose when it is opened again, with its reason.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import probe_lib as lib  # noqa: E402
from probe_lib import Allow, Finding, Probe  # noqa: E402

lint = lib.lint
MIN_SCROLL = 40.0
TOLERANCE = 1.0

# A page that opens at its top on purpose: rule key, scenario regex, step regex, reason.
ALLOWED: list[dict] = []


class Scenario:
    def __init__(
        self,
        name: str,
        job: dict,
        compare: list[tuple[str, str]],
        rule_ids: list[str],
        extra_env: dict[str, str] | None = None,
        rows: bool = False,
        slide: bool = False,
    ):
        self.name = name
        self.job = job
        # (earlier save, later save): the offset of the page at the second equals the first.
        self.compare = compare
        self.rule_ids = rule_ids
        self.extra_env = extra_env or {}
        self.rows = rows
        self.slide = slide


# ---- the offset of the page -------------------------------------------------------------


def page_offset(probe: Probe) -> tuple[float, float] | None:
    """(offset, scrolled length) of the page's own scroll container: the largest one.

    A scroll view's offset is on the box that holds its content, which is the scroll view itself or
    its first child, so the larger of the two is the view's.
    """
    best = None
    for scroller in probe.scrollers:
        node = scroller["node"]
        x, y, w, h = probe.box(node)
        area = w * h
        offset = max(
            abs(other["offset"][1])
            for other in probe.scrollers
            if other["node"] == node or probe.parent(other["node"]) == node
        )
        inner = max(
            other["inner"][1]
            for other in probe.scrollers
            if other["node"] == node or probe.parent(other["node"]) == node
        )
        if offset > 0 or inner > h:
            if best is None or area > best[0]:
                best = (area, offset, inner)
    if best is None:
        return None
    return best[1], best[2]


def first_row(probe: Probe) -> str | None:
    """The name on the first file row that is in the list, below the column header: where a virtual list is scrolled to."""
    tops = [
        probe.box(i)[1] for i in range(len(probe.nodes)) if probe.title(i) == "ScrollView" and probe.box(i)[3] > 200
    ]
    if not tops:
        return None
    top = min(tops)
    rows = sorted(
        (probe.box(i)[1], i)
        for i in range(len(probe.nodes))
        if probe.title(i) == "FileRow" and probe.box(i)[1] >= top - 1
    )
    kids = probe.children()
    for _, row in rows:
        stack = [row]
        while stack:
            cur = stack.pop(0)
            if probe.flags(cur) & lib.TEXT and probe.text(cur).strip():
                return probe.text(cur).strip()
            stack += kids.get(cur, [])
    return None


def judge(
    name: str,
    ids: list[str],
    probes: dict[str, Probe],
    compare: list[tuple[str, str]],
    tolerance: float = TOLERANCE,
    rows: bool = False,
) -> list[Finding]:
    found: list[Finding] = []
    if rows:
        return judge_rows(name, ids, probes, compare)
    for before, after in compare:
        a, b = probes.get(before), probes.get(after)
        if a is None or b is None:
            found.append(
                Finding(
                    "scroll-walk-broken",
                    ids,
                    name,
                    f"{before} -> {after}",
                    f"the walk did not save {before if a is None else after}: its script stopped early",
                )
            )
            continue
        offset_a, offset_b = page_offset(a), page_offset(b)
        if offset_a is None or offset_b is None:
            found.append(
                Finding(
                    "scroll-walk-broken",
                    ids,
                    name,
                    f"{before} -> {after}",
                    "the capture has no scroll container: the page cannot scroll at this size",
                )
            )
            continue
        if offset_a[0] < MIN_SCROLL:
            found.append(
                Finding(
                    "scroll-walk-broken",
                    ids,
                    name,
                    f"{before}",
                    f"the page was scrolled only {offset_a[0]:.0f} px (of {offset_a[1]:.0f} px of content): the walk proves nothing; make the window smaller or scroll further",
                    {"offset": offset_a[0]},
                )
            )
            continue
        if abs(offset_a[0] - offset_b[0]) > tolerance:
            found.append(
                Finding(
                    "scroll-lost",
                    ids,
                    name,
                    f"{before} -> {after}",
                    f"the scroll offset was {offset_a[0]:.0f} px and is {offset_b[0]:.0f} px after: it is not kept",
                    {"before": offset_a[0], "after": offset_b[0]},
                )
            )
    return found


def judge_rows(name: str, ids: list[str], probes: dict[str, Probe], compare: list[tuple[str, str]]) -> list[Finding]:
    """A virtual list keeps no offset in the tree: it is where the first row is. `start` is the list before it was scrolled."""
    found: list[Finding] = []
    start = first_row(probes["start"]) if "start" in probes else None
    for before, after in compare:
        a, b = probes.get(before), probes.get(after)
        if a is None or b is None:
            found.append(
                Finding(
                    "scroll-walk-broken",
                    ids,
                    name,
                    f"{before} -> {after}",
                    f"the walk did not save {before if a is None else after}: its script stopped early",
                )
            )
            continue
        row_a, row_b = first_row(a), first_row(b)
        if row_a is None or row_b is None:
            found.append(
                Finding(
                    "scroll-walk-broken", ids, name, f"{before} -> {after}", "the capture has no file row in the list"
                )
            )
        elif start is not None and row_a == start:
            found.append(
                Finding(
                    "scroll-walk-broken",
                    ids,
                    name,
                    f"{before}",
                    f"the list is still at its first row {start!r} after the scroll: the walk proves nothing; scroll further",
                )
            )
        elif row_a != row_b:
            found.append(
                Finding(
                    "scroll-lost",
                    ids,
                    name,
                    f"{before} -> {after}",
                    f"the first row in the list was {row_a!r} and is {row_b!r} after: the scroll position is not kept",
                )
            )
    return found


# ---- the slide of a subpage ----------------------------------------------------------------------

SLIDE_TOLERANCE = 1.5  # px


def scroll_of(probe: Probe, index: int) -> float:
    """How far the nearest scroll container above a node is scrolled."""
    for node in [index, *probe.ancestors(index)]:
        held = [s for s in probe.scrollers if s["node"] == node or probe.parent(s["node"]) == node]
        if held:
            return max(abs(s["offset"][1]) for s in held)
    return 0.0


def page_texts(probe: Probe) -> dict[str, tuple[float, float]]:
    """Each text of a page's content that is mounted once: (x, y down its page, before any scroll).

    The sidebar and the header are not page content: they do not slide with the page."""
    seen: dict[str, list[tuple[float, float]]] = {}
    for i in range(len(probe.nodes)):
        if not probe.flags(i) & lib.TEXT or not any(probe.title(up) == "Page" for up in probe.ancestors(i)):
            continue
        text = probe.text(i).strip()
        if text:
            x, y, _, _ = probe.box(i)
            seen.setdefault(text, []).append((x, y + scroll_of(probe, i)))
    return {text: spots[0] for text, spots in seen.items() if len(spots) == 1}


def judge_slide(
    name: str, ids: list[str], probes: dict[str, Probe], origin: str, sub: str, prefix: str
) -> list[Finding]:
    """Frames `prefix`NN of a slide against the settled captures of the two pages, each page against itself.

    A page's own texts (the ones the other page does not have, so a text is never matched across the
    two pages) must be, in every frame, where the page settles: down the page exactly, and across by one
    slide offset that is the same for all of its texts. The page below moves across too on a pop (it comes in under the leaving one). This holds the layout under the slide still: a card that changes height, a row that
    arrives above others, the veil lifting into a different structure, all move a text off its settled
    place.
    """
    found: list[Finding] = []
    a, b = probes.get(origin), probes.get(sub)
    frames = sorted((k, p) for k, p in probes.items() if k.startswith(prefix) and k[len(prefix) :].isdigit())
    if a is None or b is None or not frames:
        return [
            Finding(
                "scroll-walk-broken",
                ids,
                name,
                prefix,
                f"the walk did not save {origin}, {sub} and the frames {prefix}NN",
            )
        ]
    ref_a, ref_b = page_texts(a), page_texts(b)
    own = {
        "origin": {t: v for t, v in ref_a.items() if t not in ref_b},
        "sub": {t: v for t, v in ref_b.items() if t not in ref_a},
    }
    compared = 0
    for key, probe in frames:
        now = page_texts(probe)
        for page, ref in own.items():
            there = [t for t in ref if t in now]
            if len(there) < 3:
                continue
            compared += 1
            steps = [now[t][0] - ref[t][0] for t in there]
            if max(steps) - min(steps) > SLIDE_TOLERANCE:
                worst = max(there, key=lambda t: abs(now[t][0] - ref[t][0] - steps[0]))
                found.append(
                    Finding(
                        "slide-reflow",
                        ids,
                        name,
                        key,
                        f"the {page} page's text is not one slide offset from its settled place: x offsets span {max(steps) - min(steps):.0f} px ({worst!r})",
                    )
                )
            moved = [(t, now[t][1] - ref[t][1]) for t in there if abs(now[t][1] - ref[t][1]) > SLIDE_TOLERANCE]
            if moved:
                text, dy = moved[0]
                found.append(
                    Finding(
                        "slide-reflow",
                        ids,
                        name,
                        key,
                        f"the {page} page's {text!r} is {dy:+.0f} px down its page from where it settles ({len(moved)} of {len(there)} texts): the layout changed under the slide",
                    )
                )
    if compared == 0:
        found.append(
            Finding(
                "scroll-walk-broken",
                ids,
                name,
                prefix,
                "no frame of the slide had three texts of a page: the walk proves nothing",
            )
        )
    return found


def pushed(name: str, ids: list[str], probes: dict[str, Probe], origin_only: str, step: str) -> list[Finding]:
    """The walk really opened the subpage: a text only the origin page has is gone from `step`'s capture."""
    probe = probes.get(step)
    if probe is None:
        return []
    if any(probe.text(i).strip() == origin_only for i in range(len(probe.nodes)) if probe.flags(i) & lib.TEXT):
        return [
            Finding(
                "scroll-walk-broken",
                ids,
                name,
                step,
                f"{origin_only!r} is still on screen after the click: the subpage was not opened, so the walk proves nothing (is the row in view at that offset?)",
            )
        ]
    return []


# ---- scenarios ----------------------------------------------------------------------------------


def settings_scenarios() -> list[Scenario]:
    binary = str(lib.TARGET / "debug" / "bingux-settings")
    size = "1000x520"

    def job(name: str, page: str, script: str, env: dict[str, str] | None = None) -> dict:
        return {
            "name": name,
            "argv": [binary, "--snapshot"],
            "pages": [page],
            "size": size,
            "script": script,
            "env": env or {},
        }

    return [
        Scenario(
            "settings-network-push-pop",
            job(
                "settings-network-push-pop",
                "network",
                "wait 1500; scroll 600,300,-220; wait 700; save a.png; click-text Wi-Fi; wait 1500; save sub.png; key alt+left; wait 1800; save b.png",
            ),
            [("a", "b")],
            ["LS-05"],
        ),
        Scenario(
            "settings-network-data-lands",
            job(
                "settings-network-data-lands",
                "network",
                "wait 400; scroll 600,300,-220; wait 500; save a.png; wait 3500; save b.png",
                {"BINGUX_UI_RESULT_DELAY_MS": "1500"},
            ),
            [("a", "b")],
            ["LS-05", "LS-06"],
            {"BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS": "16"},
        ),
    ]


SLIDE_FRAMES = 12  # a slide is 250 ms, 15 frames of 16 ms: the notches stay inside it


def slide_script(push_wheel: bool, back_wheel: bool) -> str:
    """The Network page scrolled, Proxy pushed and popped, a wheel notch sent at every frame of the slides asked for."""
    notch = "scroll 600,300,-40; "
    steps = ["wait 1500; scroll 600,300,-220; wait 700; save a.png; click-text Wi-Fi"]
    steps += [f"{notch if push_wheel else ''}wait 16; save p{i:02d}.png" for i in range(SLIDE_FRAMES)]
    steps += ["wait 1500; save sub.png; key alt+left"]
    steps += [f"{notch if back_wheel else ''}wait 16; save q{i:02d}.png" for i in range(SLIDE_FRAMES)]
    steps += ["wait 1800; save b.png"]
    return "; ".join(steps)


def slide_scenarios() -> list[Scenario]:
    binary = str(lib.TARGET / "debug" / "bingux-settings")

    def scenario(name: str, push_wheel: bool, back_wheel: bool) -> Scenario:
        job = {
            "name": name,
            "argv": [binary, "--snapshot"],
            "pages": ["network"],
            "size": "1000x520",
            "script": slide_script(push_wheel, back_wheel),
            "env": {"BINGUX_UI_RESULT_DELAY_MS": "500"},
        }
        return Scenario(name, job, [("a", "b")], ["LS-05", "LS-06"], slide=True)

    return [
        scenario("settings-network-slide-no-wheel", False, False),
        scenario("settings-network-slide-wheel-push", True, False),
        scenario("settings-network-slide-wheel-back", False, True),
        scenario("settings-network-slide-wheel-both", True, True),
    ]


def files_fixture(workdir: Path) -> Path:
    folder = workdir / "files-fixture" / "Long"
    (folder / "Sub").mkdir(parents=True, exist_ok=True)
    for i in range(400):
        (folder / f"File {i:04d}.txt").write_text("fixture\n")
    (folder / "Sub" / "inner.txt").write_text("fixture\n")
    return folder


def files_scenarios(workdir: Path) -> list[Scenario]:
    binary = str(lib.TARGET / "debug" / "bingux-files")
    folder = files_fixture(workdir)
    home = workdir / "files-home"
    state = {
        "HOME": str(home),
        "XDG_STATE_HOME": str(home / "state"),
        "XDG_CONFIG_HOME": str(home / "config"),
        "XDG_DATA_HOME": str(home / "data"),
        "BINGUX_FILES_SNAPSHOT_SIZE": "1100x600",
    }

    def job(name: str, script: str) -> dict:
        return {
            "name": name,
            "argv": [binary, "--snapshot"],
            "pages": [str(folder)],
            "env": dict(state, BINGUX_FILES_SNAPSHOT_SCRIPT=script),
        }

    return [
        Scenario(
            "files-tab-switch",
            job(
                "files-tab-switch",
                "wait 1500; save start.png; scroll 600,300,-900; wait 700; save a.png; key ctrl+t; wait 1200; key alt+1; wait 1200; save b.png",
            ),
            [("a", "b")],
            ["LS-05", "LS-02"],
            rows=True,
        ),
        Scenario(
            "files-parent-and-back",
            job(
                "files-parent-and-back",
                "wait 1500; save start.png; scroll 600,300,-900; wait 700; save a.png; click-text files-fixture; wait 1500; save sub.png; key alt+left; wait 1500; save b.png",
            ),
            [("a", "b")],
            ["LS-05"],
            rows=True,
        ),
    ]


# ---- fixtures ------------------------------------------------------------------------------------


def fixture(offset: float, inner: float = 2000.0) -> Probe:
    titles = ["Window", "ScrollView"]
    nodes = [(-1, 0, 0, 0, 1000, 500, 1, -1), (0, 1, 0, 50, 1000, 450, 1 | lib.SCROLLER, -1)]
    return Probe(
        "fixture",
        1.0,
        (1000.0, 500.0),
        titles,
        [],
        nodes,
        [],
        [],
        [{"node": 1, "offset": [0, -offset], "inner": [1000, inner]}],
        "fixture",
    )


def slide_fixture(words: list[str], dx: float = 0.0, dy: float = 0.0) -> Probe:
    """A page of three texts, `dx` across and the middle one `dy` down from where it rests."""
    nodes = [(-1, 0, 0, 0, 1000, 500, 1, -1), (0, 1, 300, 0, 700, 500, 1, -1)]
    for at, y in enumerate((50.0, 100.0 + dy, 150.0)):
        nodes.append((1, 2, 300 + dx, y, 100, 20, lib.VISIBLE | lib.TEXT, at))
    return Probe("fixture", 1.0, (1000.0, 500.0), ["Window", "Page", "Label"], words, nodes, [], [], [], "fixture")


def run_fixtures() -> list[str]:
    problems = []
    kept = judge("fixture-kept", ["LS-05"], {"a": fixture(220), "b": fixture(220)}, [("a", "b")])
    if kept:
        problems.append(f"fixture: an offset that is kept was reported: {[f.what for f in kept]}")
    lost = judge("fixture-lost", ["LS-05"], {"a": fixture(220), "b": fixture(0)}, [("a", "b")])
    if not any(f.rule == "scroll-lost" for f in lost):
        problems.append("fixture: an offset that reset to the top was not found")
    drift = judge("fixture-drift", ["LS-05"], {"a": fixture(220), "b": fixture(214)}, [("a", "b")])
    if not any(f.rule == "scroll-lost" for f in drift):
        problems.append("fixture: an offset that drifted by 6 px was not found")
    still = judge("fixture-never-scrolled", ["LS-05"], {"a": fixture(0), "b": fixture(0)}, [("a", "b")])
    if not any(f.rule == "scroll-walk-broken" for f in still):
        problems.append("fixture: a walk that never scrolled passed on zero")
    origin, sub = ["one", "two", "three"], ["four", "five", "six"]
    base = {"a": slide_fixture(origin), "sub": slide_fixture(sub)}
    clean = judge_slide(
        "fixture-slide",
        ["LS-05"],
        {**base, "p00": slide_fixture(sub, dx=300), "p01": slide_fixture(origin, dx=-80)},
        "a",
        "sub",
        "p",
    )
    if clean:
        problems.append(f"fixture: a page that slides as one block was reported: {[f.what for f in clean]}")
    moved = judge_slide("fixture-reflow", ["LS-05"], {**base, "p00": slide_fixture(sub, dx=300, dy=9)}, "a", "sub", "p")
    if not any(f.rule == "slide-reflow" for f in moved):
        problems.append("fixture: a text that sits 9 px off its settled place during a slide was not found")
    return problems


# ---- main ---------------------------------------------------------------------------------------------


def run_one(scenario: Scenario, workdir: Path, timeout: int, variant: lib.Variant) -> dict:
    result = lib.run_job(dict(scenario.job), workdir, timeout, variant, scenario.extra_env)
    result["scenario"] = scenario
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    lib.add_sweep_args(parser, variants="dark")
    parser.add_argument("--fixtures", action="store_true", help="only check the comparison on its own probes")
    args = parser.parse_args()

    problems = run_fixtures()
    for problem in problems:
        print(problem)
    print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
    if args.fixtures or problems:
        return 1 if problems else 0

    if args.build:
        lint.build()
    workdir = lib.workdir_of(args, "scroll-continuity-")
    scenarios: list[Scenario] = []
    if args.only in (None, "settings"):
        scenarios += settings_scenarios() + slide_scenarios()
    if args.only in (None, "files"):
        scenarios += files_scenarios(workdir)
    for scenario in scenarios:
        binary = Path(scenario.job["argv"][0])
        if not binary.exists():
            sys.exit(f"{binary} is missing: build with --build")
    variant = lib.parse_variants(args.variants)[0]
    results: list[dict] = []
    findings: list[Finding] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [pool.submit(run_one, scenario, workdir, args.timeout, variant) for scenario in scenarios]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            scenario: Scenario = result["scenario"]
            by_stem = {Path(probe.capture).stem: probe for probe in result["probes"]}
            found = judge(scenario.name, scenario.rule_ids, by_stem, scenario.compare, rows=scenario.rows)
            if scenario.slide or scenario.name == "settings-network-push-pop":
                found += pushed(scenario.name, scenario.rule_ids, by_stem, "Data usage", "sub")
            if scenario.slide:
                found += judge_slide(scenario.name, scenario.rule_ids, by_stem, "a", "sub", "p")
                found += judge_slide(scenario.name, scenario.rule_ids, by_stem, "b", "sub", "q")
            findings += found
            results.append(result)
            print(f"{result['status']:>10}  {scenario.name}  ({len(found)} findings)", flush=True)
    measured = len(scenarios)
    allow = Allow([{"rule": e["rule"], "capture": e["scenario"], "where": e["step"], "why": e["why"]} for e in ALLOWED])
    return lib.finish("scroll-continuity", findings, allow, args, results, measured, workdir, "walks")


if __name__ == "__main__":
    sys.exit(main())
