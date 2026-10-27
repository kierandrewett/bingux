#!/usr/bin/env python3
"""Measure and verify bsm's boot and display handover in QEMU (spec section 11, phase 8).

usage: tests/bsm-boot.py [--iso ISO] [--stage DIR] [--out DIR] [--only PHASE[,PHASE...]]
                         [--keep-work]

Phases (each boots its own VM from the image's own boot loader, two display heads, serial root
shell, frame log of every head, and the guest tracer tests/bsm-boot-trace.py):

  handover   cold boot with `bingux.bsm`: Plymouth -> bsm greeter. Every frame QEMU's VNC
             server delivers is classified (splash, black, placeholder, flat, content); the
             tracer logs who holds DRM master, the CRTC state, who owns the scanout
             framebuffer and the VT mode; the journal and systemd give the ordering.
  modeset    the same boot with `drm.debug`: the kernel's own log of every CRTC disable,
             enable and mode set, SET_MASTER/DROP_MASTER with the process name, and fbcon's
             takeover. Authoritative for "was the mode reset", where frames are sampled.
  greetd     cold boot WITHOUT `bingux.bsm`: greetd and its greeter start, bsm does not
             (screenshot), and the plymouth ordering drop-in does not stop plymouth-quit.
  dm         display-manager tool on the image's own bsm.service: enable, disable, a foreign
             display manager that must not be clobbered, package files enable nothing.
  fallback   bsm's binary is broken by a drop-in: three starts fail, bsm-fallback waits for the
             give-up and then greetd comes up (screenshot); repair, bsm returns.
  session    VT switch to a text console and back (chvt and Ctrl+Alt+F3), then two real
             logins and `systemctl restart bsm`: what survives and what is shown.

Without --stage the image is measured as shipped (the 2026.10.27 image predates the handover):
the checks that assert the handover are reported as `measured`, not as failures. With --stage
DIR (tests/bsm-vm-inject.py stage --build host --out DIR) the package's unit text, scripts and
binaries are used from a cold boot (tests/bsm_boot_support.py, "Staged boot") and every check
is asserted.

Artifacts land in distro/out/bsm-test/boot/ (boot/baseline/ without --stage): report.txt,
checks.json, measurements.json, per phase a directory with the tracer log, journal and systemd
excerpts, frame tables, stills and a contact sheet of the handover. The video and working files
stay under /tmp/bsm-vm.

Exit status: 0 every asserted check passed, 1 one failed, 2 nothing failed but a phase is blocked.
"""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import bsm_boot_analysis as A  # noqa: E402
import bsm_boot_support as S  # noqa: E402
from distro_vm import ROOT, find_iso, missing_tools  # noqa: E402

PHASES = ["handover", "modeset", "greetd", "dm", "fallback", "session"]
USER_A = ("live", "live")
USER_B = ("bsmtest", "bsmtest")
SHELL_TIMEOUT = 900
GREETER_TIMEOUT = 600


class Blocked(Exception):
    """A phase cannot run: a prerequisite is missing. Not a pass, not a failure."""


class Context:
    def __init__(self, iso, stage, out, keep_work):
        self.iso = iso
        self.stage = Path(stage) if stage else None
        self.out = out
        self.keep_work = keep_work
        self.base = Path(os.environ.get("BSM_VM_DIR", "/tmp/bsm-vm"))
        self.base.mkdir(parents=True, exist_ok=True)
        self.checks = {}
        self.measured = {}
        self.phases = {}
        self.numbers = {}
        self.report = []
        self.phase = None

    @property
    def staged(self):
        return self.stage is not None

    def log(self, text):
        print(text, flush=True)
        self.report.append(text)

    def check(self, label, ok, detail="", handover=False):
        """Record a check. A `handover` check is only asserted on a staged boot; on the shipped image it is measured."""
        name = f"{self.phase}: {label}"
        suffix = f"  [{detail}]" if detail else ""
        if handover and not self.staged:
            self.measured[name] = {"ok": bool(ok), "detail": detail}
            self.log(("holds " if ok else "absent") + f"  {name}{suffix}  (measured: the image predates the handover)")
        else:
            self.checks[name] = bool(ok)
            self.log(("ok    " if ok else "FAIL  ") + f"{name}{suffix}")
        return bool(ok)

    def number(self, label, value):
        self.numbers[f"{self.phase}: {label}"] = value
        self.log(f"      {self.phase}: {label} = {value}")

    def dump(self, name, text):
        folder = self.out / self.phase
        folder.mkdir(parents=True, exist_ok=True)
        (folder / name).write_text(text)

    def phase_dir(self):
        folder = self.out / self.phase
        folder.mkdir(parents=True, exist_ok=True)
        return folder

    def read_dump(self, name):
        path = self.out / self.phase / name
        return path.read_text() if path.is_file() else ""


class Booted:
    """One VM for a phase: start, wait for the serial shell, stop and clean up."""

    def __init__(self, ctx, **options):
        self.ctx = ctx
        self.options = options
        self.work = Path(tempfile.mkdtemp(prefix=f"bsm-boot-{ctx.phase}-", dir=ctx.base))
        self.vm = None

    def __enter__(self):
        options = dict(self.options)
        if self.ctx.staged and options.pop("staged", True):
            options["stage"] = self.ctx.stage
        else:
            options.pop("staged", None)
            if self.ctx.staged:
                # The shipped unit files, with the staged tree on the 9p share for the test to copy.
                options["inject"] = self.ctx.stage
        self.vm = S.BootVm(self.work, iso=self.ctx.iso, record=self.work / "record", **options)
        self.load_start = os.getloadavg()
        self.vm.start()
        self.ctx.log(f"      boot: work {self.work}, host load {self.load_start[0]:.0f}")
        if not self.vm.start_shell(SHELL_TIMEOUT):
            raise Blocked("no serial shell within %d s" % SHELL_TIMEOUT)
        return self.vm

    def __exit__(self, *_):
        vm = self.vm
        folder = self.ctx.phase_dir()
        try:
            (folder / "serial.log").write_bytes(vm.log)
            (folder / "host-load.txt").write_text(f"at start {self.load_start}\nat end {os.getloadavg()}\n")
        except Exception:  # noqa: BLE001
            pass
        vm.stop()
        timeline = self.work / "record" / "timeline.json"
        if timeline.is_file():
            shutil.copy(timeline, folder / "timeline.json")
        video = self.work / "record" / "flow.mp4"
        if video.is_file() and video.stat().st_size < 25_000_000:
            shutil.copy(video, folder / "flow.mp4")
        if not self.ctx.keep_work:
            shutil.rmtree(self.work, ignore_errors=True)


# ---- helpers ---------------------------------------------------------------------


def script(vm, text, timeout=120):
    """Run a shell script in the guest (written through the serial shell) and return its output."""
    vm.run("cat > /tmp/bsm-boot-test.sh <<'BSMBOOTEOS'\n" + text + "\nBSMBOOTEOS", 30)
    return vm.run("sh /tmp/bsm-boot-test.sh 2>&1", timeout)


def kv(output):
    """key=value lines of a script's output."""
    found = {}
    for line in output.splitlines():
        if "=" in line and not line.startswith(" "):
            key, _, value = line.partition("=")
            found[key.strip()] = value.strip()
    return found


def save_frame(path, rgb):
    Image.fromarray(rgb).save(path)


def wait_greeter(vm, timeout, heads=(0,)):
    """Wait until every head in `heads` shows content after having shown the splash; returns host seconds or None."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        logs = vm.frame_logs()
        ok = True
        for head in heads:
            frames = logs[head].frames
            if not frames or frames[-1].scene != "content" or A.interlude(frames) is None:
                ok = False
        if ok:
            return time.monotonic() - vm.t0
        time.sleep(1.5)
    return None


def wait_content(vm, head, timeout):
    """Wait until the newest frame of `head` is content (any scene before)."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frames = vm.frame_logs()[head].frames
        if frames and frames[-1].scene == "content":
            return time.monotonic() - vm.t0
        time.sleep(1.5)
    return None


def contact_sheet(keyframes, path, title):
    """A grid of the key frames (one per scene change) with time and scene on each tile."""
    if not keyframes:
        return
    tiles = []
    for frame, rgb in keyframes:
        image = Image.fromarray(rgb).convert("RGB").resize((320, 200))
        draw = ImageDraw.Draw(image)
        draw.rectangle((0, 0, 319, 14), fill=(0, 0, 0))
        draw.text((3, 1), f"{frame.t:7.2f}s {frame.scene}", fill=(255, 255, 0))
        tiles.append(image)
    columns = 4
    rows = (len(tiles) + columns - 1) // columns
    sheet = Image.new("RGB", (columns * 324 + 4, rows * 204 + 22), (40, 40, 40))
    ImageDraw.Draw(sheet).text((6, 4), title, fill=(255, 255, 255))
    for index, tile in enumerate(tiles):
        sheet.paste(tile, (4 + (index % columns) * 324, 20 + (index // columns) * 204))
    sheet.save(path)


def handover_keyframes(log, inter):
    """Key frames from the one before the last splash frame to the first content frame after it."""
    if inter is None:
        return log.keyframes[-12:]
    chosen = [(f, rgb) for f, rgb in log.keyframes if inter["splash_last"] - 120 <= f.t <= inter["content_first"] + 0.5]
    return chosen[:16]


def frame_csv(frames, path):
    with path.open("w") as handle:
        handle.write("t,width,height,kind,mean,scene\n")
        for f in frames:
            handle.write(f"{f.t:.3f},{f.size[0]},{f.size[1]},{f.kind},{f.mean:.2f},{f.scene}\n")


# ---- phase: handover ---------------------------------------------------------------


def collect_boot_facts(ctx, vm):
    """Guest-side evidence of the boot: unit timestamps, critical chain, journal excerpts."""
    ctx.dump("loadavg.txt", vm.run("cat /proc/loadavg /proc/uptime", 15))
    units = "plymouth-start plymouth-quit plymouth-quit-wait bsm-seatd bsm-helper.socket bsm getty@tty1 greetd systemd-user-sessions systemd-logind bingux-os-firstboot graphical.target multi-user.target"
    out = vm.run(f"for u in {units}; do echo \"$u $(systemctl show -p ActiveEnterTimestampMonotonic --value $u) $(systemctl is-active $u)\"; done", 60)
    ctx.dump("unit-times.txt", out)
    ctx.dump("critical-chain-bsm.txt", vm.run("systemd-analyze critical-chain bsm.service 2>&1 | head -40", 60))
    ctx.dump("critical-chain-graphical.txt", vm.run("systemd-analyze critical-chain graphical.target 2>&1 | head -40", 60))
    ctx.dump("blame.txt", vm.run("systemd-analyze blame 2>&1 | head -25", 60))
    ctx.dump("analyze.txt", vm.run("systemd-analyze 2>&1 | head -4", 30))
    journal = vm.run(
        "journalctl -b -o short-monotonic --no-pager -u bsm -u bsm-seatd -u bsm-helper -u bsm-fallback -u plymouth-start -u plymouth-quit "
        "-u plymouth-quit-wait -u plymouth-read-write -u getty@tty1 -u greetd -u bsm-boot-inject | cut -c1-260",
        120,
    )
    ctx.dump("journal-units.txt", journal)
    ctx.dump("journal-bsm.txt", vm.run("journalctl -b -o short-monotonic --no-pager _COMM=bsm _COMM=bsm-plymouth-handover | grep -v -E 'EGL|egl|input device|New device|Removed device|Supported' | cut -c1-240", 120))
    ctx.dump("ordering-cycles.txt", vm.run("journalctl -b --no-pager | grep -i -E 'ordering cycle|dependency cycle|deleted to break' | head -20; echo end", 60))
    return parse_unit_times(out), A.journal_rows(journal)


def parse_unit_times(text):
    times = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[1].isdigit():
            times[parts[0]] = (int(parts[1]) / 1e6, parts[2])
    return times


def phase_handover(ctx):
    with Booted(ctx) as vm:
        seen = wait_greeter(vm, GREETER_TIMEOUT, heads=(0, 1))
        ctx.check("the greeter is on both displays after the splash", seen is not None, "" if seen is None else f"first seen by {seen:.1f} s")
        time.sleep(8)
        offset, uncertainty = vm.mono_offset()
        ctx.number("guest clock to host clock (s), uncertainty", f"{offset:.3f} +- {uncertainty:.3f}")
        trace_text = vm.run("cat /run/bsm-boot-trace.log", 120)
        ctx.dump("tracer.log", trace_text)
        events = A.parse_trace(trace_text)
        unit_times, journal = collect_boot_facts(ctx, vm)
        cmdline = vm.run("cat /proc/cmdline", 10)
        ctx.check("the kernel command line has bingux.bsm", "bingux.bsm" in cmdline)
        analyse_handover(ctx, vm, events, offset, unit_times, journal)
        ctx.check("no ordering cycle in the boot journal", "ordering cycle" not in ctx.read_dump("ordering-cycles.txt").lower() and "dependency cycle" not in ctx.read_dump("ordering-cycles.txt").lower())


def analyse_handover(ctx, vm, events, offset, unit_times, journal):
    folder = ctx.phase_dir()
    summaries = {}
    for head, log in vm.frame_logs().items():
        frames = log.frames
        frame_csv(frames, folder / f"frames-head{head}.csv")
        runs = A.scene_runs(frames)
        (folder / f"scenes-head{head}.txt").write_text("\n".join(f"{n:12} {a:9.3f} .. {b:9.3f} frames={c}" for n, a, b, c in runs) + "\n")
        inter = A.interlude(frames)
        summaries[head] = inter
        contact_sheet(handover_keyframes(log, inter), folder / f"handover-head{head}.png", f"head {head}: scene changes around the handover")
        ctx.number(f"head {head}: VNC reconnects", log.reconnects)
        if inter is None:
            ctx.check(f"head {head}: the recording saw the splash and then the greeter", False, "no splash frame followed by a content frame")
            continue
        ctx.number(f"head {head}: last splash frame (s)", round(inter["splash_last"], 2))
        ctx.number(f"head {head}: first greeter frame (s)", round(inter["content_first"], 2))
        ctx.number(f"head {head}: frames between splash and greeter", len(inter["entries"]))
        ctx.number(f"head {head}: seconds between the first non-splash frame and the greeter", round(inter["seconds"], 2))
        ctx.number(f"head {head}: black, placeholder or size-change frames in between", inter["bad_frames"])
        ctx.number(f"head {head}: seconds those lasted (lower bound)", round(inter["bad_seconds"], 2))
        ctx.number(f"head {head}: seconds by scene in between", {k: round(v, 2) for k, v in inter["by_scene"].items()})
        ctx.number(f"head {head}: longest gap between VNC updates around the handover (s)", round(A.frame_gaps(frames, inter["splash_last"] - 2, inter["content_first"] + 2), 2))
        ctx.check(
            f"head {head}: no black, placeholder or size-change frame between the splash and the greeter",
            inter["bad_frames"] == 0,
            f"{inter['bad_frames']} frames, {inter['bad_seconds']:.1f} s",
            handover=True,
        )
        ctx.check(f"head {head}: the greeter replaces the splash with nothing else in between", len(inter["entries"]) == 0, f"{len(inter['entries'])} frames of {sorted(set(e[1] for e in inter['entries']))}", handover=True)
    summary = A.summarise_trace(events, "1")
    ctx.dump("trace-summary.json", json.dumps(summary, indent=2, default=str))
    to_host = lambda t: None if t is None else t + offset  # noqa: E731
    lost, gone, first = summary["splash_master_lost"], summary["splash_gone"], summary["bsm_first_flip"]
    ctx.number("DRM master chain (guest s)", [(round(t, 2), h) for t, h in summary["masters"]])
    for key in ("splash_master_lost", "splash_gone", "bsm_first_flip"):
        ctx.number(f"tracer: {key} (host s)", None if summary[key] is None else round(summary[key] + offset, 2))
    ctx.number("tracer: next master after Plymouth", summary["next_master"])
    ctx.number("tracer: CRTC disabled after the splash", [(round(t + offset, 2), c) for t, c in summary["crtc_disabled"]])
    ctx.number("tracer: planes switched off after the splash", [(round(t + offset, 2), p) for t, p in summary["scanout_off"]])
    ctx.number("tracer: planes owned by fbcon after the splash", [(round(t + offset, 2), p) for t, p in summary["fbcon_planes"]])
    ctx.number("tracer: VT mode changes (guest s)", [(round(t, 2), m) for t, m in summary["vt_modes"]])
    ctx.check("Plymouth was DRM master before bsm started (the splash owned the display)", summary["splash_start"] is not None, "no plymouthd client seen" if summary["splash_start"] is None else "")
    ctx.check("no CRTC was disabled after the splash", not summary["crtc_disabled"], f"{len(summary['crtc_disabled'])} disables", handover=True)
    ctx.check("no plane was switched off and fbcon never took the scanout after the splash", not summary["scanout_off"] and not summary["fbcon_planes"], f"{len(summary['scanout_off'])} off, {len(summary['fbcon_planes'])} fbcon", handover=True)
    ctx.check("every CRTC kept its one mode (1280x800) from the splash to the greeter", all(len(set(h)) == 1 for h in summary["modes"].values()) and bool(summary["modes"]), str(summary["modes"]), handover=True)
    ctx.check(
        "bsm's seatd held the DRM master before Plymouth exited",
        summary["next_master"] is not None and gone is not None and summary["next_master"][0] <= gone + 0.05,
        f"next master {summary['next_master']}, splash gone at {gone}",
        handover=True,
    )
    ctx.check("bsm scanned out its own framebuffer before Plymouth exited", first is not None and gone is not None and first <= gone + 0.05, f"bsm first flip {first}, splash gone {gone}", handover=True)
    ctx.check("the VT stays in seatd's process mode after Plymouth exits (VT switching keeps working)", bool(summary["vt_modes"]) and summary["vt_modes"][-1][1] == "process", str(summary["vt_modes"][-3:]), handover=True)
    head0 = summaries.get(0)
    if head0 is not None and first is not None:
        # The VNC server delivers a frame well after the flip (2.2 s on a host at load 50), so the frame
        # time is not used to order events: the tracer's flip time is.
        ctx.number("VNC lag: the greeter frame reached the recorder this long after bsm's first flip (s)", round(head0["content_first"] - to_host(first), 2))
    # systemd and the journal.
    bsm_up = unit_times.get("bsm")
    quit_up = unit_times.get("plymouth-quit")
    ctx.number("unit active times (guest s)", {k: (round(v[0], 2), v[1]) for k, v in unit_times.items()})
    ctx.check("plymouth-quit.service ran after bsm was up", bsm_up is not None and quit_up is not None and quit_up[0] >= bsm_up[0], f"bsm {bsm_up}, plymouth-quit {quit_up}", handover=True)
    scene = A.first_row(journal, "first scene")
    ctx.number("journal: first scene line", None if scene is None else scene[1][:160])
    ctx.check("the greeter, not the hold timeout, ended the hold (first scene: Presented)", scene is not None and "Presented" in scene[1], "" if scene is None else scene[1][-80:], handover=True)
    start = A.first_row(journal, "Starting Bingux Session Manager")
    started = A.first_row(journal, "Started Bingux Session Manager")
    deactivate = A.first_row(journal, "splash deactivated")
    retain = A.first_row(journal, "splash withdrawn")
    ctx.number("journal: bsm start job begins (guest s)", None if start is None else start[0])
    ctx.number("journal: splash deactivated (guest s)", None if deactivate is None else deactivate[0])
    ctx.number("journal: bsm start job ends, READY (guest s)", None if started is None else started[0])
    ctx.number("journal: splash withdrawn (guest s)", None if retain is None else retain[0])
    ctx.check(
        "the splash was deactivated before bsm started and withdrawn after READY",
        deactivate is not None and retain is not None and started is not None and deactivate[0] <= retain[0] and started[0] <= retain[0] + 0.5,
        f"deactivate {deactivate and deactivate[0]}, ready {started and started[0]}, withdrawn {retain and retain[0]}",
        handover=True,
    )
    chain = ctx.read_dump("critical-chain-bsm.txt")
    ctx.check("bsm.service does not wait for plymouth-quit.service or plymouth-quit-wait.service (critical chain)", "plymouth-quit" not in chain.split("\n", 3)[-1], "", handover=True)


# ---- phase: modeset ----------------------------------------------------------------


def phase_modeset(ctx):
    with Booted(ctx, drm_debug=True) as vm:
        seen = wait_greeter(vm, GREETER_TIMEOUT, heads=(0,))
        ctx.check("the greeter is on the display after the splash", seen is not None)
        time.sleep(8)
        offset, _ = vm.mono_offset()
        raw = vm.run(
            "dmesg --time-format=raw 2>/dev/null | grep -E 'drm_atomic_helper_commit_crtc_|drm_mode_setcrtc|DRM_IOCTL_SET_MASTER|DRM_IOCTL_DROP_MASTER|drm_open_helper|drm_file_free|drm_client_dev_restore|bsm-boot-trace' | cut -c1-220",
            240,
        )
        ctx.dump("kernel-drm.txt", raw)
        commits = vm.run("dmesg --time-format=raw 2>/dev/null | grep -c 'drm_atomic_commit\\]'", 120).strip().splitlines()[-1]
        ctx.number("atomic commits logged by the kernel in this boot", commits)
        rows = A.parse_dmesg(raw)
        shift = A.kernel_to_monotonic(rows)
        ctx.check("the tracer's kernel marks put the kernel log on the guest clock", shift is not None)
        events = A.drm_events(rows)
        splash_modeset, later = A.modesets_after_splash(events)
        ctx.number("kernel time of the splash's own last SETCRTC", splash_modeset)
        ctx.number("modesets after the splash's own (kernel time, kind, CRTC)", [(round(t, 3), k, d) for t, k, d in later])
        masters = [(round(t, 3), k, d) for t, k, d in events if k in ("set-master", "drop-master")]
        ctx.number("SET_MASTER and DROP_MASTER calls (kernel time, kind, process pid)", masters)
        opens = [(round(t, 3), k, d) for t, k, d in events if k in ("open", "close")]
        ctx.number("DRM opens and closes", opens)
        ctx.check("no CRTC was disabled, enabled or had its mode set after the splash set it", not later, f"{len(later)} modeset events", handover=True)
        plymouth_leaves = next((t for t, k, d in events if k == "drop-master" and d.startswith("plymouthd") and splash_modeset is not None and t > splash_modeset), None)
        # Plymouth's `deactivate` also drops the master; the fbdev restore that matters is the one after its last close.
        closed = next((t for t, k, d in events if k == "close" and d.startswith("plymouthd")), None)
        restores = A.fbdev_restores_after(events, closed if closed is not None else (plymouth_leaves or 0.0))
        ctx.number("fbdev restores (fbcon taking the planes) after Plymouth's last close", [round(t, 3) for t, _, _ in restores])
        ctx.check("fbcon did not take the CRTCs when Plymouth left", not restores, f"{len(restores)} fbdev restores", handover=True)
        seatd_master = next((t for t, k, d in events if k == "set-master" and d.startswith("seatd")), None)
        ctx.check("seatd took the DRM master while Plymouth had not yet closed its fd" if closed is not None else "seatd took the DRM master", seatd_master is not None and (closed is None or seatd_master <= closed), f"seatd master {seatd_master}, plymouthd closed {closed}", handover=True)
        ctx.number("host load factor: this boot logs every ioctl, so its timings are slower than handover's", True)


# ---- phase: greetd -----------------------------------------------------------------


def phase_greetd(ctx):
    with Booted(ctx, bsm_param=False) as vm:
        seen = wait_greeter(vm, GREETER_TIMEOUT, heads=(0,))
        ctx.check("greetd's greeter is on the screen after the splash", seen is not None)
        time.sleep(5)
        folder = ctx.phase_dir()
        frame = vm.frame()
        if frame is not None:
            save_frame(folder / "greetd-greeter.png", frame)
        status = script(
            vm,
            "echo cmdline=$(cat /proc/cmdline | tr ' ' '\\n' | grep -c '^bingux.bsm$')\n"
            "echo greetd=$(systemctl is-active greetd)\n"
            "echo bsm=$(systemctl is-active bsm)\n"
            "echo bsm_condition=$(systemctl show -p ConditionResult --value bsm.service)\n"
            "echo seatd=$(systemctl is-active bsm-seatd)\n"
            "echo helper=$(systemctl is-active bsm-helper.socket)\n"
            "echo quit=$(systemctl is-active plymouth-quit)\n"
            "echo quitwait=$(systemctl is-active plymouth-quit-wait)\n"
            "echo bsm_procs=$(pgrep -c -x bsm)\n"
            "echo alias=$(readlink /etc/systemd/system/display-manager.service)\n"
            "echo dropin=$(systemctl cat plymouth-quit.service | grep -c 'After=bsm.service')\n"
            "echo plymouth_pid=$(pgrep -c -x plymouthd)\n",
        )
        ctx.dump("status.txt", status)
        facts = kv(status)
        ctx.check("the kernel command line has no bingux.bsm", facts.get("cmdline") == "0")
        ctx.check("greetd is the display manager and runs", facts.get("greetd") == "active", facts.get("greetd", ""))
        ctx.check("bsm did not start (its condition is unmet)", facts.get("bsm") != "active" and facts.get("bsm_condition") == "no", f"{facts.get('bsm')}, condition {facts.get('bsm_condition')}")
        ctx.check("bsm-seatd, bsm-helper.socket and the bsm process are not running", facts.get("seatd") != "active" and facts.get("helper") != "active" and facts.get("bsm_procs") in ("0", None, ""), f"{facts.get('seatd')} {facts.get('helper')} {facts.get('bsm_procs')}")
        ctx.check("display-manager.service still points at greetd", (facts.get("alias") or "").endswith("greetd.service"), facts.get("alias", ""))
        ctx.check("plymouth-quit.service ran (the ordering drop-in did not hold it back) and plymouthd is gone", facts.get("quit") == "active" and facts.get("plymouth_pid") in ("0", ""), f"quit {facts.get('quit')}, plymouthd processes {facts.get('plymouth_pid')}")
        if ctx.staged:
            ctx.check("the ordering drop-in is installed on this boot", facts.get("dropin") not in (None, "0"), facts.get("dropin", ""))
        # What greetd's own handover looks like, for comparison with bsm's.
        offset, _ = vm.mono_offset()
        inter = A.interlude(vm.frame_logs()[0].frames)
        if inter is not None:
            ctx.number("greetd: frames between splash and greeter", len(inter["entries"]))
            ctx.number("greetd: seconds between the first non-splash frame and the greeter", round(inter["seconds"], 2))
            ctx.number("greetd: black, placeholder or size-change frames in between", inter["bad_frames"])
            ctx.number("greetd: seconds by scene in between", {k: round(v, 2) for k, v in inter["by_scene"].items()})
        contact_sheet(handover_keyframes(vm.frame_logs()[0], inter), folder / "handover-head0.png", "greetd boot, head 0: scene changes around the handover")
        frame_csv(vm.frame_logs()[0].frames, folder / "frames-head0.csv")
        ctx.dump("tracer.log", vm.run("cat /run/bsm-boot-trace.log", 120))
        collect_boot_facts(ctx, vm)


# ---- phase: dm ---------------------------------------------------------------------

DM_SCRIPT = r"""
set -u
A=/etc/systemd/system/display-manager.service
TOOL=bsm-set-display-manager
snap() {
    echo "$1.alias=$(readlink $A 2>/dev/null || echo none)"
    echo "$1.bsm=$(systemctl is-enabled bsm.service 2>/dev/null)"
    echo "$1.socket=$(systemctl is-enabled bsm-helper.socket 2>/dev/null)"
    echo "$1.greetd=$(systemctl is-enabled greetd.service 2>/dev/null)"
    echo "$1.state=$(cat /var/lib/bsm/previous-display-manager 2>/dev/null || echo none)"
}
echo "tool=$(command -v $TOOL)"
snap s0
echo "--- package files: nothing enabled, no alias"
echo "p.alias_in_unit=$(grep -c '^Alias=' /usr/lib/systemd/system/bsm.service)"
R=/tmp/pkgroot; rm -rf $R; mkdir -p $R/usr/lib/systemd/system
cp /usr/lib/systemd/system/bsm.service /usr/lib/systemd/system/bsm-seatd.service /usr/lib/systemd/system/bsm-helper.service /usr/lib/systemd/system/bsm-helper.socket /usr/lib/systemd/system/bsm-fallback.service $R/usr/lib/systemd/system/
echo "p.root_bsm=$(systemctl --root=$R is-enabled bsm.service 2>&1 | tail -1)"
echo "p.root_alias=$(ls $R/etc/systemd/system/display-manager.service 2>&1 | grep -c 'No such')"
echo "p.pacman_alias=$(pacman -Ql bsm 2>/dev/null | grep -c 'display-manager.service')"
echo "--- 1 enable (greetd -> bsm)"
$TOOL enable; echo "rc1=$?"
snap s1
echo "--- 2 enable again"
$TOOL enable; echo "rc2=$?"
snap s2
echo "--- 3 disable (bsm -> greetd)"
$TOOL disable; echo "rc3=$?"
snap s3
echo "--- 4 a foreign display manager is in place"
cat > /etc/systemd/system/fakedm.service <<EOF
[Unit]
Description=fake display manager
[Service]
ExecStart=/bin/true
[Install]
Alias=display-manager.service
EOF
systemctl daemon-reload
systemctl disable greetd.service >/dev/null 2>&1
systemctl enable fakedm.service >/dev/null 2>&1
snap f0
$TOOL enable; echo "rc4=$?"
snap f1
$TOOL disable; echo "rc5=$?"
snap f2
$TOOL enable --force; echo "rc6=$?"
snap f3
$TOOL disable; echo "rc7=$?"
snap f4
echo "--- 5 restore"
systemctl disable fakedm.service >/dev/null 2>&1; rm -f /etc/systemd/system/fakedm.service; systemctl daemon-reload
systemctl enable greetd.service >/dev/null 2>&1
snap s9
"""


def phase_dm(ctx):
    with Booted(ctx, staged=False) as vm:
        if ctx.staged:
            # The unit file must be the image's real file for `systemctl enable`, so this phase boots the
            # shipped image and takes the staged tool and units over the live root afterwards.
            vm.run("mkdir -p /mnt/bsm-inject; modprobe 9pnet_virtio; modprobe 9p; mountpoint -q /mnt/bsm-inject || mount -t 9p -o trans=virtio,version=9p2000.L,ro bsminject /mnt/bsm-inject", 60)
            vm.run("cd /mnt/bsm-inject/root && cp usr/bin/bsm-set-display-manager /usr/bin/ && cp usr/libexec/bsm/fallback usr/libexec/bsm/bsm-plymouth-handover /usr/libexec/bsm/ && cp usr/lib/systemd/system/bsm-fallback.service /usr/lib/systemd/system/ && systemctl daemon-reload", 60)
        out = script(vm, DM_SCRIPT, 240)
        ctx.dump("dm-tool.txt", out)
        f = kv(out)
        ends = lambda key, suffix: (f.get(key) or "").endswith(suffix)  # noqa: E731
        ctx.check("the image starts with greetd as the display-manager alias", ends("s0.alias", "greetd.service"), f.get("s0.alias", ""))
        ctx.check("the package's unit has no Alias=display-manager.service", f.get("p.alias_in_unit") == "0", f.get("p.alias_in_unit", ""))
        ctx.check("package files enable nothing: bsm.service is disabled in a scratch root", f.get("p.root_bsm") in ("disabled", "static"), f.get("p.root_bsm", ""))
        ctx.check("package files create no display-manager alias", f.get("p.root_alias") == "1" and f.get("p.pacman_alias") in ("0", ""), f"scratch root alias absent: {f.get('p.root_alias')}, pacman file list matches: {f.get('p.pacman_alias')}")
        ctx.check("enable replaces greetd: alias -> bsm.service, bsm and its socket enabled, greetd disabled, previous recorded", f.get("rc1") == "0" and ends("s1.alias", "bsm.service") and f.get("s1.bsm") == "enabled" and f.get("s1.socket") == "enabled" and f.get("s1.greetd") != "enabled" and f.get("s1.state") == "greetd.service", f"rc {f.get('rc1')} {f.get('s1.alias')} bsm {f.get('s1.bsm')} socket {f.get('s1.socket')} greetd {f.get('s1.greetd')} state {f.get('s1.state')}")
        ctx.check("enable twice changes nothing", f.get("rc2") == "0" and f.get("s2.alias") == f.get("s1.alias") and f.get("s2.state") == "greetd.service", f"rc {f.get('rc2')} state {f.get('s2.state')}")
        ctx.check("disable restores greetd: alias -> greetd.service, bsm and its socket disabled, record removed", f.get("rc3") == "0" and ends("s3.alias", "greetd.service") and f.get("s3.bsm") != "enabled" and f.get("s3.socket") != "enabled" and f.get("s3.greetd") == "enabled" and f.get("s3.state") == "none", f"rc {f.get('rc3')} {f.get('s3.alias')} bsm {f.get('s3.bsm')} greetd {f.get('s3.greetd')} state {f.get('s3.state')}")
        ctx.check("a foreign display-manager alias is not clobbered by enable (refused, nothing changed)", ends("f0.alias", "fakedm.service") and f.get("rc4") != "0" and f.get("f1.alias") == f.get("f0.alias") and f.get("f1.bsm") != "enabled", f"rc {f.get('rc4')} alias {f.get('f0.alias')} -> {f.get('f1.alias')} bsm {f.get('f1.bsm')}")
        ctx.check("disable when bsm never replaced anything leaves the foreign alias alone", f.get("rc5") == "0" and f.get("f2.alias") == f.get("f0.alias"), f"rc {f.get('rc5')} alias {f.get('f2.alias')}")
        ctx.check("enable --force replaces the foreign manager and records it", f.get("rc6") == "0" and ends("f3.alias", "bsm.service") and f.get("f3.state") == "fakedm.service", f"rc {f.get('rc6')} alias {f.get('f3.alias')} state {f.get('f3.state')}")
        ctx.check("disable brings the foreign manager back", f.get("rc7") == "0" and ends("f4.alias", "fakedm.service") and f.get("f4.state") == "none", f"rc {f.get('rc7')} alias {f.get('f4.alias')} state {f.get('f4.state')}")
        ctx.check("the image's own greetd alias is restored at the end", ends("s9.alias", "greetd.service"), f.get("s9.alias", ""))


# ---- phase: fallback ---------------------------------------------------------------


def phase_fallback(ctx):
    with Booted(ctx, staged=False) as vm:
        if ctx.staged:
            vm.run("mkdir -p /mnt/bsm-inject; modprobe 9pnet_virtio; modprobe 9p; mountpoint -q /mnt/bsm-inject || mount -t 9p -o trans=virtio,version=9p2000.L,ro bsminject /mnt/bsm-inject", 60)
            vm.run("cd /mnt/bsm-inject/root && cp usr/libexec/bsm/fallback usr/libexec/bsm/bsm-plymouth-handover /usr/libexec/bsm/ && cp usr/lib/systemd/system/bsm-fallback.service /usr/lib/systemd/system/ && systemctl daemon-reload", 60)
        for _ in range(60):
            if vm.run("systemctl is-active bsm", 20).split()[-1] == "active":
                break
            time.sleep(3)
        ctx.check("bsm runs before the test", vm.run("systemctl is-active bsm", 20).split()[-1] == "active")
        seen = wait_content(vm, 0, GREETER_TIMEOUT)
        ctx.check("bsm's greeter is on the screen before the test", seen is not None)
        folder = ctx.phase_dir()
        stamp = vm.run("date '+%Y-%m-%d %H:%M:%S'", 10).strip().splitlines()[-1]
        before = vm.run("pgrep -x bsm; systemctl show -p NRestarts --value bsm", 15)
        vm.run("mkdir -p /run/systemd/system/bsm.service.d && printf '[Service]\\nExecStart=\\nExecStart=/bin/false\\n' > /run/systemd/system/bsm.service.d/90-break.conf && systemctl daemon-reload", 30)
        mark = time.monotonic() - vm.t0
        vm.run("systemctl restart --no-block bsm", 30)
        came = None
        trail = []
        for _ in range(100):
            time.sleep(3)
            states = vm.run("echo bsm=$(systemctl is-active bsm) greetd=$(systemctl is-active greetd) fallback=$(systemctl is-active bsm-fallback)", 20).strip().splitlines()[-1]
            trail.append(f"{time.monotonic() - vm.t0 - mark:6.1f} s  {states}")
            if "greetd=active" in states:
                came = time.monotonic() - vm.t0 - mark
                break
        ctx.dump("fallback-states.txt", "\n".join(trail) + "\n")
        ctx.check("greetd came up after bsm failed", came is not None, "" if came is None else f"{came:.0f} s after the restart")
        time.sleep(4)
        journal = vm.run(f"journalctl -b -o short-monotonic --no-pager -u bsm -u bsm-fallback -u greetd --since '{stamp}' | grep -v 'greetd\\[' | cut -c1-230", 60)
        ctx.dump("fallback-journal.txt", journal)
        rows = A.journal_rows(journal)
        failures = [t for t, line in rows if "bsm.service: Failed with result" in line]
        limit = A.first_row(rows, "start-limit-hit")
        greetd_up = A.first_row(rows, "Started Greeter daemon")
        said = A.first_row(rows, "bsm-fallback: bsm failed; starting greetd")
        ctx.number("bsm failed starts (journal)", len(failures))
        ctx.check("bsm failed for good: the start limit was hit", limit is not None, "" if limit is None else limit[1][-90:])
        ctx.check("bsm-fallback started greetd and said so in the journal", said is not None)
        ctx.check(
            "greetd was started after bsm gave up, not at the first failed attempt (no second display manager next to a bsm that restarts)",
            greetd_up is not None and limit is not None and greetd_up[0] >= limit[0] and len(failures) >= 2,
            f"failures at {[round(t, 1) for t in failures]}, limit hit {limit and limit[0]}, greetd {greetd_up and greetd_up[0]}",
        )
        ctx.check("bsm.service is failed (not restarting for ever) and greetd runs", vm.run("echo $(systemctl is-active bsm) $(systemctl is-active greetd)", 20).split()[-2:] == ["failed", "active"])
        shown = wait_content(vm, 0, 420)
        ctx.check("greetd's greeter is on the screen after the fallback", shown is not None)
        time.sleep(3)
        frame = vm.frame()
        if frame is not None:
            save_frame(folder / "fallback-greetd.png", frame)
        # Repair: the machine goes back to bsm without a reboot.
        vm.run("rm -f /run/systemd/system/bsm.service.d/90-break.conf /run/systemd/system/greetd.service.d/90-bsm-fallback.conf; systemctl daemon-reload; systemctl stop greetd; systemctl reset-failed bsm bsm-fallback; systemctl start bsm", 200)
        back = vm.run("echo $(systemctl is-active bsm) $(systemctl is-active greetd)", 20).split()[-2:]
        ctx.check("after removing the fault and restarting bsm, bsm runs and greetd does not", back == ["active", "inactive"], str(back))
        again = wait_content(vm, 0, 420)
        time.sleep(4)
        frame = vm.frame()
        if frame is not None:
            save_frame(folder / "after-repair-bsm.png", frame)
        ctx.check("bsm's greeter is back on the screen", again is not None)
        ctx.dump("tracer.log", vm.run("tail -n 400 /run/bsm-boot-trace.log", 120))


# ---- phase: session ----------------------------------------------------------------


def load_walkthrough():
    spec = importlib.util.spec_from_file_location("bsm_qemu", Path(__file__).with_name("bsm-qemu.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def region_change(vm, head, before, threshold=40):
    """Number of decimated pixels that differ from `before` in the newest frame of `head`."""
    plane = vm.sampler.latest_plane(head)
    if plane is None or before is None:
        return None
    return int((np.abs(plane - before) > threshold).sum())


def phase_session(ctx):
    bq = load_walkthrough()
    with Booted(ctx) as vm:
        folder = ctx.phase_dir()
        wait_content(vm, 0, GREETER_TIMEOUT)
        run = bq.Run(vm, folder / "walkthrough")
        (folder / "walkthrough").mkdir(exist_ok=True)
        vm.run(f"id {USER_B[0]} >/dev/null 2>&1 || (useradd -m -G video,audio -s /bin/bash {USER_B[0]} && echo {USER_B[0]}:{USER_B[1]} | chpasswd)", 30)
        # bsm lists accounts at start: restart once so the second account is on the greeter.
        vm.run("systemctl restart bsm", 240)
        time.sleep(5)
        wait_content(vm, 0, GREETER_TIMEOUT)
        vt_checks(ctx, vm, bq, run)
        restart_checks(ctx, vm, bq, run)


def vt_checks(ctx, vm, bq, run):
    folder = ctx.phase_dir()
    time.sleep(6)
    pids = vm.pids("bsm")
    ctx.check("one bsm process before the VT switch", len(pids) == 1, str(pids))
    reference = vm.sampler.latest_plane(0)
    vt_before = vm.run("fgconsole", 10).strip().splitlines()[-1]
    ctx.check("bsm's seat is on VT 1", vt_before == "1", vt_before)
    since = vm.run("date '+%Y-%m-%d %H:%M:%S'", 10).strip().splitlines()[-1]
    scene_marks = {}
    for label, switch, back in (("chvt", lambda: vm.run("chvt 3", 20), lambda: vm.run("chvt 1", 20)), ("Ctrl+Alt+F3", lambda: vm.key("ctrl-alt-f3"), lambda: vm.key("ctrl-alt-f1"))):
        log = vm.frame_logs()[0]
        first = len(log.frames)
        t_switch = time.monotonic() - vm.t0
        switch()
        time.sleep(5)
        on_vt = vm.run("fgconsole", 10).strip().splitlines()[-1]
        frame = vm.frame()
        if frame is not None:
            save_frame(folder / f"vt3-{label.replace('+', '')}.png", frame)
        shell_ok = vm.run("echo alive-on-$(fgconsole)", 20).strip().splitlines()[-1]
        t_back = time.monotonic() - vm.t0
        back()
        time.sleep(6)
        after_vt = vm.run("fgconsole", 10).strip().splitlines()[-1]
        scenes = [(round(f.t - t_back, 2), f.scene) for f in log.frames[first:] if f.t >= t_back - 0.2]
        scene_marks[label] = scenes
        ctx.check(f"{label}: the console VT 3 became active and a root shell works there", on_vt == "3" and shell_ok.endswith("3"), f"{on_vt}, {shell_ok}")
        ctx.check(f"{label}: back on VT 1", after_vt == "1", after_vt)
        returned = [s for t, s in scenes if t >= -0.2]
        ctx.number(f"{label}: scenes of head 0 from the switch back (s relative, scene)", scenes[:10])
        ctx.check(f"{label}: no black frame after returning to bsm's VT (only the greeter's scene)", returned and "black" not in returned and "placeholder" not in returned and "size-change" not in returned, str(returned[:6]))
        plane = vm.sampler.latest_plane(0)
        similar = None if plane is None or reference is None else float(1.0 - np.abs(plane - reference).mean() / 255.0)
        ctx.check(f"{label}: the greeter is intact after returning", similar is not None and similar > 0.9, f"similarity to the picture before: {similar}")
    # Input is restored: keys typed into the greeter reach its password field.
    before = vm.sampler.latest_plane(0)
    vm.type_text("abc")
    time.sleep(2)
    changed = region_change(vm, 0, before, 30)
    ctx.check("keyboard input reaches the greeter after the VT round trips", changed is not None and changed > 8, f"{changed} pixels changed")
    ctx.dump("vt-journal.txt", vm.run(f"journalctl -b -o short-monotonic --no-pager -u bsm --since '{since}' | grep -i -E 'paused|activated|VT|input device|resume' | cut -c1-200", 60))
    ctx.check("bsm kept its pid across the VT switches", vm.pids("bsm") == pids, f"{pids} -> {vm.pids('bsm')}")
    ctx.check("bsm logged the pause and the resume of its seat", "paused" in ctx.read_dump("vt-journal.txt") and "activated" in ctx.read_dump("vt-journal.txt"))


def restart_checks(ctx, vm, bq, run):
    folder = ctx.phase_dir()
    try:
        run.step("boot", bq.step_boot)
        run.step("greeter", bq.step_greeter)
        run.step("login-a", bq.step_login_a)
        run.step("switch-b", bq.step_switch_b)
    finally:
        ctx.dump("walkthrough-prerequisites.json", json.dumps(run.steps, indent=2, default=str))
    prerequisites = {name: info.get("status") for name, info in run.steps.items()}
    ctx.number("walkthrough steps used as prerequisites", prerequisites)
    if prerequisites.get("login-a") != "passed" or prerequisites.get("switch-b") != "passed":
        raise Blocked(f"two logins could not be established ({prerequisites}); see walkthrough-prerequisites.json")
    uid_a, uid_b = run.state["uid_a"], run.state["uid_b"]
    before = {
        "bsm": vm.pids("bsm"),
        "seatd": vm.pids("seatd"),
        "helper": vm.pids("bsm-helper", "root"),
        "gnoblin_a": run.gnoblin_pid(USER_A[0]),
        "gnoblin_b": run.gnoblin_pid(USER_B[0]),
        "sessions": [line for line in vm.run("loginctl list-sessions --no-legend", 20).splitlines() if USER_A[0] in line or USER_B[0] in line],
    }
    ctx.number("before the restart", before)
    status_before, text = vm.bsmctl_json("status")
    ctx.dump("bsmctl-status-before-restart.txt", text)
    sessions_before = run.sessions()
    ctx.dump("bsmctl-sessions-before-restart.json", json.dumps(sessions_before, indent=2))
    desktop_b = {h: vm.sampler.latest_plane(h) for h in range(2)}
    start_events = vm.event_count()
    t_restart = time.monotonic() - vm.t0
    out = vm.run("date +%s.%N; systemctl restart bsm; echo restart-rc=$?; date +%s.%N", 300)
    ctx.dump("restart-output.txt", out)
    t_done = time.monotonic() - vm.t0
    ctx.number("systemctl restart bsm took (s)", round(t_done - t_restart, 1))
    # Everything sampled from the restart on, on both displays.
    worst = {}
    for head in range(2):
        window = vm.sampler.window(head, t_restart + vm.t0 - 1.0, t_done + vm.t0 + 8.0)
        sims = [float(1.0 - np.abs(s.plane - desktop_b[head]).mean() / 255.0) for s in window] if desktop_b[head] is not None else []
        worst[head] = max(sims) if sims else None
    ctx.number("highest similarity of any frame since the restart to B's desktop (per head)", worst)
    ctx.check("no frame since the restart shows the foreground desktop of B", all(v is not None and v < 0.93 for v in worst.values()), str(worst))
    time.sleep(10)
    after = {
        "bsm": vm.pids("bsm"),
        "seatd": vm.pids("seatd"),
        "helper": vm.pids("bsm-helper", "root"),
        "gnoblin_a": run.gnoblin_pid(USER_A[0]),
        "gnoblin_b": run.gnoblin_pid(USER_B[0]),
        "sessions": [line for line in vm.run("loginctl list-sessions --no-legend", 20).splitlines() if USER_A[0] in line or USER_B[0] in line],
    }
    ctx.number("after the restart", after)
    status_after, text = vm.bsmctl_json("status")
    ctx.dump("bsmctl-status-after-restart.txt", text)
    sessions_after = vm.bsmctl_json("sessions")[0]
    ctx.dump("bsmctl-sessions-after-restart.json", json.dumps(sessions_after, indent=2))
    ctx.check("bsm restarted under a new pid", after["bsm"] and after["bsm"] != before["bsm"], f"{before['bsm']} -> {after['bsm']}")
    ctx.check("both users' Gnoblin processes survived the restart (same pids)", after["gnoblin_a"] == before["gnoblin_a"] and after["gnoblin_b"] == before["gnoblin_b"], f"A {before['gnoblin_a']} -> {after['gnoblin_a']}, B {before['gnoblin_b']} -> {after['gnoblin_b']}")
    ctx.check("both logind sessions survived", len(after["sessions"]) >= len(before["sessions"]) >= 2, f"{len(before['sessions'])} -> {len(after['sessions'])}")
    shown = {h: vm.frame_logs()[h].frames[-1].scene if vm.frame_logs()[h].frames else None for h in range(2)}
    ctx.number("scene on each head after the restart", shown)
    ctx.check("the greeter is on both displays after the restart (not a desktop, not black)", all(v == "content" for v in shown.values()), str(shown))
    foreground = []

    def walk(node):
        if isinstance(node, dict):
            if node.get("foreground") is True:
                foreground.append(node.get("id"))
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(sessions_after)
    ctx.check("bsm reports no session as foreground after the restart (recovered sessions are protected)", isinstance(status_after, dict) and not status_after.get("foreground") and not foreground, f"status foreground {status_after.get('foreground') if isinstance(status_after, dict) else None}, sessions {foreground}")
    ctx.dump("restart-journal.txt", vm.run("journalctl -b -o short-monotonic --no-pager -u bsm -u bsm-helper --since '-4min' | grep -v -E 'EGL|egl|input device|New device|Removed device|Supported|MESA|radv|zink' | cut -c1-240 | tail -80", 90))
    # Resuming needs the account's password: the greeter takes it and what happens is recorded as measured.
    resume = {}
    for user, uid_key in ((USER_A, "uid_a"), (USER_B, "uid_b")):
        mark = vm.event_count()
        vm.bsmctl_json(f"select-user {run.state[uid_key]}")
        time.sleep(3)
        run.type_password(user[1])
        done = vm.wait_monitor(lambda e: e.get("event") == "transition" and e.get("phase") in ("completed", "committed"), 90, since=mark)
        time.sleep(3)
        kinds = [(e.get("kind"), e.get("phase")) for _, e in vm.monitor_events()[mark:] if e.get("event") == "transition"]
        gn = run.gnoblin_pid(user[0])
        resume[user[0]] = {"transition_done": done is not None, "transitions": kinds[:6], "gnoblin": gn}
        run.shot(f"resumed-{user[0]}")
        break  # one account is enough to learn whether resuming works; the second would only repeat it
    ctx.number("authenticating after the restart (first account)", resume)
    first_user = USER_A[0]
    info = resume[first_user]
    ctx.check("authenticating the first account after the restart resumed its session with the same Gnoblin", info["transition_done"] and info["gnoblin"] == before["gnoblin_a"], str(info))
    ctx.dump("resume-journal.txt", vm.run("journalctl -b -o short-monotonic --no-pager -u bsm -u bsm-helper --since '-3min' | grep -v -E 'EGL|egl|input device|New device|Removed device|Supported|MESA|radv|zink' | cut -c1-240 | tail -60", 90))


# ---- main --------------------------------------------------------------------------


RUNNERS = {
    "handover": phase_handover,
    "modeset": phase_modeset,
    "greetd": phase_greetd,
    "dm": phase_dm,
    "fallback": phase_fallback,
    "session": phase_session,
}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--iso")
    parser.add_argument("--stage", help="directory staged by tests/bsm-vm-inject.py (package units, scripts and binaries)")
    parser.add_argument("--out")
    parser.add_argument("--only", help="comma separated phases: " + ",".join(PHASES))
    parser.add_argument("--keep-work", action="store_true")
    args = parser.parse_args()
    iso = find_iso(args.iso)
    if iso is None or not Path(iso).is_file():
        print("no live image; run distro/build.sh first")
        return 1
    missing = missing_tools("qemu-system-x86_64", "ffmpeg")
    if missing:
        print("missing tool: " + ", ".join(missing))
        return 1
    default_out = ROOT / "distro/out/bsm-test/boot"
    out = Path(args.out) if args.out else (default_out if args.stage else default_out / "baseline")
    out.mkdir(parents=True, exist_ok=True)
    wanted = [p.strip() for p in args.only.split(",")] if args.only else PHASES
    unknown = [p for p in wanted if p not in RUNNERS]
    if unknown:
        print("unknown phase: " + ", ".join(unknown))
        return 1
    ctx = Context(iso, args.stage, out, args.keep_work)
    ctx.log(f"image {iso}; {'staged from ' + str(ctx.stage) if ctx.staged else 'image as shipped'}; out {out}")
    for name in wanted:
        ctx.phase = name
        ctx.log(f"\n== {name}   (host load {os.getloadavg()[0]:.0f})")
        started = time.time()
        try:
            RUNNERS[name](ctx)
            failed = [k for k, v in ctx.checks.items() if k.startswith(name + ":") and not v]
            ctx.phases[name] = {"status": "failed" if failed else "passed", "failed": failed}
        except Blocked as blocked:
            ctx.phases[name] = {"status": "blocked", "reason": str(blocked)}
            ctx.log(f"BLOCKED {name}: {blocked}")
        except Exception as error:  # noqa: BLE001
            import traceback

            ctx.phases[name] = {"status": "failed", "reason": f"{type(error).__name__}: {error}"}
            ctx.checks[f"{name}: (crashed) {error}"] = False
            ctx.log(f"CRASH {name}: {type(error).__name__}: {error}\n{traceback.format_exc()}")
        ctx.phases[name]["seconds"] = round(time.time() - started)
    (out / "report.txt").write_text("\n".join(ctx.report) + "\n")
    (out / "checks.json").write_text(json.dumps(ctx.checks, indent=2) + "\n")
    (out / "measured.json").write_text(json.dumps(ctx.measured, indent=2) + "\n")
    (out / "measurements.json").write_text(json.dumps(ctx.numbers, indent=2, default=str) + "\n")
    (out / "phases.json").write_text(json.dumps(ctx.phases, indent=2) + "\n")
    print()
    for name, info in ctx.phases.items():
        print(f"{info['status']:8} {name}" + (f"  ({info.get('reason')})" if info.get("reason") else "") + (f"  failed: {len(info['failed'])}" if info.get("failed") else ""))
    print("artifacts:", out)
    statuses = {info["status"] for info in ctx.phases.values()}
    if "failed" in statuses or any(not v for v in ctx.checks.values()):
        return 1
    return 2 if "blocked" in statuses else 0


if __name__ == "__main__":
    sys.exit(main())
