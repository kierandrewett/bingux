# Bingux TODO

This file tracks work that still affects the standalone desktop shell. Finished
work belongs in Git history and does not stay here as a second changelog.

## Release blockers

- [ ] Build the native RPM in Fedora 43 x86_64.
- [ ] Install the RPM into a clean user account and start the user target.
- [ ] Verify Gnoblin login, logout, lock and unlock with the packaged shell.
- [ ] Verify a compositor without Gnoblin still starts the general shell.
- [ ] Package or document a matching Qt 6 QMLTermWidget build for the terminal.
- [ ] Publish the first COPR build only after the clean install check passes.

## Login screen and lock screen

- [x] Bingux greeter for greetd on Gnoblin's greeter mode (`docs/login-screen.md`).
- [x] Shared `SessionScreen` for the lock screen and the greeter, with the
      session bar, session contexts and the capability gate.
- [x] Compositor background on both sides of the login so no frame is black.
- [x] Answer Gnoblin's `gnoblinctl session lock` request by starting
      `bingux-lock` from `bingux-statusd`.
- [x] Lock, unlock with the password and log out in the QEMU test
      (`tests/distro-boot.py`), with Gnoblin `a05f29ae`, `2317ff42` and
      `4f33ee52` (frame callbacks, lock surface paint, commits after the lock
      surface is gone). Not tried on real hardware.
- [ ] Lock on idle timeout and on logind `Lock` (lid close, suspend). Both
      reach Gnoblin as the same lock request; only the idle policy is missing.
- [ ] Adopt `StatusBar` in the desktop top bar, greeter, lock and kiosk; the component exists and `tests/status-bar-parity.py` matches the desktop top bar.
- [ ] Let the locked `StatusBar` show the user's own extensions once an extension
      capability contract exists; today `contexts` is recorded only.
- [ ] Verify the greeter on real hardware: fingerprint hint, multi-monitor,
      the exit into a fresh session.

## Bingux OS

- [x] Build the live image (`distro/build.sh`) and run `tests/distro-boot.py`:
      greeter, sign-in, wallpaper, lock, unlock and log out in headless QEMU.
- [x] Install to a blank virtual disk with `bingux-os-install` (archinstall),
      boot it through its own bootloader and sign in: `tests/distro-install.py`.
- [x] Document how the pacman repository is published
      (`distro/publish-repo.sh`, `distro/README.md`).
- [x] Name the image Bingux OS everywhere the user sees it: boot menus, splash
      (Plymouth `bingux` theme), motd, `/etc/os-release` and `/etc/issue`; checked by
      `tests/distro-branding.py`.
- [ ] Rename the remaining Arch strings the user can see: the choose-mirror
      tool and the archinstall package name stay, the pacman mirror list stays Arch's.
- [ ] Publish the repository for real and sign its packages and database; today
      it is unsigned (`SigLevel = Optional TrustAll`).
- [ ] Boot the image on real hardware and install there. The installer needs
      UEFI; a BIOS-only machine is not supported yet.
- [ ] Let `bingux-os-install` pick the disk from a list, and offer encryption.

## Boot and handover baseline

- [x] Five back-to-back `tests/distro-boot.py` runs on the 2026.10.21 ISO
      (`distro/out/baseline/`): runs 1, 3, 5 passed; run 2 failed 8 checks, run 4
      failed 13. Submit-to-session-bar: run 1 10 s, run 3 18 s, run 5 11 s; run 2
      318 s (second typing at 411 s); run 4 never started a session.
- [x] Run 2 cause, in the harness: it slept 10 s and typed the password while the
      screen still showed the bare wallpaper; the keys were lost and the greeter
      sat at an empty field until a second typing. `tests/distro-boot.py` now waits
      for the greeter clock, and retypes once if the greeter is still up after a
      minute. Not re-run.
- [x] Black installer kiosk (c0c2c3a ISO, 4 of 4 boots, `MUTTER_DEBUG=kms` log in
      one): `distro/installer/gnoblin/installer.lua:12` called `os.getenv`, and
      Gnoblin's Lua has no `os` library ("attempt to index a nil value (global
      'os')"). Gnoblin then runs its built-in defaults: black background, no
      `bingux-installer-ui`, nm-applet and the other XDG autostart programs. Fixed:
      `~/.config/gnoblin/conf.d` path, and the greeter's `bingux-statusd` autostart is
      off (the live account's user manager already runs one; the second exits on the
      busy socket and Gnoblin shows "bingux-statusd exited with status 1"). Checked
      in the booted ISO by replacing the file and restarting the compositor: the
      installer card paints, with no error panel. Test:
      `tests/installer-gnoblin-config.py` (Gnoblin's own loader).
- [ ] Gnoblin (owner): a fallback configuration leaves the screen black. With the
      broken config the compositor paints two frames and stops: last
      `BACKEND: Unclipped stage paint` at 02:15:34.468, page flip presented at
      02:15:34.541, then no stage paint for minutes; virtio-gpu fences 15/15, plane 35
      keeps FB 47 (allocated by gnoblin); pointer motion and a new client do not
      repaint. `chvt 2; chvt 1` repaints at once and shows the "Configuration failed
      to load" panel. Same on a compositor restart with the broken config. The
      recovery panel must queue a stage redraw when it appears.
- [x] Black live installer on 5f53de8.iso: UEFI runs 1, 3, 4, 5 and BIOS run 6 of the E2E
      agent, and 5 of 5 boots in my VMs on a loaded host (`~/.cache/bingux-black2/runs/boot1..5/bundle.txt`;
      in-guest replays `exp2` to `exp6`). Two failures, one cause: the guardian waits for the
      helper units before it starts the supervisor.
      - Gnoblin's guardian runs `systemctl --user start gnoblin-session.target`
        (`gnoblin-runtime-environment.c:441`, called at `gnoblin-runtime.c:265`) before it spawns the
        session supervisor (`gnoblin-runtime.c:323`) that sends the compositor its configuration. The
        compositor stops waiting 10 s after it started (`gnoblin-control-connection.c:235`). The start job
        waited for `bingux.target`, so for `bingux-greeter-export.service` (run 1: "Consumed 4.257s CPU
        time over 8.199s wall clock time", then "Reached target Bingux helper services" and "Reached target
        Gnoblin graphical session" in the same second). Login to that target: 14 s (run 1), 15 s (run 3),
        19 s (run 4), 16 s (run 5), 19 s (run 6); 7 s in the good run 2.
      - A. The compositor times out first. My boot 3: `session-last.log` "05:08:10 compositor spawned
        pid=828", "05:08:25 session supervisor spawned pid=1133", "compositor pid=828 exited
        status=1"; `compositor-last.log`: "Failed to receive Gnoblin runtime config: timed out waiting
        for Gnoblin runtime configuration". The guardian exits, greetd starts the greeter (runs 3, 5, 6;
        the greeter hit the same slow start in runs 3 and 5 and stayed black). Gaps spawn to supervisor
        in my five boots: 13, 22, 15, 20 and 18 s.
      - B. The compositor survives and the Lua runtime falls back (runs 1 and 4: supervisor and worker
        pids far apart, worker with `--resume-runtime-worker`, nm-applet registered). Journal of a
        replay with the new script (`exp5/new1.txt`): "gnoblin: runtime supervisor: compositor did not
        complete the private runtime handshake" (10 s after the supervisor), then "restarting Lua
        runtime worker", "configuration fell back to the built-in defaults: The Lua runtime restarted;
        built-in defaults are being applied"; or "could not suspend Mutter for supervisor recovery;
        keeping the session alive: Mutter did not acknowledge worker suspension" and the note
        `$XDG_RUNTIME_DIR/gnoblin/config-fallback` reads "Lua runtime recovery failed. The compositor is
        still running, but configuration and Lua callbacks are unavailable" (`exp4/new1.txt`).
        `installer.lua` never loads, `bingux-installer-ui` never starts.
      - Not Xwayland: no Xwayland error in any `compositor-last.log` ("Using public X11 display :0");
        the installer comes up in good boots. The hand-run abort ("Failed to open Xauthority file")
        had no logind session.
      Measured on four busy CPUs: `systemctl --user start gnoblin-session.target` took 6.54 s and
      0.41 s with `bingux.target` `DefaultDependencies=no` (`exp1`). The first start of the compositor
      still takes 14 to 30 s on this host ("Running Mutter" to "Using Wayland display name"), even with
      the libraries in memory (a warm-up unit made no difference to the first try, so it is not
      shipped); the second takes 2 to 3 s.
      Fixed here: `bingux.target` `DefaultDependencies=no`; `bingux-greeter-export.service` and the
      image's `pacman-init.service` (9 s of CPU) yield the CPU; `bingux-live-session` starts the
      compositor again (3 tries) when it exits with an error, has no `compositor-v1.sock` after 60 s, or
      writes a fallback note, and logs to the journal (`journalctl -t bingux-live-session`). In-guest
      replay (caches dropped, 6 busy loops): with the old files 0 of 3 reached the installer (2 of the 3
      ended in the greeter); with the final script 9 of 9 did, each after one restart: 59 to 62 s
      with the busy loops (`exp5/new1,2`), 43 to 51 s without them (`exp6/new1,2`), and 35 to 50 s in the
      five runs that also had the libraries read first (`exp5/warm*`, `exp6/warm*`). Tests: `tests/live-session.test.py`,
      `tests/session-start-load.test.py`. Not run: a real E2E boot of a rebuilt ISO.
- [ ] Gnoblin (owner): the three start-up limits are fixed times, and a loaded machine passes them
      (see the item above for the log lines). (1) Spawn the supervisor before `activate_session()`, or
      start the target with `--no-block`: the compositor's 10 s clock runs while the guardian waits.
      (2) The worker's 10 s handshake limit (`STARTUP_TIMEOUT_MS`, `gnoblin-runtime-worker.c:340`) and the
      supervisor's 3 s `WORKER_CONFIG_TIMEOUT_MS` should count from the compositor's readiness, or be
      much longer: the first start takes 14 to 30 s here. (3) After "could not suspend Mutter for
      supervisor recovery" the session stays up with no Lua runtime and no way to start the shell:
      exit with an error so the login manager can start the session again.
- [ ] Black greeter on the 2026.10.21 and 2e59048 ISOs (run 4, fin2, fin3): in every
      black boot `pgrep -c -f bingux-greeter` was 3 or 2 (the Gnoblin processes,
      whose `--config` path contains `bingux-greeter`); in every good boot it was 4
      (the same plus the greeter client). So the greeter client never started: the
      compositor ran a fallback configuration, then hit the redraw bug above. Likely
      trigger (not proven): Gnoblin kills the first Lua worker when it has not sent a
      configuration within 3 s (`WORKER_CONFIG_TIMEOUT_MS`) and retries with embedded
      defaults; boots are slow and loaded at that moment. Not reproduced: on c0c2c3a,
      four greeter restarts with dropped caches and four busy CPUs all painted the
      login card, and no log showed a fallback. Those runs kept no Gnoblin log; next
      black boot, read `~/.local/state/gnoblin/session-last.log` and look for "did not
      produce a startup configuration in time". Update: on 5f53de8.iso the same slow start
      reproduced for the live session (previous item), through the compositor's 10 s limit and
      the worker's 10 s handshake limit; the 3 s worker limit did not fire in any log I kept.
      `bingux-greeter-session` has no restart loop yet, so a greeter that starts slowly can
      still stay black.
- [ ] Gnoblin (owner): first frame waits for `activate_session()`. The guardian spawns
      the session supervisor (which produces the compositor's configuration) only
      after the dozen `systemctl --user` and D-Bus commands; the compositor blocks on
      that configuration. Guardian to "Running Mutter": 7.3, 6.6, 6.0 and 5.5 s in four
      boots. Then 4.9 to 6.8 s more to "Added device" (cold squashfs reads).
- [x] Capture: frames come from the VNC framebuffer only (QMP screendump removed,
      `tests/distro_capture.py`, commit 862e055).
- [x] Session, greeter and lock detection (`tests/distro_capture.py`): greeter by
      its login card; session by the bar clock text, a lit top strip and blue
      wallpaper. The bar-text test rejects the Plymouth splash, which matched the
      old session test. Lock by the lock clock box.
- [ ] Verify the GL renderer (`virtio-vga-gl`) in QEMU. Not verified: no run records
      which display path was taken, and the serial log and reports contain no
      renderer string. Add a probe that reads the compositor's GL renderer.
- [x] Boot menu timeout: the UEFI ISO boots through systemd-boot (`loader.conf`,
      15 s from releng) and BIOS through syslinux (`TIMEOUT 150`); GRUB only reads
      `loopback.cfg`, so the earlier GRUB change did not reach a normal boot. Menu on
      screen 15.6 s in four boots. Now `efiboot/loader/loader.conf` and
      `syslinux/archiso_sys.cfg` wait 5 s, and syslinux's DEFAULT names `bingux`
      (releng's `arch` label does not exist). Expected: 10 s less per boot.
- [x] `ldconfig.service` on every live boot: the overlay root has no
      `/etc/.updated`, so `ConditionNeedsUpdate` held and the linker cache was rebuilt
      before `sysinit.target`: 20.7, 20.9, 17.1 and 20.4 s on the path to greetd.
      Every live entry now passes `systemd.condition_needs_update=0` (the image
      already has the cache, hwdb, catalogue and accounts from pacman's hooks).
      Expected: about 20 s less. `tests/distro-startup.py [--iso ISO]` checks both.
- [ ] Start-up costs left (c0c2c3a, host seconds since power-on, four boots): OVMF
      6-7 s; loader reads kernel and the 245 MB initramfs (105 MB of it Nvidia GSP
      firmware from the `kms` hook) in 4-12 s; kernel and initramfs 3.6-6.3 s;
      `bingux-os-firstboot` 1.5-2.1 s before greetd; Plymouth quits at 56.6-73.6 s and
      the console shows until the first compositor frame at 69.6-89.3 s (Gnoblin, see
      above); first installer card 3.8 s after the wallpaper on a cold QML cache,
      1.2-1.5 s warm (compositor restarts). `pacman-init.service` (9.6-11.7 s of key
      generation) runs during the compositor start.
- [ ] The installer's first frame is the bright, unblurred gradient for about 0.1 s
      (frame mean 53.7, then 37.9) before the dimmed background; seen on every
      installer start.
- [ ] Greeter user manager (fix 4): `bingux.target` pulled bingux-searchd,
      bingux-statusd, xdg-desktop-portal-bingux and bingux-greeter-export into the
      `greeter` account's user manager. Those five units now carry
      `ConditionUser=!greeter` and `ConditionUser=!greetd`. Target: none of the five
      start for `greeter`. Not done here: wireplumber, gnome-keyring and at-spi user
      units still start for it (third-party units, not shipped by this repo).
- [ ] Greeter gsettings readers (fix 4): Theme.qml's accent, colour-scheme and
      `gsettings monitor` processes now run only outside greeter and kiosk. Target: no
      `gsettings` process in the greeter process list. Baseline not measured.
- [ ] Greeter submit-to-session-bar (fix 4): baseline median 11 s (runs 1, 3, 5 =
      10, 18, 11 s). Target: median 8 s or less over five runs. Expected effect is
      from fewer greeter-side helpers; unmeasured until the next ISO run.
- [ ] QML precompile for the greeter and installer trees: not done. Quickshell loads
      loose `.qml` and this repo has no `qmlcachegen` step. Proposal: measure
      first; a cache step only if `tests/distro-boot.py` shows QML load as the cost.
- [ ] Plymouth to greeter (fix 5): the `greetd.service.d/plymouth.conf` drop-in
      changes nothing on Arch: greetd's own unit already has
      `After=systemd-user-sessions.service plymouth-quit-wait.service` (checked with
      `systemctl show greetd -p After` in the VM). The splash quits before greetd
      starts, so the text console shows for about 13-16 s until Gnoblin's first frame.
- [ ] Re-run `tests/distro-boot.py` on the new ISO with the fixed sign-in wait.
- [ ] Re-run `tests/distro-install.py` on an ISO that contains `bingux-installer`;
      the 2026.10.21 image predates the package and the test cannot pass on it.

## Shell maintenance

- [x] Build Quickshell into `bingux-shell` and run the shells from the packed
      archive; the installed command names are symlinks to the binary.
- [ ] Port the Python helpers (`shell/bingux/*.py`) to the Rust multicall
      binary `bingux-helper` (`packages/bingux-helper`), one helper at a time.
      QML runs them as `Helper.command("name", [args])`. Ported: `process-action`.
      Next: GIO and JSON helpers (launch-application, extensions, steam-games,
      session-activity, calendar-events, calendar-launch), then fold microphone,
      camera and control-centre state into `bingux-statusd`, capture and preview
      last. `Python.helper()` and `BINGUX_HELPER_DIR` go when the last file
      goes, with `python-gobject` and friends.
- [x] Replace remaining direct `qs` launches with the installed launcher or an
      explicit `BINGUX_QUICKSHELL` value.
- [x] Move optional Python helpers behind clear feature checks.
- [x] Keep every long-running helper at core limit zero and test crash recovery.
- [ ] Record real hardware evidence for the OSD producer path.

## Shell regression net

- [x] `tests/qmldir-complete.py` (in `make check`): every QML directory has a
      qmldir, every type file is listed (singleton-marked) or allow-listed, and
      every entry points at a file. `--committed` checks HEAD; pass a QML tree
      as an argument to check another tree.
- [x] `tests/lib/shell_source.py`: the one place that locates `shell.qml`
      blocks for splicing tests. A moved anchor fails with a clear message.
- [x] `tests/desktop-baseline.py`: headless sway, production shell, settles on
      two identical frames, then compares the full desktop and the top bar with
      `tests/fixtures/desktop-baseline/`. The clock and CPU/RAM readout are
      masked. Tolerance: 0.2% of pixels, each channel moving by at most 8.
      Run `QML_IMPORT_PATH=build/qml QS_TEST_BIN=build/bingux-shell/quickshell/src/bingux-shell python3 tests/desktop-baseline.py`; add `--update` to
      regenerate the references after an intended visual change.
- [ ] `tests/desktop-baseline.py` top-bar drift (542 px, 1.32%) fails on HEAD too; the reference is stale or the readout is not masked. Pre-existing.
- [ ] `tests/lock-ui.py` volume-key step can fail headless (pipewire Errno 112); it passed on HEAD and on the tree when re-run.

## Extensions

- [x] Discover XDG extension folders with explicit enable state.
- [x] Register QML widgets in the existing desktop layout.
- [x] Preserve placement when an extension is missing or disabled.
- [x] Provide actions, events, shared controls and anchored popups.
- [x] Test preview isolation, unload cleanup and fresh-process placement restore.
- [ ] Add the Home Assistant extension after its server and entity contract is
      chosen.

## Documentation

- [x] Keep the standalone build and install path as the public entry point.
- [x] Keep extension documentation separate from shell implementation details.
- [x] Add a short troubleshooting page for Qt, layer-shell and helper sockets.
- [x] Add a contributor guide with formatting, tests and package checks.

## Installer backend features (plan schema v2)

- [x] F1 storage and system options: xfs root, `ntp`, `root_password`, swap file, zram and swap
      partition (encrypted swap via crypttab). Unit tests in `distro/installer/tests/unit`; matrix
      cases `uefi-xfs`, `uefi-ext4-swapfile`, `uefi-btrfs-zram`, `uefi-luks-ext4-swappart`. Full
      matrix on ISO 1759213 (installer from the working tree overlay): 208 PASS, 0 FAIL.
- [x] F2 BIOS with LUKS2 root (GRUB, unencrypted ext4 `/boot`). Matrix `bios-luks-ext4` reaches
      the LUKS prompt and the greeter on the same run.
- [x] F3 autologin via greetd `initial_session` (installed config, user-disableable). Unit tests in
      `distro/installer/tests/unit/test_configure.py`; matrix case `uefi-ext4-autologin` passes:
      gnoblin runs for the user at boot and greetd keeps the greeter as the default session.
- [ ] F4 `free_space` and `alongside` (blivet shrink: ext4, btrfs, NTFS; os-prober and systemd-boot entries).
- [ ] F5 `manual` partitioning (explicit partition list, validated server-side).
