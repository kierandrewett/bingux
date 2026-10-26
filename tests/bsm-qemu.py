#!/usr/bin/env python3
"""Boot a Bingux OS image with bsm and walk the acceptance scenario of the spec (section 18).

usage: tests/bsm-qemu.py [--iso ISO] [--inject DIR] [--out DIR] [--only STEP[,STEP...]]
                         [--no-bsm-param] [--smoke] [--keep-work]

The ISO boots through its own boot loader with `bingux.bsm` on the kernel command
line, with two virtual displays (one virtio-gpu, two heads), the serial root
shell of tests/distro_vm.py and a frame recorder per display. Steps:

  boot        two displays seen by the guest, bsm binaries present, a second user exists
  greeter     bsm is a persistent process named bsm, the greeter runs as the greeter
              account without Gnoblin, accounts are listed, selecting an account only
              changes the greeter, both displays show the greeter
  login-a     real PAM login of the first account: progress milestones arrive in order
              with evidence, no desktop pixel before readiness, the ready desktop fades
              in OVER the greeter on both displays at the same time
  lock-a      lock goes through bsm, the greeter shows the lock scene, the desktop is gone
  unlock-a    unlock slides the greeter scene UP, same Gnoblin pid, same bsm pid
  switch-b    switch to the second account, real progress, B fades in, A stays running
  back-to-a   switch back: no relaunch, no progress, the existing desktop is revealed
  logout-a    A's session ends alone; B survives; frames fade into the greeter
  greeter-kill  greeter killed while a session is locked: no desktop pixel shows
  power       a denied (inhibited) power request restores a scene; a real power off
              runs through bsm with the cover and the guest powers off

A step whose prerequisite is missing (no bsm in the image, no hosted Gnoblin) is
reported `blocked` with the reason, never `passed`. Exit status: 0 all steps
passed, 1 any check failed, 2 nothing failed but something is blocked.

Against an image that does not contain bsm yet, `--inject DIR` (tests/bsm-vm-inject.py
stage) copies freshly built binaries into the live guest; hosted Gnoblin cannot be
injected, so the steps that need a desktop stay blocked until the image is rebuilt.
`--smoke` runs only boot and greeter and is what a plain image gets.

Artifacts land in BINGUX_DISTRO_OUT (default distro/out/bsm-test/): checks.json
(name -> bool; blocked counts as false), steps.json (name -> passed/failed/blocked
with the reason), report.txt, serial.log, timeline.json, one PNG per stage and
display, frame series as CSV (alpha, offset), bsmctl dumps and the monitor log.
The video and working files stay under /tmp/bsm-vm.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from bsm_frames import blend_residual, coverage, fade_alpha, is_monotone, similarity, slide_offset  # noqa: E402
from bsm_harness import HEAD_COUNT, MONITOR_LOG, BsmVm  # noqa: E402
from distro_vm import ROOT, find_iso, missing_tools  # noqa: E402

USER_A = ("live", "live")
USER_B = ("bsmtest", "bsmtest")
GREETER_USER = "bingux-greeter"
BSM_BINARIES = ["/usr/bin/bsm", "/usr/bin/bsmctl", "/usr/libexec/bsm/bsm-helper", "/usr/libexec/bsm/bsm-host"]
# A fade or slide needs at least this many intermediate samples to count as continuous motion.
MIN_INTERMEDIATE = 3
STAGE_ORDER = [
    "auth_accepted",
    "pam_session",
    "host_connected",
    "gnoblin_ready",
    "outputs_configured",
    "bingux_launched",
    "surfaces_mapped",
    "desktop_ready",
    "ready_to_reveal",
]


class Blocked(Exception):
    """A step cannot run: a prerequisite is missing. Not a pass, not a failure."""


class Run:
    def __init__(self, vm, out):
        self.vm = vm
        self.out = out
        self.report = []
        self.steps = {}
        self.checks = {}
        self.current = None
        self.state = {}

    # Recording.

    def log(self, text):
        print(text, flush=True)
        self.report.append(text)

    def check(self, label, ok, detail=""):
        self.checks[f"{self.current}: {label}"] = bool(ok)
        self.log(("ok    " if ok else "FAIL  ") + f"{self.current}: {label}" + (f"  [{detail}]" if detail else ""))
        return bool(ok)

    def stamp(self, name):
        self.vm.recorder.mark(name)
        return time.monotonic()

    def shot(self, name):
        for head in range(HEAD_COUNT):
            frame = self.frame(head)
            if frame is not None:
                from PIL import Image

                Image.fromarray(frame).save(self.out / f"{name}-display{head}.png")

    def frame(self, head):
        return self.vm.head_sources()[head]()

    def csv(self, name, header, rows):
        with (self.out / f"{name}.csv").open("w") as handle:
            handle.write(",".join(header) + "\n")
            for row in rows:
                handle.write(",".join(f"{value:.4f}" if isinstance(value, float) else str(value) for value in row) + "\n")

    def dump(self, name, text):
        (self.out / f"{name}.txt").write_text(text)

    # Steps.

    def step(self, name, function):
        self.current = name
        self.log(f"\n== {name}")
        try:
            before = {k for k in self.checks if k.startswith(name + ":")}
            function(self)
            mine = {k for k, v in self.checks.items() if k.startswith(name + ":")} - before
            failed = [k for k in mine if not self.checks[k]]
            self.steps[name] = {"status": "failed" if failed else "passed", "failed": failed}
            if not mine:
                self.steps[name] = {"status": "failed", "reason": "step made no checks"}
        except Blocked as reason:
            self.steps[name] = {"status": "blocked", "reason": str(reason)}
            self.checks[f"{name}: (blocked) {reason}"] = False
            self.log(f"BLOCKED {name}: {reason}")
        except Exception as error:  # noqa: BLE001 - a crashed step is a failed step with its message
            self.steps[name] = {"status": "failed", "reason": f"{type(error).__name__}: {error}"}
            self.checks[f"{name}: (crashed) {error}"] = False
            self.log(f"CRASH {name}: {type(error).__name__}: {error}")

    # Guest facts.

    def bsm_pids(self):
        return self.vm.pids("bsm")

    def user_uid(self, name):
        out = self.vm.run(f"id -u {name}", 10).strip().splitlines()
        return int(out[-1]) if out and out[-1].isdigit() else None

    def need_bsm(self):
        missing = [path for path in BSM_BINARIES if not self.vm.guest_has(path)]
        if missing:
            raise Blocked("not in this image: " + ", ".join(missing))
        for _ in range(60):
            if self.bsm_pids():
                break
            time.sleep(2)
        if not self.bsm_pids():
            raise Blocked("bsm is not running (systemctl is-active bsm: " + self.vm.run("systemctl is-active bsm", 10).strip().splitlines()[-1] + ")")
        self.vm.ensure_monitor()

    def need_hosted_gnoblin(self):
        out = self.vm.run("gnoblin --help 2>&1 | grep -c -- --hosted", 20).strip().splitlines()
        if not out or out[-1].strip() in ("", "0"):
            raise Blocked("this image's gnoblin has no --hosted mode (hosted Gnoblin needs the rebuilt image)")

    def assert_same_bsm(self, label):
        pids = self.bsm_pids()
        recorded = self.state.setdefault("bsm_pid", pids[0] if pids else None)
        self.check(f"bsm pid unchanged {label}", bool(pids) and pids == [recorded], f"{pids} vs {recorded}")

    def sessions(self):
        data, text = self.vm.bsmctl_json("sessions")
        self.dump(f"bsmctl-sessions-{int(time.monotonic() - self.vm.t0)}", text)
        return data

    def session_id_of(self, uid):
        found = []

        def walk(node):
            if isinstance(node, dict):
                if node.get("uid") == uid:
                    for key in ("id", "session", "session_id"):
                        if isinstance(node.get(key), str):
                            found.append(node[key])
                for value in node.values():
                    walk(value)
            elif isinstance(node, list):
                for value in node:
                    walk(value)

        walk(self.sessions())
        return found[0] if found else None

    def gnoblin_pid(self, user):
        pids = self.vm.pids("gnoblin", user)
        return pids[0] if pids else None

    def type_password(self, password):
        self.vm.type_text(password)
        self.vm.key("ret")

    # Frame series.

    def reference(self, head, seconds=0.0):
        """Newest plane of a display, after waiting `seconds` for the picture to settle."""
        time.sleep(seconds)
        return self.vm.sampler.latest_plane(head)

    def settle(self, seconds=2.0):
        time.sleep(seconds)


def transition_times(events, kind):
    """(animating, completed) host times of the newest transition of `kind`, or None parts."""
    animating = completed = None
    for when, event in events:
        if event.get("event") == "transition" and event.get("kind") == kind:
            if event.get("phase") == "animating":
                animating, completed = when, None
            elif event.get("phase") in ("completed", "committed") and completed is None and animating is not None:
                completed = when
    return animating, completed


# ---- steps ---------------------------------------------------------------------


def step_boot(run):
    vm = run.vm
    cmdline = vm.run("cat /proc/cmdline", 10)
    run.check("kernel command line has bingux.bsm", "bingux.bsm" in cmdline, cmdline.strip()[-120:])
    connectors = vm.run("ls /sys/class/drm | grep -E '^card[0-9]+-' ", 10).split()
    virtual = [c for c in connectors if "Virtual" in c]
    run.dump("drm-connectors", "\n".join(connectors))
    modes = vm.run("for f in /sys/class/drm/card*-*/status; do echo $f $(cat $f); done", 10)
    run.dump("drm-status", modes)
    connected = [line for line in modes.splitlines() if line.rstrip().endswith(" connected")]
    run.check(
        "guest sees two connected virtual connectors",
        len(virtual) >= HEAD_COUNT and len(connected) >= HEAD_COUNT,
        f"{connectors}; connected: {len(connected)}",
    )
    frames = {head: run.frame(head) for head in range(HEAD_COUNT)}
    run.check("both displays deliver frames", all(f is not None for f in frames.values()), str({h: None if f is None else f.shape for h, f in frames.items()}))
    run.state["uid_a"] = run.user_uid(USER_A[0])
    vm.run(f"id {USER_B[0]} >/dev/null 2>&1 || (useradd -m -G video,audio -s /bin/bash {USER_B[0]} && echo {USER_B[0]}:{USER_B[1]} | chpasswd)", 30)
    run.state["uid_b"] = run.user_uid(USER_B[0])
    run.check("two accounts exist", run.state["uid_a"] is not None and run.state["uid_b"] is not None and run.state["uid_a"] != run.state["uid_b"])
    run.shot("boot")
    present = [path for path in BSM_BINARIES if vm.guest_has(path)]
    run.state["bsm_installed"] = len(present) == len(BSM_BINARIES)
    run.log("bsm binaries in this image: " + (", ".join(present) or "none"))


def step_greeter(run):
    vm = run.vm
    run.need_bsm()
    pids = run.bsm_pids()
    run.state["bsm_pid"] = pids[0]
    run.check("exactly one bsm process", len(pids) == 1, str(pids))
    comm = vm.run(f"cat /proc/{pids[0]}/comm", 10).strip().splitlines()[-1]
    exe = vm.run(f"readlink /proc/{pids[0]}/exe", 10).strip().splitlines()[-1]
    run.check("/proc/PID/comm is bsm", comm == "bsm", comm)
    run.check("bsm runs the installed binary", exe == "/usr/bin/bsm", exe)
    run.check("bsm.service is active", vm.run("systemctl is-active bsm", 10).strip().endswith("active"))
    rows = vm.processes()
    owner = next((r[2] for r in rows if r[0] == pids[0]), "")
    run.check("bsm is not root", owner == "bsm", owner)
    greeter = [r for r in rows if r[2] == GREETER_USER]
    run.check("greeter processes run as the greeter account", bool(greeter), f"{len(greeter)} processes")
    tree = vm.descendants([r[0] for r in greeter], rows)
    names = {r[3] for r in greeter} | {comm for _, comm in tree}
    run.check("no gnoblin in the greeter's process tree", not ({"gnoblin", "mutter"} & names), str(sorted(names)))
    run.check("no gnoblin at all before a login", not [r for r in rows if r[3] == "gnoblin"])
    run.check("no Quickshell desktop shell before a login", not [r for r in rows if "bingux-shell" in r[4] and r[2] != GREETER_USER])
    data, text = vm.bsmctl_json("users")
    run.dump("bsmctl-users", text)
    uids = set()

    def walk(node):
        if isinstance(node, dict):
            if isinstance(node.get("uid"), int):
                uids.add(node["uid"])
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(data)
    run.check("the account list holds both accounts", {run.state["uid_a"], run.state["uid_b"]} <= uids, str(sorted(uids)))
    run.check("no system account is listed", not (uids & {0, 1, 2}) and all(u >= 1000 for u in uids), str(sorted(uids)))

    # Selecting an account changes the greeter, not the session state.
    vm.bsmctl_json(f"select-user {run.state['uid_a']}")
    run.settle(2.5)
    plane_a = {h: run.reference(h) for h in range(HEAD_COUNT)}
    vm.bsmctl_json(f"select-user {run.state['uid_b']}")
    run.settle(2.5)
    plane_b = {h: run.reference(h) for h in range(HEAD_COUNT)}
    run.shot("greeter-b")
    vm.bsmctl_json(f"select-user {run.state['uid_a']}")
    run.settle(2.0)
    run.shot("greeter-a")
    differ = [similarity(plane_a[h], plane_b[h]) for h in range(HEAD_COUNT)]
    run.check("selecting another account changes the greeter picture", min(differ) < 0.9995, f"similarity {differ}")
    run.check("selecting an account starts no Gnoblin", not vm.pids("gnoblin"))
    events = vm.monitor_events()
    run.check("selecting an account changes no foreground session", not [e for _, e in events if e.get("event") == "foreground_changed" and e.get("session")])
    for head in range(HEAD_COUNT):
        plane = plane_a[head]
        run.check(f"display {head} shows the greeter, not black", plane is not None and coverage(plane) > 0.03, f"coverage {None if plane is None else round(coverage(plane), 3)}")
    run.state["greeter_plane"] = plane_a
    run.assert_same_bsm("after the greeter checks")


def login_and_reveal(run, who, uid_key):
    """Authenticate `who`, wait for the reveal, return the evidence dict."""
    vm = run.vm
    uid = run.state[uid_key]
    user, password = who
    vm.bsmctl_json(f"select-user {uid}")
    run.settle(2.5)
    greeter = {h: run.reference(h) for h in range(HEAD_COUNT)}
    started = run.stamp(f"login {user}")
    before_events = len(vm.monitor_events())
    run.type_password(password)
    done = vm.wait_monitor(lambda e: e.get("event") == "transition" and e.get("kind") == "new_login_fade" and e.get("phase") in ("completed", "committed"), 240)
    events = vm.monitor_events()[before_events:]
    run.check("the reveal transition completed", done is not None)
    if done is None:
        raise RuntimeError("no new_login_fade completion within 240 s")
    run.settle(2.5)
    desktop = {h: run.reference(h) for h in range(HEAD_COUNT)}
    return {"started": started, "events": events, "greeter": greeter, "desktop": desktop, "uid": uid, "user": user}


def check_progress(run, ev):
    """Milestones arrive in documented order, each after the process that is its evidence."""
    models = [e for _, e in ev["events"] if e.get("event") == "progress"]
    run.check("progress events were observed", len(models) >= 5, f"{len(models)} events")
    reached_orders = []
    last = []
    ok_prefix = True
    for model in models:
        body = model.get("model", model)
        reached = body.get("reached", [])
        ok_prefix &= reached[: len(last)] == last and reached == STAGE_ORDER[: len(reached)]
        last = reached
        reached_orders.append(len(reached))
    run.check("every progress model extends the previous one in stage order", ok_prefix)
    run.check("the final model has every stage", last == STAGE_ORDER, str(last))
    run.check("progress never moves backwards", is_monotone(reached_orders), str(reached_orders))
    # Evidence: a stage is not reached before the process that proves it started.
    arrival = {}
    for when, event in ev["events"]:
        if event.get("event") == "progress":
            for stage in event.get("model", event).get("reached", []):
                arrival.setdefault(stage, when)
    run.csv("progress-" + ev["user"], ["stage", "host_time_s"], [(s, arrival[s] - run.vm.t0) for s in STAGE_ORDER if s in arrival])
    clock = run.vm._offset
    started = run.vm.run(
        f"for p in $(pgrep -u {ev['user']} -x gnoblin) $(pgrep -u {ev['user']} -f bingux-shell); do "
        "awk -v b=$(awk '/btime/{print $2}' /proc/stat) -v h=$(getconf CLK_TCK) '{print $1, b + $22/h}' /proc/$p/stat; done", 20
    )
    run.dump("process-starts-" + ev["user"], started)
    starts = [float(line.split()[1]) for line in started.splitlines() if len(line.split()) == 2 and line.split()[1].replace(".", "").isdigit()]
    if starts and "gnoblin_ready" in arrival:
        gnoblin_start = min(starts) - clock + (time.monotonic() - time.time())
        run.check("gnoblin_ready arrives after the Gnoblin process started", arrival["gnoblin_ready"] >= gnoblin_start - 0.5, f"{arrival['gnoblin_ready'] - gnoblin_start:.2f} s after")


def check_fade_over_greeter(run, ev, label):
    events = run.vm.monitor_events()
    animating, completed = transition_times(events, "new_login_fade")
    run.check("the reveal transition reports animating then completed", animating is not None and completed is not None)
    if animating is None or completed is None:
        return
    alphas = {}
    for head in range(HEAD_COUNT):
        g, d = ev["greeter"][head], ev["desktop"][head]
        before = run.vm.sampler.window(head, ev["started"], animating - 0.05)
        during = run.vm.sampler.window(head, animating - 0.2, completed + 0.2)
        early = [fade_alpha(s.plane, g, d) for s in before]
        early = [a for a in early if a is not None]
        run.check(f"display {head}: no desktop pixel before the reveal starts", bool(early) and max(early) < 0.06, f"max alpha {max(early) if early else None} over {len(early)} samples")
        series = [(s.t, fade_alpha(s.plane, g, d), s) for s in during]
        series = [(t, a, s) for t, a, s in series if a is not None]
        alphas[head] = series
        run.csv(f"fade-{label}-display{head}", ["t_s", "alpha"], [(t - run.vm.t0, a) for t, a, _ in series])
        values = [a for _, a, _ in series]
        middle = [(t, a, s) for t, a, s in series if 0.1 < a < 0.9]
        run.check(f"display {head}: opacity rises monotonically 0 to 1", is_monotone(values) and values and min(values) < 0.1 and max(values) > 0.9, f"min {min(values, default=None)}, max {max(values, default=None)}, n {len(values)}")
        run.check(f"display {head}: the fade has at least {MIN_INTERMEDIATE} intermediate frames", len(middle) >= MIN_INTERMEDIATE, f"{len(middle)} frames")
        residual = max((blend_residual(s.plane, g, d, a) for _, a, s in middle), default=None)
        run.check(f"display {head}: the greeter stays visible under the rising desktop", residual is not None and residual < 6.0, f"max blend residual {residual}")
        blackest = min((coverage(s.plane) for s in during), default=1.0)
        run.check(f"display {head}: no black or placeholder frame during the fade", blackest > 0.02, f"min coverage {blackest:.3f}")
    crossings = {}
    for head, series in alphas.items():
        for t, a, _ in series:
            if a >= 0.5:
                crossings[head] = t
                break
    if len(crossings) == HEAD_COUNT:
        skew = abs(crossings[0] - crossings[1])
        run.check("the displays fade together", skew < 0.15, f"{skew * 1000:.0f} ms between the 50% points")


def step_login_a(run):
    run.need_bsm()
    run.need_hosted_gnoblin()
    run.assert_same_bsm("before the login")
    ev = login_and_reveal(run, USER_A, "uid_a")
    run.state["ev_a"] = ev
    check_progress(run, ev)
    check_fade_over_greeter(run, ev, "a")
    run.shot("desktop-a")
    run.state["gnoblin_a"] = run.gnoblin_pid(USER_A[0])
    run.check("Gnoblin of A runs as A", run.state["gnoblin_a"] is not None)
    run.check("the Bingux shell runs as A", bool(run.vm.run(f"pgrep -u {USER_A[0]} -f bingux-shell", 10).strip()))
    run.check("no Gnoblin of A's runs as root or the greeter", not [p for p in run.vm.processes() if p[3] == "gnoblin" and p[2] in ("root", GREETER_USER, "bsm")])
    out = run.vm.run(f"loginctl list-sessions --no-legend | grep -w {USER_A[0]}", 20)
    run.dump("loginctl-a", out)
    run.check("A has a real logind session", bool(out.strip()), out.strip()[:100])
    run.state["session_a"] = run.session_id_of(run.state["uid_a"])
    run.check("bsm lists A's session", run.state["session_a"] is not None)
    run.assert_same_bsm("after the login")


def step_lock_a(run):
    run.need_bsm()
    if "session_a" not in run.state or not run.state.get("session_a"):
        raise Blocked("no session of A (login-a did not pass)")
    vm = run.vm
    desktop = {h: run.reference(h) for h in range(HEAD_COUNT)}
    run.state["desktop_a"] = desktop
    run.stamp("lock requested")
    out, text = vm.bsmctl_json(f"lock {run.state['session_a']}")
    run.dump("bsmctl-lock", text)
    locked = vm.wait_monitor(lambda e: e.get("event") == "lock_state_changed" and e.get("locked") is True, 30)
    run.check("bsm reports the session locked", locked is not None)
    run.settle(2.5)
    run.state["lock_scene"] = {h: run.reference(h) for h in range(HEAD_COUNT)}
    run.shot("locked-a")
    for head in range(HEAD_COUNT):
        sim = similarity(run.state["lock_scene"][head], desktop[head])
        run.check(f"display {head}: the locked picture is not the desktop", sim < 0.97, f"similarity {sim:.4f}")
    run.check("A's Gnoblin keeps running while locked", run.gnoblin_pid(USER_A[0]) == run.state["gnoblin_a"])
    run.check("the logind session of A reports a locked hint", "LockedHint=yes" in vm.run(f"loginctl show-session $(loginctl list-sessions --no-legend | awk '$3==\"{USER_A[0]}\"{{print $1; exit}}') -p LockedHint", 20))
    run.assert_same_bsm("after the lock")


def slide_checks(run, label, scene, below, start, end):
    events_start, events_end = start, end
    for head in range(HEAD_COUNT):
        during = run.vm.sampler.window(head, events_start - 0.2, events_end + 0.3)
        series = []
        for sample in during:
            measured = slide_offset(sample.plane, scene[head], below[head])
            if measured is not None:
                series.append((sample.t, measured[0], measured[1]))
        height = scene[head].shape[0] * 4
        run.csv(f"slide-{label}-display{head}", ["t_s", "offset_px", "error"], [(t - run.vm.t0, o, e) for t, o, e in series])
        offsets = [o for _, o, _ in series]
        middle = [(t, o, e) for t, o, e in series if 0.05 * height < o < 0.95 * height]
        run.check(f"display {head}: the greeter scene moves up monotonically", bool(offsets) and is_monotone(offsets), f"n {len(offsets)}")
        run.check(f"display {head}: it starts at the top and leaves the screen", bool(offsets) and min(offsets) < 0.05 * height and max(offsets) > 0.95 * height, f"min {min(offsets, default=None)}, max {max(offsets, default=None)}")
        run.check(f"display {head}: the slide has at least {MIN_INTERMEDIATE} intermediate frames", len(middle) >= MIN_INTERMEDIATE, f"{len(middle)} frames")
        worst = max((e for _, _, e in middle), default=None)
        run.check(f"display {head}: what the slide reveals is the existing desktop", worst is not None and worst < 8.0, f"max fit error {worst}")
        # Easing: ease-out. Early velocity must exceed late velocity.
        if len(middle) >= 4:
            half = len(middle) // 2
            first = (middle[half - 1][1] - middle[0][1]) / max(middle[half - 1][0] - middle[0][0], 1e-3)
            second = (middle[-1][1] - middle[half][1]) / max(middle[-1][0] - middle[half][0], 1e-3)
            run.check(f"display {head}: the slide eases out (fast then slow)", first > second, f"{first:.0f} vs {second:.0f} px/s")


def unlock(run, who, uid_key, scene, desktop, label, expect_progress):
    vm = run.vm
    user, password = who
    vm.bsmctl_json(f"select-user {run.state[uid_key]}")
    run.settle(2.0)
    before = len(vm.monitor_events())
    run.stamp(f"unlock {user}")
    run.type_password(password)
    done = vm.wait_monitor(lambda e: e.get("event") == "transition" and e.get("kind") == "unlock_slide_up" and e.get("phase") in ("completed", "committed"), 90)
    run.check("the unlock transition completed", done is not None)
    events = vm.monitor_events()
    animating, completed = transition_times(events, "unlock_slide_up")
    run.check("the unlock transition reports animating then completed", animating is not None and completed is not None)
    progress = [e for _, e in events[before:] if e.get("event") == "progress"]
    run.check("resuming a running session shows no startup progress", bool(progress) == expect_progress, f"{len(progress)} progress events")
    if animating is not None and completed is not None:
        slide_checks(run, label, scene, desktop, animating, completed)


def step_unlock_a(run):
    run.need_bsm()
    if "lock_scene" not in run.state:
        raise Blocked("A is not locked (lock-a did not pass)")
    pid_before = run.gnoblin_pid(USER_A[0])
    unlock(run, USER_A, "uid_a", run.state["lock_scene"], run.state["desktop_a"], "unlock-a", False)
    run.settle(2.0)
    run.shot("unlocked-a")
    run.check("the same Gnoblin of A is running after the unlock", run.gnoblin_pid(USER_A[0]) == pid_before == run.state["gnoblin_a"], f"{pid_before} -> {run.gnoblin_pid(USER_A[0])}")
    run.assert_same_bsm("after the unlock")
    for head in range(HEAD_COUNT):
        sim = similarity(run.reference(head), run.state["desktop_a"][head])
        run.check(f"display {head}: the desktop is back after the unlock", sim > 0.9, f"similarity {sim:.3f}")


def step_switch_b(run):
    run.need_bsm()
    if not run.state.get("session_a"):
        raise Blocked("no session of A")
    vm = run.vm
    run.state["desktop_a"] = {h: run.reference(h) for h in range(HEAD_COUNT)}
    vm.bsmctl_json(f"lock {run.state['session_a']}")
    vm.wait_monitor(lambda e: e.get("event") == "lock_state_changed" and e.get("locked") is True, 30)
    run.settle(2.5)
    run.state["lock_scene"] = {h: run.reference(h) for h in range(HEAD_COUNT)}
    sessions = run.sessions()
    run.dump("sessions-before-b", json.dumps(sessions, indent=2) if sessions is not None else "")
    ev = login_and_reveal(run, USER_B, "uid_b")
    run.state["ev_b"] = ev
    check_progress(run, ev)
    check_fade_over_greeter(run, ev, "b")
    run.shot("desktop-b")
    run.state["gnoblin_b"] = run.gnoblin_pid(USER_B[0])
    run.check("B has its own Gnoblin", run.state["gnoblin_b"] is not None and run.state["gnoblin_b"] != run.state["gnoblin_a"])
    run.check("A's Gnoblin kept running behind B", run.gnoblin_pid(USER_A[0]) == run.state["gnoblin_a"])
    # Isolation: no process of either user can see the other's runtime directory content.
    leak = run.vm.run(f"runuser -u {USER_B[0]} -- sh -c 'ls /run/user/{run.state['uid_a']} 2>&1 | head -1'", 20)
    run.check("B cannot read A's runtime directory", "denied" in leak.lower() or "cannot" in leak.lower(), leak.strip()[:80])
    run.state["session_b"] = run.session_id_of(run.state["uid_b"])
    run.check("bsm lists both sessions", run.state["session_b"] is not None and run.session_id_of(run.state["uid_a"]) is not None)
    run.assert_same_bsm("after the switch")


def step_back_to_a(run):
    run.need_bsm()
    if not run.state.get("session_b"):
        raise Blocked("no session of B")
    vm = run.vm
    pid_a = run.gnoblin_pid(USER_A[0])
    run.state["desktop_b"] = {h: run.reference(h) for h in range(HEAD_COUNT)}
    vm.bsmctl_json(f"lock {run.state['session_b']}")
    vm.wait_monitor(lambda e: e.get("event") == "lock_state_changed" and e.get("locked") is True, 30)
    run.settle(2.5)
    vm.bsmctl_json(f"select-user {run.state['uid_a']}")
    run.settle(2.5)
    scene = {h: run.reference(h) for h in range(HEAD_COUNT)}
    run.shot("picker-a")
    unlock(run, USER_A, "uid_a", scene, run.state["desktop_a"], "back-to-a", False)
    run.settle(2.0)
    run.check("A's original Gnoblin is back, not relaunched", run.gnoblin_pid(USER_A[0]) == pid_a == run.state["gnoblin_a"], f"{pid_a} -> {run.gnoblin_pid(USER_A[0])}")
    run.check("B keeps running", run.gnoblin_pid(USER_B[0]) == run.state["gnoblin_b"])
    run.assert_same_bsm("after switching back")


def step_logout_a(run):
    run.need_bsm()
    if not run.state.get("session_a"):
        raise Blocked("no session of A")
    vm = run.vm
    desktop_a = {h: run.reference(h) for h in range(HEAD_COUNT)}
    greeter = run.state["lock_scene"]
    before = len(vm.monitor_events())
    run.stamp("logout requested")
    out, text = vm.bsmctl_json(f"logout {run.state['session_a']}")
    run.dump("bsmctl-logout", text)
    done = vm.wait_monitor(lambda e: e.get("event") == "operation_finished", 60)
    run.check("the logout operation finished successfully", done is not None and done[1].get("success") is True)
    events = vm.monitor_events()
    animating, completed = transition_times(events, "logout_fade")
    run.check("bsm ran a logout fade", animating is not None)
    run.settle(3.0)
    run.shot("logged-out-a")
    run.check("A's session is gone", not vm.pids("gnoblin", USER_A[0]) and not vm.run(f"pgrep -u {USER_A[0]} -f bingux-shell", 10).strip())
    run.check("B's Gnoblin survives A's logout", run.gnoblin_pid(USER_B[0]) == run.state["gnoblin_b"])
    run.assert_same_bsm("after the logout")
    after_planes = {}
    for head in range(HEAD_COUNT):
        after = after_planes[head] = run.reference(head)
        a_visible = fade_alpha(after, run.state["desktop_b"][head], desktop_a[head])
        run.check(f"display {head}: neither desktop is on screen after the logout", coverage(after) > 0.03 and (a_visible is None or a_visible < 0.1), f"coverage {coverage(after):.3f}")
    if animating is not None:
        for head in range(HEAD_COUNT):
            window = run.vm.sampler.window(head, animating - 0.2, (completed or animating + 1.0) + 0.3)
            alphas = [fade_alpha(s.plane, desktop_a[head], after_planes[head]) for s in window]
            alphas = [a for a in alphas if a is not None]
            run.csv(f"logout-fade-display{head}", ["alpha_greeter"], [(a,) for a in alphas])
            run.check(f"display {head}: A's desktop fades out into the greeter", is_monotone(alphas) and alphas and alphas[-1] > 0.9, f"n {len(alphas)}")


def step_greeter_kill(run):
    run.need_bsm()
    if not run.state.get("session_b"):
        raise Blocked("no session of B")
    vm = run.vm
    vm.bsmctl_json(f"lock {run.state['session_b']}")
    vm.wait_monitor(lambda e: e.get("event") == "lock_state_changed" and e.get("locked") is True, 30)
    run.settle(2.5)
    scene = {h: run.reference(h) for h in range(HEAD_COUNT)}
    desktop = run.state["desktop_b"]
    killed = time.monotonic()
    vm.run(f"pkill -9 -u {GREETER_USER}", 10)
    run.stamp("greeter killed")
    run.settle(12.0)
    for head in range(HEAD_COUNT):
        window = vm.sampler.window(head, killed, time.monotonic())
        worst = 0.0
        for sample in window:
            a = fade_alpha(sample.plane, scene[head], desktop[head])
            if a is not None:
                worst = max(worst, a)
        run.check(f"display {head}: no desktop pixel after the greeter died", worst < 0.12, f"max alpha {worst:.3f} over {len(window)} samples")
        run.check(f"display {head}: the compositor shows an opaque cover or the greeter", bool(window) and all(similarity(s.plane, scene[head]) > 0.9 or coverage(s.plane) < 0.03 for s in window), f"{len(window)} samples")
    run.check("B's session stays running but protected", run.gnoblin_pid(USER_B[0]) == run.state["gnoblin_b"])
    run.assert_same_bsm("after the greeter died")
    came_back = False
    for _ in range(40):
        time.sleep(1.0)
        if vm.pids("bingux-shell", GREETER_USER) or any(p[2] == GREETER_USER for p in vm.processes()):
            came_back = True
            break
    run.check("the greeter is started again", came_back)
    run.settle(5.0)
    for head in range(HEAD_COUNT):
        sim = similarity(run.reference(head), scene[head])
        run.check(f"display {head}: the lock scene is back", sim > 0.9, f"similarity {sim:.3f}")


def step_power(run):
    run.need_bsm()
    vm = run.vm
    # A blocking shutdown inhibitor: bsm must refuse and restore a scene, never leave a black screen.
    vm.run("nohup systemd-inhibit --what=shutdown --mode=block --who=bsm-test --why=denied-power-test sleep 600 >/dev/null 2>&1 &", 10)
    time.sleep(1.5)
    scene = {h: run.reference(h) for h in range(HEAD_COUNT)}
    before = len(vm.monitor_events())
    run.stamp("denied power requested")
    out, text = vm.bsmctl_json("reboot")
    run.dump("bsmctl-reboot-denied", text)
    finished = vm.wait_monitor(lambda e: e.get("event") == "operation_finished", 60)
    run.check("the denied reboot finished as a failure", finished is not None and finished[1].get("success") is False, str(finished[1] if finished else None))
    run.check("the failure carries the inhibited reason", finished is not None and finished[1].get("reason") in ("inhibited", "shutdown_rejected", "unauthorised"), str(finished[1].get("reason") if finished else None))
    run.settle(4.0)
    run.shot("power-denied")
    for head in range(HEAD_COUNT):
        after = run.reference(head)
        run.check(f"display {head}: a scene is restored after the denied request", coverage(after) > 0.03 and similarity(after, scene[head]) > 0.9, f"coverage {coverage(after):.3f}, similarity {similarity(after, scene[head]):.3f}")
    run.check("no session was killed by the denied request", bool(vm.pids("gnoblin")))
    vm.run("pkill -f 'sleep 600'; pkill -f denied-power-test", 10)
    time.sleep(1.0)
    mark = vm.event_mark()
    run.stamp("power off requested")
    vm.bsmctl_json("poweroff")
    started = time.monotonic()
    animating = vm.wait_monitor(lambda e: e.get("event") == "transition" and e.get("kind") == "power_fade" and e.get("phase") == "animating", 30)
    run.check("bsm runs the power fade", animating is not None)
    if animating is not None:
        for head in range(HEAD_COUNT):
            window = vm.sampler.window(head, animating[0] - 0.1, time.monotonic())
            levels = [float(s.plane.mean()) for s in window]
            run.csv(f"power-fade-display{head}", ["mean_luma"], [(v,) for v in levels])
            run.check(f"display {head}: the cover darkens monotonically", len(levels) >= 3 and is_monotone(levels, decreasing=True), f"n {len(levels)}")
    run.check("the guest powers off through logind", vm.wait_shutdown(mark, 120))
    run.state["shutdown_after"] = time.monotonic() - started


STEPS = [
    ("boot", step_boot),
    ("greeter", step_greeter),
    ("login-a", step_login_a),
    ("lock-a", step_lock_a),
    ("unlock-a", step_unlock_a),
    ("switch-b", step_switch_b),
    ("back-to-a", step_back_to_a),
    ("logout-a", step_logout_a),
    ("greeter-kill", step_greeter_kill),
    ("power", step_power),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--iso")
    parser.add_argument("--inject", help="directory staged by tests/bsm-vm-inject.py")
    parser.add_argument("--out", default=os.environ.get("BINGUX_DISTRO_OUT", str(ROOT / "distro/out/bsm-test")))
    parser.add_argument("--only", help="comma separated step names")
    parser.add_argument("--no-bsm-param", action="store_true", help="boot without bingux.bsm")
    parser.add_argument("--smoke", action="store_true", help="only boot and greeter")
    parser.add_argument("--keep-work", action="store_true")
    args = parser.parse_args()

    iso = find_iso(args.iso)
    if iso is None or not iso.is_file():
        print("no live image; run distro/build.sh first")
        return 1
    missing = missing_tools("qemu-system-x86_64", "ffmpeg", "ffprobe")
    if missing:
        print("missing tool: " + ", ".join(missing))
        return 1
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for stale in list(out.glob("*.png")) + list(out.glob("*.csv")) + list(out.glob("*.txt")):
        stale.unlink()
    base = Path(os.environ.get("BSM_VM_DIR", "/tmp/bsm-vm"))
    base.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="bsm-qemu-", dir=base))
    vm = BsmVm(work, iso=iso, record=work / "record", inject=args.inject, bsm_param=not args.no_bsm_param)
    run = Run(vm, out)
    wanted = [s.strip() for s in args.only.split(",")] if args.only else [n for n, _ in STEPS]
    if args.smoke:
        wanted = ["boot", "greeter"]
    try:
        vm.start()
        run.log(f"image {iso}; work {work}")
        if not vm.start_shell():
            run.log("FAIL no serial shell")
            run.checks["serial shell"] = False
        else:
            run.checks["serial shell"] = True
            vm.run("systemctl is-system-running --wait 2>&1 | head -1", 120)
            if args.inject:
                run.log("injecting " + args.inject)
                applied = vm.run("mkdir -p /mnt/bsm-inject && modprobe 9pnet_virtio; modprobe 9p; mount -t 9p -o trans=virtio,version=9p2000.L,ro bsminject /mnt/bsm-inject && sh /mnt/bsm-inject/apply.sh 2>&1 | tail -n 30", 600)
                run.dump("inject", applied)
                run.checks["inject applied"] = "INJECT-DONE" in applied
                run.log(applied)
                time.sleep(8)
            for name, function in STEPS:
                if name in wanted:
                    run.step(name, function)
                else:
                    run.steps[name] = {"status": "skipped"}
            if vm.alive():
                run.dump("ps-final", vm.run("ps -eo pid,ppid,user:12,comm,args --forest | cut -c1-200", 30))
                run.dump("journal-bsm", vm.run("journalctl -b --no-pager -o short-iso -u bsm -u bsm-helper -u bsm-seatd | tail -n 300", 90))
                run.dump("journal-errors", vm.run("journalctl -b -p err --no-pager -o cat | tail -n 80", 60))
                run.dump("bsmctl-status", vm.bsmctl_json("status")[1])
                run.dump("monitor-log", vm.run(f"cat {MONITOR_LOG}", 30))
    finally:
        (out / "serial.log").write_bytes(vm.log)
        (out / "report.txt").write_text("\n".join(run.report) + "\n")
        (out / "checks.json").write_text(json.dumps(run.checks, indent=2) + "\n")
        (out / "steps.json").write_text(json.dumps(run.steps, indent=2) + "\n")
        vm.stop()
        for name in ("timeline.json",):
            source = work / "record" / name
            if source.is_file():
                shutil.copy(source, out / name)
        video = work / "record" / "flow.mp4"
        if video.is_file() and video.stat().st_size < 40_000_000:
            shutil.copy(video, out / "flow.mp4")
        if not args.keep_work:
            shutil.rmtree(work, ignore_errors=True)
    print()
    for name, info in run.steps.items():
        print(f"{info['status']:8} {name}" + (f"  ({info.get('reason')})" if info.get("reason") else ""))
    print("artifacts:", out)
    statuses = {info["status"] for info in run.steps.values()}
    if "failed" in statuses or not run.checks.get("serial shell", False):
        return 1
    return 2 if "blocked" in statuses else 0


if __name__ == "__main__":
    sys.exit(main())
