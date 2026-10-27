# bsm security and failure scenarios: harness and results

Spec section 15 lists the mandatory security and failure scenarios. `tests/bsm-qemu-security.py`
implements ten of them against a real VM (UEFI, two displays, accounts `alice` and `bob` with
groups `audio` and `video` only). Each user's desktop carries a marker (a full-screen window of one
saturated colour on every monitor, `tests/bsm_guest_probe.py marker`), so a frame that shows the
colour is a frame in which that desktop reached the screen
([`tests/bsm_security_frames.py`](../../tests/bsm_security_frames.py), unit tests in
`tests/bsm-security-frames.test.py`).

```sh
flock /tmp/bsm-vm.lock python3 tests/bsm-qemu-security.py            # all ten, about an hour
flock /tmp/bsm-vm.lock python3 tests/bsm-qemu-security.py --only 1,4  # some
python3 tests/bsm-security-frames.test.py                              # frame analysis, no VM
```

Output: `distro/out/bsm-test/security/` (`report.md`, `results.json`, one directory per scenario).
Exit status 0 all passed, 1 something failed, 2 something is blocked. Only one VM may run on the
build host at a time; do not publish a run taken above a load average of 8.

## Status

"Run" means the harness completed the scenario in a VM and the checks are in `result.json`.
"Manual" means the behaviour was seen once in a VM by hand (commands in the notes below); a manual
observation is not a harness result.

| # | Scenario | Status | Evidence |
| - | -------- | ------ | -------- |
| 1 | Isolation between accounts | **run: 64 of 65 checks ok**. The failing check (a repeated publish of the same image changed nothing) was a harness data bug, fixed by a unique image per run; not re-run. | `s01-isolation/` |
| 2 | Greeter crash while locked | implemented, not run. Manual: see below. | |
| 3 | Gnoblin crash | implemented, not run | |
| 4 | PAM helper crash, account switch mid-auth | **run: fail**, 15 ok and 2 failed, both failures the faillock finding below | `s04-auth/` |
| 5 | Output change during lock and fade | implemented, not run. Manual: see below. | |
| 6 | DPMS, suspend and resume | implemented (second VM through SeaBIOS), not run. Manual: see below. | |
| 7 | Portal and capture isolation | implemented, not run. Manual: see below. | |
| 8 | Unauthorised and authorised power | implemented, not run | |
| 9 | No DRM reset, VT change or black frame | implemented (kernel probes on the virtio-gpu CRTC enable and disable, a VT and a udev watcher), not run | |
| 10 | logind, polkit, audio, systemd --user | implemented, not run; facts so far in [logind-findings.md](logind-findings.md) | |

Scenario 1 proved, with real processes and real sockets: alice gets `EACCES` on the greeter,
control and helper sockets; with the greeter's group alice, and root, are closed by bsm with no
protocol byte; while bob's login is held open (a test shim delays his bridge 28 s) the brokered
`/run/bsm/sessions/<id>.sock` (mode 0666) was probed by alice, by alice inside a user namespace that
maps her to bob's uid, by root, by the greeter's uid and by bob's own other process, early (before
bsm knows the bridge pid) and late: every probe was closed without a byte, the rejections are in the
journal (`peer uid .. is not the expected uid`, `peer pid .. is not the expected pid`), and the real
bridge still used the socket afterwards. Cross-session `lock`, `logout`, `select-user`, reboot, power
off and suspend are `unauthorised` through `bsmctl` and through raw D-Bus (`org.bingux.Session1.Error.Unauthorised`)
and change no session; alice's `PublishAppearance` changed only her own cache directory; alice cannot
create or overwrite files in bob's or an invented uid's cache directory, cannot read bob's home,
runtime directory, buses, PipeWire, compositor and readiness sockets, cannot ptrace bsm, the helper,
the greeter or bob's Gnoblin and bridge, and cannot read bob's clipboard (a positive control showed
bob's own paste works).

## Manual observations (not harness results)

* **Greeter impostor (2):** with a process running as the greeter's uid hammering both greeter sockets
  while the greeter was killed, bsm logged `greeter connection rejected: peer pid .. is not the greeter pid ..`
  and `greeter control connection refused (uid 969, pid ..)`; the real greeter reconnected.
* **PAM child killed (4):** `kill -9` of the `bsm-auth` child ends the operation as `helper_failure`, the
  greeter opens a new conversation, no session appears and `pam_faillock` has no record.
* **Output removed (5):** `SetDesktopSize 0x0` on the head-1 VNC server removes the output (`bsmctl outputs`: 2 to 1),
  1280x800 adds it back. When the output vanished bsm logged `greeter lost; sessions stay protected until it returns`
  and the greeter was respawned about 1.5 s later; the hosted shells logged `no outputs - creating placeholder screen`.
  Frames were not analysed.
* **Suspend (6):** S3 works under SeaBIOS and hangs under edk2 OVMF; details in [logind-findings.md](logind-findings.md).
* **Capture (7):** Mutter's ScreenCast and RemoteDesktop work on the account's own bus and reach only its own
  session; bob's bus gives `Permission denied` to alice; a video-group account gets handle 0 from `GETFB` and
  `EACCES` from `SET_MASTER` on `/dev/dri/card1`.

## Findings

| Finding | Seen by | Owner |
| ------- | ------- | ----- |
| `pam_faillock` counts an aborted or timed-out PAM conversation as a failed login: three account switches or three idle minutes at the lock screen lock the account for the faillock unlock time (10 minutes by default). Cause: the helper's conversation returns `PAM_CONV_ERR` to libpam on cancel or timeout, so the stack reaches `pam_faillock authfail`. Fixed in `91226e16`: the child sends its verdict and exits inside the conversation, so the stack never sees a failure (a killed child leaves no record). The helper owner verified it with its root script in the VM (4 aborted logins and 4 aborted unlocks leave no record, 3 wrong passwords still lock); scenario 4 has **not** been re-run against it yet. To re-run: `CARGO_BUILD_JOBS=4 python3 tests/bsm-vm-inject.py stage --out /tmp/bsm-vm/inject-sec && flock /tmp/bsm-vm.lock python3 tests/bsm-qemu-security.py --only 4 --skip-bios --inject /tmp/bsm-vm/inject-sec`. | scenario 4 (both checks), by hand | bsm-helper |
| A one-shot `bsmctl reboot --json` from a hosted session always ends `unauthorised`: polkit is asked about the caller's bus name after `bsmctl` exited (`NameHasNoOwner`). The shell's `BsmSession.qml` runs that form. Check the polkit subject (a logind session subject does not depend on the caller staying connected). | by hand | bsm-manager |
| `bingux-greeter-export.service` fails with `Permission denied` under bsm and restarts every 3 s in every hosted session. | by hand, journal | shell integration, packaging |
| The first Gnoblin start of a fresh VM can exceed the runtime supervisor's handshake wait; the supervisor falls back to the built-in configuration, Bingux never launches and bsm fails the login after 120 s. | by hand, three fresh VMs | hosted Gnoblin |
| An output removal drops the greeter for about 1.5 s (safe: the cover shows, sessions stay protected). | by hand | compositor, greeter QML |
| `profile.json` in the world-readable cache holds `source_sha256` of the private original. | by hand | appearance |
| After an S3 resume under SeaBIOS a freshly started bsm never became ready. | by hand | open, not investigated |
