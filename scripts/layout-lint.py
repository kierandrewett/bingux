#!/usr/bin/env python3
"""Fail when a Bingux window has a visual layout defect.

This is the headless run of the inspector's Layout tab. The tab and this
script share one set of rules (packages/bingux-ui/src/inspector/layout_checks.rs):
uneven padding, an item off its row's centre line, siblings of one kind at
different heights, a box with dead space under its content, clipped content,
overlapping controls, text cut off while there is room, and misaligned
baselines. They are visual defects, not accessibility problems.

The apps render through `bingux_ui::snapshot` without a compositor. With
BINGUX_UI_LAYOUT_LINT=FILE set, every capture the run saves also appends the
findings of its frame to FILE, and this script reads them. It runs

  * every Bingux Settings page (the sidebar panels and a few subpages) at the
    desktop size and at 390x800,
  * the Settings popups, sheets and dialogs that can be opened by a click, at
    both sizes, and
  * every bingux-taxonomy page, which holds each component and state of
    bingux-ui, at desktop and phone size, with the states that open a menu or
    a dialog.

    python3 scripts/layout-lint.py                  # everything
    python3 scripts/layout-lint.py --only settings  # Settings only
    python3 scripts/layout-lint.py --only taxonomy  # taxonomy only
    python3 scripts/layout-lint.py --pages network,sound --sizes 390x800
    python3 scripts/layout-lint.py --json /tmp/layout.json

Exit status is 1 when there is a finding that ALLOWED does not cover. Fix the
component, or token, that causes a finding. Where a box is lopsided on
purpose, add an entry to ALLOWED with the reason: a rule, a regex for the
capture and a regex for the component path or the node.

Build first, with the usual shared target:

    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build --features snapshot \
        --manifest-path packages/bingux-settings/Cargo.toml
    CARGO_TARGET_DIR=/var/tmp/bingux-settings-target cargo build \
        --manifest-path packages/bingux-taxonomy/Cargo.toml

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
SETTINGS = ROOT / "packages" / "bingux-settings"
TAXONOMY = ROOT / "packages" / "bingux-taxonomy"
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))

DESKTOP = "default"
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
}

# Findings that are right by design. Each entry: the rule's key, a regex that
# must match the capture's name, a regex that must match the component path or
# the node's description, and why the box is meant to look like this. An entry
# that matches nothing is reported, so the list cannot rot.
ALLOWED: list[dict] = []


def allowed(finding: dict, capture: str) -> dict | None:
    for entry in ALLOWED:
        if (
            entry["rule"] == finding["rule"]
            and re.search(entry["capture"], capture)
            and re.search(entry["where"], f"{finding['path']} {finding['target']}")
        ):
            return entry
    return None


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
    env["BINGUX_UI_LAYOUT_LINT"] = str(lint)
    env["XDG_CACHE_HOME"] = str(workdir / "cache" / job["name"])
    if job.get("size") and job["size"] != DESKTOP:
        env["BINGUX_SETTINGS_SNAPSHOT_SIZE"] = job["size"]
    if job.get("script"):
        env["BINGUX_SETTINGS_SNAPSHOT_SCRIPT"] = job["script"]
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
                    "script": f"wait 1200; {click}; wait 1200; save popup",
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


def taxonomy_jobs() -> list[dict]:
    # `--snapshot` writes desktop, phone and state captures for every page.
    return [
        {
            "name": "taxonomy",
            "argv": [str(TARGET / "debug" / "bingux-taxonomy"), "--snapshot"],
            "pages": [],
        }
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", choices=["settings", "taxonomy"])
    parser.add_argument("--pages", default="", help="comma-separated Settings pages")
    parser.add_argument("--sizes", default=f"{DESKTOP},{PHONE}", help="Settings sizes, `default` is desktop")
    parser.add_argument("--jobs", type=int, default=2, help="captures to run at once")
    parser.add_argument("--timeout", type=int, default=240, help="seconds per capture run")
    parser.add_argument("--json", help="write every finding to this file")
    parser.add_argument("--keep", help="keep captures and logs in this directory")
    parser.add_argument("--build", action="store_true", help="build the two binaries first")
    parser.add_argument("--show-allowed", action="store_true", help="list the findings ALLOWED covers")
    args = parser.parse_args()

    if args.build:
        build()
    settings = TARGET / "debug" / "bingux-settings"
    taxonomy = TARGET / "debug" / "bingux-taxonomy"
    jobs: list[dict] = []
    if args.only in (None, "settings"):
        if not settings.exists():
            sys.exit(f"{settings} is missing: build with --build")
        pages = [page for page in args.pages.split(",") if page] or panels() + SUBPAGES
        jobs += settings_jobs(pages, [size for size in args.sizes.split(",") if size])
    if args.only in (None, "taxonomy") and not args.pages:
        if not taxonomy.exists():
            sys.exit(f"{taxonomy} is missing: build with --build")
        jobs += taxonomy_jobs()

    workdir = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix="layout-lint-"))
    workdir.mkdir(parents=True, exist_ok=True)
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [pool.submit(run_job, job, workdir, args.timeout * (30 if job["name"] == "taxonomy" else 1)) for job in jobs]
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
            capture = Path(shot["capture"]).stem
            where = f"{result['name']}:{capture}"
            for finding in shot["findings"]:
                finding = dict(finding, capture=where)
                finding["allowed"] = (allowed(finding, where) or {}).get("why")
                findings.append(finding)

    used = {entry["why"] for finding in findings for entry in [allowed(finding, finding["capture"])] if entry}
    unused = [entry for entry in ALLOWED if entry["why"] not in used]
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
