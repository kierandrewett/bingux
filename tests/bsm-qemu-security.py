#!/usr/bin/env python3
"""Spec section 15, the mandatory security and failure scenarios, run in the QEMU VM.

usage: tests/bsm-qemu-security.py [--iso ISO] [--inject DIR] [--out DIR] [--only 1,2,...]
                                  [--skip-bios] [--keep-work]

One Bingux OS VM (UEFI, the image's own boot loader, `bingux.bsm` on the kernel command
line, two virtual displays, two ordinary accounts `alice` and `bob`), the serial root shell
and a frame sampler per display. Every scenario asserts real state: processes, `bsmctl`,
logind, the journal, sockets and files as other accounts see them, and the frames on both
displays. Each user's desktop carries a marker (a full-screen window of one saturated
colour on every monitor), so a frame that shows the colour is a frame in which that desktop
reached the screen. Nothing mocks a privileged path: logins are typed at the greeter.

  1  isolation   an unprivileged account cannot reach the greeter, helper or session sockets,
                 cannot become a session surface, spoof a uid, act on another session over
                 D-Bus, publish for another uid or read another account's files and clipboard
  2  greeter     kill -9 the greeter while locked: no desktop on screen, an impostor with the
                 greeter's uid is refused, the greeter returns and unlock works
  3  gnoblin     kill -9 a foreground and a background-locked hosted Gnoblin: cleanup,
                 return to the greeter without a stale desktop, the other session survives
  4  auth        PAM child killed, helper restarted, account switched mid-authentication
  5  outputs     a display removed, shrunk and re-added during lock, a fade and an unlock
  6  suspend     DPMS and S3 suspend and resume preserve the lock (a second VM: SeaBIOS)
  7  capture     portal, screencast, remote desktop and DRM access across accounts
  8  power       unauthorised, inhibited, cancelled, refused and authorised power requests,
                 and the real reset of the VM
  9  drm         no DRM reset and no VT change over login, lock, unlock, switch and logout,
                 black frames counted
 10  logind      what logind, polkit, audio and systemd --user do for seatless hosted sessions

Each scenario ends pass, fail or blocked (a prerequisite is missing; never a pass). Output:
BINGUX_DISTRO_OUT or distro/out/bsm-test/security/: results.json, report.md and one directory
per scenario with state dumps, journal excerpts, PNGs, frame series (CSV) and probe output.
Exit status: 0 everything passed, 1 something failed, 2 nothing failed but something is
blocked. Working files and the video stay under /tmp (BSM_VM_DIR).

`--inject DIR` copies a tree staged by tests/bsm-vm-inject.py into the guest first, to test
a fixed crate against the existing image.

Only one VM may run on the shared build host at a time, so run it under the global lock:

    flock /tmp/bsm-vm.lock python3 tests/bsm-qemu-security.py

The run takes about an hour. The first Gnoblin start of a fresh VM is slow; a login that times
out at BinguxLaunched is retried up to three times and reported. Do not publish a result taken
at a host load average above 8.

Status of the scenarios: tests/bsm-qemu-security.py implements all ten; the results of the runs
that completed are in docs/bsm/security-scenarios-results.md.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from bsm_harness import HEAD_COUNT, MONITOR_LOG, enable_head  # noqa: E402
from bsm_security_frames import LEAK_FRACTION, MARKER_NAMES, VISIBLE_FRACTION, MarkerSampler  # noqa: E402
from bsm_security_lib import (  # noqa: E402
    GREETER_USER,
    GUEST_PROBE,
    GUEST_STATE_DIR,
    PASSWORDS,
    Blocked,
    Evidence,
    Guest,
    Result,
    SecVm,
)
from distro_vm import ROOT, find_iso, missing_tools  # noqa: E402

DEFAULT_OUT = ROOT / "distro/out/bsm-test/security"
HEADS = tuple(range(HEAD_COUNT))
ERROR_UNAUTHORISED = "unauthorised"


# ---- shared checks ---------------------------------------------------------------------------


def hidden(g, r, label, start, end, users=MARKER_NAMES, heads=HEADS, minimum=3):
    """No desktop marker of `users` on any display between `start` and `end` (host monotonic).

    Fails when the window holds fewer than `minimum` frames per display: a window that was
    not observed proves nothing.
    """
    leaks = {name: [s for s in g.sampler.window(start, end) if s.head in heads and s.fractions[name] > LEAK_FRACTION] for name in users}
    counts = {head: len(g.sampler.window(start, end, head)) for head in heads}
    worst = {name: max((s.fractions[name] for s in g.sampler.window(start, end) if s.head in heads), default=0.0) for name in users}
    detail = f"{sum(counts.values())} frames in {end - start:.1f} s; largest marker share " + ", ".join(f"{n} {v:.3f}" for n, v in worst.items())
    ok = not any(leaks.values()) and all(c >= minimum for c in counts.values())
    if any(leaks.values()):
        first = min(s.t for found in leaks.values() for s in found)
        detail += f"; first leak {first - start:.1f} s in"
    return r.check(label, ok, detail)


def shows(g, r, label, user, start, end, heads=HEADS):
    """The desktop of `user` fills every display in the window (the positive control for `hidden`)."""
    seen = {head: g.sampler.visible(start, end, user, head) for head in heads}
    detail = ", ".join(f"display {head}: {len(found)} frames" for head, found in seen.items())
    return r.check(label, all(seen.values()), detail)


def settle(seconds):
    time.sleep(seconds)


def faillock_count(g, user):
    out = g.sh(f"faillock --user {user}", 15)
    return sum(1 for line in out.splitlines() if re.match(r"\d{4}-\d\d-\d\d ", line.strip()))


def error_code(parsed):
    return parsed.get("error") if isinstance(parsed, dict) else None


def state_of(g, user):
    entry = g.session(user)
    return None if entry is None else (entry.get("state"), entry.get("generation"), entry.get("id"))


def gnoblin_pids(g):
    return {user: g.gnoblin_compositor(user) for user in PASSWORDS}


def mark_since(g):
    return g.epoch()


# ---- scenario 1: isolation between accounts ---------------------------------------------------------


def steady(g, r, label, user, start, end, heads=HEADS, minimum=3):
    """The desktop of `user` stayed on every display for the whole window: no flicker, no greeter flash."""
    counts, worst = {}, {}
    for head in heads:
        window = g.sampler.window(start, end, head)
        counts[head] = len(window)
        worst[head] = min((s.fractions[user] for s in window), default=0.0)
    ok = all(counts[h] >= minimum and worst[h] >= VISIBLE_FRACTION for h in heads)
    return r.check(label, ok, ", ".join(f"display {h}: {counts[h]} frames, smallest marker share {worst[h]:.2f}" for h in heads))


def closed_without_a_byte(result):
    """A connection the server accepted at the socket level and then dropped before sending anything."""
    return isinstance(result, dict) and result.get("connect") == "ok" and result.get("received_bytes") == 0 and result.get("closed") in ("reset", "eof")


def s1_isolation(g, r, ev):
    uid_a, uid_b = g.uids["alice"], g.uids["bob"]
    g.ensure_foreground("alice")
    g.ensure_marker("alice")
    g.logout("bob")
    bsm = g.bsm_pid()
    epoch0 = g.epoch()
    t0 = time.monotonic()
    g.ensure_foreground("alice")
    g.ensure_marker("alice")

    # 1a: the privileged sockets, as alice, as alice with the greeter's group, and as root.
    sockets = {
        "greeter Wayland": ("wayland", "/run/bsm/greeter-wayland.sock"),
        "greeter control": ("control", "/run/bsm/greeter-control.sock"),
        "helper": ("helper", "/run/bsm-helper/helper.sock"),
    }
    seen = {}
    for label, (kind, path) in sockets.items():
        seen[f"alice -> {label}"] = plain = g.probe("connect", "--kind", kind, "--path", path, "--user", "alice")
        r.check(f"alice cannot connect to the {label} socket (file permissions)", plain.get("connect") == "EACCES", plain)
        if kind != "helper":
            seen[f"alice+greeter group -> {label}"] = grouped = g.probe("connect", "--kind", kind, "--path", path, "--user", "alice", "--groups", GREETER_USER)
            r.check(f"with the greeter's group alice reaches the {label} socket and bsm closes it without a protocol byte", closed_without_a_byte(grouped), grouped)
        seen[f"root -> {label}"] = rooted = g.probe("connect", "--kind", kind, "--path", path)
        r.check(f"root is not exempt: the {label} socket is closed without a protocol byte", closed_without_a_byte(rooted), rooted)
    listing = g.probe("access", "--user", "alice", "/run/bsm/sessions")
    seen["alice lists /run/bsm/sessions"] = listing
    r.check("alice cannot list the brokered session sockets", listing["paths"][0].get("list") == "EACCES", listing["paths"][0])
    ev.json("sockets.json", seen)

    # 1b: bob signs in while his brokered socket is probed by everybody else, before and after bsm knows his bridge.
    gid_b = int(g.sh("id -g bob", 10).strip().splitlines()[-1])
    watch = f"{GUEST_STATE_DIR}/watch.json"
    userns = (
        f"alice-in-a-user-namespace-as-bob=runuser -u alice -- unshare --user --map-user={uid_b} --map-group={gid_b} "
        f"python3 {GUEST_PROBE} connect --kind session --path {{path}} --wait 14"
    )
    g.script(
        f"""echo 28 > {GUEST_STATE_DIR}/bridge-delay
: > {watch}
nohup python3 {GUEST_PROBE} watch-sockets --identities alice,bob,root,{GREETER_USER} --cmd '{userns}' \\
    --timeout 600 --hold 14 --later 5 --consumed-within 150 > {watch} 2>&1 &
""",
        timeout=30,
    )
    g.lock("alice")
    login_epoch = g.epoch()
    try:
        g.login("bob")
    finally:
        g.sh(f"rm -f {GUEST_STATE_DIR}/bridge-delay", 10)
    finished = g.wait_until(lambda: "PROBE-JSON" in g.sh(f"cat {watch}", 15), 120, poll=3.0)
    attack = {}
    if finished:
        text = g.sh(f"cat {watch}", 15)
        attack = json.loads(text[text.index("PROBE-JSON ") + len("PROBE-JSON "):].splitlines()[0])
    ev.json("pending-socket-attack.json", attack)
    r.check("the watcher saw bob's brokered socket appear", bool(attack.get("socket")), attack.get("socket"))
    r.check("the brokered socket is a world-connectable file, so only bsm's checks protect it", attack.get("socket_mode") == "0o666", attack.get("socket_mode"))
    for moment in ("early", "late"):
        for who, result in (attack.get(moment) or {}).items():
            r.check(f"{who} cannot become bob's session surface ({moment}): closed without a protocol byte", closed_without_a_byte(result), result)
    r.check("rejected attempts did not burn the socket: the real bridge used it afterwards", attack.get("socket_existed_before_late_probes") is True and attack.get("socket_consumed_by_bridge") is True, {k: attack.get(k) for k in ("socket_existed_before_late_probes", "socket_consumed_by_bridge")})
    entry_b = g.session("bob")
    r.check("bob's session reached the foreground through the real bridge", entry_b is not None and entry_b.get("foreground") and entry_b.get("state") == "foreground", str(entry_b))
    bridge = g.bridge("bob")
    exe = g.sh(f"readlink /proc/{bridge}/exe", 10).strip().splitlines()[-1] if bridge else ""
    r.check("the bridge bsm accepted is bsm-host running as bob", exe == "/usr/libexec/bsm/bsm-host" and g.sh(f"ps -o user= -p {bridge}", 10).strip().splitlines()[-1] == "bob", exe)
    text = g.journal(ev, "journal-brokered-socket.txt", login_epoch, units=("bsm",), lines=400)
    rejected = [line for line in text.splitlines() if "connection rejected" in line]
    r.check("bsm logged a rejection for every foreign uid and for bob's other process", any("peer uid " + str(uid_a) in line for line in rejected) and any("peer uid 0 " in line for line in rejected) and any("peer pid" in line for line in rejected), f"{len(rejected)} rejection lines")
    g.ensure_marker("bob")

    # 1c: bsm's D-Bus methods, as alice (inside her session scope and outside any) and as bob.
    entry_a = g.session("alice")
    sa, sb = entry_a["id"], entry_b["id"]
    before = {s["id"]: (s["state"], s["locked"], s["generation"]) for s in g.sessions()}
    foreground_before = g.status().get("foreground")
    window_start = time.monotonic()
    cases = [
        ("lock bob's session as alice, inside her own session", "alice", True, f"lock {sb}"),
        ("log out bob's session as alice, inside her own session", "alice", True, f"logout {sb}"),
        ("choose bob's account in the greeter as alice", "alice", True, f"select-user {uid_b}"),
        ("lock bob's session as alice, from outside any session", "alice", False, f"lock {sb}"),
        ("log out bob's session as alice, from outside any session", "alice", False, f"logout {sb}"),
        ("reboot as alice from outside any session", "alice", False, "reboot"),
        ("power off as alice from outside any session", "alice", False, "poweroff"),
        ("suspend as alice from outside any session", "alice", False, "suspend"),
        ("lock alice's session as bob, inside his own session", "bob", True, f"lock {sa}"),
        ("log out alice's session as bob, inside his own session", "bob", True, f"logout {sa}"),
        ("choose alice's account in the greeter as bob", "bob", True, f"select-user {uid_a}"),
    ]
    answers = {}
    for label, user, scope, arguments in cases:
        parsed, raw = g.ctl(arguments, user=user, scope=scope)
        answers[label] = raw
        r.check(f"{label}: denied as {ERROR_UNAUTHORISED}", error_code(parsed) == ERROR_UNAUTHORISED, raw[:140])
    parsed, raw = g.ctl("switch-user", user="alice", scope=True)
    answers["switch user as alice while bob is on screen"] = raw
    r.check("asking to switch user from a background session is refused", error_code(parsed) is not None, raw[:140])
    for label, command in (
        ("gdbus RequestLock", f"gdbus call --system --dest org.bingux.Session1 --object-path /org/bingux/Session1 --method org.bingux.Session1.RequestLock {sb}"),
        ("dbus-send RequestLogout", f"dbus-send --system --print-reply --dest=org.bingux.Session1 /org/bingux/Session1 org.bingux.Session1.RequestLogout string:{sb}"),
    ):
        out = g.sh(f"runuser -u alice -- {command} 2>&1", 30)
        answers[label] = out
        r.check(f"{label} on the system bus as alice returns the error org.bingux.Session1.Error.Unauthorised", "org.bingux.Session1.Error.Unauthorised" in out, out.strip()[:140])
    after = {s["id"]: (s["state"], s["locked"], s["generation"]) for s in g.sessions()}
    status = g.status()
    window_end = time.monotonic()
    ev.json("dbus-denials.json", answers)
    r.check("none of the refused requests changed a session (state, lock, generation)", before == after, f"{before} -> {after}")
    r.check("no operation started and bob stayed the foreground session", status.get("operation") is None and status.get("foreground") == foreground_before == sb, str(status))
    steady(g, r, "bob's desktop stayed on both displays through the refused requests", "bob", window_start, window_end)
    hidden(g, r, "alice's desktop never reached the screen while she was locked behind bob", window_start, window_end, users=("alice",))

    # 1d: appearance publishing and the greeter cache.
    shade = 80 + int(time.time()) % 30  # a different image every run, so the publication always changes alice's copy
    g.script(
        f"""runuser -u alice -- python3 -c "from PIL import Image; Image.new('RGB', (96, 96), (20, 90, {shade})).save('/home/alice/teal.png')"
find /var/lib/bingux/greeter -type f ! -name .lock -exec sha256sum {{}} + | sort -k2 > {GUEST_STATE_DIR}/cache-before.txt
""",
        timeout=40,
    )
    published = g.probe("publish", "--user", "alice", "--image", "/home/alice/teal.png")
    settle(3.0)
    g.sh(f"find /var/lib/bingux/greeter -type f ! -name .lock -exec sha256sum {{}} + | sort -k2 > {GUEST_STATE_DIR}/cache-after.txt", 30)
    changed = g.sh(f"diff {GUEST_STATE_DIR}/cache-before.txt {GUEST_STATE_DIR}/cache-after.txt | grep -E '^[<>]' | awk '{{print $3}}' | sort -u", 20).split()
    ev.json("publish.json", {"call": published, "changed_files": changed})
    r.check("alice's PublishAppearance call is accepted", published.get("reply") == "ok", published)
    r.check("it changed only alice's own cache directory", bool(changed) and all(path.startswith(f"/var/lib/bingux/greeter/{uid_a}/") for path in changed), changed)
    creates = g.probe("create", "--user", "alice", f"/var/lib/bingux/greeter/{uid_b}/planted", "/var/lib/bingux/greeter/4242")
    replace = g.probe("access", "--user", "alice", f"/var/lib/bingux/greeter/{uid_b}/profile.json")
    ev.json("cache-writes.json", {"create": creates, "overwrite": replace})
    r.check("alice cannot create files in bob's cache directory or a cache directory of her own choosing", all(item["create"] == "EACCES" for item in creates["results"]), creates["results"])
    r.check("alice cannot overwrite bob's published profile", replace["paths"][0].get("write") == "EACCES", replace["paths"][0])
    original = g.probe("access", "--user", "bob", "/home/alice/teal.png")
    copy = g.probe("access", "--user", "bob", f"/var/lib/bingux/greeter/{uid_a}/wallpaper")
    ev.json("cache-reads.json", {"original": original, "published_copy": copy})
    r.check("bob cannot read alice's original image", original["paths"][0].get("stat") == "EACCES" or original["paths"][0].get("read") == "EACCES", original["paths"][0])
    r.note(f"bob can read alice's re-encoded published copy ({copy['paths'][0].get('read')}): world-readable by design, the documented privacy trade-off")

    # 1e: files, sockets and processes of the other account, as alice.
    greeter_dirs = g.sh("ls -d /run/bsm-greeter/*/rt 2>/dev/null", 10).split()
    targets = [f"/home/bob", f"/run/user/{uid_b}", f"/run/user/{uid_b}/bus", f"/run/user/{uid_b}/pipewire-0", f"/run/user/{uid_b}/bsm-{sb}",
               f"/run/user/{uid_b}/gnoblin/compositor-v1.sock", f"/run/user/{uid_b}/bsm/ready.sock", "/var/lib/bsm"] + greeter_dirs
    found = g.probe("access", "--user", "alice", *targets)
    ev.json("alice-sees-bob.json", found)
    for entry in found["paths"]:
        denied = entry.get("stat") == "EACCES" or entry.get("list") == "EACCES" or entry.get("read") == "EACCES"
        r.check(f"alice cannot reach {entry['path']}", denied, entry)
    bob_bus = g.probe("rd-inject", "--user", "alice", "--bus", f"unix:path=/run/user/{uid_b}/bus", "x")
    r.check("alice cannot open a connection to bob's session bus", "Permission denied" in str(bob_bus.get("connect")), bob_bus)
    greeter = g.greeter_pids()
    victims = [pid for pid in [bsm, (g.pids("bsm-helper", user="root") or [None])[0], (greeter or [None])[0], g.gnoblin_compositor("bob"), g.bridge("bob")] if pid]
    trace = g.probe("ptrace", "--user", "alice", *victims)
    ev.json("ptrace.json", trace)
    r.check("alice cannot ptrace bsm, the helper, the greeter or bob's Gnoblin and bridge", all(v == "EPERM" for v in trace["ptrace"].values()), trace["ptrace"])
    procs = g.probe("access", "--user", "alice", f"/proc/{bsm}/environ", f"/proc/{g.gnoblin_compositor('bob')}/environ", f"/proc/{bsm}/mem")
    r.check("alice cannot read the environment or memory of bsm or of bob's Gnoblin", all(p.get("read") == "EACCES" for p in procs["paths"]), [p.get("read") for p in procs["paths"]])

    # 1f: clipboards.
    token = f"SECRET-OF-BOB-{int(time.time())}"
    own = lambda user, command: g.sh(  # noqa: E731
        f"python3 {GUEST_PROBE} scope-exec --scope-of {user} --user {user} --env WAYLAND_DISPLAY=bsm-{ {'alice': sa, 'bob': sb}[user] } -- sh -c '{command}' 2>&1", 30
    )
    own("bob", f"printf %s {token} | wl-copy")
    settle(1.5)
    bobs_own = own("bob", "wl-paste --no-newline")
    alice_own = own("alice", "wl-paste --no-newline")
    across = g.sh(f"runuser -u alice -- env XDG_RUNTIME_DIR=/run/user/{uid_a} WAYLAND_DISPLAY=/run/user/{uid_b}/bsm-{sb} wl-paste --no-newline 2>&1", 30)
    greeter_paste = g.sh(f"runuser -u alice -- env WAYLAND_DISPLAY=/run/bsm/greeter-wayland.sock wl-paste --no-newline 2>&1", 30)
    ev.json("clipboard.json", {"bob_own_session": bobs_own, "alice_own_session": alice_own, "alice_at_bobs_socket": across, "alice_at_greeter_socket": greeter_paste})
    if token in bobs_own:
        r.check("positive control: bob's clipboard holds his secret inside his own session", True)
    else:
        r.check("positive control: bob's clipboard holds his secret inside his own session", None, f"wl-paste in bob's session printed {bobs_own.strip()[:80]!r}")
    r.check("alice's own session does not see bob's clipboard", token not in alice_own, alice_own.strip()[:80])
    r.check("alice cannot connect to bob's Wayland socket, so cannot read his clipboard", token not in across and ("Permission denied" in across or "Failed to connect" in across), across.strip()[:120])
    r.check("alice cannot read a clipboard through the greeter's socket", token not in greeter_paste and ("Permission denied" in greeter_paste or "Failed to connect" in greeter_paste), greeter_paste.strip()[:120])

    # The attacks changed nothing on screen or in bsm.
    r.check("bsm kept its pid through all of it", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")
    ev.frames("frames.csv", t0, time.monotonic(), t0)
    ev.png("end")
    g.snapshot(ev, "end")


# ---- scenario 2: the greeter dies while a session is locked --------------------------------------------


def cover_frames(g, start, end, head):
    """Frames that are the opaque cover (#0d1117): dark, uniform, not black."""
    return [s for s in g.sampler.window(start, end, head) if 8.0 <= s.mean <= 30.0 and s.p99 < 40.0]


def keyboard_reaches_session(g, user):
    """Positive control for the key logs: a key typed on the physical keyboard reaches `user`'s marker window."""
    before = len(g.key_log(user))
    g.vm.key("q")
    settle(1.5)
    return len(g.key_log(user)) > before


def s2_greeter(g, r, ev):
    g.ensure_pair(foreground="alice", background="bob")
    keys_work = keyboard_reaches_session(g, "alice")
    g.lock("alice")
    bsm = g.bsm_pid()
    pids = gnoblin_pids(g)
    entry_a, entry_b = g.session("alice"), g.session("bob")
    r.check("before the crash alice is locked behind the greeter and bob is locked in the background", entry_a["locked"] and entry_b["locked"] and not entry_b["foreground"], f"{entry_a['state']}, {entry_b['state']}")
    settle(3.0)
    pre_start = time.monotonic()
    settle(2.0)
    ev.png("before-kill")
    old = g.greeter_pids()
    shell_before = g.pids("bingux-shell", user=GREETER_USER)
    keys = {user: len(g.key_log(user)) for user in PASSWORDS}
    epoch0 = g.epoch()
    # An impostor with the greeter's uid (not its pid) hammers both greeter sockets from before the kill until after the respawn.
    g.script(f"nohup python3 {GUEST_PROBE} impostor --seconds 20 > {GUEST_STATE_DIR}/impostor.json 2>&1 &", timeout=20)
    settle(1.5)
    t_start = time.monotonic()
    kill_epoch = g.kill(old)
    t_kill = g.host_time(kill_epoch)
    settle(1.0)
    g.vm.type_text("zzz")  # keys with no greeter to take them
    back = g.wait_until(lambda: [p for p in g.pids("bingux-shell", user=GREETER_USER) if p not in shell_before] and g.status().get("greeter_connected"), 120, poll=1.0)
    t_back = time.monotonic()
    settle(10.0)
    t_end = time.monotonic()
    r.check("bsm keeps its pid while the greeter is dead", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")
    r.check("a new greeter connects by itself", bool(back), f"greeter shell pids {shell_before} -> {g.pids('bingux-shell', user=GREETER_USER)}")
    hidden(g, r, "no desktop pixel of either account on either display from the kill until after the respawn", t_kill - 1.0, t_end)
    for head in HEADS:
        outage = [s for s in g.sampler.window(t_kill, t_back, head)]
        covers = cover_frames(g, t_kill, t_back, head)
        r.check(f"display {head}: the opaque cover is on screen while no greeter exists", bool(covers) or len(outage) < 3, f"{len(covers)} cover frames of {len(outage)} in the {t_back - t_kill:.1f} s outage")
        r.check(f"display {head}: no black frame during the outage", not [s for s in outage if s.black], f"{sum(1 for s in outage if s.black)} black of {len(outage)}")
    sessions = {u: g.session(u) for u in PASSWORDS}
    r.check("both sessions stay protected and running: still locked, same Gnoblin processes", all(sessions[u] and sessions[u]["locked"] for u in PASSWORDS) and gnoblin_pids(g) == pids, f"{gnoblin_pids(g)} vs {pids}")
    # Keyboard: nothing typed with no greeter reached either desktop.
    if keys_work:
        r.check("keystrokes typed while no greeter existed did not reach either desktop", all(len(g.key_log(u)) == keys[u] for u in PASSWORDS), {u: len(g.key_log(u)) - keys[u] for u in PASSWORDS})
    else:
        r.check("keystrokes typed while no greeter existed did not reach either desktop", None, "positive control failed: a physical key did not reach alice's marker window while she was on screen")
    # The impostor.
    finished = g.wait_until(lambda: "PROBE-JSON" in g.sh(f"cat {GUEST_STATE_DIR}/impostor.json", 15), 60, poll=3.0)
    impostor = {}
    if finished:
        text = g.sh(f"cat {GUEST_STATE_DIR}/impostor.json", 15)
        impostor = json.loads(text[text.index("PROBE-JSON ") + len("PROBE-JSON "):].splitlines()[0])
    ev.json("impostor.json", impostor)
    stats = impostor.get("stats") or {}
    r.check("an impostor running as the greeter's uid made many attempts on both greeter sockets", all(stats.get(k, {}).get("attempts", 0) >= 10 for k in ("wayland", "control")), {k: stats.get(k) for k in ("wayland", "control")})
    r.check("it never received a single byte, so it never became the greeter", all(stats.get(k, {}).get("bytes", 1) == 0 for k in ("wayland", "control")), {k: stats.get(k, {}).get("bytes") for k in ("wayland", "control")})
    text = g.journal(ev, "journal-greeter-crash.txt", epoch0, units=("bsm",), lines=400)
    r.check("bsm logged the impostor as not being the greeter pid", "is not the greeter pid" in text, f"{text.count('is not the greeter pid')} lines")
    r.check("the real greeter attached afterwards (greeter connected)", "greeter connected" in text)
    # Unlock still works: the lock scene is back, the password unlocks alice, same Gnoblin.
    ev.png("after-respawn")
    unlock_start = time.monotonic()
    g.unlock("alice")
    unlock_end = time.monotonic()
    settle(3.0)
    shows(g, r, "after the unlock alice's desktop is on both displays again", "alice", unlock_end, time.monotonic())
    hidden(g, r, "bob's desktop stayed hidden through the unlock", unlock_start, time.monotonic(), users=("bob",))
    r.check("alice's Gnoblin is the same process as before the crash", g.gnoblin_compositor("alice") == pids["alice"], f"{pids['alice']} -> {g.gnoblin_compositor('alice')}")
    r.check("bsm still has its pid", g.bsm_pid() == bsm)
    ev.frames("frames.csv", pre_start, time.monotonic(), t_start)
    ev.leak_pngs(pre_start)
    g.snapshot(ev, "end")


# ---- scenario 3: a hosted Gnoblin dies --------------------------------------------------------------------


def logind_user_sessions(g, user):
    """Ids of the logind sessions of class `user` of the account (columns: SESSION UID USER SEAT LEADER CLASS ...)."""
    found = []
    for row in g.sh("loginctl list-sessions --no-legend --no-pager", 20).splitlines():
        parts = row.split()
        if len(parts) >= 6 and parts[2] == user and parts[5] == "user":
            found.append(parts[0])
    return found


def scope_state(g, scope):
    """(systemd activity, whether the cgroup directory still exists) of a session scope."""
    active = g.sh(f"systemctl is-active {scope} 2>&1 | tail -n 1", 20).strip().splitlines()[-1]
    present = g.sh(f"find /sys/fs/cgroup/user.slice -maxdepth 2 -name {scope} | head -n 1", 20).strip()
    return active, bool(present)


def crash_and_observe(g, r, ev, label, victim, killed_user, process_kind, expect_foreground):
    """Kill -9 a process of `killed_user`'s hosted session and check cleanup, the screen and the survivor."""
    other = "bob" if killed_user == "alice" else "alice"
    entry = g.session(killed_user)
    sid = entry["id"]
    bsm = g.bsm_pid()
    survivor_pid = g.gnoblin_compositor(other)
    logind_before = logind_user_sessions(g, killed_user)
    scope = g.probe("scope-of", "--user", killed_user).get("scope")
    scope_unit = scope.rsplit("/", 1)[-1] if scope else None
    mark, epoch = g.mark(), g.epoch()
    t_start = time.monotonic()
    t_kill = g.host_time(g.kill([victim]))
    gone = g.wait_until(lambda: g.session(killed_user) is None, 90, poll=1.0)
    t_gone = time.monotonic()
    settle(12.0)
    t_end = time.monotonic()
    r.check(f"{label}: bsm survives with the same pid", g.bsm_pid() == bsm and bsm is not None, f"{bsm} -> {g.bsm_pid()}")
    r.check(f"{label}: {killed_user}'s session is removed from bsm", bool(gone), f"gone after {t_gone - t_kill:.1f} s")
    rows, _ = g.events(mark)
    kinds = [e.get("event") for _, e in rows]
    r.check(f"{label}: bsm announced the change (sessions_changed)", "sessions_changed" in kinds, kinds[:12])
    text = g.journal(ev, f"journal-{label}.txt", epoch, units=("bsm", "bsm-helper"), lines=250)
    r.check(f"{label}: bsm recorded the session as terminated", f'session terminated session="{sid}"' in text or f"session terminated" in text, f"session {sid}")
    left = g.wait_until(lambda: not logind_user_sessions(g, killed_user), 40, poll=2.0)
    r.check(f"{label}: logind closed {killed_user}'s user session", bool(left), f"before {logind_before}, after {logind_user_sessions(g, killed_user)}")
    if scope_unit:
        active, present = scope_state(g, scope_unit)
        r.check(f"{label}: the session scope {scope_unit} is gone and holds no process", not present and active != "active", f"systemd {active}, cgroup directory {'present' if present else 'gone'}")
    survivors = [p for p in g.procs() if p[2] == killed_user and p[3] in ("gnoblin", "bsm-host", "bingux")]
    r.check(f"{label}: no Gnoblin, bridge or shell of {killed_user} is left", not survivors, [(p[0], p[3]) for p in survivors])
    hidden(g, r, f"{label}: the dead session's desktop is not on screen after the crash", t_kill + 3.0, t_end, users=(killed_user,))
    last = g.sampler.last_leak_time(t_kill - 1.0, t_end, killed_user)
    r.check(f"{label}: the last frame that showed it came within 3 s of the kill", last is None or last - t_kill <= 3.0, "never shown" if last is None else f"{last - t_kill:.2f} s after the kill")
    hidden(g, r, f"{label}: {other}'s desktop never reached the screen", t_kill - 1.0, t_end, users=(other,))
    status = g.status()
    r.check(f"{label}: the display returned to the greeter and no session is foreground and unlocked", status.get("greeter_connected") and g.foreground_unlocked() is None and status.get("seat") in ("greeter", "authenticating", "locked_greeter"), f"seat {status.get('seat')}")
    survivor = g.session(other)
    r.check(f"{label}: {other}'s session survives with the same Gnoblin and is still locked", survivor is not None and survivor.get("locked") and g.gnoblin_compositor(other) == survivor_pid, f"{survivor_pid} -> {g.gnoblin_compositor(other)}")
    black = [s for s in g.sampler.window(t_kill, t_end) if s.black]
    r.check(f"{label}: no black frame from the crash to the greeter", not black, f"{len(black)} black frames")
    ev.png(f"after-{label}")
    ev.frames(f"frames-{label}.csv", t_start, t_end, t_start)
    ev.leak_pngs(t_start, f"leak-{label}")
    return survivor_pid


def s3_gnoblin(g, r, ev):
    # 3a: alice is on screen when her compositor dies; bob is locked in the background.
    g.ensure_pair(foreground="alice", background="bob")
    compositor = g.gnoblin_compositor("alice")
    shows(g, r, "before the crash alice's desktop is on both displays", "alice", time.monotonic() - 4.0, time.monotonic())
    crash_and_observe(g, r, ev, "foreground-compositor", compositor, "alice", "compositor", True)
    ev.png("alice-gone")
    bob_pid = g.gnoblin_compositor("bob")
    start = time.monotonic()
    g.unlock("bob")
    settle(3.0)
    shows(g, r, "the surviving locked session can still be unlocked: bob's desktop appears on both displays", "bob", start, time.monotonic())
    r.check("bob's Gnoblin is the same process after the unlock", g.gnoblin_compositor("bob") == bob_pid, f"{bob_pid} -> {g.gnoblin_compositor('bob')}")

    # 3b: bob is locked in the background behind alice's lock screen when his launcher process dies.
    g.ensure_pair(foreground="alice", background="bob")
    g.lock("alice")
    alice_pid = g.gnoblin_compositor("alice")
    leader = g.gnoblin_leader("bob")
    r.check("bob's session has its hosted leader process to kill", leader is not None)
    if leader:
        crash_and_observe(g, r, ev, "background-leader", leader, "bob", "leader", False)
        start = time.monotonic()
        g.unlock("alice")
        settle(3.0)
        shows(g, r, "alice, the surviving locked session, can still be unlocked", "alice", start, time.monotonic())
        r.check("alice's Gnoblin is the same process after the unlock", g.gnoblin_compositor("alice") == alice_pid, f"{alice_pid} -> {g.gnoblin_compositor('alice')}")
    g.snapshot(ev, "end")


# ---- scenario 5: an output changes during a lock, a fade and an unlock -----------------------------------


def resize_head(g, head, width, height):
    """Ask QEMU's VNC server of a display head for another desktop size; 0x0 switches the head off."""
    path = g.vm.work / ("vnc.sock" if head == 0 else f"vnc{head}.sock")
    connection = enable_head(path, width, height)
    time.sleep(1.0)
    connection.close()


def bsm_outputs(g):
    data, _ = g.ctl("outputs")
    return data if isinstance(data, list) else []


def s5_outputs(g, r, ev):
    g.ensure_pair(foreground="alice", background="bob")
    bsm = g.bsm_pid()
    entry_a = g.session("alice")
    r.check("two outputs at the start", len(bsm_outputs(g)) == 2, bsm_outputs(g))
    t0 = time.monotonic()

    # 5a: display 1 is switched off in the middle of the lock fade.
    mark = g.mark()
    g.ctl(f"lock {entry_a['id']}")
    anim = g.wait_event(lambda e: e.get("event") == "transition" and e.get("kind") == "lock" and e.get("phase") == "animating", 60, mark, poll=0.5)
    r.check("the lock fade started", anim is not None)
    t_anim = anim[0] if anim else time.monotonic()
    settle(0.8)
    resize_head(g, 1, 0, 0)
    done = g.finished(mark, 90)
    settle(8.0)
    t_5a = time.monotonic()
    r.check("the lock finished although an output vanished in the middle of it", done is not None and done[1].get("success") is True, done[1] if done else None)
    r.check("bsm sees one output", len(bsm_outputs(g)) == 1, bsm_outputs(g))
    hidden(g, r, "no desktop pixel on either display from the start of the lock fade until display 1 is gone", t_anim, t_5a)
    r.check("bsm kept its pid", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")
    ev.png("display1-off-locked")

    # 5b: a new output appears while the session is locked.
    t_b = time.monotonic()
    resize_head(g, 1, 1280, 800)
    came = g.wait_until(lambda: len(bsm_outputs(g)) == 2, 60, poll=2.0)
    settle(10.0)
    t_5b = time.monotonic()
    r.check("bsm sees the new output", bool(came), bsm_outputs(g))
    hidden(g, r, "no desktop pixel on the remaining or the newly connected display while it is locked", t_b, t_5b)
    last = g.sampler.latest(1)
    r.check("the new output shows the greeter, not black and not a desktop", last is not None and not last.black and last.p99 > 100.0 and last.fractions["alice"] < LEAK_FRACTION, None if last is None else f"mean {last.mean:.0f}, p99 {last.p99:.0f}, size {last.width}x{last.height}")
    ev.png("display1-back-locked")

    # 5c: display 1 changes size (shrunk, then restored).
    t_c = time.monotonic()
    resize_head(g, 1, 800, 600)
    shrunk = g.wait_until(lambda: any(o.get("width") == 800 for o in bsm_outputs(g)), 60, poll=2.0)
    settle(8.0)
    resize_head(g, 1, 1280, 800)
    g.wait_until(lambda: all(o.get("width") == 1280 for o in bsm_outputs(g)) and len(bsm_outputs(g)) == 2, 60, poll=2.0)
    settle(8.0)
    t_5c = time.monotonic()
    r.check("bsm followed the new size of display 1", bool(shrunk), bsm_outputs(g))
    hidden(g, r, "no desktop pixel while display 1 is shrunk and restored", t_c, t_5c)
    r.check("bsm kept its pid", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")

    # 5d: display 1 is switched off in the middle of the unlock slide (alice is authenticated; bob must stay hidden).
    g.begin_auth("alice")
    mark = g.mark()
    t_d = time.monotonic()
    g.type_secret(PASSWORDS["alice"])
    anim = g.wait_event(lambda e: e.get("event") == "transition" and e.get("kind") == "unlock_slide_up" and e.get("phase") == "animating", 120, mark, poll=0.5)
    r.check("the unlock slide started", anim is not None)
    settle(1.0)
    resize_head(g, 1, 0, 0)
    done = g.finished(mark, 120)
    settle(8.0)
    outcome = done[1] if done else None
    r.note(f"unlock with an output removed mid-slide ended as {outcome}")
    if not (outcome and outcome.get("success")):
        # A refused reveal is a safe outcome; the account then unlocks on the remaining output.
        g.begin_auth("alice")
        retry_mark = g.mark()
        g.type_secret(PASSWORDS["alice"])
        done = g.finished(retry_mark, 120)
        settle(8.0)
    t_5d = time.monotonic()
    hidden(g, r, "bob's desktop never reached either display during the unlock and the output change", t_d, t_5d, users=("bob",))
    shows_head0 = bool(g.sampler.visible(t_5d - 6.0, t_5d, "alice", head=0))
    r.check("alice's desktop is on the remaining display afterwards", shows_head0, f"{len(g.sampler.visible(t_5d - 6.0, t_5d, 'alice', head=0))} frames")
    r.check("no black frame on the remaining display during the unlock", not g.sampler.black(t_d, t_5d, head=0), f"{len(g.sampler.black(t_d, t_5d, head=0))} black")
    resize_head(g, 1, 1280, 800)
    back = g.wait_until(lambda: len(bsm_outputs(g)) == 2, 60, poll=2.0)
    both = g.wait_until(lambda: g.sampler.visible(time.monotonic() - 3.0, time.monotonic(), "alice", head=1), 90, poll=3.0)
    r.check("the re-added output shows alice's desktop once its surface is ready", bool(back) and bool(both), f"outputs {len(bsm_outputs(g))}")
    hidden(g, r, "bob's desktop never reached either display while the output came back", t_5d, time.monotonic(), users=("bob",))
    r.check("bsm kept its pid through every output change", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")
    ev.frames("frames.csv", t0, time.monotonic(), t0)
    ev.leak_pngs(t0)
    ev.png("end")
    g.snapshot(ev, "end")


# ---- scenario 7: capture and remote control across accounts ----------------------------------------------


def s7_capture(g, r, ev):
    uid_a, uid_b = g.uids["alice"], g.uids["bob"]
    g.ensure_pair(foreground="alice", background="bob")
    sa, sb = g.session("alice")["id"], g.session("bob")["id"]
    bsm = g.bsm_pid()
    bus_a, bus_b = f"unix:path=/run/user/{uid_a}/bus", f"unix:path=/run/user/{uid_b}/bus"
    t0 = time.monotonic()

    # Inside alice's own session: capture and remote control work, for alice's session only.
    cast = g.probe("screencast", "--user", "alice")
    r.check("alice can open a ScreenCast of her own session on her own bus (positive control)", "stream" in cast, cast)
    keys_before = g.key_log("alice")
    inject = g.probe("rd-inject", "--user", "alice", "qw")
    settle(1.5)
    keys_after = g.key_log("alice")
    r.check("alice can inject keys into her own session through RemoteDesktop (positive control)", inject.get("injected") == "qw" and keys_after[len(keys_before):][:2] == ["q", "w"], {"probe": inject, "marker saw": keys_after[len(keys_before):]})
    names = g.sh(f"runuser -u alice -- busctl --address={bus_a} list --no-pager --no-legend 2>&1", 30)
    owners = {line.split()[2] for line in names.splitlines() if len(line.split()) >= 3 and line.split()[2] not in ("-",)}
    ev.text("alice-bus-names.txt", names)
    r.check("every connection on alice's session bus belongs to alice", owners <= {"alice"}, sorted(owners))

    # Across accounts: nothing of bob's session bus, sockets or streams is reachable.
    cast_b = g.probe("screencast", "--user", "alice", "--bus", bus_b)
    inject_b = g.probe("rd-inject", "--user", "alice", "--bus", bus_b, "zz")
    r.check("alice cannot open a ScreenCast on bob's session bus", "Permission denied" in str(cast_b.get("connect")) and "stream" not in cast_b, cast_b)
    r.check("alice cannot inject input into bob's session through RemoteDesktop", "Permission denied" in str(inject_b.get("connect")) and "injected" not in inject_b, inject_b)
    r.check("nothing alice injected reached bob's desktop", [k for k in g.key_log("bob") if k in ("z",)] == [], g.key_log("bob"))
    system = g.sh(f"runuser -u alice -- gdbus call --system --dest org.gnome.Mutter.ScreenCast --object-path /org/gnome/Mutter/ScreenCast --method org.gnome.Mutter.ScreenCast.CreateSession \"{{}}\" 2>&1", 30)
    r.check("no ScreenCast service exists on the system bus: bsm's own output cannot be captured through D-Bus", "ServiceUnknown" in system, system.strip()[:140])
    listing = g.sh("busctl --system list --no-pager --no-legend 2>&1", 30)
    ev.text("system-bus-names.txt", listing)
    capture_names = [line.split()[0] for line in listing.splitlines() if re.search(r"ScreenCast|RemoteDesktop|portal|Screenshot|wayland|mutter|gnoblin", line, re.I) and line.split()[0].startswith("org.")]
    r.check("the system bus carries no capture, portal or remote-desktop name", not capture_names, capture_names)
    sockets = g.probe(
        "access", "--user", "alice",
        f"/run/user/{uid_b}/gnoblin/compositor-v1.sock", f"/run/user/{uid_b}/bsm-{sb}", f"/run/user/{uid_b}/pipewire-0", f"/run/user/{uid_b}/pulse/native", f"/run/user/{uid_b}/bsm/ready.sock",
    )
    ev.json("bob-sockets-seen-by-alice.json", sockets)
    r.check("alice cannot reach bob's compositor, Wayland, PipeWire, PulseAudio or readiness sockets", all(p.get("stat") == "EACCES" for p in sockets["paths"]), [(p["path"].rsplit("/", 1)[-1], p.get("stat")) for p in sockets["paths"]])
    pactl = g.sh(f"runuser -u alice -- env PULSE_SERVER=unix:/run/user/{uid_b}/pulse/native pactl info 2>&1 | head -n 3", 30)
    r.check("alice cannot use bob's PipeWire or PulseAudio server", "Server String" not in pactl and "User Name: bob" not in pactl, pactl.strip()[:140])
    wayland = g.sh(f"runuser -u alice -- env WAYLAND_DISPLAY=/run/user/{uid_b}/bsm-{sb} XDG_RUNTIME_DIR=/run/user/{uid_a} wl-paste -l 2>&1 | head -n 2", 30)
    r.check("alice cannot attach a Wayland client to bob's Gnoblin", "Failed to connect" in wayland or "Permission denied" in wayland, wayland.strip()[:140])

    # The physical display and the physical input devices.
    drm_a = g.probe("drm", "--user", "alice")
    drm_root = g.probe("drm", "--user", "root")
    ev.json("drm-access.json", {"alice": drm_a, "root": drm_root})
    handles = [s.get("getfb_handle") for s in drm_a.get("scanouts", [])]
    if drm_a.get("open") == "ok":
        r.check("alice (member of video) can open the card node but is no DRM master", drm_a.get("set_master") in ("EACCES", "EPERM", "EBUSY"), drm_a.get("set_master"))
        r.check("alice cannot obtain a buffer handle for either scanout framebuffer (GETFB returns 0), so cannot read the screen", bool(handles) and all(h == 0 for h in handles), handles)
    else:
        r.check("alice cannot open the DRM card node", drm_a.get("open") in ("EACCES", "EPERM"), drm_a.get("open"))
    r.note(f"control: root gets buffer handles {[s.get('getfb_handle') for s in drm_root.get('scanouts', [])]} for the same scanouts (CAP_SYS_ADMIN), as designed")
    nodes = g.probe("access", "--user", "alice", "/dev/uinput", *g.sh("ls /dev/input/event* 2>/dev/null", 10).split()[:3])
    r.check("alice can neither inject physical input (/dev/uinput) nor read raw input devices", all(p.get("stat") == "ok" and p.get("write") in ("EACCES", "EPERM") and p.get("read") in ("EACCES", "EPERM") for p in nodes["paths"]), [(p["path"], p.get("read"), p.get("write")) for p in nodes["paths"]])

    # alice's portal: reachable by alice only.
    portal = g.sh(
        f"runuser -u alice -- gdbus call --address {bus_a} --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop "
        "--method org.freedesktop.DBus.Properties.Get org.freedesktop.portal.ScreenCast version 2>&1", 60,
    )
    portal_b = g.sh(
        f"runuser -u alice -- gdbus call --address {bus_b} --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop "
        "--method org.freedesktop.DBus.Properties.Get org.freedesktop.portal.ScreenCast version 2>&1", 60,
    )
    ev.json("portal.json", {"alice_own_portal": portal, "alice_at_bobs_portal": portal_b})
    r.check("alice's own portal answers on her bus", "(<" in portal, portal.strip()[:140])
    r.check("alice cannot reach bob's portal", "Permission denied" in portal_b or "Could not connect" in portal_b, portal_b.strip()[:140])
    steady(g, r, "alice's desktop stayed on both displays through every probe", "alice", t0, time.monotonic())
    hidden(g, r, "bob's desktop never reached the screen", t0, time.monotonic(), users=("bob",))
    r.check("bsm kept its pid", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")
    g.snapshot(ev, "end")


# ---- scenario 9: no DRM reset, no VT change, no black frame over the normal lifecycle -----------------------

KPROBES = {
    "crtc_enable": "virtio_gpu:virtio_gpu_crtc_atomic_enable",
    "crtc_disable": "virtio_gpu:virtio_gpu_crtc_atomic_disable",
    "set_crtc": "drm_mode_setcrtc",
    "atomic_commits": "drm_mode_atomic_ioctl",
}


def install_counters(g):
    """Kernel probes that count CRTC enables and disables (a modeset), legacy SETCRTC and atomic commits; a VT and a udev watcher."""
    probes = "\n".join(f"echo 'p:bsmsec/{name} {target}' >> $T/kprobe_events" for name, target in KPROBES.items())
    out = g.script(
        f"""T=/sys/kernel/tracing
echo 0 > $T/events/bsmsec/enable 2>/dev/null
echo > $T/kprobe_events
{probes}
echo 1 > $T/events/bsmsec/enable
cat $T/kprobe_events
pkill -f 'vt-watch' ; pkill -f 'udevadm monitor' ; true
: > {GUEST_STATE_DIR}/vt.log
: > {GUEST_STATE_DIR}/udev.log
nohup sh -c 'last=; while :; do now=$(cat /sys/class/tty/tty0/active); if [ "$now" != "$last" ]; then echo "$(date +%s.%N) $now" >> {GUEST_STATE_DIR}/vt.log; last=$now; fi; sleep 0.1; done # vt-watch' >/dev/null 2>&1 &
nohup udevadm monitor --kernel --subsystem-match=drm > {GUEST_STATE_DIR}/udev.log 2>&1 &
sleep 1
""",
        timeout=40,
    )
    return out


def read_counters(g):
    counts = {}
    for line in g.sh("cat /sys/kernel/tracing/kprobe_profile", 20).splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[1].isdigit():
            counts[parts[0]] = int(parts[1])
    vt = [line.split() for line in g.sh(f"cat {GUEST_STATE_DIR}/vt.log", 10).splitlines() if line.strip()]
    udev = [line for line in g.sh(f"cat {GUEST_STATE_DIR}/udev.log", 10).splitlines() if " change " in line or " add " in line or " remove " in line]
    return {"probes": counts, "vt_values": [v[-1] for v in vt], "udev_drm_events": len(udev)}


def s9_drm(g, r, ev):
    g.logout_all()
    bsm = g.bsm_pid()
    installed = install_counters(g)
    ev.text("kprobes-installed.txt", installed)
    missing = [name for name in KPROBES if f"bsmsec/{name}" not in installed]
    if len(missing) == len(KPROBES):
        raise Blocked("no kernel probe could be installed (tracefs or kprobes unavailable in this guest)")
    before = read_counters(g)
    stages = []
    t0 = time.monotonic()

    def stage(name, action):
        started = time.monotonic()
        action()
        settle(2.0)
        stages.append((name, started, time.monotonic()))

    stage("new login of alice (fade in over the greeter)", lambda: g.login("alice"))
    stage("lock alice", lambda: g.lock("alice"))
    stage("unlock alice (greeter slides up)", lambda: g.unlock("alice"))
    stage("switch to bob: lock alice, new login of bob", lambda: (g.lock("alice"), g.login("bob")))
    stage("switch back: lock bob, unlock alice", lambda: (g.lock("bob"), g.unlock("alice")))
    stage("log out alice (fade out)", lambda: g.logout("alice"))
    stage("log out bob", lambda: g.logout("bob"))
    t_end = time.monotonic()
    after = read_counters(g)
    delta = {name: after["probes"].get(name, 0) - before["probes"].get(name, 0) for name in KPROBES if name not in missing}
    table = []
    for name, start, end in stages:
        row = {"stage": name, "seconds": round(end - start, 1)}
        for head in HEADS:
            window = g.sampler.window(start, end, head)
            row[f"display{head}_frames"] = len(window)
            row[f"display{head}_black"] = sum(1 for s in window if s.black)
        table.append(row)
    ev.json("lifecycle.json", {"counters_before": before, "counters_after": after, "deltas": delta, "stages": table, "missing_probes": missing})
    ev.frames("frames.csv", t0, t_end, t0)
    total = {head: len(g.sampler.window(t0, t_end, head)) for head in HEADS}
    black = {head: len(g.sampler.black(t0, t_end, head)) for head in HEADS}
    r.note(f"counters over the lifecycle: {delta}; VT values {sorted(set(after['vt_values']))}; udev DRM events {after['udev_drm_events'] - before['udev_drm_events']}; frames {total}; black {black}")
    for name in ("crtc_enable", "crtc_disable", "set_crtc"):
        if name in delta:
            label = {"crtc_enable": "no CRTC was enabled", "crtc_disable": "no CRTC was disabled", "set_crtc": "no legacy SETCRTC was issued"}[name]
            r.check(f"{label} (no modeset) across login, lock, unlock, switch and logout", delta[name] == 0, f"{name}: {delta[name]} hits")
    if "atomic_commits" in delta:
        r.check("the lifecycle drove the display (atomic commits were counted, so the zero counts above are not an idle probe)", delta["atomic_commits"] > 20, f"{delta['atomic_commits']} atomic commits")
    r.check("the virtual terminal never changed", len(set(after["vt_values"])) == 1 and len(set(before["vt_values"])) <= 1, after["vt_values"])
    r.check("the kernel sent no DRM uevent (no hotplug, no reset)", after["udev_drm_events"] - before["udev_drm_events"] == 0, f"{after['udev_drm_events'] - before['udev_drm_events']} events")
    for head in HEADS:
        r.check(f"display {head}: no black frame in {total[head]} frames over the whole lifecycle", total[head] >= 50 and black[head] == 0, f"{black[head]} black of {total[head]}")
    r.check("bsm kept its pid across the lifecycle", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")
    r.check("both outputs are still there", len(bsm_outputs(g)) == 2, bsm_outputs(g))
    g.snapshot(ev, "end")


# ---- scenario 10: what logind, polkit, audio and systemd --user do for seatless hosted sessions -------------

POLKIT_ACTIONS = [
    "org.freedesktop.login1.reboot",
    "org.freedesktop.login1.reboot-multiple-sessions",
    "org.freedesktop.login1.power-off",
    "org.freedesktop.login1.power-off-multiple-sessions",
    "org.freedesktop.login1.suspend",
    "org.freedesktop.login1.suspend-multiple-sessions",
    "org.freedesktop.login1.hibernate",
    "org.freedesktop.login1.halt",
    "org.freedesktop.login1.lock-sessions",
    "org.freedesktop.login1.set-self-linger",
    "org.freedesktop.login1.manage",
    "org.freedesktop.login1.reboot-ignore-inhibit",
    "org.freedesktop.systemd1.manage-units",
    "org.freedesktop.NetworkManager.network-control",
    "org.freedesktop.NetworkManager.wifi.scan",
    "org.freedesktop.NetworkManager.settings.modify.own",
    "org.freedesktop.NetworkManager.settings.modify.system",
    "org.freedesktop.udisks2.filesystem-mount",
    "org.freedesktop.udisks2.filesystem-mount-system",
    "org.freedesktop.udisks2.eject-media",
    "org.freedesktop.color-manager.create-device",
    "org.freedesktop.color-manager.install-system-wide",
    "net.hadess.PowerProfiles.switch-profile",
    "org.freedesktop.timedate1.set-timezone",
    "org.freedesktop.hostname1.set-static-hostname",
    "org.freedesktop.Accounts.user-administration",
    "org.freedesktop.policykit.exec",
]


def show(g, session, *props):
    out = g.sh("loginctl show-session {} {}".format(session, " ".join(f"-p {p}" for p in props)), 20)
    return dict(line.split("=", 1) for line in out.splitlines() if "=" in line)


def s10_logind(g, r, ev):
    uid_a = g.uids["alice"]
    g.ensure_pair(foreground="alice", background="bob")
    rows = g.sh("loginctl list-sessions --no-legend --no-pager", 20)
    ev.text("loginctl-list-sessions.txt", rows)
    ids = {}
    for line in rows.splitlines():
        parts = line.split()
        if len(parts) >= 6 and parts[5] in ("user", "manager"):
            ids.setdefault((parts[2], parts[5]), parts[0])
    facts = {}
    for user in ("alice", "bob"):
        sid = ids.get((user, "user"))
        if not sid:
            r.check(f"{user} has a logind session of class user", False, rows)
            continue
        props = show(g, sid, "Id", "Type", "Class", "Service", "Remote", "Active", "State", "Seat", "VTNr", "Desktop", "Scope", "Leader", "LockedHint", "IdleHint", "CanLock")
        facts[user] = props
        ev.json(f"loginctl-show-session-{user}.json", props)
        r.check(f"{user}: wayland, class user, created by the PAM service bsm-login, not remote", props.get("Type") == "wayland" and props.get("Class") == "user" and props.get("Service") == "bsm-login" and props.get("Remote") == "no", {k: props.get(k) for k in ("Type", "Class", "Service", "Remote")})
        r.check(f"{user}: active with no seat and no VT, desktop gnoblin", props.get("Active") == "yes" and props.get("State") == "active" and props.get("Seat", "") == "" and props.get("VTNr") == "0" and props.get("Desktop") == "gnoblin", {k: props.get(k) for k in ("Active", "State", "Seat", "VTNr", "Desktop")})
        ev.text(f"loginctl-show-user-{user}.txt", g.sh(f"loginctl show-user {user} --no-pager", 20))
        manager = ids.get((user, "manager"))
        r.check(f"{user}: logind also holds a session of class manager for the user manager", manager is not None, ids)
        running = g.sh(f"runuser -u {user} -- env XDG_RUNTIME_DIR=/run/user/{g.uids[user]} systemctl --user is-system-running 2>&1", 30).strip().splitlines()[-1]
        r.check(f"{user}: systemd --user is running", running in ("running", "degraded"), running)
    # LockedHint follows bsm's lock state.
    sid_a = ids.get(("alice", "user"))
    if sid_a:
        unlocked = show(g, sid_a, "LockedHint").get("LockedHint")
        g.lock("alice")
        locked = show(g, sid_a, "LockedHint").get("LockedHint")
        g.unlock("alice")
        again = show(g, sid_a, "LockedHint").get("LockedHint")
        r.check("LockedHint follows the lock: no, yes, no", (unlocked, locked, again) == ("no", "yes", "no"), (unlocked, locked, again))
        # logind's own Lock request.
        mark = g.mark()
        g.sh(f"loginctl lock-session {sid_a}", 20)
        reacted = g.wait_event(lambda e: e.get("event") == "lock_state_changed" and e.get("locked") is True, 12, mark)
        r.note("loginctl lock-session " + ("locked the hosted session through bsm" if reacted else "did not lock the hosted session: logind's Lock signal is not wired to bsm, the shell uses bsmctl lock instead"))
        if reacted:
            g.finished(mark, 60)
            g.unlock("alice")
    # polkit, for the processes of alice's hosted session.
    pid = g.gnoblin_compositor("alice")
    present = set(g.sh("pkaction 2>/dev/null", 30).split())
    table = {}
    for action in POLKIT_ACTIONS:
        if action not in present:
            table[action] = "not installed"
            continue
        out = g.sh(f"pkcheck --action-id {action} --process {pid} 2>&1; echo rc=$?", 30).strip().splitlines()
        rc = out[-1].replace("rc=", "") if out else "?"
        table[action] = {"0": "yes", "1": "no (authorisation required, not granted)", "2": "authentication required", "3": "dismissed"}.get(rc, f"rc {rc}")
    ev.json("polkit-for-alices-session.json", table)
    r.check("polkit grants a hosted session the power actions its bsm.rules name", all(table.get(a) in ("yes", "not installed") for a in ("org.freedesktop.login1.reboot", "org.freedesktop.login1.power-off", "org.freedesktop.login1.suspend")), {a: table[a] for a in POLKIT_ACTIONS[:6]})
    r.check("polkit still asks for authorisation for what bsm.rules does not name", all(table.get(a) != "yes" for a in ("org.freedesktop.login1.halt", "org.freedesktop.udisks2.filesystem-mount-system", "org.freedesktop.systemd1.manage-units", "org.freedesktop.hostname1.set-static-hostname", "org.freedesktop.Accounts.user-administration")), {a: table.get(a) for a in ("org.freedesktop.login1.halt", "org.freedesktop.udisks2.filesystem-mount-system", "org.freedesktop.systemd1.manage-units", "org.freedesktop.hostname1.set-static-hostname", "org.freedesktop.Accounts.user-administration")})
    create = g.script(
        f"""runuser -u alice -- busctl call org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager CreateSession 'uusssssussbssa(sv)' {uid_a} 0 bsm-login wayland user '' '' 0 '' '' false '' '' 0 2>&1 | head -n 2""",
        timeout=30,
    )
    r.check("an unprivileged process cannot create a logind session that claims the service name bsm-login", "Call failed" in create or "denied" in create.lower() or "Access" in create, create.strip()[:140])
    # Audio.
    pactl = g.sh(f"runuser -u alice -- env XDG_RUNTIME_DIR=/run/user/{uid_a} pactl info 2>&1 | head -n 12", 30)
    sinks = g.sh(f"runuser -u alice -- env XDG_RUNTIME_DIR=/run/user/{uid_a} pactl list short sinks 2>&1", 30)
    acl = g.sh("getfacl -p /dev/snd/controlC0 2>&1 | head -n 12; ls -l /dev/snd 2>&1 | head -n 12; cat /proc/asound/cards 2>&1", 30)
    ev.text("audio.txt", f"## pactl info (alice)\n{pactl}\n## sinks\n{sinks}\n## /dev/snd and ACL\n{acl}")
    r.check("PipeWire runs for the hosted session and answers alice's pactl", "PipeWire" in pactl, pactl.strip().splitlines()[-1] if pactl.strip() else "")
    if "HDA" in acl or "controlC0" in acl:
        r.check("with a sound card present, alice's PipeWire has a hardware sink (access through the audio group, not a seat ACL)", "alsa" in sinks.lower() and "user:alice" not in acl, sinks.strip()[:200])
    else:
        r.check("with a sound card present, alice's PipeWire has a hardware sink (access through the audio group, not a seat ACL)", None, "the VM has no sound card")
    inhibit = g.sh("systemd-inhibit --list --no-pager 2>&1", 30)
    ev.text("inhibitors.txt", inhibit)
    r.check("bsm holds a delay inhibitor for sleep", "bsm" in inhibit and "sleep" in inhibit, inhibit.strip().splitlines()[1:4])
    devices = g.sh("ls /dev/video* 2>&1; lsusb 2>&1 | head -n 5", 20)
    ev.text("camera-usb.txt", devices)
    r.check("camera and USB device access for a seatless session", None, "the VM has no camera or USB storage device to test with")
    ev.json("facts.json", {"sessions": facts, "polkit": table})
    g.snapshot(ev, "end")


# ---- scenario 8: unauthorised, inhibited, cancelled, refused and authorised power ---------------------------

POLKIT_DENY_RULE = "/etc/polkit-1/rules.d/00-bsm-test-deny.rules"


def power_events(rows):
    return [(w, e) for w, e in rows if e.get("event") == "transition" and e.get("kind") == "power_fade"]


def click(g, x, y, width=1280, height=800):
    """A left click on display 0 through QEMU's absolute pointer (QMP input-send-event)."""
    ax, ay = round(x / width * 32767), round(y / height * 32767)
    g.vm.qmp("input-send-event", {"events": [{"type": "abs", "data": {"axis": "x", "value": ax}}, {"type": "abs", "data": {"axis": "y", "value": ay}}]})
    time.sleep(0.4)
    g.vm.qmp("input-send-event", {"events": [{"type": "btn", "data": {"down": True, "button": "left"}}]})
    time.sleep(0.15)
    g.vm.qmp("input-send-event", {"events": [{"type": "btn", "data": {"down": False, "button": "left"}}]})


def guest_back(g, timeout=420):
    """After the guest reset itself: wait for the new boot's root shell and prepare it again."""
    vm = g.vm
    vm.log = b""
    time.sleep(2.0)
    if not vm.start_shell(timeout=timeout):
        raise Blocked("the guest did not come back to a root shell after the reset")
    vm.run("systemctl is-system-running --wait 2>&1 | head -1", 240)
    # A guest reset switches display head 1 off again; the second display needs its size handed over once more.
    resize_head(g, 1, 1280, 800)
    g.markers.clear()
    g.install_tools()
    g.prepare()


def s8_power(g, r, ev):
    uid_a, uid_b = g.uids["alice"], g.uids["bob"]
    g.ensure_pair(foreground="alice", background="bob")
    sa, sb = g.session("alice")["id"], g.session("bob")["id"]
    bsm = g.bsm_pid()
    t0 = time.monotonic()
    qmp_mark = len(g.vm._qmp._events)

    def reset_seen():
        return bool(g.vm.qmp_events("RESET", qmp_mark))

    def state():
        return {s["id"]: (s["state"], s["locked"], s["generation"]) for s in g.sessions()}

    # 8a: requests by accounts that may not make them. No fade, no state change, the desktop stays.
    before = state()
    mark = g.mark()
    start = time.monotonic()
    cases = [
        ("alice outside any session: reboot", "alice", False, "reboot"),
        ("alice outside any session: power off", "alice", False, "poweroff"),
        ("alice outside any session: suspend", "alice", False, "suspend"),
        ("bob outside any session: reboot", "bob", False, "reboot"),
        ("alice in her session: log out bob", "alice", True, f"logout {sb}"),
        ("bob in his session: log out alice", "bob", True, f"logout {sa}"),
        ("alice outside any session: log out bob", "alice", False, f"logout {sb}"),
    ]
    answers = {}
    for label, user, scope, arguments in cases:
        parsed, raw = g.ctl(arguments, user=user, scope=scope)
        answers[label] = raw
        r.check(f"{label}: denied as {ERROR_UNAUTHORISED}", error_code(parsed) == ERROR_UNAUTHORISED, raw[:140])
    settle(3.0)
    rows, _ = g.events(mark)
    ev.json("denials.json", answers)
    r.check("no fade started for any refused request", not power_events(rows), [e.get("phase") for _, e in power_events(rows)])
    r.check("no session changed and nothing is in flight", state() == before and g.status().get("operation") is None)
    steady(g, r, "alice's desktop stayed on both displays", "alice", start, time.monotonic())
    r.check("the guest did not reset", not reset_seen())

    # 8b: a blocking shutdown inhibitor refuses the request; the scene is untouched.
    g.script("nohup systemd-inhibit --what=shutdown --mode=block --who=bsm-security-test --why=held-by-the-test sleep 900 >/dev/null 2>&1 &", timeout=20)
    settle(2.0)
    mark = g.mark()
    start = time.monotonic()
    parsed, raw = g.ctl("reboot")
    done = g.finished(mark, 60)
    settle(4.0)
    g.sh("pkill -f 'sleep 900'", 10)
    r.check("a reboot with a blocking inhibitor finishes as a failure named inhibited", done is not None and done[1].get("success") is False and done[1].get("reason") == "inhibited", done[1] if done else raw)
    steady(g, r, "alice's desktop is on screen after the inhibited request", "alice", time.monotonic() - 3.0, time.monotonic())
    r.check("no black frame during the inhibited request", not g.sampler.black(start, time.monotonic()), f"{len(g.sampler.black(start, time.monotonic()))} black frames")

    # 8c: a request cancelled during the cover fade; the desktop comes back.
    mark = g.mark()
    start = time.monotonic()
    parsed, raw = g.ctl("reboot")
    operation = parsed.get("operation_id") if isinstance(parsed, dict) else None
    anim = g.wait_event(lambda e: e.get("event") == "transition" and e.get("kind") == "power_fade" and e.get("phase") == "animating", 60, mark, poll=0.5)
    r.check("the reboot request started the cover fade", anim is not None, raw)
    settle(1.0)
    g.ctl(f"cancel {operation}")
    done = g.wait_event(lambda e: e.get("event") == "operation_finished" and e.get("operation_id") == operation, 60, mark)
    settle(8.0)
    r.check("the cancelled request finished as cancelled", done is not None and done[1].get("success") is False and done[1].get("reason") == "cancelled", done[1] if done else None)
    shows(g, r, "the desktop is back on both displays after the cancel (no permanent cover or black)", "alice", time.monotonic() - 4.0, time.monotonic())
    r.check("no black frame from the request to the restored desktop", not g.sampler.black(start, time.monotonic()), f"{len(g.sampler.black(start, time.monotonic()))} black frames")
    r.check("the guest did not reset", not reset_seen())

    # 8d: logind refuses after bsm authorised and faded to the cover (polkit denies the bsm account).
    g.script(f"""cat > {POLKIT_DENY_RULE} <<'EOF'
polkit.addRule(function(action, subject) {{
    if (subject.user == "bsm" && action.id.indexOf("org.freedesktop.login1.") == 0) {{ return polkit.Result.NO; }}
}});
EOF
chmod 644 {POLKIT_DENY_RULE}
""", timeout=20)
    settle(3.0)
    mark = g.mark()
    start = time.monotonic()
    try:
        parsed, raw = g.ctl("reboot")
        anim = g.wait_event(lambda e: e.get("event") == "transition" and e.get("kind") == "power_fade" and e.get("phase") == "animating", 60, mark, poll=0.5)
        done = g.finished(mark, 90)
        settle(8.0)
    finally:
        g.sh(f"rm -f {POLKIT_DENY_RULE}", 10)
    r.check("the cover faded in before logind was asked", anim is not None)
    r.check("logind's refusal ends the operation as a failure with a stable code", done is not None and done[1].get("success") is False and done[1].get("reason") in ("shutdown_rejected", "logind_failure"), done[1] if done else raw)
    shows(g, r, "the desktop is back on both displays after logind refused", "alice", time.monotonic() - 4.0, time.monotonic())
    r.check("no black frame from the refused request to the restored desktop", not g.sampler.black(start, time.monotonic()), f"{len(g.sampler.black(start, time.monotonic()))} black frames")
    r.check("the guest did not reset", not reset_seen())
    r.check("bsm kept its pid through every refused or cancelled request", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")
    g.snapshot(ev, "before-reboot")

    # 8e: a hosted session asks to reboot while another account is signed in. First the form the shell uses
    # (one shot: bsmctl exits at once), then with --wait. Policy decides; a standard user must not get an
    # unattended reboot over another account's session.
    mark = g.mark()
    epoch = g.epoch()
    parsed, raw = g.ctl("reboot", user="alice", scope=True)
    done = g.finished(mark, 40)
    ev.text("hosted-reboot-oneshot.txt", f"{raw}\n{done}")
    r.note(f"hosted session, one-shot `bsmctl reboot --json` (the shell's form): {raw[:120]}; operation ended as {done[1] if done else None}")
    text = g.journal(ev, "journal-hosted-reboot-oneshot.txt", epoch, units=("bsm",), lines=60)
    oneshot_cause = "NameHasNoOwner" if "NameHasNoOwner" in text else ""
    if oneshot_cause:
        r.finding(
            "a one-shot `bsmctl reboot` from a hosted session (the form the shell's BsmSession.qml runs) is always refused: bsm asks polkit about the caller's bus name "
            "after bsmctl has exited, polkit answers NameHasNoOwner and bsm fails closed as unauthorised; the shell's Restart and Shut Down therefore never work from a hosted session"
        )
    settle(2.0)
    hosted_result = f"{GUEST_STATE_DIR}/hosted-reboot.json"
    g.script(
        f"""nohup python3 {GUEST_PROBE} scope-exec --scope-of alice --user alice -- bsmctl reboot --wait --json > {hosted_result} 2>&1 &""",
        timeout=20,
    )
    reset = g.vm.wait_reset(qmp_mark, 150)
    if reset:
        r.check("a standard user cannot reboot the machine while another account is signed in", False, "the hosted session's `bsmctl reboot --wait` was authorised by polkit and the guest reset")
        r.finding(
            "polkit lets a hosted session reboot the machine while another account is signed in: bsm.rules answers YES for the -multiple-sessions actions, "
            "so any standard user's `bsmctl reboot --wait` reboots over locked sessions of other users without an administrator"
        )
    else:
        text = g.sh(f"cat {hosted_result}", 15).strip()
        r.check("a standard user cannot reboot the machine while another account is signed in", True, text[:140])
    rebooted = reset
    if rebooted:
        guest_back(g)
    # 8f: the authorised path as root: the cover on both displays, then the machine actually resets.
    if not rebooted:
        qmp_mark = len(g.vm._qmp._events)
        mark = g.mark()
        parsed, raw = g.ctl("reboot")
        anim = g.wait_event(lambda e: e.get("event") == "transition" and e.get("kind") == "power_fade" and e.get("phase") == "animating", 60, mark, poll=0.5)
        r.check("root's reboot starts the cover fade", anim is not None, raw)
        t_anim = anim[0] if anim else time.monotonic()
        settle(5.0)
        for head in HEADS:
            window = [s for s in g.sampler.window(t_anim, t_anim + 5.0, head) if s.mean >= 1.0]
            covers = [s for s in window if 8.0 <= s.mean <= 30.0 and s.p99 < 40.0]
            r.check(f"display {head}: the cover is on screen before the machine resets", bool(covers), f"{len(covers)} cover frames of {len(window)}")
            r.check(f"display {head}: no desktop marker during the reboot fade's last seconds", not g.sampler.leaks(t_anim + 3.0, t_anim + 5.0, "alice", head) and not g.sampler.leaks(t_anim + 3.0, t_anim + 5.0, "bob", head))
        reset = g.vm.wait_reset(qmp_mark, 150)
        r.check("the VM actually resets (QEMU reports a guest reset)", reset)
        if reset:
            guest_back(g)
    else:
        r.note("root's reboot path was not repeated: the hosted session's reboot already reset the guest")
        qmp_mark = len(g.vm._qmp._events)
    ev.json("qmp-events.json", [e["event"] for e in g.vm._qmp._events])

    # 8g: the greeter's own power menu with a policy that refuses: bsm answers unauthorised and nothing happens.
    g.script(
        """printf '\\n[greeter_power]\\nsuspend = false\\nreboot = false\\npoweroff = false\\n' >> /etc/bsm/bsm.toml
systemctl restart bsm
""",
        timeout=120,
    )
    if not g.wait_greeter(120):
        raise Blocked("the greeter did not come back after the policy change")
    g.start_monitor()
    settle(5.0)
    qmp_mark = len(g.vm._qmp._events)
    mark = g.mark()
    start = time.monotonic()
    click(g, 1235, 764)
    settle(2.0)
    ev.png("greeter-power-menu")
    g.vm.key("down")
    settle(0.5)
    g.vm.key("ret")
    settle(1.5)
    g.vm.key("ret")
    done = g.wait_event(lambda e: e.get("event") == "operation_finished", 40, mark)
    settle(6.0)
    ev.png("greeter-power-refused")
    rows, _ = g.events(mark)
    ev.json("greeter-power-events.json", [e for _, e in rows])
    r.check("the greeter's Restart is refused by the greeter_power policy with a stable code", done is not None and done[1].get("success") is False and done[1].get("reason") == ERROR_UNAUTHORISED, done[1] if done else "no operation finished; did the click reach the power button?")
    r.check("the greeter is still on screen and the guest did not reset", not reset_seen() and not g.sampler.black(start, time.monotonic()))
    g.sh("sed -i '/^\\[greeter_power\\]/,$d' /etc/bsm/bsm.toml; systemctl restart bsm", 120)
    g.wait_greeter(120)
    ev.frames("frames.csv", t0, time.monotonic(), t0)
    g.snapshot(ev, "end")


# ---- scenario 6: DPMS and suspend (runs in a second VM booted through SeaBIOS) -----------------------------


UEFI_S3 = {}


def dpms_state(g):
    out = g.script("for f in /sys/class/drm/card*-*/dpms; do echo $f $(cat $f); done", timeout=20)
    return {line.split()[0].split("/")[-2]: line.split()[1] for line in out.splitlines() if len(line.split()) == 2}


def uefi_s3_probe(g, out_dir):
    """The last act of the UEFI VM: suspend it and record that it never resumes (edk2 OVMF hangs in its S3 resume path)."""
    result = {}
    qmp_mark = len(g.vm._qmp._events)
    g.script("nohup sh -c 'sleep 3; systemctl suspend' >/dev/null 2>&1 &", timeout=20)
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline and g.vm.run_state() != "suspended":
        time.sleep(1.0)
    result["reached_suspended"] = g.vm.run_state() == "suspended"
    time.sleep(5.0)
    g.vm.qmp("system_wakeup")
    time.sleep(20.0)
    result["state_after_wakeup"] = g.vm.run_state()
    rips = []
    for _ in range(4):
        text = g.vm.monitor("info registers")
        match = re.search(r"RIP=([0-9a-f]+)", text)
        rips.append(match.group(1) if match else None)
        time.sleep(1.0)
    result["rip_samples"] = rips
    probe = g.vm.run("echo alive-after-resume", 15)
    result["guest_answers_on_serial"] = "alive-after-resume" in probe
    result["firmware"] = "edk2 OVMF (UEFI)"
    result["qmp_events"] = [e["event"] for e in g.vm._qmp._events[qmp_mark:]]
    (out_dir / "uefi-s3.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def s6_suspend(g, r, ev):
    g.ensure_foreground("alice")
    g.ensure_marker("alice")
    bsm = g.bsm_pid()
    pid = g.gnoblin_compositor("alice")
    sid = g.session("alice")["id"]
    dpms_before = dpms_state(g)
    ev.json("dpms-before.json", dpms_before)
    r.check("both connectors report DPMS On before the sleep", len(dpms_before) >= 2 and all(v.lower() == "on" for v in dpms_before.values()), dpms_before)
    r.check("alice's desktop is on both displays before the sleep", bool(g.sampler.visible(time.monotonic() - 5.0, time.monotonic(), "alice", 0)) and bool(g.sampler.visible(time.monotonic() - 5.0, time.monotonic(), "alice", 1)))
    qmp_mark = len(g.vm._qmp._events)
    t_start = time.monotonic()
    epoch0 = g.epoch()
    g.script("nohup sh -c 'sleep 2; systemctl suspend' >/dev/null 2>&1 &", timeout=20)
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline and g.vm.run_state() != "suspended":
        time.sleep(0.5)
    t_susp = time.monotonic()
    suspended = g.vm.run_state() == "suspended"
    r.check("the guest reached ACPI S3 (QEMU reports the machine suspended)", suspended, g.vm.run_state())
    if not suspended:
        raise Blocked("the guest never reached S3, so resume cannot be tested")
    time.sleep(6.0)
    ev.png("while-suspended")
    last = g.sampler.last_leak_time(t_start, t_susp, "alice")
    r.check("alice's desktop was off both displays before the guest froze (bsm protected first, then released the sleep inhibitor)", last is not None and last < t_susp - 0.2 or last is None, "never shown" if last is None else f"last shown {t_susp - last:.1f} s before the suspend")
    g.vm.qmp("system_wakeup")
    t_wake = time.monotonic()
    alive = g.wait_until(lambda: "ok-resumed" in g.vm.run("echo ok-resumed", 8), 180, poll=2.0)
    r.check("the guest resumed from S3 and answers on its console", bool(alive), g.vm.run_state())
    if not alive:
        raise Blocked("the guest did not come back from S3")
    g.refresh_clock()
    r.check("QEMU delivered the SUSPEND and WAKEUP events", [e["event"] for e in g.vm._qmp._events[qmp_mark:] if e["event"] in ("SUSPEND", "WAKEUP")] == ["SUSPEND", "WAKEUP"], [e["event"] for e in g.vm._qmp._events[qmp_mark:]])
    back = g.wait_until(lambda: g.status().get("greeter_connected"), 120, poll=3.0)
    settle(8.0)
    t_after = time.monotonic()
    r.check("bsm survived the sleep with the same pid and the greeter is connected", g.bsm_pid() == bsm and bool(back), f"{bsm} -> {g.bsm_pid()}")
    entry = g.session("alice")
    r.check("alice's session is locked after the resume and is the same session", entry is not None and entry.get("locked") and entry.get("id") == sid, str(entry))
    locked_hint = show(g, g.sh("loginctl list-sessions --no-legend --no-pager | awk '$3==\"alice\" && $6==\"user\" {print $1; exit}'", 20).strip(), "LockedHint").get("LockedHint")
    r.check("logind's LockedHint of her session is yes after the resume", locked_hint == "yes", locked_hint)
    hidden(g, r, "no desktop pixel on either display from the resume until the password is typed", t_wake, t_after)
    last_frames = [g.sampler.latest(h) for h in HEADS]
    r.check("the greeter is what is on screen after the resume (not black)", all(s is not None and not s.black for s in last_frames), [None if s is None else f"mean {s.mean:.0f}" for s in last_frames])
    # Authentication is required: keys that are not the password change nothing.
    g.vm.type_text("xyz")
    g.vm.key("ret")
    settle(6.0)
    entry = g.session("alice")
    r.check("a wrong password after the resume does not unlock", entry is not None and entry.get("locked"), str(entry))
    hidden(g, r, "still no desktop pixel after a wrong password", t_after, time.monotonic())
    dpms_after = dpms_state(g)
    ev.json("dpms-after.json", dpms_after)
    r.check("both connectors report DPMS On after the resume (bsm sends SetDpms on at resume)", len(dpms_after) >= 2 and all(v.lower() == "on" for v in dpms_after.values()), dpms_after)
    off = g.sh("command -v modetest >/dev/null && modetest -M virtio_gpu -c 2>&1 | grep -m 3 -B1 -A2 DPMS", 30)
    ev.text("modetest-dpms.txt", off)
    r.check("DPMS off: bsm never issues SetDpms off and no other process can blank a connector bsm masters", None, "blocked: the manager only ever sends SetDpms{on: true} (at resume); there is no idle or user trigger for DPMS off to test")
    start = time.monotonic()
    g.unlock("alice")
    settle(3.0)
    shows(g, r, "the correct password unlocks alice: her desktop is on both displays", "alice", start, time.monotonic())
    r.check("alice's Gnoblin is the same process after the sleep and the unlock", g.gnoblin_compositor("alice") == pid, f"{pid} -> {g.gnoblin_compositor('alice')}")
    r.check("bsm kept its pid", g.bsm_pid() == bsm)
    g.journal(ev, "journal-suspend.txt", epoch0, units=("bsm", "bsm-helper", "systemd-logind", "systemd-suspend"), lines=200)
    ev.frames("frames.csv", t_start, time.monotonic(), t_start)
    ev.leak_pngs(t_start)
    g.snapshot(ev, "end")
    if UEFI_S3:
        r.check("suspend under UEFI (edk2 OVMF): S3 resume hangs in the firmware, so the UEFI VM cannot resume", None, UEFI_S3)
        r.finding(
            "ACPI S3 resume never completes under the host's edk2 OVMF (QEMU 10.1, one and four vCPUs): after system_wakeup the vCPU spins at one RIP inside the PEI phase. "
            "SeaBIOS resumes, so the suspend scenario runs in a second VM booted through SeaBIOS and the syslinux menu"
        )


# ---- scenario 4: authentication failures --------------------------------------------------------


def s4_auth(g, r, ev):
    uid = g.uids
    if not g.wait_greeter(60):
        raise Blocked("no greeter to authenticate at")
    g.logout_all()
    bsm = g.bsm_pid()
    t0, epoch0 = time.monotonic(), g.epoch()
    sessions_before = {s["id"] for s in g.sessions()}

    # 4a: the PAM child dies in the middle of a conversation.
    g.begin_auth("alice")
    g.reset_faillock()
    child = g.pids("bsm-auth")
    mark = g.mark()
    g.kill(child)
    outcome = g.wait_event(lambda e: e.get("event") == "operation_finished", 25, mark)
    r.check("killing the PAM child ends the conversation visibly with a stable code", outcome is not None and outcome[1].get("success") is False and outcome[1].get("reason") == "helper_failure", outcome[1] if outcome else "no operation_finished")
    recovered = g.wait_until(lambda: [p for p in g.pids("bsm-auth") if p not in child], 40, poll=1.0)
    r.check("the greeter opens a new conversation by itself", bool(recovered), f"new auth child {recovered}")
    r.check("no session was created by the dead conversation", {s["id"] for s in g.sessions()} == sessions_before)
    r.check("an aborted PAM child leaves no pam_faillock record", faillock_count(g, "alice") == 0, f"{faillock_count(g, 'alice')} records for alice")
    g.snapshot(ev, "after-auth-child-kill")

    # 4b: the helper itself restarts in the middle of a conversation.
    g.begin_auth("alice")
    old_greeter = g.greeter_pids()
    mark = g.mark()
    g.sh("systemctl restart bsm-helper", 60)
    back = g.wait_until(lambda: g.status().get("greeter_connected") and g.status().get("helper_connected") and [p for p in g.greeter_pids() if p not in old_greeter], 90, poll=2.0)
    r.check("after a helper restart a new greeter connects", bool(back), f"greeter pids {old_greeter} -> {g.greeter_pids()}")
    r.check("the helper restart revealed no session", {s["id"] for s in g.sessions()} == sessions_before and g.foreground_unlocked() is None)
    r.check("bsm kept its pid through the helper restart", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")

    # 4c: the account is switched in the middle of typing a password. "bo" + "b" is bob's password.
    g.begin_auth("alice")
    g.reset_faillock()
    g.vm.type_text("bo")
    before = set(g.pids("bsm-auth"))
    g.ctl(f"select-user {uid['bob']}")
    g.wait_until(lambda: g.status().get("selected_uid") == uid["bob"], 20, poll=1.0)
    opened = g.conversation_open(before, 45)
    counts_after_switch = {name: faillock_count(g, name) for name in PASSWORDS}
    g.vm.type_text("b")
    g.vm.key("ret")
    settle(8.0)
    r.check("a secret typed for one account is not carried over to the next (no session for bob)", g.session("bob") is None and g.session("alice") is None, str(g.sessions()))
    r.check("the second account's conversation did open", opened)
    r.check("the stray keystroke did reach bob's PAM conversation and was refused (pam_faillock recorded it)", faillock_count(g, "bob") >= 1, f"{faillock_count(g, 'bob')} records for bob")
    r.check("switching accounts mid-authentication is not a failed login of the first account", counts_after_switch["alice"] == 0, f"pam_faillock records after the switch: {counts_after_switch}")
    # Alice's own password typed while bob is selected authorises nobody.
    before = set(g.pids("bsm-auth"))
    g.vm.type_text(PASSWORDS["alice"])
    g.vm.key("ret")
    settle(8.0)
    r.check("alice's password does not sign bob in, and bob's prompt does not sign alice in", g.session("bob") is None and g.session("alice") is None, str(g.sessions()))
    r.check("bob stays the selected account", g.status().get("selected_uid") == uid["bob"])
    g.journal(ev, "journal-wrong-account.txt", epoch0, units=("bsm", "bsm-helper"))
    ev.png("greeter-after-wrong-account")

    # 4d: the prompt is left unanswered until the helper's own prompt timeout (60 s).
    g.begin_auth("alice")
    g.reset_faillock()
    settle(75.0)
    unanswered = faillock_count(g, "alice")
    r.check("an unanswered prompt that times out is not a failed login (idle lock screen must not lock the account)", unanswered == 0, f"{unanswered} pam_faillock records for alice after 75 s")
    if unanswered:
        r.finding(
            "an idle or aborted PAM conversation is recorded by pam_faillock as a failed login: after three idle minutes at the greeter, or three account switches, "
            "the account is locked out for the faillock unlock time (default 10 minutes) although nobody typed a wrong password"
        )

    # 4e: a fresh login works after all of it (faillock reset: the lockout above is that finding, not this check).
    locked_out = faillock_count(g, "alice")
    g.reset_faillock()
    entry_before = g.session("alice")
    g.login("alice")
    entry = g.session("alice")
    r.check("a fresh login of alice works afterwards", entry is not None and entry.get("foreground") and entry.get("uid") == uid["alice"] and entry_before is None, str(entry))
    r.check("bob was never signed in by the failed attempts", g.session("bob") is None)
    r.check("bsm kept its pid through every step", g.bsm_pid() == bsm, f"{bsm} -> {g.bsm_pid()}")
    owner = g.sh(f"ps -o user= -p {g.gnoblin_compositor('alice') or 1}", 10).strip().splitlines()[-1]
    r.check("the new hosted Gnoblin runs as alice", owner == "alice", owner)
    ev.json("summary.json", {"faillock_records_for_alice_before_final_login": locked_out, "seconds": round(time.monotonic() - t0, 1)})
    g.snapshot(ev, "end")


# ---- running ------------------------------------------------------------------------------------

SCENARIOS = {
    1: ("isolation", "an unprivileged account cannot reach, impersonate or read another account or bsm", s1_isolation),
    2: ("greeter", "the greeter dies while a session is locked", s2_greeter),
    3: ("gnoblin", "a hosted Gnoblin dies, foreground or locked in the background", s3_gnoblin),
    5: ("outputs", "an output changes during a lock, a fade and an unlock", s5_outputs),
    7: ("capture", "portal, screencast, remote desktop and DRM access across accounts", s7_capture),
    9: ("drm", "no DRM reset, no VT change and no black frame over the normal lifecycle", s9_drm),
    10: ("logind", "logind, polkit, audio and systemd --user for seatless hosted sessions", s10_logind),
    6: ("suspend", "DPMS and S3 suspend and resume preserve the lock", s6_suspend),
    8: ("power", "unauthorised, inhibited, cancelled, refused and authorised power requests", s8_power),
    4: ("auth", "PAM child killed, helper restarted, account switched mid-authentication", s4_auth),
}


def run_scenario(g, number, out_root):
    number = int(str(number).lstrip("s"))
    name, title, function = SCENARIOS[number]
    ev = Evidence(out_root, f"s{number:02d}-{name}", g.vm, g.sampler)
    r = Result(number, name, title)
    print(f"\n== S{number} {name}: {title}", flush=True)
    started = time.monotonic()
    try:
        function(g, r, ev)
    except Blocked as reason:
        r.blocked_reason = str(reason)
        print(f"BLOCKED S{number}: {reason}", flush=True)
    except Exception as error:  # noqa: BLE001 - a crashed scenario is a failed scenario with its message
        import traceback

        r.crashed = f"{type(error).__name__}: {error}"
        print(traceback.format_exc(), flush=True)
        print(f"CRASH S{number}: {r.crashed}", flush=True)
    if r.crashed or r.blocked_reason:
        try:
            g.snapshot(ev, "stopped")
            ev.png("stopped")
        except Exception as error:  # noqa: BLE001
            print(f"no state dump after the stop: {error}", flush=True)
    r.seconds = time.monotonic() - started
    r.evidence = ev.files
    ev.json("result.json", r.as_dict())
    return r


# ---- the run -------------------------------------------------------------------------------------------

UEFI_ORDER = [4, 1, 7, 10, 2, 5, 3, 9, 8]  # signs in at the greeter first, ends with the scenario that resets the guest
BIOS_ORDER = [6]  # suspend needs SeaBIOS: edk2 OVMF hangs on S3 resume


def boot(base, iso, *, bios, inject, out, label):
    work = Path(tempfile.mkdtemp(prefix=f"bsm-sec-{label}-", dir=base))
    vm = SecVm(work, iso=iso, record=work / "record", bios=bios, inject=None if bios else inject)
    vm.start()
    if not vm.start_shell(timeout=420):
        vm.stop()
        raise Blocked(f"no serial shell in the {label} VM")
    vm.run("systemctl is-system-running --wait 2>&1 | head -1", 240)
    if inject and not bios:
        applied = vm.run(
            "mkdir -p /mnt/bsm-inject && modprobe 9pnet_virtio; modprobe 9p; mount -t 9p -o trans=virtio,version=9p2000.L,ro bsminject /mnt/bsm-inject "
            "&& sh /mnt/bsm-inject/apply.sh 2>&1 | tail -n 30", 600,
        )
        (out / f"inject-{label}.txt").write_text(applied)
        time.sleep(8)
    sampler = MarkerSampler(vm.head_sources())
    sampler.start()
    guest = Guest(vm, sampler, out)
    guest.install_tools()
    guest.prepare()
    return vm, sampler, guest, work


def shutdown(vm, sampler, work, keep, out, label):
    sampler.stop()
    vm.stop()
    video = work / "record" / "flow.mp4"
    if video.is_file() and video.stat().st_size < 30_000_000:
        shutil.copy(video, out / f"flow-{label}.mp4")
    if not keep:
        shutil.rmtree(work, ignore_errors=True)


def environment(iso):
    import subprocess

    def run(*command):
        try:
            return subprocess.run(command, capture_output=True, text=True, timeout=20).stdout.strip().splitlines()[0]
        except Exception:  # noqa: BLE001
            return "unknown"

    return {"iso": iso.name, "date": time.strftime("%Y-%m-%d %H:%M:%S"), "qemu": run("qemu-system-x86_64", "--version"), "harness_commit": run("git", "-C", str(ROOT), "rev-parse", "--short", "HEAD")}


def write_report(out, results, env):
    rows = ["| # | Scenario | Status | Checks (ok / fail / blocked) | Evidence |", "| - | -------- | ------ | ---------------------------- | -------- |"]
    details = []
    for number in sorted(SCENARIOS):
        name, title, _ = SCENARIOS[number]
        result = results.get(number)
        if result is None:
            rows.append(f"| {number} | {title} | not run | | |")
            continue
        checks = result.checks
        counts = (sum(1 for c in checks if c["ok"] is True), sum(1 for c in checks if c["ok"] is False), sum(1 for c in checks if c["ok"] is None))
        rows.append(f"| {number} | {title} | **{result.status}** | {counts[0]} / {counts[1]} / {counts[2]} | `s{number:02d}-{name}/` |")
        lines = [f"### {number}. {title}: {result.status}"]
        if result.crashed:
            lines.append(f"- crashed: {result.crashed}")
        if result.blocked_reason:
            lines.append(f"- blocked: {result.blocked_reason}")
        lines += [f"- FAIL: {c['label']} ({c['detail'][:200]})" for c in checks if c["ok"] is False]
        lines += [f"- blocked: {c['label']} ({c['detail'][:200]})" for c in checks if c["ok"] is None]
        lines += [f"- finding: {f}" for f in result.findings]
        lines += [f"- note: {n}" for n in result.notes]
        details.append("\n".join(lines))
    text = "# bsm security and failure scenarios\n\n" + "\n".join(f"- {k}: {v}" for k, v in env.items()) + "\n\n" + "\n".join(rows) + "\n\n" + "\n\n".join(details) + "\n"
    (out / "report.md").write_text(text)
    (out / "results.json").write_text(json.dumps({"environment": env, "scenarios": [results[n].as_dict() for n in sorted(results)]}, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--iso")
    parser.add_argument("--inject", help="directory staged by tests/bsm-vm-inject.py")
    parser.add_argument("--out", default=os.environ.get("BINGUX_DISTRO_OUT_SECURITY", str(DEFAULT_OUT)))
    parser.add_argument("--only", help="comma separated scenario numbers, for example 1,4")
    parser.add_argument("--skip-bios", action="store_true", help="do not run the suspend scenario's second VM")
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
    wanted = {int(n) for n in args.only.split(",")} if args.only else set(SCENARIOS)
    base = Path(os.environ.get("BSM_VM_DIR", "/tmp/bsm-vm"))
    base.mkdir(parents=True, exist_ok=True)
    results = {}
    env = environment(iso)

    def run_phase(order, bios, label):
        todo = [n for n in order if n in wanted]
        if not todo:
            return
        vm = sampler = work = None
        try:
            vm, sampler, guest, work = boot(base, iso, bios=bios, inject=args.inject, out=out, label=label)
            for number in todo:
                results[number] = run_scenario(guest, number, out)
            if not bios and 6 in wanted and not args.skip_bios:
                # Last act of the UEFI VM: record that it cannot resume from S3 (it is not used afterwards).
                try:
                    (out / "s06-suspend").mkdir(exist_ok=True)
                    UEFI_S3.update(uefi_s3_probe(guest, out / "s06-suspend"))
                except Exception as error:  # noqa: BLE001
                    print(f"UEFI S3 probe failed: {error}", flush=True)
        except Blocked as reason:
            for number in todo:
                if number not in results:
                    blocked = Result(number, SCENARIOS[number][0], SCENARIOS[number][1])
                    blocked.blocked_reason = f"the {label} VM could not be prepared: {reason}"
                    results[number] = blocked
        finally:
            if vm is not None:
                shutdown(vm, sampler, work, args.keep_work, out, label)

    run_phase(UEFI_ORDER, False, "uefi")
    if not args.skip_bios:
        run_phase(BIOS_ORDER, True, "bios")
    write_report(out, results, env)
    print()
    for number in sorted(results):
        print(f"{results[number].status:8} S{number} {SCENARIOS[number][0]}")
    print("report:", out / "report.md")
    statuses = {r.status for r in results.values()}
    return 1 if "fail" in statuses else (2 if "blocked" in statuses else 0)


if __name__ == "__main__":
    sys.exit(main())
