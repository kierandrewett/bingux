# Bingux architecture

Bingux is a NixOS configuration framework. It provides reusable system modules and a profile boundary. It does not select a desktop environment, window manager, shell, user name, application set, or hardware target.

## Configuration layers

| Layer | Location | Responsibility |
| --- | --- | --- |
| Flake | `flake.nix` | Pins shared dependencies and exposes configurations. |
| Host | `hosts/<name>/` | Describes machine or virtual-machine facts. |
| Generic modules | `modules/` | Defines reusable NixOS options and safe system defaults. |
| Profile | `profiles/<name>/` | Selects personal software, user data, secrets, and optional desktop integration. |

`lib/mk-host.nix` imports the generic modules, then one selected profile, then host-specific modules. The selected profile is the user-specific NixOS module boundary: it can set the supported `bingux.*` options and supported NixOS or Home Manager options needed by that profile. Host modules must not contain personal configuration.

## Selection rules

The generic configuration can set safe shared operating-system defaults, such as firewall and network behaviour. It must not enable profile-specific desktop UI, application sets, user-specific services, system identity, or an unsafe performance setting.

The flake can pin sources that only one profile uses. Pinning a source makes it reproducible. A flake may import an optional module globally to expose its options; the selected profile must still enable and configure the source it uses.

## Kernel and performance policy

`modules/system/performance.nix` supports Nixpkgs Zen and XanMod package sets plus pinned CachyOS BORE variants. The default remains the Nixpkgs kernel. The user must select another package set in a profile or host.

CPU vulnerability mitigations remain enabled by default. `bingux.performance.disableCpuMitigations` is opt-in and must only be set after a local threat-model decision.

## Optional network clients

`bingux.networking.tailscale.enable` starts the system `tailscaled` service and
starts `tailscale systray` as the profile user in graphical sessions. The
normal-user client publishes a StatusNotifierItem. The Bingux tray renders it
and delegates its application menu, including right-click actions.

Tailscale login state remains system state. Do not put an auth key in the Nix
configuration. If unattended enrolment is needed, provide the key as a
runtime-only SOPS secret and use it outside the Nix store.

## Profile secrets

The secrets module supports encrypted profile data in `profiles/<name>/secrets/`, with
`bingux.secrets.defaultSopsFile` pointing at the encrypted file and
`bingux.secrets.entries` declaring each decrypted file. The age key defaults to
`/var/lib/sops-nix/key.txt`. `bingux-secrets-init` is installed only when
`bingux.secrets.enable = true`. The committed Kieran profile is currently
bootstrap-only: it enables `bingux.secrets`, but has no committed `.sops.yaml`,
`defaultSopsFile`, entries, or encrypted secret file. No working profile secret is
present in this repository.

The first host bootstrap has two stages:

1. Enable `bingux.secrets` with no entries, then run `sudo bingux-secrets-init` on the host. The command creates the root-owned age private key outside the Nix store at the default `/var/lib/sops-nix/key.txt` path and prints only its public recipient.
2. Add that public recipient to `profiles/<name>/secrets/.sops.yaml`, encrypt the profile secret file locally, declare its entries, and deploy the profile again.

Commit the encrypted file and public recipients. Do not commit an age private key, a plaintext secret file, or a copied `/run/secrets/` file. Keep `bingux.secrets.age.generateKey` disabled after bootstrap. If the private key is lost, a replacement key cannot decrypt existing profile secrets.

## Gnoblin profile contract

Gnoblin is an optional profile desktop choice. It is not part of the Bingux generic system contract.

A profile that selects Gnoblin needs these stable integration points:

- Gnoblin implements `zwlr_layer_shell_v1` for externally owned layer-shell surfaces.
- Gnoblin implements `zwlr_foreign_toplevel_manager_v1` so an external dock can list and control application windows.
- Gnoblin must not own the notification or on-screen-display user interface for this session. Bingux provides those surfaces when the profile selects its desktop shell.
- Gnoblin sets `hasNotifications` to `false` for the Bingux session mode. This
  removes native MessageTray banners but leaves the notification backend and
  portal support available to an external notification service.
- Gnoblin emits `org.gnoblin.Shell.OsdRequested` on
  `/org/gnoblin/Shell` when its master OSD feature or a matching `osd-*`
  feature suppresses a native OSD request. Its `(uissddas)` payload is
  `[protocolVersion, monitorIndex, icon, label, level, maxLevel, outputNames]`.
  Bingux supports protocol version `2` only. `outputNames` identifies the
  physical Mutter connectors in the target logical monitor. Gnoblin logs and
  drops a suppressed OSD that has no usable handoff. It does not restore native
  OSD ownership after a failed handoff. When Gnoblin renders an OSD itself, it
  emits no handoff signal.
  `bingux-statusd` validates the signal and forwards it through the local OSD
  socket. The QML process does not subscribe to the D-Bus interface directly.
- Gnoblin emits `org.gnoblin.Shell.SuperReleased` on `/org/gnoblin/Shell`
  after Super is released with no other input. Its `(ut)` payload is
  `[protocolVersion, monotonicUsec]`. Bingux supports protocol version `1`
  only and ignores other versions. The event is a one-way edge, not key state.
  The Bingux search service uses it to show the search surface. Holding Super
  has no action in the shell.
- The Bingux desktop-shell process owns its own layer-shell surfaces. It does not patch or depend on GNOME Shell UI.

`docs/desktop-shell.md` defines the Bingux desktop-shell, socket, and
search-provider contracts. It is the source of truth for the interface between
the shell and its provider host.

The consumed D-Bus method and signal names are explicit in this contract. Another profile can use a different desktop choice without a compatibility layer.

## Output and profile matrix

The current flake exposes these outputs:

| Output | Profile or host | System | Purpose |
| --- | --- | --- | --- |
| `nixosConfigurations.bingux-vm` | `generic` | `x86_64-linux` | NixOS VM test host. |
| `nixosConfigurations.bingux-kieran-vm` | `kieran` | `x86_64-linux` | Kieran-profile NixOS VM test host. |
| `packages.<system>.bingux-statusd` | — | `x86_64-linux`, `aarch64-linux` | Status daemon package. |
| `packages.<system>.bingux-searchd` | — | `x86_64-linux`, `aarch64-linux` | Search daemon package. |
| `packages.<system>.bingux-inventory` | — | `x86_64-linux`, `aarch64-linux` | Inventory package. |
| `packages.x86_64-linux.bingux-generic-install-iso` | `generic` | `x86_64-linux` | Installer image, embedding `bingux-generic.iso`. |
| `packages.x86_64-linux.bingux-kieran-install-iso` | `kieran` | `x86_64-linux` | Installer image, embedding `bingux-kieran.iso`. |

The repository does not expose a Proxmox API client. Use an operator-owned
runner or the Proxmox API directly for disposable VM validation. This keeps
credentials and destructive infrastructure operations outside the system
configuration repository.

The repository currently exposes no real hardware host. The installer image
outputs are x86_64-only. `hosts/iso/default.nix` disables ZFS only when the
selected kernel name starts with `cachyos-`; Kieran selects CachyOS, while the
generic profile keeps the default Nixpkgs kernel.

## Installation images and Proxmox

`hosts/iso/` adds the `bingux-installer` image variant through the current
`image.modules` interface. It does not import an image-building module into a
host configuration. This keeps the normal system configuration separate from
the installer image derivation.

Proxmox validation uses the installer ISO and an external operator-owned
runner. The runner must upload the ISO, create a disposable VM with an
ownership name and tag, boot it, retain redacted task evidence, and delete the
VM only after validation. `docs/proxmox.md` defines the required secret
boundary and API sequence.
