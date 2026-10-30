#!/usr/bin/env python3
"""Fail when a tooltip is drawn narrower than its text.

A tooltip bubble is as wide as its text on one line, up to `Dim::TooltipMax`
(280 px) or the window less its margins. It is never as wide as the control it
describes or the box that control sits in. The bug this guards against drew
"New tab" over a 36 px button as a vertical pill, one letter a line, because
the bubble was laid out in the room of the button's cell. A unit test with a
wide control cannot see it; this script hovers the real controls.

The apps render through `bingux_ui::snapshot` without a compositor. With
BINGUX_UI_TOOLTIP_CHECK=FILE set, every capture the run saves also hovers each
named icon button of its frame (an icon button is named by its tooltip, so these
are the controls that say what they do only in a tooltip), waits for the bubble
and appends what it measured to FILE (`Snapshot::tooltip_findings`,
packages/bingux-ui/src/controls/tooltip_check.rs). This script reads them. It
runs

  * every Bingux Settings page, subpage and click-opened popup, at the desktop
    size and at 390x800,
  * every bingux-taxonomy page, which holds each component and state of
    bingux-ui, and
  * the Files window (`bingux-files --snapshot`): the tab strip, the toolbar,
    the places and the file view.

A bubble is a finding when its text is on more than one line while it is
narrower than the cap (`wrapped narrow`), when it has fewer than three
characters a line (`vertical`), when it is not inside the window, and when it
covers its control. A control that shows no bubble is counted, not reported.

    python3 scripts/tooltip-check.py                  # everything
    python3 scripts/tooltip-check.py --only settings  # Settings only
    python3 scripts/tooltip-check.py --only files     # Files only
    python3 scripts/tooltip-check.py --pages network,sound --sizes 390x800
    python3 scripts/tooltip-check.py --json /tmp/tooltips.json

Exit status is 1 when a bubble is a finding that ALLOWED does not cover, when a
capture run produced nothing or crashed, and when the whole run hovered no
control at all (the check would be passing on nothing). Fix the bubble, not the
check; where a narrow bubble is meant, add an entry to ALLOWED with the reason.

Build first (`--build` does this):

    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build --features snapshot \
        --manifest-path packages/bingux-settings/Cargo.toml
    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build \
        --manifest-path packages/bingux-taxonomy/Cargo.toml
    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build --features snapshot \
        --manifest-path packages/bingux-files-app/Cargo.toml

Needs nothing beyond the standard library.
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
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = ROOT / "packages" / "bingux-files-app"

# The layout lint owns the list of pages, popups and the way a capture is run.
_spec = importlib.util.spec_from_file_location("layout_lint", ROOT / "scripts" / "layout-lint.py")
assert _spec and _spec.loader
lint = importlib.util.module_from_spec(_spec)
sys.modules["layout_lint"] = lint
_spec.loader.exec_module(lint)

DESKTOP = lint.DESKTOP
PHONE = lint.PHONE

# Bubbles that are narrow on purpose. Each entry: a regex that must match the
# capture's name, a regex that must match the control's name (its tooltip), the
# rule, and why. An entry that matches nothing is reported, so the list cannot rot.
ALLOWED: list[dict] = []
# The rules of design/rules.json this check holds (read by scripts/rules-check.py).
RULE_IDS = {"*": ["TP-01", "LS-04"]}


def allowed(finding: dict) -> dict | None:
    for entry in ALLOWED:
        if (
            entry["rule"] == finding["rule"]
            and re.search(entry["capture"], finding["capture"])
            and re.search(entry["where"], finding["name"])
        ):
            return entry
    return None


def read_tips(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


# What a capture's log says when the app crashed inside it.
CRASH = re.compile(r"panicked|unrecoverable|relaunch")
CRASH_CONTEXT = 2
CRASH_LINES = 6


def crash_lines(log: Path) -> list[str]:
    if not log.exists():
        return []
    lines = log.read_text(errors="replace").splitlines()
    found: list[str] = []
    skip_until = -1
    for index, line in enumerate(lines):
        if index <= skip_until or not CRASH.search(line):
            continue
        found.extend(lines[index : index + 1 + CRASH_CONTEXT])
        skip_until = index + CRASH_CONTEXT
        if len(found) >= CRASH_LINES * (1 + CRASH_CONTEXT):
            break
    return found


def run(job: dict, workdir: Path, timeout: int) -> dict:
    """One capture run, once; a flaky crash must not pass because a second run
    happened not to hit it."""
    tips = workdir / "tips" / f"{job['name']}.jsonl"
    tips.parent.mkdir(parents=True, exist_ok=True)
    tips.unlink(missing_ok=True)
    job = dict(job, env=dict(job.get("env", {}), BINGUX_UI_TOOLTIP_CHECK=str(tips)))
    result = lint.run_job(job, workdir, timeout)
    result["shots"] = read_tips(tips)
    result["crash"] = crash_lines(Path(result["log"]))
    return result


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
    parser.add_argument("--show-allowed", action="store_true", help="list the findings ALLOWED covers")
    args = parser.parse_args()

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
        # One run a page: the single run probes every control of every page one after the other, took longer than
        # any one timeout, and a page that stopped it took the rest with it. Pages run two at a time (--jobs).
        import probe_lib  # numpy and PIL: its page list is read from `bingux-taxonomy --help`

        for page in probe_lib.taxonomy_pages():
            jobs += [dict(job, name=f"taxonomy-{page.replace('/', '_')}") for job in lint.taxonomy_jobs([page])]
    if args.only in (None, "files") and not args.pages:
        if not files.exists():
            sys.exit(f"{files} is missing: build with --build")
        jobs += files_jobs()

    workdir = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix="tooltip-check-"))
    workdir.mkdir(parents=True, exist_ok=True)
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [
            pool.submit(run, job, workdir, args.timeout * (5 if job["name"].startswith("taxonomy-") else 30 if job["name"] == "taxonomy" else 1))
            for job in jobs
        ]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            hovered = sum(shot["shown"] for shot in result["shots"])
            print(
                f"{result['status']:>10}  {result['name']}  ({len(result['shots'])} captures, {hovered} tooltips)",
                flush=True,
            )

    findings = []
    broken = []
    crashed = []
    controls = shown = 0
    missing: Counter = Counter()
    for result in sorted(results, key=lambda r: r["name"]):
        if result["crash"]:
            crashed.append(result)
        if result["status"] != "ok" or not result["shots"]:
            broken.append(result)
        for shot in result["shots"]:
            controls += shot["controls"]
            shown += shot["shown"]
            missing.update(shot.get("missing", []))
            where = f"{result['name']}:{Path(shot['capture']).stem}"
            for finding in shot["findings"]:
                finding = dict(finding, capture=where)
                finding["allowed"] = (allowed(finding) or {}).get("why")
                findings.append(finding)

    used = {entry["why"] for finding in findings for entry in [allowed(finding)] if entry}
    unused = [entry for entry in ALLOWED if entry["why"] not in used]
    failures = [finding for finding in findings if not finding["allowed"]]

    print()
    for finding in sorted(failures, key=lambda f: (f["rule"], f["name"], f["capture"])):
        print(f"[{finding['rule']}] {finding['capture']}")
        print(f"    {finding['name']!r}: {finding['detail']}")
        control, bubble = finding["control"], finding["bubble"]
        print(
            f"    control {control[2]:.0f}x{control[3]:.0f} at ({control[0]:.0f}, {control[1]:.0f}),"
            f" bubble {bubble[2]:.0f}x{bubble[3]:.0f} at ({bubble[0]:.0f}, {bubble[1]:.0f})"
        )
    by_rule = Counter(finding["rule"] for finding in failures)
    if args.show_allowed:
        for finding in findings:
            if finding["allowed"]:
                print(f"[allowed {finding['rule']}] {finding['capture']}: {finding['name']!r} -- {finding['allowed']}")
    for entry in unused:
        print(f"unused ALLOWED entry: {entry['rule']} {entry['capture']} {entry['where']}: {entry['why']}")
    for result in crashed:
        print(f"CRASH in {result['name']} (see {result['log']}):")
        for line in result["crash"]:
            print(f"    {line}")
    for result in broken:
        if result not in crashed:
            print(f"no captures: {result['name']} ({result['status']}), see {result['log']}")
    if missing:
        top = ", ".join(f"{name!r} x{count}" for name, count in missing.most_common(8))
        print(f"{sum(missing.values())} named icon buttons showed no bubble (not a finding): {top}")
    if shown == 0:
        print("no tooltip was drawn in any capture: the check ran on nothing")

    if args.json:
        Path(args.json).write_text(json.dumps(findings, indent=2))
    captures = sum(len(result["shots"]) for result in results)
    print(
        f"\n{captures} captures, {controls} named icon buttons, {shown} tooltips measured, "
        f"{len(failures)} findings"
        + (": " + ", ".join(f"{rule} {n}" for rule, n in sorted(by_rule.items())) if by_rule else "")
        + f", {len(findings) - len(failures)} allowed, {len(crashed)} crashed"
    )
    failed = bool(failures or broken or crashed or unused or shown == 0)
    if failed and not args.keep:
        print(f"kept the captures and logs in {workdir}")
    elif not args.keep:
        shutil.rmtree(workdir, ignore_errors=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
