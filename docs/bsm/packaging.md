# bsm packaging, boot and recovery

How bsm is installed, started, switched on and off, tested in a VM, and
recovered when it does not come up. The design is in
[architecture.md](architecture.md). Status of every item is in
[Status](#status).

bsm is opt-in. Installing the `bsm` package changes nothing about how a
machine logs in. Two things switch it on:

* the **live image** starts bsm when the kernel command line has `bingux.bsm`
  (a boot menu entry passes it);
* on an **installed system** an administrator runs
  `bsm-set-display-manager enable`.

Both leave greetd installed, so going back is one command or one reboot.

## What the package installs

| File in the repository                          | Installed as                                  | Purpose                                                                   |
| ----------------------------------------------- | --------------------------------------------- | ------------------------------------------------------------------------- |
| `packages/bsm` (`bsm`, `bsmctl`)                | `/usr/bin/bsm`, `/usr/bin/bsmctl`             | The compositor and session manager, and its CLI.                          |
| `packages/bsm-helper`                           | `/usr/libexec/bsm/bsm-helper`                 | Root helper for PAM and uid changes.                                      |
| `packages/bsm-host`                             | `/usr/libexec/bsm/bsm-host`                   | Bridge between bsm and one hosted Gnoblin.                                |
| `packages/bsm-appearance` (`bsm-imgconv`)       | `/usr/libexec/bsm/bsm-imgconv`                | Sandboxed avatar and wallpaper converter.                                 |
| `packaging/bsm/bin/bsm-fallback`                | `/usr/libexec/bsm/fallback`                   | Starts greetd when bsm failed for good.                                   |
| `packaging/bsm/bin/bsm-set-display-manager`     | `/usr/bin/bsm-set-display-manager`            | Switch the display-manager role (never run by the package).               |
| `packaging/bsm/systemd/*`                       | `/usr/lib/systemd/system/`                    | `bsm.service`, `bsm-seatd.service`, `bsm-helper.socket`, `bsm-helper.service`, `bsm-fallback.service`. |
| `packaging/bsm/sysusers/bsm.conf`               | `/usr/lib/sysusers.d/bsm.conf`                | Accounts `bsm` and `bingux-greeter`.                                      |
| `packaging/bsm/tmpfiles/bsm.conf`               | `/usr/lib/tmpfiles.d/bsm.conf`                | `/run/bsm`, `/run/bsm/sessions`, `/var/lib/bsm`, the greeter's home.      |
| `packaging/bsm/share/tmpfiles-active.conf`      | `/usr/share/bsm/tmpfiles-active.conf`         | Applied by `bsm.service` only, see [Privacy](#privacy).                   |
| `packaging/bsm/dbus/org.bingux.Session1.conf`   | `/usr/share/dbus-1/system.d/`                 | Bus policy: `bsm` owns `org.bingux.Session1`; every local account may call it, bsm decides per call. |
| `packaging/bsm/polkit/50-bsm.rules`             | `/usr/share/polkit-1/rules.d/`                | See [Polkit and logind](#polkit-and-logind).                              |
| `packaging/bsm/pam/bsm-login`, `bsm-unlock`     | `/etc/pam.d/`                                 | PAM services of the helper.                                               |

The package is built by `distro/packages/bsm/PKGBUILD` from the same source
archive as `bingux`. It installs no PAM file for the greeter (the greeter does no
authentication) and no `Alias=display-manager.service`.

The greeter itself, `bingux-bsm-greeter`, and its QML belong to the `bingux`
package. `bsm` depends on `bingux` and `gnoblin`; nothing in `bingux` or
`gnoblin` depends on `bsm`.

## Boot order

```text
plymouth-start -> ... -> plymouth-quit-wait ---+
systemd-logind, dbus ----------------------+   |
bsm-seatd.service (root, seat devices) ----+---+--> bsm.service (User=bsm)
bsm-helper.socket -------------------------+          |  Type=notify, WatchdogSec=30
                                                      +--> bsm-helper.service (root, socket-activated)
                                                      +--> greeter + hosted sessions (started by the helper)
```

* `bsm.service` is `WantedBy=graphical.target` and `Before=graphical.target`:
  bsm takes the display-manager role and does not replace `graphical.target`.
  It `Conflicts=` and orders after `getty@tty1.service`, like greetd and GDM.
* `After=plymouth-quit-wait.service` is the ordering greetd already has in this
  repository (`greetd-plymouth.conf`): the splash releases the display before bsm
  asks for DRM master. Taking the display over *without* a blank frame (keeping
  the splash until the greeter has rendered) is not done; it is listed under
  [Status](#status).
* `bsm-seatd.service` is bsm's own seatd on `/run/bsm-seatd/seatd.sock`,
  group `bsm`. bsm does not use the distribution's `seatd.service`, so a
  different socket path or group there cannot break it. bsm reaches it through
  `LIBSEAT_BACKEND=seatd` and `SEATD_SOCK`.
* `bsm.service` reports `READY=1` after the compositor is ready, the D-Bus name
  is acquired and the control socket is bound, and sends `WATCHDOG=1` at half of
  `WatchdogSec`. A unit that stops reporting is restarted
  (`Restart=on-failure`, at most three starts per minute).
* `OnFailure=bsm-fallback.service` runs `/usr/libexec/bsm/fallback` when the
  start limit is hit. It writes a runtime drop-in that empties greetd's
  `ConditionKernelCommandLine` for this boot and starts greetd, so the machine
  never stays without a login. The enabled units are not touched, so the next
  boot tries bsm again.

### Hardening and what is deliberately not set

`bsm.service` runs as `bsm` with an empty capability set, `NoNewPrivileges`,
`ProtectSystem=strict` (writable: `/run/bsm`, `/var/lib/bsm`,
`/var/lib/bingux/greeter`), `ProtectHome=yes`, a private `/tmp`, address
families `AF_UNIX` and `AF_NETLINK` only, and no realtime or SUID. Not set:

* `PrivateDevices`: bsm opens the GPU render node.
* `MemoryDenyWriteExecute`: Mesa drivers generate code at run time.
* `SystemCallFilter` and `RestrictNamespaces`: not profiled against the GPU
  stack yet.

`bsm-helper.service` has **no sandboxing and no limits at all**: no
`ProtectSystem`, `ProtectHome`, `PrivateTmp`, `PrivateDevices`, `NoNewPrivileges`,
mount namespace, seccomp filter, capability bounding set or rlimit, and not even
the harmless-looking `LimitCORE=0`, `SystemCallArchitectures=native` or
`RestrictRealtime=yes`. The greeter and every hosted desktop are started by that
process and inherit all of them: a read-only `/usr`, no setuid binaries
(`unix_chkpwd`, polkit's agent helper), no core dumps, no 32-bit programs, no
realtime threads for audio. The helper protects its own secrets with
`PR_SET_DUMPABLE=0`. `KillMode=process` keeps hosted sessions alive when the
helper restarts; `pam_systemd` moves each session's supervisor into the user's
logind scope, outside the helper's cgroup, and the next helper adopts it.

## Switching bsm on and off

### Installed system

```sh
sudo bsm-set-display-manager status
sudo bsm-set-display-manager enable          # next boot: bsm is the login manager
sudo bsm-set-display-manager enable --force  # replace a display manager that is not greetd
sudo bsm-set-display-manager disable         # back to the one that was replaced
```

`enable` records the display manager it replaces in
`/var/lib/bsm/previous-display-manager`, disables it, enables `bsm.service` and
`bsm-helper.socket`, and points
`/etc/systemd/system/display-manager.service` at `bsm.service`. It refuses to
replace an alias that points at something else unless `--force` is given, so a
GDM alias is never clobbered by accident. It never starts or stops a running
login manager, so it is safe to run from a graphical session; the change takes
effect at the next boot. `disable` removes the alias and enables the recorded
display manager again.

Removing the `bsm` package removes the alias and enables greetd again
(`pre_remove` in `bsm.install`).

The same by hand:

```sh
sudo systemctl disable --now bsm.service bsm-helper.socket   # careful: --now ends the graphical session
sudo rm -f /etc/systemd/system/display-manager.service
sudo systemctl enable greetd.service
```

### Live image: the `bingux.bsm` kernel parameter

The image carries drop-ins under `/etc/systemd/system` (source:
`packaging/bsm/image/`, laid over the image by `distro/container-build.sh`):

| Unit                                   | Condition                         |
| -------------------------------------- | --------------------------------- |
| `bsm.service`, `bsm-seatd.service`, `bsm-helper.socket`, `bsm-helper.service` | `ConditionKernelCommandLine=bingux.bsm` |
| `greetd.service`                       | `ConditionKernelCommandLine=!bingux.bsm` |

`bsm.service` and `bsm-helper.socket` are enabled in the image, so with
`bingux.bsm` bsm runs and greetd does not; without it nothing changes from the
greetd image. The boot menus have the entry *Bingux OS live with bsm session
manager, experimental* (GRUB, syslinux and systemd-boot). The live account is
`live` with password `live`, as always.

To try bsm from the default entry, edit the boot entry and append `bingux.bsm`
(systemd-boot: press `e`; GRUB: press `e`; syslinux: press `Tab`).

## Recovery

* **Blank or frozen screen.** `Ctrl+Alt+F3` for a text console. bsm uses
  seatd's VT binding, so VT switching is meant to work; this is not verified
  yet (see [Status](#status)). A root shell is also on the serial console
  (`ttyS0`) in the live image.
* **bsm does not start.** After three failed starts in a minute,
  `bsm-fallback.service` starts greetd, so a login screen appears anyway.
  `systemctl status bsm` shows why.
* **Boot straight to a text login.** Add `systemd.unit=multi-user.target` to the
  kernel command line, then `bsm-set-display-manager disable`.
* **Boot without bsm once.** Remove `bingux.bsm` from the command line (image) or
  add `systemd.mask=bsm.service` (installed system).
* **Restore greetd for good.** `sudo bsm-set-display-manager disable`.

### Logs

```sh
journalctl -b -u bsm -u bsm-helper -u bsm-seatd -u bsm-fallback
journalctl -b -u bsm -p warning
bsmctl status          # read-only; any local user
bsmctl sessions
bsmctl outputs
bsmctl transitions
bsmctl monitor --json  # live event stream, one JSON object per line
```

Logs carry session ids and generations, never credentials. The helper logs PAM
service names and result codes only.

## Privacy

bsm shows the last user's avatar and wallpaper before anyone signs in. For that
it keeps a **greeter-safe copy** of each account's avatar and wallpaper (resized
and re-encoded by `bsm-imgconv`) in `/var/lib/bingux/greeter/<uid>/`.

* The copies are readable by the `bingux-greeter` account and by `bsm`, on a disk
  that may be unencrypted even when `/home` is encrypted. Anyone with the disk
  can see them.
* An account opts out with `show_personal_images = false` (greeter profile); bsm
  then publishes no image paths for it and the greeter shows the neutral
  fallback. Existing copies are removed on the next publication.
* Appearance records are whitelisted values (scheme, accent, clock format,
  reduced motion); no path of a user's home directory is ever given to the
  greeter. bsm does not open paths under a home directory at all: the user's own
  publisher passes image file descriptors.
* The greeter Quickshell runs as `bingux-greeter` with a private home and
  runtime directory, loads only the system QML, and connects to bsm's brokered
  sockets.
* `/var/lib/bingux/greeter` is shared with the greetd login screen, where it is
  sticky and world-writable (`bingux-greeter.conf`) so each account can publish
  its own snapshot. While bsm runs, `bsm.service` takes it over (`ExecStartPre=`
  applies `tmpfiles-active.conf`: owner `bsm`, mode 0755), so a hostile local
  account cannot create a `<uid>` directory there first. The next boot without
  bsm restores the shared mode.

## Polkit and logind

A hosted session is a logind session without a seat (decision D4). Polkit treats
such a session as not local, so desktop actions that default to "allow on the
active local session" ask for an administrator password instead.
`/usr/share/polkit-1/rules.d/50-bsm.rules` handles two subjects, and returns
"not handled" for everything else, so no default is weakened for anyone else:

1. **The `bsm` account** may call `org.freedesktop.login1` power off, reboot and
   suspend (and their `-multiple-sessions` variants). bsm checks the caller
   itself first: a hosted caller through polkit `CheckAuthorization` for the
   single or the `-multiple-sessions` action depending on the other running
   sessions, the greeter through `[greeter_power]` in `/etc/bsm/bsm.toml`
   (default: suspend, reboot and power off allowed), then the inhibitors; only
   then does bsm call logind, behind its cover. `-ignore-inhibit` is never
   granted.
2. **Hosted sessions.** A subject whose logind session reports `Service=bsm-login`
   (the PAM service of hosted logins; only root can create a logind session, so
   an unprivileged process cannot claim it), `Class=user`, `Remote=no`, and
   `Active=yes` gets `YES` for a fixed list of actions whose own `allow_active`
   default is `yes`: login1 power off, reboot and suspend; udisks2 mounting,
   unlocking, ejecting and powering off the user's own removable media;
   NetworkManager network control, scanning, enable/disable and own and system
   connection settings; power-profiles-daemon switching; colord (except
   `install-system-wide`). The greeter account gets nothing.

Everything else keeps its default and therefore stays denied or asks for an
administrator: `*-other-seat`, `*-other-user`, `*-system`, `halt`, firmware,
package and hostname changes. A direct `systemctl reboot` by a hosted user is
allowed like on any local desktop but bypasses bsm's cover; the cover only exists
for requests that go through bsm.

Verified: the rule's logic was run in a JavaScript engine against subjects and
actions (granted: the `bsm` account's power actions, a `bsm-login` session's
whitelist; not granted: `-ignore-inhibit`, `-system`, a session of another
service, a subject without a session). **Not verified**: polkitd evaluating the
file, `loginctl show-session` output for a seatless session, polkit's `active`
for it, `pkcheck` per action, PipeWire and camera access for seatless sessions
(hosted users get the `audio` and `video` groups instead of seat ACLs). These
need the booted image; the checklist is in [Status](#status).

## Testing

| Command                                            | Runs                                                                    |
| -------------------------------------------------- | ----------------------------------------------------------------------- |
| `python3 tests/bsm-frames.test.py`                 | Unit tests of the frame measurements on synthetic fades and slides.     |
| `python3 tests/bsm-headless-e2e.py`                | `bsm --backend headless` with the test helper; skips (exit 77) until bsm offers it. |
| `python3 tests/bsm-qemu.py [--iso ISO] [--smoke]`  | The acceptance walkthrough in QEMU with two displays.                   |
| `python3 tests/bsm-vm-inject.py stage` then `python3 tests/bsm-qemu.py --inject /tmp/bsm-vm/inject` | The same against an older image, with freshly built binaries injected over 9p. |

`tests/bsm-qemu.py` needs QEMU with OVMF, `ffmpeg`, a host render node
(`BINGUX_DISTRO_DISPLAY=sw` uses software graphics) and about 8 GB of free memory
for the guest. It writes to `distro/out/bsm-test/`: `checks.json` (name to
bool; a blocked step counts as false), `steps.json` (passed, failed, blocked or
skipped, with the reason), `report.txt`, `serial.log`, PNG stills per display, CSV
series of fade opacity and slide offset per display, the progress milestones with
times, `bsmctl` dumps, the monitor log, and the journal of the bsm units. The
video and the working files stay under `/tmp/bsm-vm` (`BSM_VM_DIR`).

Exit status: 0 every step passed, 1 a check failed, 2 nothing failed but a step
is blocked.

### Two displays

One `virtio-vga-gl` device with `max_outputs=2` gives the guest one DRM card with
two connectors (`Virtual-1`, `Virtual-2`). QEMU reports the second as
`disconnected` until the display side hands it a size; the harness asks that
head's VNC server for a 1280x800 desktop (`SetDesktopSize`), after which both
connectors are `connected` and both heads deliver frames. Each head has its own
VNC server and recorder. Checked against the 2026.10.24 image: boot passes with
both connectors connected and 1280x800 frames on both heads.

### What the frame checks measure

Frames are sampled about 40 times a second per display (the video itself runs at
10 fps). On the decimated luma plane:

* **Fade over the greeter** (new login): per frame, the opacity of the desktop on
  top of the greeter, taken as the median of `(frame - greeter) / (desktop -
  greeter)` over the pixels where the two differ. The test asserts it stays under
  0.06 until bsm reports `animating`, then rises monotonically from under 0.1 to
  over 0.9 with at least three intermediate frames, each intermediate frame fits
  `(1 - a) * greeter + a * desktop` within a small error (the greeter stays visible
  under the rising desktop, no black flash), and both displays cross 50 % within
  150 ms of each other.
* **Slide up** (unlock, switch back): per frame, the translation `y` for which the
  top `H - y` rows equal the locked scene's rows `y..H` and the bottom `y` rows
  equal the desktop. The test asserts `y` rises monotonically from 0 to the full
  height, the revealed strip is the existing desktop, and early speed exceeds late
  speed (ease-out).
* **Greeter killed while locked**: no frame in the next 12 s is more than 12 %
  desktop, and every frame is the lock scene or an opaque cover.

These measurements are unit-tested on synthetic transitions
(`tests/bsm-frames.test.py`). The scenario itself has **not** been run against a
booted bsm: no image contains bsm yet.

### Hot injection

`tests/bsm-vm-inject.py stage` builds the crates into `/tmp/bsm-target/packaging`
and runs the PKGBUILD's own `package()` into `/tmp/bsm-vm/inject/root`, so the
staged tree is the package. `tests/bsm-qemu.py --inject DIR` shares it with the
guest (`-virtfs`, tag `bsminject`), copies it over the live root, creates the
accounts, reloads D-Bus and polkit, stops greetd and starts bsm. Limits:

* Host builds link the Fedora host's libraries. The injected binaries need the
  same sonames in the Arch guest; `apply.sh` runs `ldd` and prints every missing
  library (`--build container` compiles in an Arch container instead).
* Gnoblin is not injected. Hosted mode is part of the Gnoblin build, so steps that
  need a hosted Gnoblin report `blocked` until the image is rebuilt.
* `--shell` packs the QML tree into a new `bingux-shell.sxa`; that is enough for
  QML-only changes of the greeter. C++ plugin changes need the image.

## Troubleshooting

| Symptom                                       | Check                                                                                          |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Greetd login appears instead of bsm           | `cat /proc/cmdline` has no `bingux.bsm`; or `bsm-fallback` ran: `journalctl -u bsm-fallback`.  |
| `bsm.service` fails at once                   | `journalctl -u bsm -b`. Missing `bsm-seatd`: `systemctl status bsm-seatd`. No GPU: the DRM backend needs `/dev/dri/card*` and a free VT. |
| `bsm-helper.socket` inactive                  | `systemctl status bsm-helper.socket`; the socket is `/run/bsm-helper/helper.sock`, owner `bsm`, mode 0600. |
| Login says the password is wrong              | `journalctl -b -u bsm-helper`; try `su - USER`. Lockout comes from `pam_faillock`: `faillock --user USER`. |
| Desktop never appears after the progress bar  | `bsmctl monitor --json` shows the stage that stopped; the greeter offers Retry after a failure. `journalctl -b --user-unit gnoblin* _UID=UID`. |
| Two greeters, or a black screen on one monitor | `bsmctl outputs`; `ls /sys/class/drm/*/status`. Hotplug is handled by bsm; a stuck output needs `systemctl restart bsm`, which ends hosted sessions' foreground state (they come back locked). |
| `bsmctl` says access denied                   | Read-only commands work for any local user; `lock` and `logout` only for the caller's own session; `select-user` for root. |

## Status

Values as in architecture.md. Items under "Image rebuild" must be picked up by the
next `distro/build.sh` run; it has not been started.

| Item                                                    | Status                  | Evidence |
| ------------------------------------------------------- | ----------------------- | -------- |
| Units verify                                            | implemented and tested  | `systemd-analyze verify --root` on a staged tree: `bsm`, `bsm-seatd`, `bsm-helper` (service and socket), `bsm-fallback` load without error (the seatd man page is missing on the Fedora host only). |
| D-Bus policy                                            | implemented and tested  | `xmllint --noout`. Loaded by a bus: unverified. |
| Polkit rules                                            | implemented but unverified | Logic run in Node with a stub `polkit`; not evaluated by polkitd. |
| sysusers, tmpfiles                                      | implemented but unverified | Syntax follows the other files in the repository; not run on Arch. |
| PKGBUILD, `bsm.install`                                 | implemented but unverified | `bash -n`; not built with `makepkg` (no Arch toolchain on the host; the image build runs it). `packages/bsm` has no `Cargo.toml` yet. |
| Boot entries and image drop-ins                         | implemented but unverified | Entry text follows the existing entries; the SMBIOS `kernel-cmdline-extra` path puts `bingux.bsm` on the command line (tested). Drop-ins not booted. |
| Two-display QEMU harness                                | implemented and tested  | Smoke run against the 2026.10.24 image: two connected connectors, two frame streams. |
| Acceptance walkthrough                                  | implemented but unverified | Steps run only to "blocked": the image has no bsm. |
| Frame measurements                                      | implemented and tested  | `tests/bsm-frames.test.py`, 10 tests. |
| Hot injection                                           | implemented but unverified | See the run log in the commit message of the inject change. |
| Plymouth handover without a blank frame                 | not yet implemented     | bsm waits for `plymouth-quit-wait`, as greetd does. |
| VT switching away from bsm (`Ctrl+Alt+F3`)              | not yet implemented     | Depends on seatd's VT binding; untested. |
| `bsm.rules` against a booted seatless session           | not yet implemented     | Needs the image. |

Image rebuild must pick up: the `bsm` package (all five crates, `packages/bsm`
needs its `Cargo.toml` and lock files), `packaging/bsm/image/` (copied by
`build_iso`), the three boot entries, `bsm` and `seatd` in `packages.x86_64`, a
Gnoblin build with `--hosted`, and the `bingux` package with the greeter QML and
`bingux-bsm-greeter`.
