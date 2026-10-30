#!/usr/bin/env python3
"""A headless frame-time gate: mount each key page N times and fail over a budget.

CK-08 of design/rules.md (MO-03, BH-03, BH-04, BH-05): nothing may block the UI thread, so
a page that is opened must cost less than a frame or so. `scripts/frame-time.py` holds that
in a real headless session with real input, which needs sway and a quiet machine. This holds
the part that does not: `bingux-settings --snapshot` with BINGUX_SETTINGS_SNAPSHOT_TIMING=N
mounts each page N times from the Power page (no window, no slide, no GPU) and prints what the
mount frame cost on the UI thread, the first mount (cold caches) apart, and the median and the
best of the rest. It runs in any CI job that can run the layout lint.

    python3 scripts/frame-gate.py                       # every page, desktop, 12 rounds
    python3 scripts/frame-gate.py --pages appearance,network --rounds 20
    python3 scripts/frame-gate.py --sizes default,390x800
    python3 scripts/frame-gate.py --update-budget       # rewrite the budgets from this machine's best times
    python3 scripts/frame-gate.py --fixtures            # the verdicts on made-up timings only
    BINGUX_FRAME_GATE=report python3 scripts/frame-gate.py   # never fails: for a noisy runner

Three levels, from `scripts/frame-budget.json`, in milliseconds of the best of the rounds:

  report  every page's first, median and best time and its budget
  warn    the best time is over `warn_ms` (about a frame): printed, the exit status is 0
  fail    the best time is over `fail_ms`, or a page cannot be timed at all: exit status 1

The best of N rounds is what is gated, not the median: a busy runner makes some rounds slow but
none fast, so the least one is the page's own cost. The median is reported, and a median more
than 3 times the best is flagged `noisy` so a slow runner reads as one. Budgets are for the machine
the baseline was taken on; they are scaled by how much slower this machine runs a fixed piece of
Python than the baseline did (`calibration_ms`), by at most 4. A page with its own entry in
`pages` has its own numbers (Appearance's 21 ms switch is the reference: `warn_ms` 30, `fail_ms` 60).

The dev profile builds bingux-settings itself at opt-level 0, which makes the page-building code
several times slower than release and reads as time that a release build does not spend: build
for the gate as frame-time.py says, or read a debug build's numbers as an upper bound.
(`CARGO_PROFILE_DEV_PACKAGE_BINGUX_SETTINGS_OPT_LEVEL=2 CARGO_PROFILE_DEV_PACKAGE_BINGUX_SETTINGS_DEBUG_ASSERTIONS=false cargo build --features snapshot`)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))
BUDGET = ROOT / "scripts" / "frame-budget.json"
DESKTOP = "default"
PHONE = "390x800"
LINE = re.compile(r"^\[timing\]\s+(\S+)\s+first\s+([\d.]+) ms\s+median\s+([\d.]+) ms\s+best\s+([\d.]+) ms")
NOISY = 3.0
MAX_FACTOR = 4.0
# The rules of design/rules.json this check holds (read by scripts/rules-check.py).
RULE_IDS = {"page-mount": ["MO-03", "BH-03", "BH-04"]}

sys.path.insert(0, str(ROOT / "scripts"))
from app_env import isolated  # noqa: E402


def calibrate() -> float:
    """How long a fixed piece of Python takes here, in ms: the best of five, so a busy moment does not count."""
    best = float("inf")
    for _ in range(5):
        started = time.perf_counter()
        total = 0
        for i in range(400_000):
            total += i * i % 7
        best = min(best, (time.perf_counter() - started) * 1000)
    return best


def panels() -> list[str]:
    import importlib.util

    spec = importlib.util.spec_from_file_location("layout_lint", ROOT / "scripts" / "layout-lint.py")
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules.setdefault("layout_lint", module)
    spec.loader.exec_module(module)
    return module.panels() + ["network/wifi", "applications/default", "background/font", "keyboard/shortcuts"]


def budget_for(budget: dict, page: str) -> tuple[float, float]:
    entry = budget.get("pages", {}).get(page, {})
    return float(entry.get("warn_ms", budget["warn_ms"])), float(entry.get("fail_ms", budget["fail_ms"]))


def verdict(page: str, timing: dict, budget: dict, factor: float) -> tuple[str, str]:
    """(level, note) for one page's timing: ok, warn, fail or noisy-ok."""
    warn, fail = budget_for(budget, page)
    warn, fail = warn * factor, fail * factor
    best = timing["best"]
    note = f"best {best:.1f} ms (warn {warn:.0f}, fail {fail:.0f})"
    if best > fail:
        return "fail", note
    level = "warn" if best > warn else "ok"
    if timing["median"] > NOISY * max(best, 1.0) and timing["median"] > warn:
        note += f"; noisy: median {timing['median']:.1f} ms is {timing['median'] / max(best, 1.0):.1f}x the best"
    return level, note


def run(binary: Path, pages: list[str], size: str, rounds: int, timeout: int) -> tuple[dict[str, dict], str]:
    env = dict(os.environ, BINGUX_SETTINGS_SNAPSHOT_TIMING=str(rounds))
    for name, value in (("BINGUX_SETTINGS_FAKE_NETWORK", "online"), ("BINGUX_SETTINGS_FAKE_MODEM", "ok"), ("BINGUX_SETTINGS_FAKE_DISPLAYS", "1")):
        env.setdefault(name, value)
    env = isolated(env)  # software rendering never finds the live desktop; no write reaches the person's dconf
    if size != DESKTOP:
        env["BINGUX_SETTINGS_SNAPSHOT_SIZE"] = size
    with tempfile.TemporaryDirectory(prefix="frame-gate-") as scratch:
        env["XDG_CACHE_HOME"] = str(Path(scratch) / "cache")
        try:
            proc = subprocess.run(
                [str(binary), "--snapshot", str(Path(scratch) / "png"), ",".join(pages)],
                env=env,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return {}, f"timed out after {timeout}s"
    found: dict[str, dict] = {}
    for line in proc.stdout.splitlines():
        match = LINE.match(line)
        if match:
            name = match.group(1)
            found[name] = {"first": float(match.group(2)), "median": float(match.group(3)), "best": float(match.group(4))}
    return found, "ok" if proc.returncode == 0 else f"exit {proc.returncode}: {proc.stderr.strip().splitlines()[-1] if proc.stderr.strip() else ''}"


def fixtures() -> list[str]:
    problems = []
    budget = {"warn_ms": 20, "fail_ms": 50, "pages": {"appearance": {"warn_ms": 30, "fail_ms": 60}}}
    cases = [
        ("network", {"first": 40, "median": 12, "best": 10}, 1.0, "ok"),
        ("network", {"first": 40, "median": 30, "best": 25}, 1.0, "warn"),
        ("network", {"first": 90, "median": 80, "best": 70}, 1.0, "fail"),
        ("appearance", {"first": 40, "median": 28, "best": 25}, 1.0, "ok"),
        ("appearance", {"first": 70, "median": 66, "best": 64}, 1.0, "fail"),
        ("network", {"first": 90, "median": 80, "best": 70}, 2.0, "warn"),
    ]
    for page, timing, factor, want in cases:
        got, _ = verdict(page, timing, budget, factor)
        if got != want:
            problems.append(f"fixture: {page} {timing} at x{factor} should be {want}, got {got}")
    _, note = verdict("network", {"first": 90, "median": 120, "best": 15}, budget, 1.0)
    if "noisy" not in note:
        problems.append("fixture: a median far over the best was not flagged noisy")
    parsed = LINE.match("[timing] appearance                   first    81.2 ms  median    18.4 ms  best    16.9 ms")
    if not parsed or parsed.group(4) != "16.9":
        problems.append("fixture: the binary's timing line was not parsed")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", type=Path, default=TARGET / "debug" / "bingux-settings")
    parser.add_argument("--pages", default="", help="comma-separated pages; default every page and the subpages that load data")
    parser.add_argument("--sizes", default=DESKTOP, help="comma-separated: `default` is desktop, 390x800 the phone")
    parser.add_argument("--rounds", type=int, default=12, help="mounts of each page")
    parser.add_argument("--repeats", type=int, default=2, help="runs of the whole set; the least best time of each page is kept")
    parser.add_argument("--timeout", type=int, default=600, help="seconds per run")
    parser.add_argument("--budget", type=Path, default=BUDGET)
    parser.add_argument("--update-budget", action="store_true", help="rewrite the baseline and the per-page budgets from this machine")
    parser.add_argument("--fixtures", action="store_true", help="only check the verdicts on made-up timings")
    parser.add_argument("--json", type=Path, help="write the timings and verdicts here")
    args = parser.parse_args()

    problems = fixtures()
    for problem in problems:
        print(problem)
    print("fixtures: " + ("all judged as they should be" if not problems else f"{len(problems)} wrong"))
    if args.fixtures or problems:
        return 1 if problems else 0
    if not args.binary.exists():
        sys.exit(f"{args.binary} is missing: cargo build --features snapshot in apps/settings")
    budget = json.loads(args.budget.read_text())
    pages = [page for page in args.pages.split(",") if page] or panels()
    sizes = [size for size in args.sizes.split(",") if size]

    here = calibrate()
    factor = min(MAX_FACTOR, max(1.0, here / float(budget["calibration_ms"])))
    print(f"calibration: {here:.0f} ms here against {budget['calibration_ms']:.0f} ms at the baseline: budgets x{factor:.2f}")
    rows: list[dict] = []
    failed = 0
    warned = 0
    for size in sizes:
        best: dict[str, dict] = {}
        status = "ok"
        for _ in range(max(1, args.repeats)):
            timings, status = run(args.binary, pages, size, args.rounds, args.timeout)
            for page, timing in timings.items():
                if page not in best or timing["best"] < best[page]["best"]:
                    best[page] = timing
        tag = "phone" if size == PHONE else "desktop"
        print(f"\n{tag} ({size}), {args.rounds} mounts each, {args.repeats} runs")
        print(f"{'page':28} {'first':>8} {'median':>8} {'best':>8}  verdict")
        for page in pages:
            name = page.replace("/", "_")
            timing = best.get(name)
            if timing is None:
                failed += 1
                print(f"{page:28} {'-':>8} {'-':>8} {'-':>8}  FAIL: not timed ({status})")
                rows.append({"page": page, "size": size, "level": "fail", "note": f"not timed ({status})"})
                continue
            level, note = verdict(page, timing, budget, factor)
            warned += level == "warn"
            failed += level == "fail"
            print(f"{page:28} {timing['first']:8.1f} {timing['median']:8.1f} {timing['best']:8.1f}  {level.upper() if level != 'ok' else 'ok'}: {note}")
            rows.append({"page": page, "size": size, "level": level, "note": note, **timing})
        if args.update_budget:
            budget.setdefault("pages", {})
            for row in rows:
                if row["size"] == size and "best" in row:
                    entry = budget["pages"].setdefault(row["page"], {})
                    entry["measured_ms"] = round(row["best"], 1)
    if args.update_budget:
        budget["calibration_ms"] = round(here, 1)
        args.budget.write_text(json.dumps(budget, indent=2) + "\n")
        print(f"\nwrote {args.budget}: calibration {here:.0f} ms; the warn and fail levels are not changed, edit them by hand")
        return 0
    if args.json:
        args.json.write_text(json.dumps(rows, indent=2))
    print(f"\nframe-gate: {len(rows)} timings, {warned} over the warn level, {failed} over the fail level")
    if failed and os.environ.get("BINGUX_FRAME_GATE") == "report":
        print("BINGUX_FRAME_GATE=report: not failing")
        return 0
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
