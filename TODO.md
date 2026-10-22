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
- [ ] Black greeter: run 4 (2026.10.21 ISO) never showed the greeter clock, black
      from 167 s to 737 s. It recurred on the 2e59048 ISO: fin2 and fin3 never
      showed the login card (13 checks failed each; no `first greeter frame` in
      the timeline). fin1 and boots 2-3 passed. Cause open; `/tmp/h3/repro.py`
      not yet run.
- [x] Capture: frames come from the VNC framebuffer only (QMP screendump removed,
      `tests/distro_capture.py`, commit 862e055).
- [x] Session, greeter and lock detection (`tests/distro_capture.py`): greeter by
      its login card; session by the bar clock text, a lit top strip and blue
      wallpaper. The bar-text test rejects the Plymouth splash, which matched the
      old session test. Lock by the lock clock box.
- [ ] Verify the GL renderer (`virtio-vga-gl`) in QEMU. Not verified: no run records
      which display path was taken, and the serial log and reports contain no
      renderer string. Add a probe that reads the compositor's GL renderer.
- [ ] Run 3's boot took 57 s between the end of the GRUB menu and the kernel frame.
      Not explained: the serial log has no timestamps. The menu was GRUB's 15 s
      timeout; `distro/profile/grub/grub.cfg` and `loopback.cfg` now use 5 s, which
      removes 10 s from every boot (the menu always waits its full timeout). Target:
      power-on to greeter is 10 s shorter than the same ISO before this change;
      measure on the next ISO, three runs, timeline medians.
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
- [ ] Plymouth to greeter (fix 5): `distro/packages/bingux-os` now ships
      `greetd.service.d/plymouth.conf` (`After=plymouth-quit-wait.service`). Target:
      no text flash between splash and greeter in the ISO frame check. Not verified
      here: needs a VM frame capture.
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
