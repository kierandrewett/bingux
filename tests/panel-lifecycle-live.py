"""Opt-in: check first-use panel loading and report real shell memory in a VM."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import time


PANELS = ("controls", "calendar", "metrics", "notifications")


def ctl(*args):
    return json.loads(subprocess.check_output(["binguxctl", "ipc", "shell", *args], text=True, timeout=10))


def memory():
    processes = {}
    for path in Path("/proc").iterdir():
        if not path.name.isdigit():
            continue
        try:
            args = path.joinpath("cmdline").read_bytes().split(b"\0")
            parent = next(
                line.split()[1] for line in path.joinpath("status").read_text().splitlines() if line.startswith("PPid:")
            )
            processes[int(path.name)] = (int(parent), args)
        except (OSError, StopIteration):
            continue
    roots = {
        pid
        for pid, (_, args) in processes.items()
        if args
        and Path(os.fsdecode(args[0])).name in ("qs", "quickshell")
        and any(arg.endswith(b"/bingux/shell") for arg in args)
    }
    assert len(roots) == 1, "Run against exactly one installed desktop shell"
    rows = []
    for pid in processes:
        ancestor = pid
        seen = set()
        while ancestor in processes and ancestor not in roots and ancestor not in seen:
            seen.add(ancestor)
            ancestor = processes[ancestor][0]
        if ancestor not in roots:
            continue
        try:
            values = {}
            for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
                parts = line.split()
                if len(parts) == 3 and parts[1].isdigit():
                    values[parts[0].rstrip(":")] = int(parts[1])
            rows.append({"pid": pid, "pssKiB": values["Pss"], "swapPssKiB": values["SwapPss"]})
        except OSError:
            continue
    return {"processes": rows, "totalMiB": sum(row["pssKiB"] + row["swapPssKiB"] for row in rows) / 1024}


def wait_open(panel, expected):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if ctl("status")[panel] == expected:
            return
        time.sleep(0.05)
    raise AssertionError(f"{panel} did not reach open={expected}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--expect-unopened", action="store_true", help="Use after a fresh shell start with the default layout"
    )
    parser.add_argument("--cycles", type=int, default=8)
    parser.add_argument("--panels", nargs="+", choices=PANELS, default=list(PANELS))
    parser.add_argument("--warmup-cycles", type=int, default=3)
    parser.add_argument("--max-growth-mib", type=float, help="Fail if post-warmup memory exceeds this growth")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    assert 1 <= args.cycles <= 100
    if args.max_growth_mib is not None:
        assert 0 <= args.warmup_cycles < args.cycles
        assert args.max_growth_mib >= 0
    status = ctl("status")
    assert not any(status[name] for name in PANELS), "Start with popups closed"
    measurements = [{"phase": "unopened", **memory()}]
    print(json.dumps({key: value for key, value in measurements[-1].items() if key != "processes"}), flush=True)
    if args.expect_unopened:
        assert not any(status["loadedPanels"].values()), f"Unused panels are already loaded: {status['loadedPanels']}"
        for panel in PANELS:
            assert ctl("panel", panel, "status") == {"open": False}
            assert ctl("panel", panel, "close")["ok"]
        assert not any(ctl("status")["loadedPanels"].values()), "Reading or closing an unused panel must not load it"
    try:
        for cycle in range(args.cycles):
            for panel in args.panels:
                assert ctl("panel", panel, "open")["ok"]
                wait_open(panel, True)
                assert ctl("status")["loadedPanels"][panel]
                time.sleep(0.2)
                assert ctl("panel", panel, "close")["ok"]
                wait_open(panel, False)
                time.sleep(0.2)
            measurements.append({"phase": f"cycle-{cycle + 1}", **memory()})
            print(json.dumps({key: value for key, value in measurements[-1].items() if key != "processes"}), flush=True)
    finally:
        for panel in PANELS:
            ctl("panel", panel, "close")
        if args.report:
            args.report.write_text(json.dumps(measurements, indent=2) + "\n")
    if args.max_growth_mib is not None:
        steady = measurements[args.warmup_cycles :]
        growth = max(row["totalMiB"] for row in steady) - steady[0]["totalMiB"]
        assert growth <= args.max_growth_mib, (
            f"Popup memory grew {growth:.1f} MiB after warmup; limit {args.max_growth_mib:.1f} MiB"
        )
    print(
        "PASS: all panels open and close repeatedly" + ("; unused panels stay unloaded" if args.expect_unopened else "")
    )


if __name__ == "__main__":
    main()
