# Proxmox validation

Bingux validates a bootable installation ISO on Proxmox. The Proxmox command is
developer tooling. It is exposed as `nix run .#pve-test`; it is not installed in
a Bingux host or profile.

The command uploads an ISO as `content=iso`, creates a 64 GiB disposable VM
named `bingux-install-<vmid>` and tagged `bingux-pve-test`, attaches the ISO as
`ide2`, sets `ide2;scsi0` as the boot order, and starts the VM with 8 vCPUs and
8192 MiB of memory by default. It does not install NixOS automatically. Use the
Proxmox console to complete and inspect the installation.

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

`bingux.secrets.entries` defaults to a root-owned `0400` file. A command run
from the profile user shell cannot read that default. Give the profile user
ownership when the token is used by `nix run`:

```nix
bingux.secrets.entries.pve-api-token = {
    key = "pve-api-token";
    owner = config.bingux.user.name;
    mode = "0400";
};
```

The command needs these environment variables:

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

## Validate before creating a VM

Run a dry run first. It checks command arguments and prints the intended API
sequence, including every POST field. It does not read the token file or contact
Proxmox.

```sh
nix run .#pve-test -- create \
    --iso "$iso" \
    --evidence-dir "$state_dir/bingux/pve-evidence" \
    --dry-run
```

Build the focused command check before use:

```sh
nix build .#checks.x86_64-linux.pve-development-tool
```

## Create and inspect the VM

Create the VM only after the dry run is correct:

```sh
nix run .#pve-test -- create \
    --iso "$iso" \
    --evidence-dir "$state_dir/bingux/pve-evidence"
```

The command prints the allocated VMID and a new evidence directory. It writes
redacted Proxmox task records there. It does not destroy a VM that starts
successfully. Inspect the Proxmox console, then verify the installer boot,
systemd units, session files, portal, desktop shell, tray, dock, notification,
OSD, and search paths.

Use `--destroy-on-failure` only for a failed creation that has written evidence.
The command starts automatic cleanup only after every request that can change
the VM has a terminal task result. A terminal non-OK result can trigger cleanup.
If the create or start request is still running, times out, or has an unknown
result, the command retains the VM and evidence for manual inspection. It
deletes only a VM with the Bingux name and tag. The uploaded ISO remains in
Proxmox storage by design. Remove it only after the validation evidence is
retained.

Destroy the VM explicitly when testing is complete:

```sh
nix run .#pve-test -- destroy \
    --vmid <allocated-vmid> \
    --evidence-dir "$state_dir/bingux/pve-evidence"
```

Destroy first verifies that the VM is named `bingux-install-<vmid>` and tagged
`bingux-pve-test`. It refuses all other VMs, including VMs created by an older
Bingux version without the tag. Remove an older disposable VM manually after
you verify its identity. For an owned VM, it reads the power state. It deletes
an already stopped VM without a power request. For a running VM, it requests
the QEMU graceful-shutdown endpoint and waits for its task to finish before
deleting the VM. If graceful shutdown fails or times out, it requests the
explicit QEMU stop endpoint and waits for that task before issuing the delete.
If both shutdown attempts fail, the delete is not attempted. The fallback is
destructive, so use it only for a disposable VM.

The dry-run plan shows the ownership check, state probe, graceful shutdown,
bounded task wait, force-stop fallback, and delete sequence without contacting
Proxmox. All cleanup evidence stays in the local evidence directory and is
redacted before it is written.

## Sources

- NixOS image variants: <https://nixos.org/manual/nixos/stable/#sec-image-nixos-rebuild-build-image>
- Nix configuration and trusted users: <https://nix.dev/manual/nix/latest/command-ref/conf-file.html>
- Proxmox VE API: <https://pve.proxmox.com/pve-docs/api-viewer/index.html>
