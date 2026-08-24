# Bingux architecture

Bingux is a NixOS configuration framework. It provides reusable system modules and a profile boundary. It does not select a desktop environment, window manager, shell, user name, application set, or hardware target.

## Configuration layers

| Layer | Location | Responsibility |
| --- | --- | --- |
| Flake | `flake.nix` | Pins shared dependencies and exposes configurations. |
| Host | `hosts/<name>/` | Describes machine or virtual-machine facts. |
| Generic modules | `modules/` | Defines reusable NixOS options and safe system defaults. |
| Profile | `profiles/<name>/` | Selects personal software, user data, secrets, and optional desktop integration. |

`lib/mk-host.nix` imports the generic modules, then one selected profile, then host-specific modules. A profile can change only its declared options. Host modules must not contain personal configuration.

## Selection rules

The generic configuration can set safe shared operating-system defaults, such as firewall and network behaviour. It must not enable profile-specific desktop UI, application sets, user-specific services, system identity, or an unsafe performance setting.

The flake can pin sources that only one profile uses. Pinning a source makes it reproducible. It does not enable that source in a generic configuration. A profile must import and configure every optional source that it uses.

## Kernel and performance policy

`modules/system/performance.nix` supports Nixpkgs Zen and XanMod package sets plus pinned CachyOS BORE variants. The default remains the Nixpkgs kernel. The user must select another package set in a profile or host.

CPU vulnerability mitigations remain enabled by default. `bingux.performance.disableCpuMitigations` is opt-in and must only be set after a local threat-model decision.

## Profile secrets

A profile stores encrypted secret data in `profiles/<name>/secrets/`. It configures `bingux.secrets.defaultSopsFile` with that encrypted file and declares each decrypted file through `bingux.secrets.entries`.

The first host bootstrap has two stages:

1. Enable `bingux.secrets` with no entries, then run `sudo bingux-secrets-init` on the host. The command creates the root-owned age private key outside the Nix store and prints only its public recipient.
2. Add that public recipient to `profiles/<name>/secrets/.sops.yaml`, encrypt the profile secret file locally, declare its entries, and deploy the profile again.

Commit the encrypted file and public recipients. Do not commit an age private key, a plaintext secret file, or a copied `/run/secrets/` file. Keep `bingux.secrets.age.generateKey` disabled after bootstrap. If the private key is lost, a replacement key cannot decrypt existing profile secrets.

## Gnoblin profile contract

Gnoblin is an optional profile desktop choice. It is not part of the Bingux generic system contract.

A profile that selects Gnoblin needs these stable integration points:

- Gnoblin implements `zwlr_layer_shell_v1` for externally owned layer-shell surfaces.
- Gnoblin implements `zwlr_foreign_toplevel_manager_v1` so an external dock can list and control application windows.
- Gnoblin must not own the notification or on-screen-display user interface for this session. Bingux provides those surfaces when the profile selects its desktop shell.
- Gnoblin emits a versioned `org.gnoblin.Shell` D-Bus signal when the Super key is released. The Bingux search service uses this signal to show its search surface. Holding Super has no action in the shell.
- The Bingux desktop-shell process owns its own layer-shell surfaces. It does not patch or depend on GNOME Shell UI.

The exact D-Bus method and signal names must be documented in Gnoblin before Bingux consumes them. This keeps the protocol explicit and lets another profile use a different desktop choice without a compatibility layer.
