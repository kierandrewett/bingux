"""Host-side library of tests/bsm-qemu-security.py: the VM, guest control, evidence and results.

Built on tests/bsm_harness.py (`BsmVm`: two display heads, serial root shell, QEMU monitor
and QMP, VNC recorders) and tests/bsm_security_frames.py (marker frames). What this adds:

- `SecVm`: a BsmVm that can sleep (ACPI S3 is hidden by QEMU unless asked for) and can boot
  the image through SeaBIOS and its syslinux menu, because the UEFI firmware of this host
  (edk2 OVMF) hangs on S3 resume.
- `Guest`: everything a scenario does inside the VM. Real flows only: accounts sign in by
  typing their password at the greeter with QEMU key presses, sessions are locked and
  unlocked through bsmctl and the greeter, and state is read from bsmctl, logind, /proc and
  the journal. Nothing mocks a privileged path.
- `Evidence` and `Result`: what a scenario records, and the pass / fail / blocked verdict.

Nothing here knows what a scenario expects; tests/bsm-qemu-security.py does.
"""

from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import re
import shlex
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from bsm_harness import HEAD_COUNT, MONITOR_LOG, BsmVm  # noqa: E402
from bsm_security_frames import MARKER_COLOURS, MarkerSampler  # noqa: E402

GUEST_PROBE_SOURCE = Path(__file__).with_name("bsm_guest_probe.py")
GUEST_PROBE = "/tmp/bsm_guest_probe.py"
GUEST_BRIDGE_SHIM = "/usr/local/bin/bsm-test-bridge"
GUEST_STATE_DIR = "/run/bsm-test"
GREETER_USER = "bingux-greeter"

# Ordinary accounts: audio and video like every desktop user, nothing else (no wheel, no input).
PASSWORDS = {"alice": "alice", "bob": "bob"}
SLOW_TRANSITIONS = {"new_login_ms": 4000, "unlock_ms": 4000, "lock_ms": 3000, "logout_ms": 4000, "power_ms": 4000}

# The bridge of a hosted session starts through this shim, so a scenario can hold a login open
# at the point where the brokered socket exists but the bridge has not connected: the file
# /run/bsm-test/bridge-delay holds the seconds to wait. No delay file, no delay.
BRIDGE_SHIM = """#!/bin/sh
# Test shim installed by tests/bsm-qemu-security.py: delay the bridge, then run the real one.
if [ -r /run/bsm-test/bridge-delay ]; then
    sleep "$(cat /run/bsm-test/bridge-delay)"
fi
exec /usr/libexec/bsm/bsm-host "$@"
"""


class Blocked(Exception):
    """A scenario (or a check) cannot run: a prerequisite is missing. Not a pass, not a failure."""


# ---- the VM -------------------------------------------------------------------------------

KEYS_FOR = {" ": "spc", "_": "shift-minus", "=": "equal", "-": "minus", ".": "dot"}


class SecVm(BsmVm):
    """BsmVm with ACPI S3 switched on, optionally booted through SeaBIOS and syslinux."""

    def __init__(self, work, *, iso, record, bios=False, inject=None, **kwargs):
        super().__init__(work, iso=iso, record=record, inject=inject, uefi=not bios, bsm_param=not bios, **kwargs)
        self.bios = bios

    def display_args(self):
        args = super().display_args()
        # QEMU hides the S3 sleep state on q35 by default; S4 stays off (no disk to hibernate to).
        return args + ["-global", "ICH9-LPC.disable_s3=0", "-global", "ICH9-LPC.disable_s4=1"]

    def start(self):
        super().start()
        if self.bios:
            self.choose_bsm_menu_entry()

    def type_raw(self, text):
        for char in text:
            if char.isupper():
                self.key(f"shift-{char.lower()}")
            else:
                self.key(KEYS_FOR.get(char, char))
            time.sleep(0.05)

    def choose_bsm_menu_entry(self):
        """syslinux menu: `b` is the hotkey of the bsm entry; Tab edits its command line, to give the overlay room."""
        deadline = time.monotonic() + 9.0
        while time.monotonic() < deadline:
            self.key("b")
            time.sleep(0.4)
        self.key("tab")
        time.sleep(0.8)
        self.type_raw(" cow_spacesize=2G")
        time.sleep(0.3)
        self.key("ret")

    # QEMU facts the scenarios use.

    def qmp(self, command, arguments=None, timeout=30):
        return self._qmp.execute(command, arguments, timeout)

    def run_state(self):
        return self.qmp("query-status")["status"]

    def qmp_events(self, name, since=0):
        with self._qmp._changed:
            return [e for e in self._qmp._events[since:] if e["event"] == name]


# ---- evidence and results ----------------------------------------------------------------


class Evidence:
    """Files of one scenario, below distro/out/bsm-test/security/<scenario>/."""

    def __init__(self, root, name, vm, sampler):
        self.dir = Path(root) / name
        self.dir.mkdir(parents=True, exist_ok=True)
        self.name = name
        self.vm = vm
        self.sampler = sampler
        self.files = []

    def _note(self, filename):
        relative = f"{self.name}/{filename}"
        if relative not in self.files:
            self.files.append(relative)
        return self.dir / filename

    def text(self, filename, content):
        self._note(filename).write_text(content if content.endswith("\n") else content + "\n")

    def json(self, filename, value):
        self._note(filename).write_text(json.dumps(value, indent=2, sort_keys=True, default=str) + "\n")

    def png(self, label):
        """The newest frame of every display."""
        from PIL import Image

        for head, source in self.vm.head_sources().items():
            frame = source()
            if frame is not None:
                Image.fromarray(frame).save(self._note(f"{label}-display{head}.png"))

    def frames(self, filename, start, end, base):
        self.sampler.save_csv(self._note(filename), start, end, base)

    def leak_pngs(self, since, prefix="leak"):
        """The full frames the sampler kept the first time a marker appeared after `since`."""
        from PIL import Image

        written = 0
        for t, head, name, frame in list(self.sampler.leak_frames):
            if t >= since and written < 4:
                Image.fromarray(frame).save(self._note(f"{prefix}-{name}-display{head}-{written}.png"))
                written += 1
        return written


class Result:
    """Checks of one scenario. A check is True, False or None (blocked: it could not be made)."""

    def __init__(self, number, name, title):
        self.number = number
        self.name = name
        self.title = title
        self.checks = []
        self.notes = []
        self.findings = []
        self.evidence = []
        self.blocked_reason = None
        self.crashed = None
        self.seconds = 0.0

    def check(self, label, ok, detail=""):
        self.checks.append({"label": label, "ok": None if ok is None else bool(ok), "detail": str(detail)})
        mark = "ok     " if ok else ("BLOCKED" if ok is None else "FAIL   ")
        print(f"{mark} S{self.number}: {label}" + (f"  [{detail}]" if detail != "" else ""), flush=True)
        return bool(ok)

    def note(self, text):
        self.notes.append(text)
        print(f"       S{self.number}: {text}", flush=True)

    def finding(self, text):
        """A defect or a documented limit this scenario found. Printed in the report."""
        self.findings.append(text)
        print(f"FINDING S{self.number}: {text}", flush=True)

    @property
    def status(self):
        if self.crashed:
            return "fail"
        if any(c["ok"] is False for c in self.checks):
            return "fail"
        if self.blocked_reason or any(c["ok"] is None for c in self.checks) or not self.checks:
            return "blocked"
        return "pass"

    def as_dict(self):
        return {
            "number": self.number,
            "name": self.name,
            "title": self.title,
            "status": self.status,
            "seconds": round(self.seconds, 1),
            "checks": self.checks,
            "notes": self.notes,
            "findings": self.findings,
            "evidence": self.evidence,
            "blocked_reason": self.blocked_reason,
            "crashed": self.crashed,
        }


# ---- guest control ---------------------------------------------------------------------------


def put_file(vm, remote, data, chunk=2800):
    """Copy bytes into the guest over the serial console (base64 in chunks short enough for the tty)."""
    blob = base64.b64encode(data).decode()
    vm.run(f": > {remote}.b64", 20)
    for index in range(0, len(blob), chunk):
        vm.run(f"printf '%s' '{blob[index:index + chunk]}' >> {remote}.b64", 30)
    vm.run(f"base64 -d {remote}.b64 > {remote} && rm -f {remote}.b64", 30)


# runuser wraps the output of what it runs in systemd's OSC 3008 context marks; terminal escapes
# are never evidence, so every guest output is stripped of them before it is parsed or kept.
ESCAPES = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-9;?]*[ -/]*[@-~]")


def clean(text):
    return ESCAPES.sub("", text)


def parse_json_lines(text):
    """The last JSON document in `text` (bsmctl prints one line; probes print `PROBE-JSON {...}`)."""
    for line in reversed(text.strip().splitlines()):
        line = line.strip()
        if line.startswith("PROBE-JSON "):
            line = line[len("PROBE-JSON ") :]
        try:
            return json.loads(line)
        except ValueError:
            continue
    return None


class Guest:
    """Operations inside the VM, shared by every scenario."""

    def __init__(self, vm, sampler, out_root):
        self.vm = vm
        self.sampler = sampler
        self.out_root = Path(out_root)
        self.uids = {}
        self.offset = 0.0
        self.counter = 0
        self.base = time.monotonic()
        self.markers = {}  # user -> session id the marker belongs to

    # ---- shell -------------------------------------------------------------------------

    def sh(self, command, timeout=30):
        return clean(self.vm.run(command, timeout))

    def script(self, text, timeout=60):
        """Run a bash script from a file: no quoting trouble with the serial shell (zsh)."""
        self.counter += 1
        path = f"/tmp/bsmsec-{self.counter}.sh"
        put_file(self.vm, path, text.encode())
        return clean(self.vm.run(f"bash {path} 2>&1", timeout))

    def probe(self, *args, timeout=60):
        command = "python3 {} {} 2>&1".format(GUEST_PROBE, " ".join(shlex.quote(str(a)) for a in args))
        output = self.sh(command, timeout)
        result = parse_json_lines(output)
        if not isinstance(result, dict):
            result = {"error": "no PROBE-JSON line", "output": output[-800:]}
        return result

    def epoch(self):
        return self.vm.guest_epoch()[0]

    def host_time(self, guest_epoch):
        """Host monotonic time of a guest epoch (accurate to the serial round trip)."""
        return guest_epoch - self.offset + (time.monotonic() - time.time())

    def refresh_clock(self):
        self.offset = self.vm.clock_offset()

    # ---- set up ----------------------------------------------------------------------------

    def install_tools(self):
        put_file(self.vm, GUEST_PROBE, GUEST_PROBE_SOURCE.read_bytes())
        put_file(self.vm, GUEST_BRIDGE_SHIM, BRIDGE_SHIM.encode())
        self.vm.run(f"chmod 755 {GUEST_BRIDGE_SHIM} {GUEST_PROBE}", 20)

    def prepare(self, users=("alice", "bob")):
        """Accounts, the bridge shim, slow transitions; restart the helper and bsm so they see them."""
        accounts = "".join(
            f"id {name} >/dev/null 2>&1 || (useradd -m -G audio,video -s /bin/bash {name} && echo {name}:{PASSWORDS[name]} | chpasswd)\n"
            for name in users
        )
        toml = "[transitions]\n" + "".join(f"{key} = {value}\n" for key, value in SLOW_TRANSITIONS.items())
        self.script(
            f"""set -e
{accounts}
mkdir -p {GUEST_STATE_DIR} /etc/bsm
printf '[session]\\nbridge = ["{GUEST_BRIDGE_SHIM}"]\\n' > /etc/bsm/helper.toml
chmod 644 /etc/bsm/helper.toml
cat > /etc/bsm/bsm.toml <<'TOML'
{toml}TOML
systemctl restart bsm-helper
systemctl restart bsm
""",
            timeout=180,
        )
        for name in users:
            self.uids[name] = int(self.sh(f"id -u {name}", 10).strip().splitlines()[-1])
        self.wait_greeter(120)
        self.reset_faillock()
        self.start_monitor()
        self.refresh_clock()

    def start_monitor(self):
        """`bsmctl monitor --json` with a guest timestamp per line, appended to the shared log."""
        if self.sh("pgrep -f 'bsmctl monitor' >/dev/null && echo yes || echo no", 10).strip().endswith("yes"):
            return
        self.sh(f"mkdir -p {GUEST_STATE_DIR}; touch {MONITOR_LOG}", 10)
        self.sh(
            f"nohup sh -c 'bsmctl monitor --json 2>&1 | while IFS= read -r line; do "
            f'printf "%s %s\\n" "$(date +%s.%N)" "$line"; done\' >>{MONITOR_LOG} 2>&1 &',
            10,
        )

    def reset_faillock(self):
        self.sh("for u in live bsmtest alice bob; do faillock --user $u --reset; done", 20)

    # ---- bsmctl and the monitor ---------------------------------------------------------------

    def ctl(self, arguments, user=None, scope=False, timeout=40):
        """bsmctl ARGS --json: as root, as `user` outside a session, or as `user` inside its hosted session scope."""
        if user and scope:
            command = (
                f"python3 {GUEST_PROBE} scope-exec --scope-of {user} --user {user} -- bsmctl {arguments} --json 2>&1"
            )
        elif user:
            command = f"runuser -u {user} -- bsmctl {arguments} --json 2>&1"
        else:
            command = f"bsmctl {arguments} --json 2>&1"
        text = self.sh(command, timeout).strip()
        return parse_json_lines(text), text

    def status(self):
        data, text = self.ctl("status")
        return data if isinstance(data, dict) else {"error": text}

    def sessions(self):
        data, _ = self.ctl("sessions")
        return data if isinstance(data, list) else []

    def session(self, user):
        uid = self.uids[user]
        for entry in self.sessions():
            if entry.get("uid") == uid:
                return entry
        return None

    def mark(self):
        out = self.sh(f"wc -l < {MONITOR_LOG}", 15).strip().splitlines()
        return int(out[-1]) if out and out[-1].strip().isdigit() else 0

    def events(self, since=0):
        """[(host monotonic time, event)] for monitor lines after index `since`, and the new index."""
        out = self.sh(f"tail -n +{since + 1} {MONITOR_LOG}", 30)
        rows = []
        lines = [line for line in out.splitlines() if line.strip()]
        for line in lines:
            stamp, _, body = line.partition(" ")
            try:
                event = json.loads(body)
                when = self.host_time(float(stamp))
            except ValueError:
                continue
            if isinstance(event, dict):
                rows.append((when, event))
        return rows, since + len(lines)

    def wait_event(self, predicate, timeout, since, poll=1.5):
        """First (time, event) after monitor index `since` that satisfies `predicate`, or None."""
        deadline = time.monotonic() + timeout
        index = since
        while True:
            rows, index = self.events(index)
            for when, event in rows:
                if predicate(event):
                    return when, event
            if time.monotonic() >= deadline:
                return None
            time.sleep(poll)

    def wait_until(self, predicate, timeout, poll=2.0):
        deadline = time.monotonic() + timeout
        while True:
            value = predicate()
            if value:
                return value
            if time.monotonic() >= deadline:
                return None
            time.sleep(poll)

    # ---- processes ----------------------------------------------------------------------------

    def procs(self):
        return self.vm.processes()

    def pids(self, comm=None, user=None, contains=None):
        found = []
        for pid, _ppid, owner, name, args in self.procs():
            if comm and name != comm:
                continue
            if user and owner != user:
                continue
            if contains and contains not in args:
                continue
            found.append(pid)
        return sorted(found)

    def bsm_pid(self):
        pids = self.pids("bsm", user="bsm")
        return pids[0] if pids else None

    def gnoblin_compositor(self, user):
        pids = self.pids("gnoblin", user, "--wayland --headless")
        return pids[0] if pids else None

    def gnoblin_leader(self, user):
        pids = self.pids("gnoblin", user, "--hosted")
        return pids[0] if pids else None

    def bridge(self, user):
        pids = self.pids("bsm-host", user)
        return pids[0] if pids else None

    def greeter_pids(self):
        return self.pids(user=GREETER_USER)

    def kill(self, pids, signal=9):
        """kill the pids; the guest epoch right after the signal was sent."""
        out = self.sh(f"kill -{signal} {' '.join(str(p) for p in pids)}; date +%s.%N", 20).strip().splitlines()
        return float(out[-1]) if out else self.epoch()

    # ---- the greeter: select, authenticate, lock -------------------------------------------------

    def wait_greeter(self, timeout=90):
        def ready():
            status = self.status()
            return status.get("greeter_connected") and status.get("helper_connected")

        return self.wait_until(ready, timeout, poll=2.0)

    def select(self, user):
        self.reset_faillock()
        self.ctl(f"select-user {self.uids[user]}")
        return self.wait_until(lambda: self.status().get("selected_uid") == self.uids[user], 20, poll=1.0)

    def clear_greeter_failure(self, user):
        """A failed launch stays in the greeter until it is dismissed: choosing another account and back does that."""
        other = next(name for name in PASSWORDS if name != user)
        self.ctl(f"select-user {self.uids[other]}")
        time.sleep(3.0)
        self.ctl(f"select-user {self.uids[user]}")
        time.sleep(3.0)

    def conversation_open(self, before, timeout=60):
        """A PAM conversation is waiting for the password.

        `before`: the bsm-auth pids that existed before the account was selected. Selecting
        another account aborts the old conversation and starts a new child, so a pid that is
        not in `before` is the new conversation; when the account was already selected the
        existing child is the conversation. The first prompt needs a moment to arrive.
        """
        found = self.wait_until(lambda: [p for p in self.pids("bsm-auth") if p not in before] or (before and self.pids("bsm-auth")), timeout, poll=1.0)
        if not found:
            return False
        time.sleep(2.0)
        return True

    def type_secret(self, text):
        self.reset_faillock()
        self.vm.type_text(text)
        self.vm.key("ret")

    def begin_auth(self, user):
        """Show `user` at the greeter and wait until a conversation is ready for the password."""
        uid = self.uids[user]
        before = set(self.pids("bsm-auth"))
        self.reset_faillock()
        if self.status().get("selected_uid") != uid:
            self.ctl(f"select-user {uid}")
            if not self.wait_until(lambda: self.status().get("selected_uid") == uid, 20, poll=1.0):
                raise Blocked(f"could not select {user} in the greeter")
        else:
            before = set()
        if self.conversation_open(before, 45):
            return
        self.clear_greeter_failure(user)
        if not self.conversation_open(set(), 45):
            raise Blocked(f"no PAM conversation opened for {user}")

    def finished(self, mark, timeout):
        return self.wait_event(lambda e: e.get("event") == "operation_finished", timeout, mark)

    def login(self, user, attempts=3):
        """Sign `user` in at the greeter; returns the operation_finished event of the reveal."""
        last = None
        for attempt in range(attempts):
            self.require_greeter_mode()
            self.begin_auth(user)
            mark = self.mark()
            self.type_secret(PASSWORDS[user])
            last = self.finished(mark, 330)
            if last and last[1].get("success"):
                time.sleep(2.0)
                return last
            print(f"login of {user} did not finish ({last[1] if last else 'no result'}); retry {attempt + 1}", flush=True)
            self.clear_greeter_failure(user)
        raise Blocked(f"{user} could not sign in after {attempts} attempts: {last[1] if last else 'no operation result'}")

    def unlock(self, user):
        entry = self.session(user)
        if entry is None:
            raise Blocked(f"{user} has no session to unlock")
        self.begin_auth(user)
        mark = self.mark()
        self.type_secret(PASSWORDS[user])
        done = self.finished(mark, 180)
        if not (done and done[1].get("success")):
            raise Blocked(f"unlock of {user} did not complete: {done[1] if done else 'no result'}")
        time.sleep(2.0)
        return done

    def lock(self, user):
        entry = self.session(user)
        if entry is None:
            raise Blocked(f"{user} has no session to lock")
        mark = self.mark()
        self.ctl(f"lock {entry['id']}")
        done = self.finished(mark, 90)
        if not (done and done[1].get("success")):
            raise Blocked(f"lock of {user} did not complete: {done[1] if done else 'no result'}")
        time.sleep(1.0)
        return done

    def logout(self, user):
        """Normal logout through bsm; waits until the session is gone and the greeter is back."""
        entry = self.session(user)
        if entry is None:
            return
        self.markers.pop(user, None)
        mark = self.mark()
        self.ctl(f"logout {entry['id']}")
        done = self.finished(mark, 120)
        gone = self.wait_until(lambda: self.session(user) is None, 60, poll=2.0)
        if not gone:
            raise Blocked(f"{user} is still signed in after a logout ({done[1] if done else 'no operation result'})")
        self.wait_greeter(60)
        time.sleep(1.0)

    def logout_all(self):
        for user in PASSWORDS:
            self.logout(user)

    def foreground_unlocked(self):
        for entry in self.sessions():
            if entry.get("foreground") and not entry.get("locked"):
                return entry
        return None

    def require_greeter_mode(self):
        """A login or unlock is typed at the greeter, so no session may be foreground and unlocked."""
        shown = self.foreground_unlocked()
        if shown is not None:
            owner = next((n for n, u in self.uids.items() if u == shown["uid"]), None)
            if owner is None:
                raise Blocked(f"session {shown['id']} of an unknown account is on screen")
            self.lock(owner)

    def ensure_session(self, user):
        if self.session(user) is None:
            self.login(user)
        return self.session(user)

    def ensure_foreground(self, user):
        """`user` signed in, unlocked and on screen; any other session ends locked in the background."""
        entry = self.session(user)
        if entry is None:
            self.login(user)
        elif not (entry.get("foreground") and not entry.get("locked")):
            self.require_greeter_mode()
            self.unlock(user)
        return self.session(user)

    def ensure_locked(self, user):
        """`user` signed in and locked, the greeter on screen."""
        entry = self.ensure_session(user)
        if entry is not None and not entry.get("locked"):
            if not entry.get("foreground"):
                self.require_greeter_mode()
                self.unlock(user)
            self.lock(user)
        return self.session(user)

    def ensure_pair(self, foreground="alice", background="bob"):
        """Both signed in; `foreground` unlocked on screen, `background` locked behind it."""
        self.ensure_session(background)
        self.ensure_locked(background)
        self.ensure_foreground(foreground)
        for user in (foreground, background):
            self.ensure_marker(user)

    # ---- markers ----------------------------------------------------------------------------------

    def keylog(self, user):
        return f"/tmp/marker-{user}.keys"

    def start_marker(self, user):
        """Run the marker window of `user` inside its session scope; returns once it has windows."""
        entry = self.session(user)
        if entry is None:
            raise Blocked(f"{user} has no session for a marker")
        sid = entry["id"]
        log = f"/tmp/marker-{user}.log"
        self.sh(f"rm -f {log} {self.keylog(user)}", 10)
        self.sh(
            f"nohup python3 {GUEST_PROBE} scope-exec --scope-of {user} --user {user} --env WAYLAND_DISPLAY=bsm-{sid} "
            f"--env GDK_BACKEND=wayland -- python3 {GUEST_PROBE} marker --user {user} --colour '{MARKER_COLOURS[user]}' "
            f"--keylog {self.keylog(user)} > {log} 2>&1 &",
            10,
        )
        ready = self.wait_until(lambda: "windows on" in self.sh(f"cat {log}", 10), 90, poll=2.0)
        if not ready:
            raise Blocked(f"the marker of {user} did not start: {self.sh(f'tail -n 5 {log}', 10)[-300:]}")
        self.markers[user] = sid
        time.sleep(2.0)

    def marker_running(self, user):
        entry = self.session(user)
        if entry is None or self.markers.get(user) != entry["id"]:
            return False
        return bool(self.sh(f"pgrep -u {user} -f 'marker --user {user}' || true", 10).strip())

    def ensure_marker(self, user):
        if not self.marker_running(user):
            self.start_marker(user)

    def key_log(self, user):
        out = self.sh(f"cat {self.keylog(user)} 2>/dev/null", 10)
        return [line.split()[-1] for line in out.splitlines() if line.strip() and line.split()[0].replace(".", "").isdigit()]

    # ---- evidence helpers --------------------------------------------------------------------------

    def snapshot(self, ev, label):
        """bsmctl, logind and process state, for the evidence file `<label>-state.txt`."""
        parts = {
            "bsmctl status": self.ctl("status")[1],
            "bsmctl sessions": self.ctl("sessions")[1],
            "bsmctl outputs": self.ctl("outputs")[1],
            "loginctl list-sessions": self.sh("loginctl list-sessions --no-pager --no-legend", 20),
            "ps (bsm, greeter, helper, gnoblin, bsm-host)": self.sh(
                "ps -eo pid,ppid,user:12,etimes,comm,args --sort=pid | grep -E 'bsm|greeter|gnoblin|bingux-shell' | grep -v grep | cut -c1-180", 20
            ),
        }
        ev.text(f"{label}-state.txt", "\n\n".join(f"## {k}\n{v}" for k, v in parts.items()))

    def journal(self, ev, filename, since_epoch, units=("bsm", "bsm-helper"), lines=300):
        unit_args = " ".join(f"-u {u}" for u in units)
        text = self.sh(
            f"journalctl -b --no-pager -o short-iso --since '@{int(since_epoch)}' {unit_args} 2>&1 | grep -v -E 'MESA|radv|ZINK|vkEnum|report outside' | cut -c1-260 | tail -n {lines}",
            90,
        )
        ev.text(filename, text)
        return text
