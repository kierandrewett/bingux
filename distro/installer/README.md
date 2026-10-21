# bingux-installer

Disk installer backend for the Bingux OS live ISO. Two console scripts:

- `bingux-installer-daemon`: root daemon, socket-activated on `/run/bingux-installer.sock`.
- `bingux-installer`: client CLI (`probe`, `version`, `install --plan FILE|-`, `cancel`).

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
| `unsupported` | `encryption.enabled` with BIOS firmware | Nothing | `encryption is not supported with BIOS firmware` |
| `disk_not_found` | `disk` is not a whole-disk block device in the probe, or blivet does not see it | Nothing | `no disk <path> was found`, `blivet does not see <path>` |
| `disk_is_live_medium` | `disk` holds the live ISO | Nothing | `<path> is the live medium` |
| `disk_too_small` | Disk is under 8 GiB | Nothing | `disk is <n> bytes; at least 8589934592 bytes (8 GiB) is required` |
| `other_os_present` | Probe found another OS on the disk and `other_os` is `refuse` | Nothing | `<path> holds <os list>; set other_os to allow to erase it` |
| `missing_tool` | A tool the install needs is not on `PATH` (checked before any write) | Nothing | `missing tools: <names>` |
| `cancelled` | Cancel accepted before the commit point; checked between phases | Nothing (no write has happened) | `install cancelled before the disk was modified` |
| `cancel_refused_after_commit` | `cancel` after the commit point | Install continues | `the disk is already being modified; cancel is refused` |
| `no_install` | `cancel` with no install running | Nothing | `no install is running` |
| `payload_failed` | Live payload `arch/x86_64/airootfs.sfs` missing (preflight, nothing written); squashfs mount, bind mount or rsync failed. After commit the disk is already erased. | Before commit: nothing. After: partial copy, not bootable; re-run the install | `payload_failed: <ExcType>: <detail>`, or `rsync failed (<exit>): <rsync error lines>` |
| `storage_failed` | Partitioning, LUKS, blkid or mkfs failed | Partition table and filesystems partly written; re-run the install, which reinitialises the disk | `storage_failed: <ExcType>: <detail>`, or `partitioning failed: <detail>` |
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
- `{"op":"install","plan":{...}}`: plan schema v1 below.
- `{"op":"cancel"}`: sent on a separate connection. Sets the cancel flag of the running install.

Events (not terminal):

- `{"event":"progress","phase":"<name>","percent":<0-100 or null>,"message":"..."}`
- `{"event":"log","message":"..."}`

Terminal lines:

- `{"event":"result","ok":true,"data":{...}}`
- `{"event":"error","code":"<code>","message":"<human text>"}`

Probe result data: `{"firmware":"uefi"|"bios","live_disk":"/dev/..."|null,"disks":[{"path","size_bytes","model","removable","is_live_medium","other_os":[...]}],"tools_missing":[...],"installer_version":"0.1.0"}`.

Install phases, in order: `validate`, `preflight`, `partition` (the commit point, the first destructive write), `mount`, `copy`, `strip`, `configure`, `bootloader`, `finish`.
Cancel is honoured only before the commit point. Only one install runs at a time.

Install result data: `{"disk":...,"firmware":...,"filesystem":...}`. The daemon never reboots.

The client CLI prints progress to stderr. `--json` prints the terminal line verbatim to stdout.
Exit status: 0 on `result ok`, 1 on a server error or when the daemon cannot be reached, 2 on usage errors (bad arguments, unreadable or invalid plan file).

## Plan schema v1

A JSON object. Unknown top-level keys are refused (`invalid_plan`).

- `version`: `1` (required).
- `disk`: whole-disk block device, `/dev/...` (required). Must not be the live medium.
- `confirm_erase`: must equal `disk` exactly (required). Everything on the disk is destroyed.
- `firmware`: `"auto"` (default), `"uefi"` or `"bios"`. Must match the booted firmware unless `auto`.
- `filesystem`: `"ext4"` (default) or `"btrfs"`. Btrfs creates subvolumes `@` (mounted at `/`) and `@home` (mounted at `/home`).
- `encryption`: `{"enabled": false}` (default), or `{"enabled": true, "passphrase": "<8+ chars>"}` for LUKS2 around the root partition. Refused with `unsupported` under BIOS.
- `other_os`: `"refuse"` (default) or `"allow"`.
- `hostname`: RFC 1123 label, 1-63 characters.
- `timezone`: `"Region/City"`, must exist under `/usr/share/zoneinfo`.
- `locale`: `^[A-Za-z]{2,3}_[A-Za-z]{2}(\.UTF-8)?$`, must be listed in `/usr/share/i18n/SUPPORTED`.
- `keymap`: `^[A-Za-z0-9_-]{1,64}$` (console keymap name).
- `users`: non-empty list of `{"name","password","admin","full_name"}`. `name` matches `^[a-z_][a-z0-9_-]{0,31}$` and is not `root`. `password` is 8+ characters with no control characters. `full_name` is optional, at most 128 characters, with no control characters, `:` or `,`.
- `kernel_args`: optional list of strings, each matching `^[A-Za-z0-9_.=,:/+-]{1,200}$`. Refused if an argument starts with `root=`, `init=`, `rd.luks.` or `rootflags=`.

The root account is always locked. Passwords are hashed with `openssl passwd -6 -stdin` and never
appear on a command line or in a log.

Example:

```json
{
  "version": 1,
  "disk": "/dev/vda",
  "confirm_erase": "/dev/vda",
  "firmware": "auto",
  "filesystem": "btrfs",
  "encryption": {"enabled": false},
  "other_os": "refuse",
  "hostname": "bingux-box",
  "timezone": "Europe/London",
  "locale": "en_GB.UTF-8",
  "keymap": "uk",
  "users": [
    {"name": "kieran", "password": "correct horse", "admin": true, "full_name": "Kieran"}
  ],
  "kernel_args": ["loglevel=4"]
}
```

## Partition and boot layout

GPT, whole disk, always.

- UEFI: p1 ESP, FAT32, 512 MiB, label `BINGUX_ESP`, mounted at `/boot` (`umask=0077`); p2 root for the rest.
- BIOS: p1 BIOS boot, 1 MiB, flag `bios_grub`; p2 root for the rest. `/boot` lives on root.
- Root: ext4 or btrfs, label `BINGUX_ROOT`. With encryption, LUKS2 on p2 with mapper name `bingux-root` and the filesystem inside it.
- Minimum disk size 8 GiB.
- No swap, no LVM, no RAID.

Kernel command line: `rw`, then the root argument (`root=UUID=<fs>`, or with LUKS
`rd.luks.name=<luks-uuid>=bingux-root root=/dev/mapper/bingux-root`), `rootflags=subvol=@` on btrfs,
then `splash quiet loglevel=3 rd.udev.log_level=3 vt.global_cursor_default=0`, then `kernel_args`.

Bootloader: UEFI uses `bootctl install --esp-path=/boot` with `loader/loader.conf` and an entry
`bingux.conf`. BIOS uses GRUB (`grub-install --target=i386-pc --recheck`, `grub-mkconfig`).

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
  `bios-btrfs`, and `bios-luks-ext4`, which must be refused with `unsupported` and leave the disk blank.
- Artifacts: `distro/out/installer-backend/matrix/<case>/{install,boot}/` (serial transcript, command
  log, screenshots) and `distro/out/installer-backend/matrix/<case>/results.json`. Exit status is 0 only
  when every case passes.
- The serial getty autologins root, so the boot checks run as root and the user is checked with
  `runuser` and group membership. A password login through the console is not exercised.
- Driver API and smoke test: `tests/vm/README.md`.
