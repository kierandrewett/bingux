# bsm boot and display handover

How the display passes from the boot splash (Plymouth) to bsm, what was measured,
what is still not seamless, and how to get out when it goes wrong. Spec section 11,
phase 8. Design: [architecture.md](architecture.md). Units, install and the
display-manager tool: [packaging.md](packaging.md).

Status words as in architecture.md. Everything below says whether it was measured in
QEMU (2026.10.27 image, virtio-gpu with virgl, two heads, 1280x800) or not. QEMU is not
hardware: the guest has virgl only, one GPU driver, no real monitors.

## What the handover has to avoid

Measured on the image as shipped (`bingux.bsm`, kernel `drm.debug`, tracer
`tests/bsm-boot-trace.py`, kernel log lines in `distro/out/bsm-test/boot/baseline/`):

| Time (guest s) | Event                                                                                         |
| -------------- | --------------------------------------------------------------------------------------------- |
| 11.0 to 11.3   | Plymouth opens the GPU, takes DRM master and sets 1280x800 on both CRTCs with `SETCRTC` (the one modeset the splash is allowed). |
| 67.70          | `plymouth-quit.service` runs a plain `plymouth quit`: DROP_MASTER, the fd closes at 67.77, VT back to text mode. |
| 67.81          | **fbcon takes both planes** (`drm_client_dev_restore fbdev`): the kernel console owns the scanout. |
| 77.89          | seatd opens the GPU for bsm and becomes DRM master, ten seconds later: bsm waited for `plymouth-quit-wait`. |
| 77.90          | bsm's `DrmDevice::new(fd, true)` **disables both CRTCs** (`mode changed, enable changed, active changed`, `disabling [CRTC:39]`, `[CRTC:46]`). |
| 97.38 to 97.74 | bsm's first commits enable both CRTCs again: two real modesets. |
| 98 to 140      | cover colour (13,15,23), then the greeter.                                                    |

On the screen (baseline run, `distro/out/bsm-test/boot/baseline/`, host load 12): splash, one black
frame (0.8 s), a frame the classifier calls content at 29.1 s (probably the kernel console,
[INFERENCE: not inspected]), the flat cover (33.2 s) and then the greeter (43.5 s). The kernel log of the same boot has
6 modeset events after the splash's own (both CRTCs disabled at 16.01, both enabled again at 20.5)
and one fbcon restore (15.08, right after Plymouth's last close). Earlier baseline boots on a host
at load average 50 showed the same structure with longer stalls (a 29.7 s gap with 2 black frames).
Frames are sampled when the picture changes; flashes shorter than the update interval are not seen.

`plymouth quit --retain-splash` as a way to avoid this was **not** shown to work on its own. In a
greetd boot with a timer running `deactivate`, `reactivate`, `deactivate` and
`quit --retain-splash`, the picture stayed on screen through the quit and the VNC stream showed a
black frame later, when greetd's compositor took the display (cause not isolated). The measured
facts the design rests on:

* `plymouth deactivate` only drops DRM master (`clients` shows `plymouthd:191:-`) and sets the
  VT to `auto`. The CRTC, the planes and Plymouth's framebuffers stay as they are, the splash
  picture stays on the display (spinner frozen). `plymouth reactivate` takes the master back.
* Once the master is dropped, seatd can `SET_MASTER` and open the device for bsm while Plymouth is
  still alive; bsm's own `SET_MASTER` fails with `EACCES`, which is harmless (seatd's open file
  is the master).
* If Plymouth is the last DRM client when it exits, the kernel restores fbcon on the planes. If
  seatd (bsm) already has the device open, there is no last close and no fbcon restore.

## The design

```text
plymouth-start --------------------------------+
bsm-seatd, bsm-helper.socket, logind, dbus ----+--> bsm.service  (does not wait for plymouth-quit*)
                                                    ExecStartPre   bsm-plymouth-handover deactivate
                                                    ExecStart      bsm --backend drm --hold-first-scene 60000
                                                    (READY=1 after the first scene is on screen)
                                                    ExecStartPost  bsm-plymouth-handover retain
                                                    ExecStopPost   bsm-plymouth-handover failed
plymouth-quit.service  ... After=bsm.service   (drop-in, ordering only)
```

1. `bsm.service` is ordered after `plymouth-start.service` and no longer after
   `plymouth-quit.service` and `plymouth-quit-wait.service`, so bsm starts as soon as logind,
   D-Bus, seatd and the helper socket are up.
2. `ExecStartPre=` runs `plymouth deactivate`: master dropped, picture kept.
3. bsm (compositor, `packages/bsm-compositor/src/gfx_drm.rs`):
   * `DrmDevice::new(fd, false)`: no connector or CRTC is disabled at start. CRTCs that bsm does
     not drive are switched off after its outputs are known.
   * An output that is already lit keeps its current mode (`get_crtc().mode()`) when the
     connector still offers it, so the first commit is a plain flip.
   * `--hold-first-scene <ms>` (config `[boot] hold_first_scene_ms`): while a CRTC was lit at
     start, bsm presents nothing for cover-only plans, so the splash stays on screen, until a
     plan has a greeter or session layer or the time is up (then the flat cover is shown). The
     unit passes 30 s.
   * `FirstScene` fires `Presented` after the page flip of the first frame with the greeter
     has completed. `bsm` sends `READY=1` only after that (bounded by hold + 10 s, with a warning
     if it was missed), so `Type=notify` ends the start job when the greeter is on screen.
4. `ExecStartPost=` runs `plymouth quit --retain-splash`: Plymouth exits while bsm already owns
   the master, the VT binding and the scanout, so the kernel has nothing to restore.
5. `plymouth-quit.service` is ordered after `bsm.service` (`plymouth-quit.service.d/10-bsm.conf`)
   so it does not blank the splash first. It is ordering only. `Conflicts=` was avoided on
   purpose: it is evaluated when the transaction is built, so a boot where
   `ConditionKernelCommandLine=` skips bsm could still stop `plymouth-quit.service` and leave
   greetd (which waits for `plymouth-quit-wait`) waiting for the splash for ever. [INFERENCE:
   from how systemd builds transactions; the greetd boot below shows `plymouth-quit` still runs
   with the drop-in in place, which is the case that matters.]
6. `ExecStopPost=` runs `bsm-plymouth-handover failed`: if bsm ended without success the splash is
   withdrawn, otherwise greetd (the fallback) would wait for it.

Greeter background and splash: the splash is flat (10,10,13), the greeter is a blue gradient and
the built-in cover is (13,15,23). The first frame with the greeter replaces the splash in one
flip; there is no crossfade, so the last splash picture and the first greeter picture differ
sharply. `[boot]`/`cover` in `bsm.toml` can set the cover to `#0a0a0d` to match the splash, but
the cover is only shown on a hold timeout or a failure now, so this is cosmetic.

## Measuring: tests/bsm-boot.py

```sh
python3 tests/bsm-vm-inject.py stage --build host --out /tmp/bsm-boot/inject
python3 tests/bsm-boot.py --stage /tmp/bsm-boot/inject     # staged: every check asserted
python3 tests/bsm-boot.py --only handover,modeset          # image as shipped: handover checks measured
python3 tests/bsm-boot.py --stage /tmp/bsm-boot/inject --only session
python3 tests/bsm-boot.test.py                             # unit tests of the analysis, no VM
```

Phases `handover`, `modeset`, `greetd`, `dm`, `fallback`, `session` each boot the image's own boot
loader in QEMU (two heads) and write to `distro/out/bsm-test/boot/` (`boot/baseline/` without
`--stage`): `report.txt`, `checks.json`, `measured.json`, `measurements.json`, `phases.json` and
per phase the tracer log, journal excerpts, `systemd-analyze critical-chain`, frame tables, stills
and a contact sheet of the handover.

How each thing is measured, and the floor of each method:

* **Frames.** The VNC server delivers a frame when the picture changes (about every 0.1 to 0.4 s
  while something moves). Every delivered frame is classified (`black`, `placeholder` for QEMU's
  640x480 "output not active" surface, `splash`, `flat`, `content`). A scene lasts until the
  next frame. A flash shorter than the VNC update interval is **not seen**: the frame counts are
  a lower bound. The video (`flow.mp4`) is 10 fps.
* **Kernel log (phase `modeset`).** `drm.debug=0x15` logs every CRTC disable, enable and mode
  set, every `SET_MASTER` and `DROP_MASTER` with the process name, opens and closes, and fbcon's
  restore. This is authoritative for "was the mode reset"; it is also slow (every ioctl is
  logged), so its timings are not used.
* **Tracer.** `tests/bsm-boot-trace.py` runs in the guest from the start of userspace (a systemd
  credential, the image is unchanged), polls every 10 ms and logs changes of the DRM clients with
  the master flag, the CRTC state, the owner of the scanout framebuffer (`allocated by`), the VT
  mode and the console mode. It cannot see the initramfs. Reading debugfs `state` perturbs the
  display slightly.
* **Clocks.** Guest `CLOCK_MONOTONIC` is mapped to host time from `/proc/uptime` over the serial
  shell (best of 12 round trips, +-0.1 s). The kernel log is mapped through marks the tracer
  writes to `/dev/kmsg`.
* **Staged boot.** The image predates the handover and ordering can only be tested at a cold
  boot, so `--stage` feeds the manager at its start the unit text that the package installs
  (extra-unit and drop-in credentials) and copies the staged binaries and scripts over the live
  root from a 9p share before bsm starts. Phases `dm` and `fallback` boot the shipped unit files
  (a generated unit cannot be `systemctl enable`d) and copy the staged tool and scripts after
  boot.
* **Host load.** The measurements ran on a shared host at load average 50 to 56 on 16 cores. All
  absolute times are inflated by that; counts, orders and "was it disabled" are not.

## Results

Artifacts: `distro/out/bsm-test/boot/` (`baseline/` is the image as shipped, the rest is the staged
boot; `report-run2.txt` and `report-run3.txt` are the raw logs, `report-run4-fallback.txt` and `fallback/` the fallback phase, run on an unloaded host).
Host load average at the start of each run is in the report (12 for the baseline, 38 to 43 for the
staged runs, which ran before the global VM lock existed or while others used the host).

| Measurement (QEMU, 2 heads, 1280x800)                         | Image as shipped            | Staged handover (this change)      |
| ------------------------------------------------------------- | --------------------------- | ---------------------------------- |
| Frames between the last splash frame and the first greeter frame | 1 black (0.8 s), then a console-like frame, cover (about 10 s) | **0** on both heads (splash, then greeter) |
| CRTCs disabled after the splash (tracer)                      | 2                           | 0                                  |
| Planes off or owned by fbcon after the splash                 | 4 off, 2 fbcon              | 0                                  |
| Modesets after the splash's own (kernel `drm.debug`)          | 6 (disable x2, set mode x2, enable x2) | 0                       |
| fbcon restore after Plymouth's last close                     | 1                           | 0                                  |
| DRM master holder chain                                       | plymouthd, nobody, seatd    | plymouthd, nobody (after `deactivate`), seatd; Plymouth exits later |
| bsm's first scanout of its own framebuffer vs Plymouth exit   | 4.9 s after                 | 0.4 s before (flip 62.52 s, quit 62.94 s guest) |
| First scene                                                   | n/a                         | `Presented` (greeter flipped, 20.6 s after the compositor started) |

Not seamless, and not claimed to be:

* The splash (flat near-black with the mark) is replaced by the greeter (blue gradient) in one flip,
  with no crossfade. That is a hard cut of colour, not a flash to black.
* The hold is bounded (60 s). A first staged run with 30 s timed out on a host at load 50
  (`first scene presented (TimedOut) after 46086 ms`, flat cover for 10.8 s, then the greeter): the
  cover replaced the splash and the greeter followed. A hold timeout is a visible step by design.
* VNC reports a picture when it changes, with a lag of seconds on a loaded host: the frame
  analysis cannot order events within that lag, so ordering claims use the tracer and the journal.
* Real hardware (a GPU whose firmware framebuffer is not the driver's, panel timing, multiple GPUs,
  efifb/simpledrm handover before the real driver loads) is not tested. In QEMU the real driver
  (virtio_gpu) was already bound when Plymouth started.

greetd boot, `bingux.bsm` absent (`greetd/`): greetd is the display manager, bsm, bsm-seatd and
bsm-helper.socket do not run, `display-manager.service` still points at greetd, `plymouth-quit`
ran and plymouthd is gone with the ordering drop-in installed, greetd's greeter is on the screen
(`greetd/greetd-greeter.png`). The greetd handover itself has black frames (4 black frames, about
30 s of black on a loaded host): it is the baseline, not something this change touches.

## Display-manager role

Package installation enables nothing and creates no alias: `bsm.service` has no `[Install]`
alias, `bsm.install` only creates accounts and prints how to opt in.
`bsm-set-display-manager` (root) is the only thing that creates or removes
`/etc/systemd/system/display-manager.service` for bsm:

```sh
bsm-set-display-manager status
bsm-set-display-manager enable            # greetd -> bsm, next boot
bsm-set-display-manager enable --force    # also replaces a display manager that is not greetd
bsm-set-display-manager disable           # back to the recorded one
```

Fixed in this phase, found by running it in the VM:

* `enable` refused to replace greetd's own alias without `--force` although greetd is the default.
  greetd is now replaced silently; any other alias still needs `--force`.
* `enable` disabled the old display manager before it knew `systemctl enable bsm.service` would work;
  a failure left the machine with no display manager. bsm is enabled first and nothing else is
  touched if that fails.
* `disable` with no record and an alias that is not bsm's would have run `systemctl enable greetd`
  over somebody else's alias. It now leaves that alone, and if re-enabling the recorded manager
  fails it puts bsm back.

## Fallback

`bsm.service` has `OnFailure=bsm-fallback.service`. **Measured:** systemd runs `OnFailure=` for every
failed start, including the ones `Restart=on-failure` retries, so the original fallback started
greetd one second into a crash loop, next to a bsm that restarted a second later (injected
`ExecStart=/bin/false`: three runs of the fallback for three attempts, greetd up at the first).
`/usr/libexec/bsm/fallback` now waits for the outcome (bsm active again: nothing to do; stopped:
nothing to do; failed after the start limit: greetd, `BSM_FALLBACK_MAX_WAIT` 300 s as the ceiling) and
the unit has `TimeoutStartSec=330`. In the VM with the new script greetd started 5.5 s after the
first failure, after `start-limit-hit`, with bsm left in `failed`. It also withdraws a splash that is
still up. Nothing touches the enabled units, so the next boot tries bsm again.

## Recovery: a root TTY and logs

* `Ctrl+Alt+F3` (or `chvt 3` as root) switches to a text console: bsm uses seatd's VT binding,
  logs `session paused (VT switched away)` and `session activated`, drops its input devices while away
  and re-adds them on return, and keeps its pid. On the first VM measurement (old binary, staged cold
  boot) `chvt 3`, `chvt 1` and `Ctrl+Alt+F3`, `Ctrl+Alt+F1` worked and the VNC stream saw the text
  console (one black frame) and then the greeter, with no black frame after returning. The new
  `session` phase repeats this with the current binary and also checks that typed keys reach the
  greeter again.
* The serial console of the live image (`ttyS0`) is a root shell.
* Logs: `journalctl -b -u bsm -u bsm-helper -u bsm-seatd -u bsm-fallback`, `bsmctl status`,
  `bsmctl monitor --json`; the splash hooks log as `bsm-plymouth-handover`.
* Rollback: remove `bingux.bsm` from the boot entry (image), `bsm-set-display-manager disable`
  (installed system), or `systemd.mask=bsm.service` for one boot. To boot to a text console:
  `systemd.unit=multi-user.target`.

## Restart of bsm and VT recovery

Measured (`session/`, staged boot, bsm pid 898, two displays):

* `chvt 3` and `Ctrl+Alt+F3` (QEMU monitor key) each reached VT 3 with a working root shell, and
  `chvt 1` / `Ctrl+Alt+F1` returned. After returning, every frame of head 0 was the greeter (no
  black frame, similarity to the picture before 0.9999 and 0.992), typed keys reached the greeter
  again (54 pixels changed), bsm kept its pid, and the journal has `session paused (VT switched
  away)` and `session activated`.

**Not verified: `systemctl restart bsm` with users logged in.** The `session` phase needs two
logins first; in the only run (`report-run3.txt`) the login of the first account did not reach the
reveal within 240 s (`no new_login_fade completion`), so the restart checks were `blocked`, not
passed. Not investigated here (host load 38, the walkthrough passes on an unloaded host according
to docs/bsm/packaging.md). What the design says (architecture invariant 9: recovered sessions come
back protected and the user authenticates) and what bsm-manager unit tests cover are in manager.md;
the VM behaviour is open. Rerun: `flock /tmp/bsm-vm.lock python3 tests/bsm-boot.py --stage DIR --only session`.
