# Bingux TODO

This file tracks work that still affects the standalone desktop shell. Finished
work belongs in Git history and does not stay here as a second changelog.

## Release blockers

- [ ] Build the RPMs in a clean Fedora 43 chroot (mock or COPR). `make rpm-package`
      builds all six on a Fedora 43 host, with the host's toolchain and `--nodeps`.
- [ ] Install the RPMs with `dnf` into a clean user account, start the user target
      and open Settings and Files from a Gnoblin session.
- [ ] Decide how COPR gets the Rust crates and the prebuilt Skia archive
      (`design/userspace.md`, question 8).
- [ ] Merge the app packages into the Bingux OS image and sign the pacman
      repository (`design/userspace.md`, section 5 and question 9).
- [ ] Verify Gnoblin login, logout, lock and unlock with the packaged shell.
- [ ] Verify a compositor without Gnoblin still starts the general shell.
- [ ] Package or document a matching Qt 6 QMLTermWidget build for the terminal.
- [ ] Publish the first COPR build only after the clean install check passes.

## Shell maintenance

- [x] Replace remaining direct `qs` launches with the installed launcher or an
      explicit `BINGUX_QUICKSHELL` value.
- [x] Move optional Python helpers behind clear feature checks.
- [x] Keep every long-running helper at core limit zero and test crash recovery.
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
- [x] Add a short troubleshooting page for Qt, layer-shell and helper sockets.
- [x] Add a contributor guide with formatting, tests and package checks.
