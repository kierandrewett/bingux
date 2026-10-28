#!/usr/bin/env python3
"""Fail when a focus ring is not drawn around the box its control paints.

Every control ends `render` with `.focusable(..)` on the node that carries its
fill (its *face*). The wrapper measures that face after layout and draws the
ring around the measured box with the face's corner radius nested. This script
checks the result on every screen the apps can draw: for each ring in each
captured frame, the ring's rectangle must be the face's border box grown by the
ring's outset (gap, width and, while it animates, travel), and its corner radius
must be the face's plus that outset. The rule is
`bingux_ui::focus::ring_check::check`, the same one the inspector's Layout tab
reports as "Focus ring off its face" (packages/bingux-ui/src/focus/ring_check.rs).

The apps render through `bingux_ui::snapshot` without a compositor. With
BINGUX_UI_RING_CHECK=FILE set, every capture the run saves also appends one JSON
line to FILE: the capture, the number of rings in its frame and the rings that
are off their face. This script reads them. It runs

  * every Bingux Settings page, subpage and click-opened popup, at the desktop
    size and at 390x800 (the two densities), and
  * every bingux-taxonomy page, which holds each component and state of
    bingux-ui, at desktop and phone size.

    python3 scripts/focus-ring-check.py                  # everything
    python3 scripts/focus-ring-check.py --only taxonomy
    python3 scripts/focus-ring-check.py --pages network,sound --sizes 390x800
    python3 scripts/focus-ring-check.py --json /tmp/rings.json

Exit status is 1 when a ring is off its face and ALLOWED does not cover it, when
a capture run produced nothing, and when the whole run found no ring at all (the
check would be passing on nothing). Fix the control: put `.focusable(..)` on the
node that paints, and pass that node's radius. Where a ring is meant to be off
its face, add an entry to ALLOWED with the reason: a regex for the capture and a
regex for the face's component path.

Build first (`--build` does this):

    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build --features snapshot \
        --manifest-path packages/bingux-settings/Cargo.toml
    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build \
        --manifest-path packages/bingux-taxonomy/Cargo.toml

Needs nothing beyond the standard library.
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

# Rings that are off their face on purpose. Each entry: a regex that must match
# the capture's name, a regex that must match the face's component path, and why.
# An entry that matches nothing is reported, so the list cannot rot.
ALLOWED: list[dict] = []


def allowed(finding: dict) -> dict | None:
    for entry in ALLOWED:
        if re.search(entry["capture"], finding["capture"]) and re.search(entry["where"], finding["label"]):
            return entry
    return None


def read_rings(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def run(job: dict, workdir: Path, timeout: int) -> dict:
    rings = workdir / "rings" / f"{job['name']}.jsonl"
    rings.parent.mkdir(parents=True, exist_ok=True)
    rings.unlink(missing_ok=True)
    job = dict(job, env=dict(job.get("env", {}), BINGUX_UI_RING_CHECK=str(rings)))
    result = lint.run_job(job, workdir, timeout)
    result["shots"] = read_rings(rings)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", choices=["settings", "taxonomy"])
    parser.add_argument("--pages", default="", help="comma-separated Settings pages")
    parser.add_argument("--sizes", default=f"{DESKTOP},{PHONE}", help="Settings sizes, `default` is desktop")
    parser.add_argument("--jobs", type=int, default=2, help="captures to run at once")
    parser.add_argument("--timeout", type=int, default=240, help="seconds per capture run")
    parser.add_argument("--json", help="write every mismatch to this file")
    parser.add_argument("--keep", help="keep captures and logs in this directory")
    parser.add_argument("--build", action="store_true", help="build the two binaries first")
    parser.add_argument("--show-allowed", action="store_true", help="list the rings ALLOWED covers")
    args = parser.parse_args()

    if args.build:
        lint.build()
    settings = lint.TARGET / "debug" / "bingux-settings"
    taxonomy = lint.TARGET / "debug" / "bingux-taxonomy"
    jobs: list[dict] = []
    if args.only in (None, "settings"):
        if not settings.exists():
            sys.exit(f"{settings} is missing: build with --build")
        pages = [page for page in args.pages.split(",") if page] or lint.panels() + lint.SUBPAGES
        jobs += lint.settings_jobs(pages, [size for size in args.sizes.split(",") if size])
    if args.only in (None, "taxonomy") and not args.pages:
        if not taxonomy.exists():
            sys.exit(f"{taxonomy} is missing: build with --build")
        jobs += lint.taxonomy_jobs()

    workdir = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix="focus-ring-check-"))
    workdir.mkdir(parents=True, exist_ok=True)
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [
            pool.submit(run, job, workdir, args.timeout * (30 if job["name"] == "taxonomy" else 1))
            for job in jobs
        ]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            rings = sum(shot["rings"] for shot in result["shots"])
            print(
                f"{result['status']:>10}  {result['name']}  ({len(result['shots'])} captures, {rings} rings)",
                flush=True,
            )

    findings = []
    broken = []
    total_rings = 0
    for result in sorted(results, key=lambda r: r["name"]):
        if result["status"] != "ok" or not result["shots"]:
            broken.append(result)
        for shot in result["shots"]:
            total_rings += shot["rings"]
            where = f"{result['name']}:{Path(shot['capture']).stem}"
            for mismatch in shot["mismatches"]:
                finding = dict(mismatch, capture=where)
                finding["allowed"] = (allowed(finding) or {}).get("why")
                findings.append(finding)

    used = {entry["why"] for finding in findings for entry in [allowed(finding)] if entry}
    unused = [entry for entry in ALLOWED if entry["why"] not in used]
    failures = [finding for finding in findings if not finding["allowed"]]

    print()
    by_label = Counter((finding["label"], finding["what"]) for finding in failures)
    for finding in sorted(failures, key=lambda f: (f["label"], f["capture"])):
        print(f"[{finding['what']}] {finding['capture']}")
        print(f"    {finding['label'] or '(no component)'}")
        print(f"    ring should be {finding['expected']}, is {finding['found']}")
    if by_label:
        print("\nBy control:")
        for (label, what), count in sorted(by_label.items(), key=lambda item: -item[1]):
            print(f"  {count:4}  {label or '(no component)'}: {what}")
    if args.show_allowed:
        for finding in findings:
            if finding["allowed"]:
                print(f"[allowed] {finding['capture']}: {finding['label']} -- {finding['allowed']}")
    for entry in unused:
        print(f"unused ALLOWED entry: {entry['capture']} {entry['where']}: {entry['why']}")
    for result in broken:
        print(f"no captures: {result['name']} ({result['status']}), see {result['log']}")
    if total_rings == 0:
        print("no focus ring found in any capture: the check ran on nothing")

    if args.json:
        Path(args.json).write_text(json.dumps(findings, indent=2))
    captures = sum(len(result["shots"]) for result in results)
    print(
        f"\n{captures} captures, {total_rings} rings, {len(failures)} off their face"
        f", {len(findings) - len(failures)} allowed"
    )
    if not args.keep:
        shutil.rmtree(workdir, ignore_errors=True)
    return 1 if failures or broken or unused or total_rings == 0 else 0


if __name__ == "__main__":
    sys.exit(main())
