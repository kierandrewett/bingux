#!/usr/bin/env python3
"""Prove in the real VM that a person at the machine can reach the account picker.

Boots the newest live ISO with bsm (two displays, as tests/bsm-qemu.py), puts
the current shell/bingux QML into the guest, and drives the real greeter with
real input only: QEMU key events and the absolute USB tablet (QMP
input-send-event). No bsmctl select-user is used to move the greeter.

    flock /tmp/bsm-vm.lock python3 tests/bsm-greeter-vm.py [--iso ISO] [--out DIR]

Steps:
  1. Login screen with two accounts: the Switch user button opens the picker;
     it lists both accounts as `Not signed in`; picking the other account
     changes the avatar and name and starts a conversation for it; nothing is
     revealed (no foreground session).
  2. Sign in as the first account with the keyboard (real PAM, real hosted
     session), wait for the reveal.
  3. `bsmctl lock`: the unlock screen shows the owner; the slide-up handle opens
     the picker (click, then a swipe from the bottom edge) listing the owner as
     `Running, locked` and the other as `Not signed in`; picking the other
     changes the screen without revealing anything.

Screenshots and the greeter's own state file (bsm-greeter-state.json) go to
--out (default distro/out/bsm-test/greeter-real). The state file is how the
harness sees why a control is shown or hidden: bsm sends the greeter's stderr
to /dev/null.
"""

import argparse
import json
from pathlib import Path
import shutil
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from bsm_harness import BsmVm, VmLock  # noqa: E402
from distro_vm import find_iso  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
USER_A = ("live", "live")
USER_B = ("bsmtest", "bsmtest")
checks = {}


def check(name, ok, detail=""):
    checks[name] = bool(ok)
    print(("ok   " if ok else "FAIL ") + name + (f" ({detail})" if detail and not ok else ""), flush=True)


class Rig:
    def __init__(self, vm, out):
        self.vm = vm
        self.out = out

    def shot(self, name, settle=1.5):
        from PIL import Image

        time.sleep(settle)
        frame = self.vm.frame()
        if frame is None:
            return None
        Image.fromarray(frame).save(self.out / f"{name}.png")
        return frame

    def state(self):
        text = self.vm.run("cat /run/bsm-greeter/*/rt/bsm-greeter-state.json 2>&1 | tail -n 1", 20).strip()
        try:
            return json.loads(text)
        except ValueError:
            return {"unreadable": text[-200:]}

    def wait_changed(self, reference, box, threshold=8, timeout=12):
        """The newest frame once it differs from `reference` inside `box` (the VNC capture lags), else the last frame."""
        deadline = time.monotonic() + timeout
        frame = None
        while time.monotonic() < deadline:
            frame = self.vm.frame()
            if frame is not None and reference is not None and diff(reference, frame, box) > threshold:
                return frame
            time.sleep(0.5)
        return frame

    def pointer(self, x, y, down=None, width=2560, height=800):
        # The tablet spans both displays side by side (bsm lays them out in a row),
        # so display 0 covers the left half of the x axis.
        events = [
            {"type": "abs", "data": {"axis": "x", "value": round(x / width * 32767)}},
            {"type": "abs", "data": {"axis": "y", "value": round(y / height * 32767)}},
        ]
        self.vm._qmp.execute("input-send-event", {"events": events})
        if down is not None:
            self.vm._qmp.execute("input-send-event", {"events": [{"type": "btn", "data": {"down": down, "button": "left"}}]})

    def click(self, x, y):
        self.pointer(x, y)
        time.sleep(0.5)
        self.pointer(x, y, True)
        time.sleep(0.15)
        self.pointer(x, y, False)

    def swipe_up(self, x=300, y0=775, y1=560):
        self.pointer(x, y0)
        time.sleep(0.5)
        self.pointer(x, y0, True)
        time.sleep(0.2)
        for step in range(1, 11):
            self.pointer(x, y0 + (y1 - y0) * step / 10)
            time.sleep(0.05)
        self.pointer(x, y1, False)

    def status(self):
        out, text = self.vm.bsmctl_json("status")
        return out if isinstance(out, dict) else {"unreadable": text[-200:]}

    def users(self):
        out, _ = self.vm.bsmctl_json("users")
        return out if isinstance(out, list) else []


def red_text(frame, box):
    import numpy as np

    crop = frame[box[1] : box[3], box[0] : box[2]].astype(int)
    mask = (crop[:, :, 0] > 225) & (crop[:, :, 1] < 150) & (crop[:, :, 2] < 130) & (crop[:, :, 0] - crop[:, :, 1] > 90)
    return int(mask.sum()) > 12


def diff(a, b, box):
    import numpy as np

    return float(np.abs(a[box[1] : box[3], box[0] : box[2]].astype(int) - b[box[1] : box[3], box[0] : box[2]].astype(int)).mean())


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--iso")
    parser.add_argument("--out", default=str(ROOT / "distro/out/bsm-test/greeter-real"))
    parser.add_argument("--no-shell-update", action="store_true", help="test the greeter QML that is in the ISO")
    args = parser.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    base = Path("/tmp/bsm-vm")
    base.mkdir(exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="greeter-real-", dir=base))
    inject = work / "inject"
    inject.mkdir()
    if not args.no_shell_update:
        subprocess = __import__("subprocess")
        subprocess.run([sys.executable, str(ROOT / "scripts/pack-shell.py"), "--version", "0.1.0", str(inject / "bingux-shell.sxa")], check=True, stdout=subprocess.DEVNULL)
        (inject / "apply.sh").write_text(
            "#!/bin/sh\nset -e\n"
            "dest=$(ls /usr/lib/bingux/bingux-shell.sxa /usr/lib64/bingux/bingux-shell.sxa 2>/dev/null | head -n 1)\n"
            'cp /mnt/bsm-inject/bingux-shell.sxa "$dest"\n'
            'echo "shell archive installed to $dest"\n'
            "echo INJECT-DONE\n"
        )
    lock = VmLock().acquire()
    vm = BsmVm(work, iso=find_iso(args.iso), record=work / "rec", inject=inject if not args.no_shell_update else None)
    vm.start()
    rig = Rig(vm, out)
    try:
        if not vm.start_shell():
            print("no serial shell")
            return 1
        vm.run("systemctl is-system-running --wait 2>&1 | head -1", 120)
        vm.run(f"id {USER_B[0]} >/dev/null 2>&1 || (useradd -m -G video,audio -s /bin/bash {USER_B[0]} && echo {USER_B[0]}:{USER_B[1]} | chpasswd)", 30)
        if not args.no_shell_update:
            applied = vm.run(
                "mkdir -p /mnt/bsm-inject && modprobe 9pnet_virtio; modprobe 9p; "
                "mount -t 9p -o trans=virtio,version=9p2000.L,ro bsminject /mnt/bsm-inject && sh /mnt/bsm-inject/apply.sh 2>&1 | tail -n 5",
                120,
            )
            print(applied)
            check("the shell archive was replaced", "INJECT-DONE" in applied)
        # bsm lists accounts at start-up: restart it so both accounts are known, with the new greeter.
        vm.run("systemctl restart bsm 2>&1 | tail -n 3", 60)
        time.sleep(25)
        vm.ensure_monitor()

        # 1. Login screen.
        status = rig.status()
        print("status", json.dumps(status)[:300])
        users = rig.users()
        check("bsm lists two accounts", len(users) >= 2, str(users)[:200])
        uid_a = next((u["uid"] for u in users if u.get("user_name") == USER_A[0]), None)
        uid_b = next((u["uid"] for u in users if u.get("user_name") == USER_B[0]), None)
        if uid_a is None or uid_b is None:
            print("accounts missing")
            return 1
        vm.bsmctl_json(f"select-user {uid_a}")  # setup only: the account the screen starts on
        time.sleep(3)
        login = rig.shot("login")
        state = rig.state()
        (out / "state-login.json").write_text(json.dumps(state, indent=2) + "\n")
        check("login: the greeter offers Switch user", state.get("canSwitchUser") is True, json.dumps(state))
        rig.click(83, 764)
        picker = rig.wait_changed(login, (420, 560, 860, 780))
        time.sleep(0.8)
        picker = vm.frame()
        from PIL import Image

        Image.fromarray(picker).save(out / "login-picker.png")
        check("login: clicking Switch user opens the picker", picker is not None and login is not None and diff(login, picker, (420, 560, 860, 780)) > 8)
        labels = vm.run("true", 5)
        rig.click(640, 735)  # the second row
        time.sleep(1.5)
        switched = rig.shot("login-other-account", 1.5)
        status = rig.status()
        check("login: choosing the other account selects it", status.get("selected_uid") == uid_b, str(status.get("selected_uid")))
        check("login: nothing was revealed", status.get("foreground") in (None, "") and status.get("seat") in ("greeter", "authenticating"), json.dumps(status)[:200])
        check("login: the avatar and name changed", switched is not None and diff(login, switched, (480, 520, 800, 660)) > 6)
        state = rig.state()
        check("login: the greeter began a conversation for it", state.get("selectedUid") == uid_b and state.get("phase") in ("prompting", "waiting"), json.dumps(state))

        # 2. Sign in as A with the keyboard.
        rig.click(83, 764)
        time.sleep(1.0)
        rig.click(640, 670)  # the first row
        time.sleep(2.5)
        check("login: the picker brings back the first account", rig.status().get("selected_uid") == uid_a)
        before = vm.event_count()
        vm.type_text(USER_A[1])
        vm.key("ret")
        shot = rig.shot("login-progress", 6)
        done = vm.wait_monitor(lambda e: e.get("event") == "transition" and e.get("kind") == "new_login_fade" and e.get("phase") in ("completed", "committed"), 240, since=before)
        check("login: the session starts and is revealed", done is not None)
        time.sleep(5)
        rig.shot("desktop", 1)

        # 3. Lock.
        session = None
        out_s, _ = vm.bsmctl_json("sessions")
        if isinstance(out_s, list):
            session = next((s.get("id") for s in out_s if s.get("uid") == uid_a), None)
        mark = vm.event_count()
        vm.bsmctl_json(f"lock {session}" if session else "lock")
        locked_event = vm.wait_monitor(lambda e: e.get("event") == "lock_state_changed" and e.get("locked") is True, 30, since=mark)
        check("lock: bsm reports the session locked", locked_event is not None)
        time.sleep(4)
        locked = rig.shot("locked", 1)
        state = rig.state()
        (out / "state-locked.json").write_text(json.dumps(state, indent=2) + "\n")
        check("lock: no stale busy message is left under the avatar", locked is not None and not red_text(locked, (400, 655, 880, 685)))
        check("lock: the greeter shows the unlock form of the owner", state.get("mode") == "unlock" and state.get("selectedUid") == uid_a, json.dumps(state))
        check("lock: the handle is offered with the conversation open", state.get("canSwitchUser") is True, json.dumps(state))
        rig.click(640, 776)
        picker = rig.wait_changed(locked, (420, 480, 860, 780))
        time.sleep(0.8)
        picker = vm.frame()
        Image.fromarray(picker).save(out / "locked-picker-click.png")
        check("lock: clicking the handle opens the picker", picker is not None and locked is not None and diff(locked, picker, (420, 480, 860, 780)) > 8)
        vm.key("esc")
        time.sleep(1.5)
        closed = rig.shot("locked-picker-closed", 0.5)
        rig.swipe_up()
        swiped = rig.wait_changed(closed, (420, 480, 860, 780))
        time.sleep(0.8)
        swiped = vm.frame()
        Image.fromarray(swiped).save(out / "locked-picker-swipe.png")
        (out / "state-after-swipe.json").write_text(json.dumps(rig.state(), indent=2) + "\n")
        check("lock: swiping up from the bottom edge opens the picker", swiped is not None and closed is not None and diff(closed, swiped, (420, 480, 860, 780)) > 8)
        # Close the picker again by dragging its handle down, then reopen it with a click to pick.
        closed_again = None
        rig.pointer(640, 626)
        time.sleep(0.5)
        rig.pointer(640, 626, True)
        time.sleep(0.2)
        for step in range(1, 11):
            rig.pointer(640, 626 + 15 * step)
            time.sleep(0.05)
        rig.pointer(640, 776, False)
        closed_again = rig.wait_changed(swiped, (420, 600, 860, 780))
        time.sleep(0.8)
        closed_again = vm.frame()
        Image.fromarray(closed_again).save(out / "locked-picker-dragged-closed.png")
        check("lock: dragging the picker handle down closes it", closed_again is not None and diff(swiped, closed_again, (420, 600, 860, 780)) > 8)
        rig.click(640, 776)
        time.sleep(2.0)
        rig.click(640, 735)
        time.sleep(1.5)
        other = rig.shot("locked-other-account", 1.5)
        status = rig.status()
        check("lock: choosing the other account selects it", status.get("selected_uid") == uid_b, str(status.get("selected_uid")))
        check("lock: nothing was revealed", status.get("foreground") in (None, "") or status.get("seat") in ("locked_greeter", "greeter", "authenticating", "switching_user"), json.dumps(status)[:240])
        check("lock: the avatar and name changed", other is not None and locked is not None and diff(locked, other, (480, 520, 800, 660)) > 6)
        users = rig.users()
        check("lock: the owner is listed as running", any(u.get("uid") == uid_a and "running" in str(u.get("availability")) for u in users), str(users)[:240])
        check("lock: the other account is listed as not signed in", any(u.get("uid") == uid_b and u.get("availability") == "not_signed_in" for u in users), str(users)[:240])
        (out / "state-final.json").write_text(json.dumps(rig.state(), indent=2) + "\n")
        (out / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
        return 0 if all(checks.values()) else 1
    finally:
        try:
            (out / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
        finally:
            vm.stop()
            lock.release()
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
