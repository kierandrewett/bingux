# Bingux execution checklist

This file tracks implementation work. A checked item means the related configuration builds and has the stated evidence.

## Emoji picker and Electron

- [x] Fold skin-tone variants into one entry and save the preference beside search.
- [x] Verify actual Discord search input on native Wayland and persist its launch flags.
- [x] Prefer native text-input caret geometry, with bounded accessibility fallback.
- [x] Verify Win+Period, caret placement and Enter insertion in Electron inputs and editable content.
- [x] Check empty inputs, Unicode sequences, input blur rejection and GTK insertion. See `docs/emoji-picker.md` for the verified scope.

## Sidebar note commands

- [x] Reference Notion's published slash-command and block lists.
- [x] Add searchable commands with arrow keys, Enter, Escape and pointer selection.
- [x] Verify headings, inline formatting, lists, tables, undo and saved notes in the editor.

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
- [x] Verify all selected Nix inputs, package attributes, and NixOS module interfaces through flake evaluation and targeted builds.
- [x] Record the cross-repository Gnoblin-to-Bingux protocol contract.

## Phase 2: Reproducible NixOS foundation

- [x] Add a pinned flake with NixOS, Home Manager, SOPS-Nix, Flatpak, CachyOS kernel, and Gnoblin inputs.
- [x] Add a generic host constructor and profile schema.
- [x] Add a VM host that needs no machine-specific disk configuration.
- [x] Add the generic system, networking, audio, graphics, and Nix modules.
- [x] Add configurable CachyOS kernel variants and safe compiler-performance defaults.
- [x] Add the age bootstrap path without plaintext secrets; leave profile secret data unconfigured until a recipient and encrypted file exist.
- [x] Build the base flake and VM closure, retaining build evidence.

## Phase 3: Kieran profile and migration

- [x] Add the Kieran profile as a consumer of the generic profile interface.
- [x] Add declarative Rust, C++, TypeScript, container, and terminal toolchains.
- [x] Add a curated application set and Flatpak declarations.
- [x] Add an inventory command that exports current Fedora, Flatpak, Cargo, pipx, and Node application candidates without private configuration.
- [x] Document temporary and declarative package installation.
- [x] Build/evaluate the Kieran VM profile, retaining build evidence.

## Phase 4: Gnoblin flake integration

- [x] Add a reproducible Gnoblin flake package and NixOS module in `~/dev/gnoblin`.
- [x] Add the minimal, versioned `org.gnoblin.Shell` Super-release signal.
- [x] Disable Gnoblin-native notification and OSD ownership for the Bingux session; the desktop-shell module check asserts both disabled features.
- [x] Expose the custom Gnoblin session to the display manager.
- [x] Validate the Gnoblin input through the Bingux flake with `--override-input gnoblin path:$HOME/dev/gnoblin`.

## Phase 5: Bingux desktop shell

- [x] Define the versioned desktop-shell and search-provider contracts.
- [x] Add the layer-shell top bar with clock, tray, metrics, privacy, input, network, audio, and power indicators.
- [x] Start the Tailscale StatusNotifierItem for profiles that enable Tailscale.
- [x] Add the dock with foreign-toplevel window actions, application menus, window counts, scroll cycling, and launch behaviour.
- [x] Add the Super-release Spotlight surface and provider host.
  - [x] Implement the bounded local socket server and Super-release subscriber.
  - [x] Implement versioned provider manifests and concurrent provider lifecycle management.
  - [x] Implement indexed application, file, SQLite, calculation, weather-cache, and AI providers.
  - [x] Implement the keyboard-first Quickshell search surface and activation flow.
  - [x] Add focused protocol/unit checks and an ignored warm-query benchmark for the calculation completion path.
- [x] Add native notifications and OSD surfaces with no dependence on GNOME Shell UI.

## Phase 6: Proxmox validation

- [x] Define the external Proxmox validation boundary, secret requirements, and ownership sequence.
- [x] Build the x86_64 NixOS installation ISO and create an isolated Bingux VM, retaining runtime evidence.
- [x] Boot the VM, check the systemd units, session files, portal, and desktop-shell services.
- [x] Exercise status, metrics, Super-release, and search workflows in the VM.
  - [x] Exercise the status and search sockets, and retain visual evidence for the top bar and Super-release search surface.
  - [x] Complete fresh graphical checks for session unlock, live metrics, the top-bar search button, Super-release, and an application result.
- [x] Exercise tray, dock, and notification interactions in disposable VM; the headless guest did not provide a hardware-backed Gnoblin `OsdRequested` producer for direct OSD runtime evidence.
- [x] Record the validation VM identifier (`2100`) and retain the runtime evidence.
- [x] Destroy validation VM `2100` after validation.

> VM `2100` (`bingux-install-2100`) was stopped and destroyed after the final committed system was deployed. Runtime evidence covers the top bar, Tailscale tray registration and fallback glyph, search overlay and activation, dock application menu and window cycling, and desktop notifications.
>
> The OSD surface, protocol, and socket checks pass. Direct OSD producer behaviour remains unverified because the headless guest did not generate a real `org.gnoblin.Shell.OsdRequested` event. Verify that path on a physical or hardware-backed session.
>
> The repository intentionally has no Proxmox API client. Use an operator-owned runner and a token read from the environment or SOPS for future VM operations.

## Phase 7: Review and hand-off

- [x] Run structural, security, performance, and duplication reviews.
- [x] Simplify code where review finds real complexity or duplication; keep intentional repetition unchanged.
- [x] Run all flake checks, Rust tests, and the relevant NixOS VM test.
- [x] Update architecture, profile, package-management, secrets, and Proxmox operations documentation.
- [x] Commit each validated increment with a conventional commit message.

## Shared notification presentation

- [x] Share notification cards across the desktop, control centre, and flat app dock lists. Verify gestures, history, normal/reduced motion, and rendered spacing.

## Region snapping

- [x] Identify the existing Tiling Shell layouts and import them once.
- [x] Keep the picker and previews in Bingux and window operations in Gnoblin.
- [x] Add top-edge selection, Ctrl-drag regions, and Super+Z keyboard selection.
- [x] Verify real Wayland and X11 input, cancellation, focus, and size restoration.
- [x] Verify the live picker, imported layouts, registered shortcut, and reserved work area.
