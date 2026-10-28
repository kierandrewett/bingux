#!/usr/bin/env python3
"""Fail when text overflows, wraps badly or hides under stress.

`scripts/layout-lint.py` renders every Settings page, popup and taxonomy
specimen once, with the strings a developer wrote. This script renders them
again under the conditions that break layouts, and fails on any finding of the
overflow rules of the inspector's Layout tab
(packages/bingux-ui/src/inspector/layout_checks.rs):

  wrapped            a letter to a line, a word split in the middle, more lines
                     than allowed, a box narrower than two glyphs
  overflow           one-line text wider than its box with no ellipsis
  outside-parent     a box that reaches outside the box that holds it
  horizontal-scroll  content wider than the window or its scroll view
  hidden-label       a control whose label is out of sight
  text-collision     two texts lying on each other
  clipped            ink cut off by a box that clips it
  overlap            siblings that partly lie on each other
  truncated          text cut off while there is room beside it

The scenarios (--list shows them):

  baseline   the strings as written, at the desktop size, 390x800 and 320x600
             and with every popup at its narrowest (BINGUX_POPUP_WIDTH=320)
  pseudo     BINGUX_PSEUDO_LOCALE=expand: every string accented and 40 % longer
  scale200   BINGUX_TEXT_SCALE=2: the type ramp at the largest accessibility
             text scale
  longdata   BINGUX_STRESS_DATA=long: names that come from the machine (networks,
             devices, users, files) are very long and unbroken
  worst      all three at once, at 320x600 and the desktop size

Right-to-left text is not run: the layout engine (torin) has no direction to
mirror, so there is nothing to render. The script says so rather than
pretending.

    python3 scripts/overflow-check.py                      # all scenarios
    python3 scripts/overflow-check.py --scenarios pseudo   # one
    python3 scripts/overflow-check.py --only settings --pages network,sound
    python3 scripts/overflow-check.py --keep /tmp/overflow  # keep the PNGs
    python3 scripts/overflow-check.py --json /tmp/overflow.json

Exit status is 1 on a finding that ALLOWED does not cover, on a capture run
that produced nothing, and on an ALLOWED entry that matched nothing. Fix the
shared component that causes a finding. Where text is meant to behave so, add
an entry to ALLOWED with the reason: a rule, a regex for the capture name, a
regex for the component path or node, and why.

It needs the binaries of layout-lint.py (use --build) and nothing beyond the
standard library.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import importlib.util
import json
import re
import shutil
import sys
import tempfile
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# The layout lint owns the list of pages, popups and the way a capture is run.
_spec = importlib.util.spec_from_file_location("layout_lint", ROOT / "scripts" / "layout-lint.py")
assert _spec and _spec.loader
lint = importlib.util.module_from_spec(_spec)
sys.modules["layout_lint"] = lint
_spec.loader.exec_module(lint)

DESKTOP = lint.DESKTOP
PHONE = lint.PHONE
SMALL = "320x600"

# The pages show more of what the machine has under stress: Bluetooth devices
# and a second wired profile, whose names are stretched by the longdata run.
lint.FAKES.update({"BINGUX_SETTINGS_FAKE_BLUETOOTH": "1", "BINGUX_SETTINGS_FAKE_WIRED": "many"})

# What the overflow rules are. The other Layout rules (padding, centring,
# dead space, baselines) are layout-lint's, and stress would only add noise.
RULES = {
    "wrapped",
    "overflow",
    "outside-parent",
    "horizontal-scroll",
    "hidden-label",
    "text-collision",
    "clipped",
    "overlap",
    "truncated",
}

# name -> (what it sets, the Settings sizes it runs at, its taxonomy run)
SCENARIOS: dict[str, dict] = {
    "baseline": {"env": {"BINGUX_POPUP_WIDTH": "320"}, "sizes": [DESKTOP, PHONE, SMALL], "taxonomy": True},
    "pseudo": {
        "env": {"BINGUX_PSEUDO_LOCALE": "expand", "BINGUX_POPUP_WIDTH": "320"},
        "sizes": [DESKTOP, PHONE, SMALL],
        "taxonomy": True,
    },
    "scale200": {
        "env": {"BINGUX_TEXT_SCALE": "2", "BINGUX_POPUP_WIDTH": "320"},
        "sizes": [DESKTOP, PHONE],
        "taxonomy": True,
    },
    "longdata": {
        "env": {"BINGUX_STRESS_DATA": "long", "BINGUX_POPUP_WIDTH": "320"},
        "sizes": [DESKTOP, PHONE],
        "taxonomy": False,
    },
    "worst": {
        "env": {
            "BINGUX_PSEUDO_LOCALE": "expand",
            "BINGUX_TEXT_SCALE": "2",
            "BINGUX_STRESS_DATA": "long",
            "BINGUX_POPUP_WIDTH": "320",
        },
        "sizes": [DESKTOP, SMALL],
        "taxonomy": False,
    },
}

# Findings that are right by design. Each entry: the rule's key, a regex that
# must match "<scenario>:<capture>", a regex that must match the component
# path or the node's description, and why the text is meant to behave so. An
# entry that matches nothing is reported, so the list cannot rot.
ALLOWED: list[dict] = []


def allowed(finding: dict) -> dict | None:
    for entry in ALLOWED:
        if (
            entry["rule"] == finding["rule"]
            and re.search(entry["capture"], finding["capture"])
            and re.search(entry["where"], f"{finding['path']} {finding['target']}")
        ):
            return entry
    return None


def scenario_jobs(name: str, spec: dict, only: str | None, pages: list[str]) -> list[dict]:
    jobs: list[dict] = []
    if only in (None, "settings"):
        settings = lint.panels() + lint.SUBPAGES if not pages else pages
        for job in lint.settings_jobs(settings, spec["sizes"]):
            if pages and job["name"].startswith("popup-"):
                continue
            jobs.append(job)
    if only in (None, "taxonomy") and not pages and spec["taxonomy"]:
        jobs += lint.taxonomy_jobs()
    for job in jobs:
        job["name"] = f"{name}-{job['name']}"
        job["env"] = {**spec["env"], **job.get("env", {})}
    return jobs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--scenarios", default=",".join(SCENARIOS), help="comma-separated scenario names")
    parser.add_argument("--list", action="store_true", help="list the scenarios and exit")
    parser.add_argument("--only", choices=["settings", "taxonomy"])
    parser.add_argument("--pages", default="", help="comma-separated Settings pages (no popups, no taxonomy)")
    parser.add_argument("--jobs", type=int, default=2, help="captures to run at once")
    parser.add_argument("--timeout", type=int, default=240, help="seconds per capture run")
    parser.add_argument("--json", help="write every finding to this file")
    parser.add_argument("--keep", help="keep captures and logs in this directory")
    parser.add_argument("--build", action="store_true", help="build the two binaries first")
    parser.add_argument("--show-allowed", action="store_true", help="list the findings ALLOWED covers")
    parser.add_argument("--all-rules", action="store_true", help="report the padding, centring and baseline rules too")
    args = parser.parse_args()

    if args.list:
        for name, spec in SCENARIOS.items():
            print(f"{name:10} sizes {', '.join(spec['sizes'])}  env {spec['env']}")
        return 0
    chosen = [name for name in args.scenarios.split(",") if name]
    unknown = [name for name in chosen if name not in SCENARIOS]
    if unknown:
        sys.exit(f"unknown scenario: {', '.join(unknown)} (see --list)")

    if args.build:
        lint.build()
    settings = lint.TARGET / "debug" / "bingux-settings"
    taxonomy = lint.TARGET / "debug" / "bingux-taxonomy"
    if args.only in (None, "settings") and not settings.exists():
        sys.exit(f"{settings} is missing: build with --build")
    if args.only in (None, "taxonomy") and not args.pages and not taxonomy.exists():
        sys.exit(f"{taxonomy} is missing: build with --build")

    pages = [page for page in args.pages.split(",") if page]
    jobs: list[dict] = []
    for name in chosen:
        jobs += scenario_jobs(name, SCENARIOS[name], args.only, pages)

    print("RTL: not run. The layout engine has no text direction to mirror, so there is no right-to-left rendering to check.")
    workdir = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix="overflow-check-"))
    workdir.mkdir(parents=True, exist_ok=True)
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [
            pool.submit(lint.run_job, job, workdir, args.timeout * (30 if job["name"].endswith("taxonomy") else 1))
            for job in jobs
        ]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            print(f"{result['status']:>10}  {result['name']}  ({len(result['shots'])} captures)", flush=True)

    findings = []
    broken = []
    for result in sorted(results, key=lambda r: r["name"]):
        if result["status"] != "ok" or not result["shots"]:
            broken.append(result)
        for shot in result["shots"]:
            capture = Path(shot["capture"])
            where = f"{result['name']}:{capture.stem}"
            for finding in shot["findings"]:
                if not args.all_rules and finding["rule"] not in RULES:
                    continue
                finding = dict(finding, capture=where, png=str(capture))
                finding["allowed"] = (allowed(finding) or {}).get("why")
                findings.append(finding)

    used = {entry["why"] for finding in findings for entry in [allowed(finding)] if entry}
    unused = [entry for entry in ALLOWED if entry["why"] not in used]
    failures = [finding for finding in findings if not finding["allowed"]]

    print()
    for finding in sorted(failures, key=lambda f: (f["rule"], f["capture"], f["node"])):
        numbers = ", ".join(f"{name} {value:g}" for name, value in finding["numbers"].items())
        print(f"[{finding['rule']}] {finding['capture']}")
        print(f"    {finding['target']}  in {finding['path'] or '(no component)'}")
        print(f"    {finding['what']}  ({numbers})")
        if args.keep:
            print(f"    {finding['png']}")
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
    by_rule = Counter(finding["rule"] for finding in failures)
    by_scenario = Counter(finding["capture"].split("-", 1)[0] for finding in failures)
    captures = sum(len(result["shots"]) for result in results)
    print(
        f"\n{captures} captures in {len(chosen)} scenarios, {len(failures)} findings"
        + (": " + ", ".join(f"{rule} {n}" for rule, n in sorted(by_rule.items())) if by_rule else "")
        + (" (" + ", ".join(f"{name} {n}" for name, n in sorted(by_scenario.items())) + ")" if by_scenario else "")
        + f", {len(findings) - len(failures)} allowed"
    )
    if not args.keep:
        shutil.rmtree(workdir, ignore_errors=True)
    return 1 if failures or broken or unused else 0


if __name__ == "__main__":
    sys.exit(main())
