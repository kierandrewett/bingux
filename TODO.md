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
- [ ] Answer Gnoblin's `gnoblinctl session lock` request by starting
      `bingux-lock` from the shell (idle timeout and logind `Lock`).
- [ ] Let `SessionBar` show the user's own extensions once an extension
      capability contract exists; today `contexts` is recorded only.
- [ ] Verify the greeter on real hardware: fingerprint hint, multi-monitor,
      the exit into a fresh session.

## Bingux OS

- [ ] Build the live image (`distro/build.sh`) and run `tests/distro-boot.py`.
- [ ] Boot the image on real hardware and install with archinstall.
- [ ] Publish the pacman repository the image is built from.

## Shell maintenance

- [ ] Replace remaining direct `qs` launches with the installed launcher or an
      explicit `BINGUX_QUICKSHELL` value.
- [ ] Move optional Python helpers behind clear feature checks.
- [ ] Keep every long-running helper at core limit zero and test crash recovery.
- [ ] Record real hardware evidence for the OSD producer path.

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
- [ ] Add a short troubleshooting page for Qt, layer-shell and helper sockets.
- [x] Add a contributor guide with formatting, tests and package checks.
