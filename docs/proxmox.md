# Proxmox validation

Bingux builds a bootable installation ISO for disposable Proxmox validation.
This repository does not contain a Proxmox API client. Use an
operator-owned runner or the Proxmox API directly. Keep the runner and its
credentials outside this repository.

The validation runner must upload the ISO as `content=iso`, create a 64 GiB
disposable VM with the name `bingux-install-<vmid>` and tag `bingux-pve-test`,
attach the ISO as `ide2`, set `ide2;scsi0` as the boot order, and start the VM
with 8 vCPUs and 8192 MiB of memory by default. It must not install NixOS
automatically. Use the Proxmox console to complete and inspect the
installation.

## Build an installation ISO

Build the profile-specific ISO without creating a `result` symlink:

```sh
iso=$(nix build --no-link --print-out-paths .#bingux-kieran-install-iso)
state_dir="${XDG_STATE_HOME:-$HOME/.local/state}"
test -r "$iso"
```

Use `.#bingux-generic-install-iso` to test the generic profile. Bingux uses the
current NixOS `image.modules` interface for this output. The CachyOS profile
installer disables ZFS support because its kernel module and the Nixpkgs ZFS
userspace package do not have a supported matching version.

## Proxmox-installed system output

Use the PVE host output when installing the system onto a Proxmox disk:

```sh
nix build --no-link --print-out-paths \
    .#nixosConfigurations.bingux-kieran-pve-vm.config.system.build.toplevel
```

The PVE host output keeps the standalone `bingux-kieran-vm` output unchanged.
It disables the `qemu-vm.nix` 9p shared-directory mounts because Proxmox does
not provide those devices, and it selects `/dev/sda` as the bootloader device
for the validation disk. Use `bingux-kieran-vm` for the NixOS-generated
standalone QEMU VM, where the `virtio-root` boot device and 9p mounts exist.

## CachyOS binary cache

When a profile selects a CachyOS kernel, Bingux configures the CachyOS cache in
the installed NixOS system. An initial build on another multi-user Nix host can
still build the kernel locally if that host does not already trust the cache.

Do not add a developer account to Nix `trusted-users` to solve this. Nix treats
that setting as equivalent to root access. Instead, have the host administrator
verify the cache source and add its URL and public key to the host Nix
configuration:

```conf
extra-substituters = https://attic.xuyh0120.win/lantian
extra-trusted-public-keys = lantian:EeAUQ+W+6r7EtwnmYjeVwx5kOGEBpjlBfPlzGlTNvHc=
```

Without that host-level configuration, the ISO remains reproducible but the
CachyOS kernel can take a long time to build locally.

## Secret boundary

Set the API token secret through `PVE_API_TOKEN_FILE`. The file must contain one
secret token line and have restrictive local permissions. Do not pass the token
on a command line, commit it, add it to a Nix expression, or write it into an
evidence directory.

On an installed Bingux host, a profile can declare a SOPS-Nix entry and point
`PVE_API_TOKEN_FILE` at the resulting `/run/secrets/<name>` file. During local
development, use an existing secret manager or a temporary mode-0600 file that
is outside this repository. The profile SOPS bootstrap process is defined in
`docs/architecture.md`.

`bingux.secrets.entries` defaults to a root-owned `0400` file. An external
runner started by the profile user cannot read that default. Give the profile
user ownership when the runner reads the token:

```nix
bingux.secrets.entries.pve-api-token = {
    key = "pve-api-token";
    owner = config.bingux.user.name;
    mode = "0400";
};
```

The runner needs these environment variables:

| Variable | Meaning |
| --- | --- |
| `PVE_API_URL` | HTTPS Proxmox API root, including `/api2/json`. |
| `PVE_API_TOKEN_ID` | Token identity in `USER@REALM!TOKEN` form. |
| `PVE_API_TOKEN_FILE` | Path to the token secret file. |
| `PVE_NODE` | Proxmox node that owns the VM. |
| `PVE_ISO_STORAGE` | Storage that accepts ISO content. |
| `PVE_VM_STORAGE` | Storage that accepts VM disks. |
| `PVE_BRIDGE` | Linux bridge for the VirtIO NIC. |
| `PVE_VM_CORES` | Optional VM vCPU count; defaults to 8 and accepts 1 through 128. |
| `PVE_VM_MEMORY_MIB` | Optional VM memory in MiB; defaults to 8192 and accepts 512 through 1048576. |
| `PVE_CA_BUNDLE` | Optional CA bundle for a private Proxmox certificate authority. |

Set the non-secret values in the current shell. Keep the token file path outside
the repository where possible:

```sh
export PVE_API_URL="https://pve.example:8006/api2/json"
export PVE_API_TOKEN_ID="user@realm!bingux"
export PVE_API_TOKEN_FILE="/run/secrets/pve-api-token"
export PVE_NODE="pve"
export PVE_ISO_STORAGE="local"
export PVE_VM_STORAGE="local-lvm"
export PVE_BRIDGE="vmbr0"
export PVE_VM_CORES="8"
export PVE_VM_MEMORY_MIB="8192"
```

## Least-privilege token

Use a dedicated token scoped to the node and the selected storages. Grant only
the capabilities required by this workflow:

| Capability | Used for |
| --- | --- |
| `Datastore.AllocateTemplate` | Uploading the installation ISO. |
| `VM.Allocate` | Allocating and deleting the disposable VM. |
| `Datastore.AllocateSpace` | Allocating the VM disk on `PVE_VM_STORAGE`. |
| `SDN.Use` | Attaching `PVE_BRIDGE` when the network is backed by Proxmox SDN; omit it for a non-SDN bridge. |
| `VM.Audit` | Reading the VM power state before cleanup. |
| `VM.PowerMgmt` | Starting, gracefully shutting down, and force-stopping the VM. |

Do not grant an administrator role or put the token secret in the command line,
repository, or evidence directory.

## Validation sequence

Use this sequence with the external runner or the Proxmox API. The repository
does not prescribe a runner command or an evidence directory.

1. Build the profile-specific ISO with the command in
   [Build an installation ISO](#build-an-installation-iso).
2. Run the runner's dry-run mode, if it has one. Confirm the ISO path, target
   node, storage names, bridge, VM name, and ownership tag. A dry run must not
   read the token file or contact Proxmox.
3. Upload the ISO to the selected storage with `content=iso`. Wait for the
   upload task to reach a terminal result before creating the VM.
4. Allocate a disposable VM on the selected node. Set the name to
   `bingux-install-<vmid>`, set the tag to `bingux-pve-test`, allocate the
   configured disk, attach the ISO as `ide2`, set `ide2;scsi0` as the boot
   order, and attach the configured VirtIO bridge.
5. Start the VM and wait for the start task to reach a terminal result. Retain
   redacted task records outside this repository.
6. Inspect the console and verify the installer boot, systemd units, session
   files, portal, desktop shell, tray, dock, notifications, OSD, and search
   paths.
7. Before cleanup, read the VM configuration and verify both the exact
   `bingux-install-<vmid>` name and the `bingux-pve-test` tag. Refuse cleanup
   when either value does not match.
8. Read the power state. Delete a stopped owned VM directly. For a running
   owned VM, request the QEMU graceful-shutdown endpoint and wait for its task.
   If graceful shutdown fails or times out, request the explicit QEMU stop
   endpoint and wait for that task before deleting the VM.
9. If a shutdown task has an unknown or non-terminal result, do not issue the
   delete request. Retain the VM and evidence for manual inspection.

The uploaded ISO remains in Proxmox storage by design. Remove it only after
the validation evidence is retained. Keep cleanup evidence redacted and
outside this repository.

## Sources

- NixOS image variants: <https://nixos.org/manual/nixos/stable/#sec-image-nixos-rebuild-build-image>
- Nix configuration and trusted users: <https://nix.dev/manual/nix/latest/command-ref/conf-file.html>
- Proxmox VE API: <https://pve.proxmox.com/pve-docs/api-viewer/index.html>
