# bingux-installer

Disk installer backend for the Bingux OS live ISO. Two console scripts:

- `bingux-installer-daemon`: root daemon, socket-activated on `/run/bingux-installer.sock`.
- `bingux-installer`: client CLI (`probe`, `version`, `install --plan FILE|-`, `cancel`).

## Live session

The live image's greetd `initial_session` runs `bingux-live-session` as the
`live` account, so it boots straight into the installer. The session loop runs one
Gnoblin compositor at a time: the installer (`gnoblin/installer.lua`, which
autostarts `bingux-installer-ui`) or the normal live desktop.

- `bingux-live-switch installer` opens the installer. `bingux-live-switch live`
  is "Try Bingux": it stops the installer and opens the live desktop.
- The desktop entry `bingux-install-live` runs `bingux-live-switch installer`.
- Exiting the compositor with no pending switch ends the login, and greetd shows
  the greeter as usual.
- `bingux-installer-ui` runs `InstallerShell.qml` from `/usr/lib/bingux/bingux-installer.sxa`,
  packed by `scripts/pack-shell.py` from `shell/bingux` with `installer/shell` overlaid.

`tests/live-session.test.py` runs the loop and switch against a fake compositor.

## Status

Implemented (plan schema v2, covered by unit tests and the install matrix):

- Filesystems: ext4, btrfs and xfs for the root (`filesystem`). xfs loads the `xfs` module into
  the initramfs.
- Storage mode `erase` on UEFI and BIOS, unencrypted and LUKS2. BIOS with LUKS2 keeps an unencrypted ext4 `/boot` (see the layout section).
- Storage mode `free_space` on a GPT disk, UEFI and BIOS, unencrypted and LUKS2: existing partitions are kept; an existing ESP or BIOS boot partition is reused and not written. Refused with `no_free_space` or `not_gpt`.
- Storage mode `alongside` on a GPT disk, UEFI and BIOS, unencrypted and LUKS2: one existing ext4, btrfs or NTFS partition (not mounted) is shrunk to `size_bytes`, then the Bingux layout is created in the freed space as for `free_space`. The shrink resizes the filesystem before the partition entry, so a failed resize leaves the entry at its old size. Refused with `cannot_shrink`, `no_free_space` or `not_gpt`.
- Storage mode `manual` on a GPT disk, UEFI and BIOS, unencrypted or LUKS2: the plan's partition list is applied as written (`reuse` and `format` keep the disk's entries, `create` takes free space). The install matrix covers `uefi-manual` (unencrypted) only; the other combinations are implemented but not matrix-verified. Refused with `invalid_plan`, `no_free_space`, or `unsupported` for encryption with a `home` partition.
- `ntp`: `false` disables `systemd-timesyncd`.
- `root_password`: a string sets the root password; `null` locks the root account.
- `swap`: `zram` (zram-generator), `file` (`/swapfile`), `partition` (swap partition; encrypted
  swap uses a random-key crypttab entry when the root is encrypted). `none` writes no swap.
- `autologin`: a user name starts that user's Gnoblin session at boot (greetd `initial_session`); the greeter stays the default session, so later logins still show it. `null` turns it off.

Planned (validated by the schema but refused with `unsupported` until the feature lands):

- Dual-boot entries (os-prober for GRUB; the shared ESP for systemd-boot).

## Failure list

Every error code the daemon can return. "Left on disk" describes the state after the error.
Phases that fail after the commit point (first destructive write) always unmount the target,
unmount the payload and close the LUKS mapper before replying.

| Code | Trigger | Left on disk | User-visible message |
|---|---|---|---|
| `bad_request` | Line is not one UTF-8 JSON object, `op` missing or unknown, line over 64 KiB, `install` without a `plan` object | Nothing | `request is not JSON: <detail>`, `request must be a JSON object`, `request has no op`, `unknown op: <op>`, `request line is too long`, `request is not valid UTF-8` |
| `not_permitted` | Peer uid is not 0 and is not in group `bingux-install` (or the group does not exist) | Nothing | `peer is not root or a member of bingux-install` |
| `busy` | `install` or `probe` while another install runs (probe mounts partitions read-only, so it is refused too) | Nothing; the running install continues | `another installation is already running` |
| `invalid_plan` | Any plan schema rule fails, unknown top-level key, `confirm_erase` mismatch, firmware not matching the booted firmware | Nothing | `invalid plan: <field>: <rule>` |
| `unsupported` | `manual` with encryption and a `home` partition | Nothing | `<feature> is not implemented yet` |
| `no_free_space` | `free_space` (or `alongside`, or manual `create`) needs a region larger than the unallocated space, or the partition table has no free entry | Nothing | `the largest unallocated region is <n> MiB; the layout needs at least <m> MiB` |
| `not_gpt` | `free_space` on a disk with no readable GPT | Nothing | `<path> has no readable GPT; free_space needs one` |
| `cannot_shrink` | `alongside` names a partition that is not on the disk, is mounted, is not ext4, btrfs or NTFS, is smaller than its `min_bytes`, or `size_bytes` frees no space | Nothing | `<partition> is mounted; it must be unmounted to shrink` or `<fstype> cannot be shrunk` |
| `disk_not_found` | `disk` is not a whole-disk block device in the probe, or blivet does not see it | Nothing | `no disk <path> was found`, `blivet does not see <path>` |
| `disk_is_live_medium` | `disk` holds the live ISO | Nothing | `<path> is the live medium` |
| `disk_too_small` | Disk is under 8 GiB | Nothing | `disk is <n> bytes; at least 8589934592 bytes (8 GiB) is required` |
| `other_os_present` | Probe found another OS on the disk and `other_os` is `refuse` (`erase` only) | Nothing | `<path> holds <os list>; set other_os to allow to erase it` |
| `missing_tool` | A tool the install needs is not on `PATH` (checked before any write) | Nothing | `missing tools: <names>` |
| `cancelled` | Cancel accepted before the commit point; checked between phases | Nothing (no write has happened) | `install cancelled before the disk was modified` |
| `cancel_refused_after_commit` | `cancel` after the commit point | Install continues | `the disk is already being modified; cancel is refused` |
| `no_install` | `cancel` with no install running | Nothing | `no install is running` |
| `payload_failed` | Live payload `arch/x86_64/airootfs.sfs` or kernel `arch/boot/x86_64/vmlinuz-linux` missing (preflight, nothing written); squashfs mount, bind mount, kernel copy or rsync failed. After commit the disk is already erased. | Before commit: nothing. After: partial copy, not bootable; re-run the install | `payload_failed: <ExcType>: <detail>`, or `rsync failed (<exit>): <rsync error lines>` |
| `storage_failed` | Partitioning, shrink, LUKS, blkid or mkfs failed | Partition table and filesystems partly written; re-run the install, which reinitialises the disk | `storage_failed: <ExcType>: <detail>`, or `partitioning failed: <detail>` |
| `config_failed` | Target configuration (hostname, locale, users, initramfs, live-file stripping) failed | Payload copied, system not configured; re-run the install | `config_failed: <ExcType>: <detail>`, or `<tool> failed in the target: <stderr tail>` |
| `bootloader_failed` | `bootctl` or `grub-install`/`grub-mkconfig` failed | Installed system without a working boot entry; re-run the install | `bootloader_failed: <ExcType>: <detail>`, or `<tool> failed in the target` |
| `internal` | Unexpected exception; the traceback goes to the journal (`journalctl -u bingux-installer`) | Depends on the phase reached | `unexpected <ExcType> during install` or `unexpected failure in the installer daemon` |

Messages never contain passwords, the LUKS passphrase or hashes; any such value that reaches a
log line is replaced by `***` before it is sent.

## Licences

- `python-blivet` 3.14.2 (the storage library): LGPL-2.1-or-later. The source headers mix
  GPL-2.0-or-later and LGPL-2.1-or-later; the upstream tree ships `COPYING` (GPL-2.0) and
  `COPYING.LESSER` (LGPL-2.1), and both are installed by `distro/packages/python-blivet`.
- This installer (`bingux_installer`): GPL-3.0-or-later.
- Payload: the live root filesystem copied to the target is a set of Arch packages and Bingux
  packages. Each package keeps its own licence; the installer does not change any of them.

## Protocol

NDJSON over the unix socket `/run/bingux-installer.sock` (group `bingux-install`, mode 0660).
UTF-8, one JSON object per line, `\n` terminated. The client sends exactly one request line.
The server replies with zero or more event lines and then exactly one terminal line.

Requests:

- `{"op":"probe"}`: firmware, disks, missing tools. Refused with `busy` during an install.
- `{"op":"version"}`: `{"installer_version":"0.1.0","protocol":1}`.
- `{"op":"install","plan":{...}}`: plan schema v2 below.
- `{"op":"cancel"}`: sent on a separate connection. Sets the cancel flag of the running install.

Events (not terminal):

- `{"event":"progress","phase":"<name>","percent":<0-100 or null>,"message":"..."}`
- `{"event":"log","message":"..."}`

Terminal lines:

- `{"event":"result","ok":true,"data":{...}}`
- `{"event":"error","code":"<code>","message":"<human text>"}`

Probe result data (protocol 1; the fields below are additive, so a client that ignores them still works):

- `firmware`, `live_disk`, `tools_missing`, `installer_version`: as before.
- `disks`: each entry `{"path","size_bytes","model","removable","is_live_medium","other_os":[...],"partition_table","free_bytes","partitions":[...]}`. `partition_table` is `"gpt"`, `"msdos"` or `null`. `free_bytes` is the unallocated space on the disk, or `null` when unknown.
- `disks[].partitions`: each entry `{"path","fstype","label","size_bytes","mountpoint","used_bytes","free_bytes","os","shrinkable","min_bytes"}`. `label` is the filesystem label or `null`. `used_bytes` and `free_bytes` are the filesystem usage or `null`. `os` is the OS name found on the filesystem (as `other_os`) or `null`. `shrinkable` is `true` when the installer can shrink the filesystem for `alongside`; `min_bytes` is then the smallest size it can shrink to, otherwise `null`. A value the probe cannot read is `null`; it is never guessed.
- `capabilities`: what this installer build implements end to end, matching the Status section. A choice is listed only when its install is implemented and covered by the install matrix. Shape: `{"storage_modes":["erase","free_space","alongside","manual"],"filesystems":["ext4","btrfs","xfs"],"swap":["none","zram","file","partition"],"encryption":{"uefi":true,"bios":true},"dual_boot":false}`. The UI offers a choice only when it is listed. When `capabilities` is missing, the UI assumes exactly that Shape, so an older daemon still gets the Status list.

Install phases, in order: `validate`, `preflight`, `partition` (the commit point, the first destructive write), `mount`, `copy`, `strip`, `configure`, `bootloader`, `finish`.
Cancel is honoured only before the commit point. Only one install runs at a time.

Install result data: `{"disk":...,"firmware":...,"filesystem":...}`. The daemon never reboots.

The client CLI prints progress to stderr. `--json` prints the terminal line verbatim to stdout.
Exit status: 0 on `result ok`, 1 on a server error or when the daemon cannot be reached, 2 on usage errors (bad arguments, unreadable or invalid plan file).

## Plan schema v2

A JSON object. Unknown top-level keys are refused (`invalid_plan`). Schema v1 is refused: `version`
must be `2`.

- `version`: `2` (required).
- `disk`: whole-disk block device, `/dev/...` (required). Must not be the live medium.
- `confirm_erase`: must equal `disk` exactly. Required when `storage.mode` is `erase` or `manual`
  (both may destroy data on the disk). Refused otherwise.
- `firmware`: `"auto"` (default), `"uefi"` or `"bios"`. Must match the booted firmware unless `auto`.
- `storage`: object (default `{"mode": "erase"}`). `mode` is one of (planned modes are refused
  with `unsupported` until implemented):
  - `erase`: wipe the disk and create the Bingux layout (below). `other_os` applies.
  - `free_space`: keep every existing partition. Create the Bingux partitions in the largest
    unallocated region of a GPT disk. An existing ESP (UEFI) or BIOS boot partition is reused when
    present; otherwise one is created. Refused with `no_free_space` when no region is large enough.
  - `alongside`: as `free_space`, after shrinking one existing partition: `"shrink": {"partition":
    "/dev/sda3", "size_bytes": <n>}`. Supported filesystems: ext4, btrfs and NTFS (not mounted).
    `size_bytes` must be at least the probe's `min_bytes` for that partition and smaller than its
    current size.
  - `manual`: `"partitions": [...]` under `storage`, an explicit list. Each entry is `{"device", "action", "role"}`
    plus `"size_bytes"` for `create` and `"fs"` for `format`/`create` (see below). Validated before any write:
    exactly one `root`; `esp` required under UEFI and `bios_boot` required under BIOS (both
    `reuse` or `create`); `boot` required with LUKS under BIOS; `reuse` is allowed only for `esp`
    and `home`; `create` sizes must fit the free space on the disk. `device` must be a partition
    of `disk`. A `create` device is a partition number not yet on the disk (`/dev/vda3` when the GPT holds partitions 1 and 2).
    - `action`: `reuse` (mount as-is, no mkfs), `format` (mkfs, destroys contents), `create` (new
      partition in free space, then mkfs).
    - `role`: `esp`, `bios_boot`, `boot`, `root`, `home`, `swap`.
    - `fs`: `ext4`, `btrfs` or `xfs`, home only, not with `reuse` (a reused home keeps its own filesystem). The root filesystem is the top-level `filesystem`.
- `filesystem`: `"ext4"` (default), `"btrfs"` or `"xfs"` for the root. Btrfs creates subvolumes `@`
  (mounted at `/`) and `@home` (mounted at `/home`). XFS cannot be shrunk, so it cannot be used with
  `alongside` on an existing XFS partition (`cannot_shrink`).
- `encryption`: `{"enabled": false}` (default), or `{"enabled": true, "passphrase": "<8+ chars>"}`.
  LUKS2 wraps the root under both firmwares. See the BIOS notes in the layout section.
- `swap`: `"none"` (default), `"zram"`, `"file"` or `"partition"`. `file` and `partition` take
  `"swap_bytes"` (1 GiB to 64 GiB). In `erase`, `free_space` and `alongside` a `partition` swap is
  created with that size. In `manual` the `swap` role partition is used and `swap_bytes` is refused.
  `file` lives on the root filesystem.
- `other_os`: `"refuse"` (default) or `"allow"`. Only meaningful for `storage.mode` `erase`; `free_space` and `alongside` keep the other OS and ignore it.
- `hostname`: RFC 1123 label, 1-63 characters.
- `timezone`: `"Region/City"`, must exist under `/usr/share/zoneinfo`.
- `locale`: `^[A-Za-z]{2,3}_[A-Za-z]{2}(\.UTF-8)?$`, must be listed in `/usr/share/i18n/SUPPORTED`.
- `keymap`: `^[A-Za-z0-9_-]{1,64}$` (console keymap name).
- `ntp`: `true` (default) enables `systemd-timesyncd`; `false` disables it.
- `users`: non-empty list of `{"name","password","admin","full_name"}`. `name` matches `^[a-z_][a-z0-9_-]{0,31}$` and is not `root`. `password` is 8+ characters with no control characters. `full_name` is optional, at most 128 characters, with no control characters, `:` or `,`.
- `root_password`: `null` (default, the root account is locked) or a string of 8+ characters with no control characters.
- `autologin`: `null` (default) or the name of a user in `users`. With a name, the installed `/etc/greetd/config.toml` gets an `[initial_session]` that starts that user's Gnoblin session at boot: the `Exec` of `/usr/share/wayland-sessions/gnoblin.desktop`, run with `XDG_SESSION_TYPE=wayland`, `XDG_SESSION_DESKTOP=gnoblin` and `XDG_CURRENT_DESKTOP` from the entry's `DesktopNames`. The packaged `[default_session]` (the greeter) is kept, so logging out shows the greeter. `null` leaves the greeter as the only session.
- `kernel_args`: optional list of strings, each matching `^[A-Za-z0-9_.=,:/+-]{1,200}$`. Refused if an argument starts with `root=`, `init=`, `rd.luks.` or `rootflags=`.

Passwords (`users`, `root_password`) are hashed with `openssl passwd -6 -stdin` and never appear on a
command line or in a log.

Example (erase, btrfs with LUKS, zram):

```json
{
  "version": 2,
  "disk": "/dev/vda",
  "confirm_erase": "/dev/vda",
  "storage": {"mode": "erase"},
  "firmware": "auto",
  "filesystem": "btrfs",
  "encryption": {"enabled": true, "passphrase": "correct horse battery"},
  "swap": "zram",
  "other_os": "refuse",
  "hostname": "bingux-box",
  "timezone": "Europe/London",
  "locale": "en_GB.UTF-8",
  "keymap": "uk",
  "ntp": true,
  "root_password": null,
  "autologin": null,
  "users": [
    {"name": "kieran", "password": "correct horse", "admin": true, "full_name": "Kieran"}
  ],
  "kernel_args": ["loglevel=4"]
}
```

## Partition and boot layout

GPT, always. In `erase` mode the disk is repartitioned from scratch; in `free_space` and `alongside`
the Bingux partitions are created in unallocated space and existing partitions are kept.
In `manual` the partitions are the plan's list (see Plan schema v2); `reuse` and `format` keep their entries, `create` takes free space.

- UEFI: ESP, FAT32, 512 MiB, label `BINGUX_ESP`, mounted at `/boot` (`umask=0077`); root for the rest.
- BIOS, unencrypted: BIOS boot, 1 MiB, flag `bios_grub`; root for the rest. `/boot` lives on root.
- BIOS, encrypted: BIOS boot, 1 MiB; `/boot` ext4, 1 GiB, unencrypted, mounted at `/boot`; LUKS root
  for the rest. Only the kernel, initramfs and GRUB live on `/boot`. GRUB then needs no
  `cryptodisk` module and no LUKS2 PBKDF support, and the passphrase is prompted once, by the
  initramfs. The UEFI ESP is unencrypted for the same reason.
- Root: ext4, btrfs or xfs, label `BINGUX_ROOT`. With encryption, LUKS2 with mapper name
  `bingux-root` and the filesystem inside it.
- Swap: `zram` writes `/etc/systemd/zram-generator.conf` (`zram-size = min(ram / 2, 4096)`,
  `compression-algorithm = zstd`). `file` creates `/swapfile` on the root filesystem (on btrfs with
  `btrfs filesystem mkswapfile`). `partition` uses a swap partition.
- Minimum disk size 8 GiB. No LVM, no RAID.

Kernel command line: `rw`, then the root argument (`root=UUID=<fs>`, or with LUKS
`rd.luks.name=<luks-uuid>=bingux-root root=/dev/mapper/bingux-root`), `rootflags=subvol=@` on btrfs,
then `splash quiet loglevel=3 rd.udev.log_level=3 vt.global_cursor_default=0`, then `kernel_args`.

Bootloader (dual-boot entries are planned; see Status): UEFI uses `bootctl install --esp-path=/boot` with `loader/loader.conf` and an entry
`bingux.conf`. systemd-boot lists a Windows Boot Manager on the same ESP automatically, so a shared
ESP (`free_space`, `alongside`) gives a dual-boot menu with no extra entry. BIOS uses GRUB
(`grub-install --target=i386-pc --recheck`, `grub-mkconfig`) with `GRUB_DISABLE_OS_PROBER=false`,
so `os-prober` adds other OSes to the menu.

Initramfs: systemd-based hooks (`base systemd autodetect microcode modconf kms [plymouth] keyboard
sd-vconsole block [sd-encrypt] filesystems fsck`), built with `mkinitcpio -P` in the target.

## Test matrix

- Unit tests: `distro/installer/tests/unit` (`cd distro/installer && python3 -m pytest -q tests/unit`).
  No root, no blivet, no real disks.
- Install matrix: `distro/installer/tests/matrix/run.py --iso <live.iso> [--case NAME ...]`, built on
  the `vmdrive` driver in `tests/vm`. Each case boots the live ISO in QEMU with a blank 10 GiB
  virtio disk (kept under `~/.cache`, deleted after the case), writes a plan, runs
  `bingux-installer --json probe` and `install --plan`, powers off, then boots the disk alone and
  checks the root filesystem, LUKS (encrypted cases), the created user in `wheel`, and `greetd` active,
  with a greeter screenshot. Cases: `uefi-ext4`, `uefi-btrfs`, `uefi-luks-ext4`, `bios-ext4`,
  `bios-btrfs`, and `bios-luks-ext4` (unencrypted ext4 `/boot`, LUKS2 root; boots to the greeter).
  Feature cases: `uefi-xfs` (xfs root, `ntp` false, root password set), `uefi-ext4-swapfile`,
  `uefi-btrfs-zram`, and `uefi-luks-ext4-swappart` (encrypted swap partition). Feature checks read
  `systemd-timesyncd` enablement, `passwd -S root`, and `swapon` output.
  Storage case `uefi-manual` pre-partitions the guest disk (ESP formatted, home formatted, root created in the free space) and checks the same boot results.
  Other-OS cases start from a host-built disk whose first partition is another OS (`OTHEROS` label, sha256 manifest). `uefi-free-space` and `bios-free-space` install into the unallocated rest (`free_space`); `uefi-alongside-ext4` and `uefi-alongside-ntfs` shrink that partition to 4 GiB first (`alongside`; NTFS via `ntfsresize`). After install the guest verifies the manifest with `sudo` and the disk must still boot. Manual cases: `manual-bios` (reuses the bios_boot partition, formats home, creates root) and `manual-luks` (reuses the ESP, creates a LUKS2 root). `uefi-free-space-refused` leaves only ~0.5 GiB free and must refuse with the other OS untouched.

Results (`~/.cache/bingux-storage/matrix/<case>/results.json`; log `~/.cache/bingux-storage/<case>.log`):

| Case | Result | Artifact |
|------|--------|----------|
| uefi-free-space | PASS (17/17 checks) | `~/.cache/bingux-storage/matrix/uefi-free-space/` |

- Artifacts: `distro/out/installer-backend/matrix/<case>/{install,boot}/` (serial transcript, command
  log, screenshots) and `distro/out/installer-backend/matrix/<case>/results.json`. Exit status is 0 only
  when every case passes.
- The serial getty autologins root, so the boot checks run as root and the user is checked with
  `runuser` and group membership. A password login through the console is not exercised.
- Driver API and smoke test: `tests/vm/README.md`.
