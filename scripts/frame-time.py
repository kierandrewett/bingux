#!/usr/bin/env python3
"""Frame-time harness for Bingux Settings: nothing may block the UI thread.

Runs the app in the private headless sway session of
/var/tmp/InspectorHarness/session.sh (see the bingux-settings-session-harness
skill), drives it with real pointer and keyboard input, and records, for each
scenario, every frame the application drew:

  * the interval between frames (wall clock, which a software renderer such as
    llvmpipe inflates, so it is reported and only gated with --gate-wall),
  * the UI thread's busy time per frame: its CPU time between one frame and the
    next (events, component renders, layout, drawing), without the presenting
    step. Presenting is the GPU submit; on the software renderer of a headless
    session (llvmpipe) it is rasterisation on the UI thread and has nothing to
    do with the app, so the gate leaves it out and the table shows it apart
    ("cpu", with it). Neither depends on waiting, and the busy figure is what
    the gate uses,
  * the stretches the UI-thread watchdog (BINGUX_UI_WATCHDOG) found blocked:
    the thread inside sleep, wait4, a read, a lock and the like for 12 ms or
    more, with the stack. Any such stretch of 50 ms or more fails the gate.

Scenarios: sidebar page switches, push and pop of every Network subpage (the
address is handed to the running copy, Back is Alt+Left), scrolling, dragging
sliders and typing in the search field. With --slow-sources MS every system
command (nmcli, gsettings, wpctl ...) waits MS milliseconds before it runs: a
shim on PATH for the programs started anywhere, and BINGUX_SLOW_SOURCES_MS for
the ones run through bingux_ui::runtime::run. The waits belong on workers, so
the numbers must not change; a command run on the UI thread shows up as a
blocked stretch and as frames that took the whole wait.

Gate (exit status 1), per scenario: busy p99 above --gate-busy (default 20 ms),
any busy frame or blocked stretch of --gate-max ms (default 50) or more, and
with --gate-wall also an interval p99 above --gate-wall milliseconds.

    python3 scripts/frame-time.py --slow-sources 500
    python3 scripts/frame-time.py --scenarios subpages --json out.json

Build first (`cargo build` in packages/bingux-settings, with the shared
CARGO_TARGET_DIR) and `bingux-config` beside it. The dev profile builds
bingux-settings itself at opt-level 0, which makes page-building code several
times slower than a release build and reads as UI-thread work that is not
there; for numbers that mean something build it as the other crates are built:

    CARGO_PROFILE_DEV_PACKAGE_BINGUX_SETTINGS_OPT_LEVEL=2 \\
    CARGO_PROFILE_DEV_PACKAGE_BINGUX_SETTINGS_DEBUG_ASSERTIONS=false cargo build

Run it on a machine that is not building something else: a busy-time figure is
CPU time, which a loaded machine inflates less than wall time but not by
nothing. Needs sway, grim-free: only the session helper's pointer and key
input.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SESSION = Path(os.environ.get("INSPECTOR_HARNESS", "/var/tmp/InspectorHarness")) / "session.sh"
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target")) / "debug"
FRAME_GAP_MS = 250  # frames further apart than this are separate bursts

# Sidebar rows at the default 1400x900 session size, with INSPECT=0.
PAGES = [("network", 170), ("bluetooth", 212), ("displays", 295), ("sound", 337),
         ("power", 379), ("keyboard", 421), ("mouse", 463), ("appearance", 546),
         ("desktop", 588), ("search", 630), ("applications", 672),
         ("accessibility", 755), ("datetime", 797), ("about", 839)]

# Every Network subpage reachable with the fake network (FAKE_NETWORK=online).
NETWORK_SUBPAGES = [
    "network/wifi", "network/wifi/saved", "network/hotspot", "network/proxy",
    "network/usage", "network/ethernet/enp42s0", "network/ethernet/enp42s0/profile",
    "network/details/Wired%20connection%201", "network/add-vpn", "network/mobile",
    "network/mobile/advanced", "network/mobile/advanced/network",
    "network/mobile/advanced/apn", "network/mobile/usage",
]

SLOW_PROGRAMS = ["nmcli", "mmcli", "gsettings", "wpctl", "pactl", "bluetoothctl", "upower",
                 "gio", "busctl", "timedatectl", "gnoblinctl", "fc-list", "fc-match",
                 "wg", "ibus", "flatpak", "lspci", "hostnamectl", "ip"]


class Session:
    def __init__(self, binary: Path, name: str, workdir: Path, slow_ms: int, watchdog_ms: int):
        self.name = name
        self.dir = Path(os.environ.get("INSPECTOR_HARNESS", "/var/tmp/InspectorHarness")) / "sessions" / name
        self.perf = workdir / "perf.log"
        self.watch = workdir / "watchdog.log"
        env = dict(os.environ, INSPECT="0", BIN=str(binary),
                   BINGUX_PERF_LOG=str(self.perf), BINGUX_UI_WATCHDOG=str(watchdog_ms),
                   BINGUX_UI_WATCHDOG_LOG=str(self.watch),
                   BINGUX_SETTINGS_FAKE_NETWORK="online", BINGUX_SETTINGS_FAKE_MODEM="ok",
                   BINGUX_SETTINGS_FAKE_DISPLAYS="1")
        config = binary.parent / "bingux-config"
        if config.exists():
            env.setdefault("BINGUX_CONFIG_BIN", str(config))
        if slow_ms:
            shims = workdir / "shims"
            shims.mkdir()
            for program in SLOW_PROGRAMS:
                real = shutil.which(program)
                if real:
                    shim = shims / program
                    shim.write_text(f'#!/bin/sh\nsleep {slow_ms / 1000}\nexec {real} "$@"\n')
                    shim.chmod(0o755)
            env["BINGUX_PATH_PREFIX"] = str(shims)
            env["BINGUX_SLOW_SOURCES_MS"] = str(slow_ms)
        self.env = env
        subprocess.run([str(SESSION), "start", name], env=env, check=True,
                       stdout=subprocess.DEVNULL, timeout=600)
        self.pid = int(subprocess.check_output(["pgrep", "-f", f"^{binary} wired"]).split()[-1])
        self.binary = binary
        self.session_env = dict(
            line.strip().split("=", 1) for line in (self.dir / "env").read_text().splitlines() if "=" in line)

    def inp(self, *commands: str) -> None:
        subprocess.run([str(SESSION), "in", self.name, *commands], env=self.env, check=True,
                       stdout=subprocess.DEVNULL, timeout=600)
        wait = 0.0
        for command in commands:
            parts = command.split()
            wait += {"sleep": int(parts[1]) / 1000 if parts[0] == "sleep" else 0,
                     "click": 0.25, "wheel": 0.03}.get(parts[0], 0.01)
        time.sleep(max(0.0, wait - 0.6))

    def key(self, *args: str) -> None:
        subprocess.run([str(SESSION), "key", self.name, *args], env=self.env,
                       stdout=subprocess.DEVNULL, timeout=60)

    def goto(self, address: str) -> None:
        """Hand an address to the running copy, which opens it."""
        subprocess.run([str(self.binary), address], env={**os.environ, **self.session_env},
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)

    def stop(self) -> None:
        subprocess.run([str(SESSION), "stop", self.name], env=self.env, stdout=subprocess.DEVNULL)


def read_frames(path: Path, t0: float, t1: float) -> list[tuple[float, float, float, float]]:
    """(unix ms, frame ms, UI-thread CPU ms, the same without presenting) of the frames that ended in [t0, t1]."""
    frames = []
    try:
        for line in path.read_text().splitlines():
            fields = line.split()
            if len(fields) >= 4 and t0 <= float(fields[0]) <= t1:
                frames.append((float(fields[0]), float(fields[1]), float(fields[2]), float(fields[3])))
    except OSError:
        pass
    return frames


def read_blocked(path: Path, t0: float, t1: float) -> list[tuple[int, str]]:
    """The (ms, place) of blocked stretches the watchdog logged in [t0, t1]."""
    found = []
    try:
        for line in path.read_text().splitlines():
            m = re.match(r"\[watchdog (\d+)\] UI thread blocked (\d+) ms in (.*) \(total\)$", line)
            if m and t0 <= float(m.group(1)) <= t1:
                found.append((int(m.group(2)), m.group(3)))
    except OSError:
        pass
    return found


def pct(values: list[float], p: float) -> float | None:
    if not values:
        return None
    values = sorted(values)
    return round(values[min(len(values) - 1, int(len(values) * p))], 1)


def measure(session: Session, label: str, action) -> dict:
    t0 = time.time() * 1000
    action()
    time.sleep(0.4)
    t1 = time.time() * 1000
    frames = read_frames(session.perf, t0, t1)
    gaps = [b[0] - a[0] for a, b in zip(frames, frames[1:]) if b[0] - a[0] < FRAME_GAP_MS]
    busy = [f[3] for f in frames]
    cpu = [f[2] for f in frames]
    blocked = read_blocked(session.watch, t0, t1 + 1000)
    result = {
        "scenario": label, "frames": len(frames),
        "interval_p50": pct(gaps, 0.5), "interval_p95": pct(gaps, 0.95),
        "interval_p99": pct(gaps, 0.99), "interval_max": round(max(gaps), 1) if gaps else None,
        "over_16_7": sum(1 for g in gaps if g > 16.7),
        "busy_p50": pct(busy, 0.5), "busy_p95": pct(busy, 0.95),
        "busy_p99": pct(busy, 0.99), "busy_max": round(max(busy), 1) if busy else None,
        "cpu_p99": pct(cpu, 0.99), "cpu_max": round(max(cpu), 1) if cpu else None,
        "blocked": blocked,
    }
    return result


def scenario_switch(session: Session):
    for page, y in PAGES:
        yield f"switch {page}", lambda y=y: session.inp(f"click 140 {y}", "sleep 1200")


def scenario_subpages(session: Session):
    session.inp("click 140 170", "sleep 1500")
    for address in NETWORK_SUBPAGES:
        yield f"push {address}", lambda a=address: (session.goto(a), time.sleep(1.6))
        yield f"pop  {address}", lambda: (session.key("-M", "alt", "-k", "Left", "-m", "alt"), time.sleep(1.2))


def scenario_scroll(session: Session):
    for page, y in (("network", 170), ("displays", 295), ("applications", 672)):
        session.inp(f"click 140 {y}", "sleep 1500")
        down = [f"wheel 840 500 4", "sleep 60"] * 8
        up = [f"wheel 840 500 -4", "sleep 60"] * 8
        yield f"scroll {page}", lambda down=down, up=up: session.inp(*down, *up)


def scenario_sliders(session: Session):
    session.inp("click 140 463", "sleep 1800")
    steps = []
    for x in range(760, 1170, 10):
        steps += [f"move {x} 254", "sleep 16"]
    back = []
    for x in range(1170, 760, -10):
        back += [f"move {x} 254", "sleep 16"]
    yield "drag mouse speed", lambda: session.inp("move 760 254", "down left", "sleep 50", *steps, *back, "up left")


def scenario_typing(session: Session):
    session.inp("click 140 24", "sleep 300")
    yield "type in search", lambda: session.key("-d", "40", "network wifi bluetooth")


SCENARIOS = {"switch": scenario_switch, "subpages": scenario_subpages, "scroll": scenario_scroll,
             "sliders": scenario_sliders, "typing": scenario_typing}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", type=Path, default=TARGET / "bingux-settings")
    parser.add_argument("--scenarios", default=",".join(SCENARIOS))
    parser.add_argument("--slow-sources", type=int, default=0, metavar="MS")
    parser.add_argument("--watchdog-ms", type=int, default=12)
    parser.add_argument("--gate-busy", type=float, default=20.0, help="busy-time p99, ms")
    parser.add_argument("--gate-max", type=float, default=50.0, help="longest busy frame or blocked stretch, ms")
    parser.add_argument("--gate-wall", type=float, default=0.0, help="interval p99, ms; off by default")
    parser.add_argument("--json", type=Path)
    parser.add_argument("--keep", action="store_true", help="keep the session and logs")
    args = parser.parse_args()
    if not args.binary.exists():
        sys.exit(f"no binary at {args.binary}")
    workdir = Path(tempfile.mkdtemp(prefix="frame-time-"))
    session = Session(args.binary, f"ft{os.getpid()}", workdir, args.slow_sources, args.watchdog_ms)
    results: list[dict] = []
    try:
        time.sleep(4)
        session.key("-k", "Shift_L")
        session.inp("click 700 450", "sleep 500")
        for name in [n for n in args.scenarios.split(",") if n]:
            for label, action in SCENARIOS[name](session):
                results.append(measure(session, label, action))
    finally:
        try:
            app_log = (session.dir / "app.log").read_text()
        except OSError:
            app_log = ""
        if not args.keep:
            session.stop()

    # The guarded wrappers in bingux_ui::runtime say so when a command or a file
    # read is made on the UI thread; any such call fails the run.
    on_ui = sorted(set(re.findall(r"^\[ui-blocking\] (.*) on the UI thread", app_log, re.M)))

    failed = 0
    print(f"{'scenario':42} {'frames':>6} | {'interval p50':>12} {'p95':>6} {'p99':>6} {'max':>6} {'>16.7':>5} | "
          f"{'busy p50':>8} {'p95':>6} {'p99':>6} {'max':>6} {'cpu p99':>7} | blocked")
    for r in results:
        reasons = []
        if not r["frames"]:
            # A scenario that drew nothing proves nothing (the session did not
            # take the input, or the app is not drawing): that is a failure.
            reasons.append("no frames recorded")
        if r["busy_p99"] is not None and r["busy_p99"] > args.gate_busy:
            reasons.append(f"busy p99 {r['busy_p99']} > {args.gate_busy}")
        if r["busy_max"] is not None and r["busy_max"] >= args.gate_max:
            reasons.append(f"busy max {r['busy_max']} >= {args.gate_max}")
        worst = max((ms for ms, _ in r["blocked"]), default=0)
        if worst >= args.gate_max:
            reasons.append(f"blocked {worst} ms")
        if args.gate_wall and r["interval_p99"] and r["interval_p99"] > args.gate_wall:
            reasons.append(f"interval p99 {r['interval_p99']} > {args.gate_wall}")
        failed += bool(reasons)
        blocked = ", ".join(f"{ms} ms {place[:60]}" for ms, place in r["blocked"][:2])
        print(f"{r['scenario']:42} {r['frames']:6} | {r['interval_p50'] or 0:12} {r['interval_p95'] or 0:6} "
              f"{r['interval_p99'] or 0:6} {r['interval_max'] or 0:6} {r['over_16_7']:5} | "
              f"{r['busy_p50'] or 0:8} {r['busy_p95'] or 0:6} {r['busy_p99'] or 0:6} {r['busy_max'] or 0:6} "
              f"{r['cpu_p99'] or 0:7} | {blocked}{'   FAIL: ' + '; '.join(reasons) if reasons else ''}")
        r["fail"] = reasons
    every = [r for r in results if r["frames"]]
    if every:
        print(f"{'ALL':42} {sum(r['frames'] for r in every):6} | busy p99 (worst scenario) "
              f"{max(r['busy_p99'] or 0 for r in every)}  busy max {max(r['busy_max'] or 0 for r in every)}  "
              f"frames over 16.7 ms (interval) {sum(r['over_16_7'] for r in every)}")
    for what in on_ui:
        print(f"FAIL: {what} was called on the UI thread (see app.log of the session with --keep)")
    if args.json:
        args.json.write_text(json.dumps({"scenarios": results, "ui_thread_calls": on_ui}, indent=2))
    print(f"logs: {workdir}" if args.keep else "")
    if not args.keep:
        shutil.rmtree(workdir, ignore_errors=True)
    if failed or on_ui:
        print(f"{failed} of {len(results)} scenarios over the gate, {len(on_ui)} UI-thread calls", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
