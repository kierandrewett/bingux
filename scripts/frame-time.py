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
sliders and typing in the search field. With --app files (and --binary the
bingux-files binary) it runs the Files app instead, in Big/ of FILES_HOME (default
/var/tmp/FileTips-home: 20000 files, one in ten a PNG; made by
scripts/files-fixture.py), and scrolls the list and the grid at three zoom levels with the wheel
and the scrollbar while the pointer has armed the hover card: files-list and
files-grid. With --slow-sources MS every system
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

sys.path.insert(0, str(Path(__file__).resolve().parent))
from app_env import isolated  # noqa: E402

SESSION = Path(os.environ.get("INSPECTOR_HARNESS", "/var/tmp/InspectorHarness")) / "session.sh"
FILES_SESSION = SESSION.with_name("files-session.sh")
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target")) / "debug"
FRAME_GAP_MS = 250  # frames further apart than this are separate bursts
# The folder the Files scenarios open: Big/ holds 20000 files, one in ten a PNG.
FILES_HOME = Path(os.environ.get("FILES_HOME", "/var/tmp/FileTips-home"))

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
    def __init__(self, binary: Path, name: str, workdir: Path, slow_ms: int, watchdog_ms: int,
                 files: bool = False):
        self.name = name
        self.script = FILES_SESSION if files else SESSION
        self.dir = Path(os.environ.get("INSPECTOR_HARNESS", "/var/tmp/InspectorHarness")) / "sessions" / name
        self.perf = workdir / "perf.log"
        self.watch = workdir / "watchdog.log"
        env = isolated(dict(os.environ, INSPECT="0", BIN=str(binary),
                   BINGUX_PERF_LOG=str(self.perf), BINGUX_UI_WATCHDOG=str(watchdog_ms),
                   BINGUX_UI_WATCHDOG_LOG=str(self.watch),
                   BINGUX_SETTINGS_FAKE_NETWORK="online", BINGUX_SETTINGS_FAKE_MODEM="ok",
                   BINGUX_SETTINGS_FAKE_DISPLAYS="1"))  # no write reaches the person's dconf
        if files:
            # The Files app in a 1200x800 window, with its own HOME.
            env.update(FILES_BIN=str(binary), FILES_HOME=str(FILES_HOME), W="1200", H="800")
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
        start = [str(self.script), "start", name] + ([str(FILES_HOME / "Big")] if files else [])
        subprocess.run(start, env=env, check=True, stdout=subprocess.DEVNULL, timeout=600)
        self.pid = int(subprocess.check_output(["pgrep", "-f", f"^{binary} "]).split()[-1])
        self.binary = binary
        self.session_env = dict(
            line.strip().split("=", 1) for line in (self.dir / "env").read_text().splitlines() if "=" in line)

    def inp(self, *commands: str) -> None:
        subprocess.run([str(self.script), "in", self.name, *commands], env=self.env, check=True,
                       stdout=subprocess.DEVNULL, timeout=600)
        wait = 0.0
        for command in commands:
            parts = command.split()
            wait += {"sleep": int(parts[1]) / 1000 if parts[0] == "sleep" else 0,
                     "click": 0.25, "wheel": 0.03}.get(parts[0], 0.01)
        time.sleep(max(0.0, wait - 0.6))

    def key(self, *args: str) -> None:
        subprocess.run([str(self.script), "key", self.name, *args], env=self.env,
                       stdout=subprocess.DEVNULL, timeout=60)

    def goto(self, address: str) -> None:
        """Hand an address to the running copy, which opens it."""
        subprocess.run([str(self.binary), address], env={**os.environ, **self.session_env},
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)

    def stop(self) -> None:
        subprocess.run([str(self.script), "stop", self.name], env=self.env, stdout=subprocess.DEVNULL)


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
        "busy_sum": round(sum(busy), 1),
        # The frames that took more than 8 ms: (ms after the scenario began, busy ms).
        "heavy": [(round(f[0] - t0), round(f[3], 1)) for f in frames if f[3] > 8],
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


def files_arm(session: Session, x: int, y: int) -> None:
    """Rest the pointer on a row or tile until its hover card is up."""
    session.inp(f"move {x} {y}", "sleep 80", f"move {x + 2} {y + 1}", "sleep 1100")


def files_wheel(session: Session, x: int, y: int):
    notch = [f"wheel {x} {y} 2", "sleep 50"] * 14 + [f"wheel {x} {y} -2", "sleep 50"] * 14
    return lambda: session.inp(*notch)


def scenario_files_list(session: Session):
    """Wheel and scrollbar scrolling over a list while the hover card is armed."""
    files_arm(session, 500, 237)
    yield "files list wheel, card armed", files_wheel(session, 502, 238)
    files_arm(session, 500, 237)
    drag = ["move 1197 150", "sleep 80", "down left", "sleep 80"]
    for y in range(150, 560, 12):
        drag += [f"move 1197 {y}", "sleep 16"]
    yield "files list scrollbar drag, card armed", lambda: session.inp(*drag, "up left")


def scenario_files_grid(session: Session):
    """The same over the grid, at three zoom levels (Ctrl+2 is the grid, Ctrl+Plus zooms in)."""
    for level, keys in enumerate((("2",), ("plus",), ("plus",)), 1):
        session.key("-M", "ctrl", "-k", *keys, "-m", "ctrl")
        # A key sent with no modifier tells the window Ctrl is up: the virtual
        # keyboard's release does not always arrive, and a held Ctrl turns the
        # wheel into a zoom.
        session.key("-k", "Escape")
        time.sleep(1.5)
        files_arm(session, 420, 260)
        yield f"files grid level {level} wheel, card armed", files_wheel(session, 422, 261)


FILES_SCENARIOS = {"files-list": scenario_files_list, "files-grid": scenario_files_grid}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--app", choices=["settings", "files"], default="settings",
                        help="files: the Files app in a folder of 20000 files (see FILES_HOME)")
    parser.add_argument("--binary", type=Path, default=None)
    parser.add_argument("--scenarios", default=None)
    parser.add_argument("--slow-sources", type=int, default=0, metavar="MS")
    parser.add_argument("--watchdog-ms", type=int, default=12)
    parser.add_argument("--gate-busy", type=float, default=20.0, help="busy-time p99, ms")
    parser.add_argument("--gate-max", type=float, default=50.0, help="longest busy frame or blocked stretch, ms")
    parser.add_argument("--gate-wall", type=float, default=0.0, help="interval p99, ms; off by default")
    parser.add_argument("--json", type=Path)
    parser.add_argument("--keep", action="store_true", help="keep the session and logs")
    args = parser.parse_args()
    scenarios = FILES_SCENARIOS if args.app == "files" else SCENARIOS
    if args.binary is None:
        args.binary = TARGET / ("bingux-files" if args.app == "files" else "bingux-settings")
    if args.scenarios is None:
        args.scenarios = ",".join(scenarios)
    if not args.binary.exists():
        sys.exit(f"no binary at {args.binary}")
    workdir = Path(tempfile.mkdtemp(prefix="frame-time-"))
    session = Session(args.binary, f"ft{os.getpid()}", workdir, args.slow_sources, args.watchdog_ms,
                      files=args.app == "files")
    results: list[dict] = []
    try:
        time.sleep(4)
        if args.app == "files":
            time.sleep(4)  # the folder of 20000 files is listed
        else:
            session.key("-k", "Shift_L")
            session.inp("click 700 450", "sleep 500")
        for name in [n for n in args.scenarios.split(",") if n]:
            for label, action in scenarios[name](session):
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
