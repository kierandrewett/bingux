# Bingux execution checklist

This file tracks implementation work. A checked item means the related configuration builds and has the stated evidence.

## Constraints

- Keep `banks/` untouched. It is user-owned data and is outside the flake.
- Do not commit credentials, private keys, generated hardware configuration, or decrypted SOPS data.
- Keep user-specific state in `profiles/<name>/`. Keep reusable system behaviour in modules.
- Do not weaken CPU security mitigations by default. Any unsafe performance option must be explicit and disabled by default.
- Use Proxmox only through a token read from the environment or SOPS. Never place a token in this repository or command output.

## Phase 1: Discovery and contracts

- [x] Confirm that the current Bingux tree is empty and preserve `banks/`.
- [x] Map Gnoblin layer-shell, foreign-toplevel, and Super-key integration points.
- [x] Identify Fedora, Flatpak, Cargo, pipx, and global Node package inventory sources.
- [x] Verify all selected Nix inputs, package attributes, and NixOS module interfaces.
- [x] Record the cross-repository Gnoblin-to-Bingux protocol contract.

## Phase 2: Reproducible NixOS foundation

- [x] Add a pinned flake with NixOS, Home Manager, SOPS-Nix, Flatpak, CachyOS kernel, and Gnoblin inputs.
- [x] Add a generic host constructor and profile schema.
- [x] Add a VM host that needs no machine-specific disk configuration.
- [x] Add the generic system, networking, audio, graphics, and Nix modules.
- [x] Add configurable CachyOS kernel variants and safe compiler-performance defaults.
- [x] Add a profile-local SOPS layout and an age bootstrap path without plaintext secrets.
- [x] Evaluate the base flake and build the VM closure.

## Phase 3: Kieran profile and migration

- [x] Add the Kieran profile as a consumer of the generic profile interface.
- [ ] Add declarative Rust, C++, TypeScript, container, and terminal toolchains.
- [ ] Add a curated application set and Flatpak declarations.
- [ ] Add an inventory command that exports current Fedora, Flatpak, Cargo, pipx, and Node application candidates without private configuration.
- [ ] Document temporary and declarative package installation.
- [x] Evaluate the Kieran VM profile.

## Phase 4: Gnoblin flake integration

- [x] Add a reproducible Gnoblin flake package and NixOS module in `~/dev/gnoblin`.
- [x] Add the minimal, versioned `org.gnoblin.Shell` Super-release signal.
- [ ] Disable Gnoblin-native notification and OSD ownership for the Bingux session.
- [x] Expose the custom Gnoblin session to the display manager.
- [ ] Build Gnoblin through the Bingux flake with a local-input override.

## Phase 5: Bingux desktop shell

- [x] Define the versioned desktop-shell and search-provider contracts.
- [x] Add the layer-shell top bar with clock, tray, metrics, privacy, input, network, audio, and power controls.
- [x] Add the dock with foreign-toplevel window actions, application menus, window counts, scroll cycling, and launch behaviour.
- [ ] Add the Super-release Spotlight surface and provider host.
- [ ] Add indexed application, file, SQLite, calculation, weather-cache, and AI providers.
- [ ] Add native notifications and OSD surfaces with no dependence on GNOME Shell UI.
- [ ] Add benchmark coverage for warm indexed search latency.

## Phase 6: Proxmox validation

- [ ] Add a Proxmox VM build and deployment command that reads all credentials from SOPS or environment variables.
- [ ] Build the NixOS QCOW2 image and create an isolated Bingux VM.
- [ ] Boot the VM, check the systemd units, session files, portal, and desktop-shell services.
- [ ] Exercise tray, dock, notification, OSD, Super-release, and search workflows in the VM.
- [ ] Record the VM identifier and destroy it only after validation evidence is retained.

## Phase 7: Review and hand-off

- [ ] Run structural, security, performance, and duplication reviews.
- [ ] Simplify code where review finds real complexity or duplication.
- [ ] Run all flake checks, Rust tests, and the relevant NixOS VM test.
- [ ] Update architecture, profile, package-management, secrets, and Proxmox operations documentation.
- [ ] Commit each validated increment with a conventional commit message.
