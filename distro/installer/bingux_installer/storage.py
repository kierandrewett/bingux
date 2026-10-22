"""Partition, encrypt and format the target disk.

commit_layout builds a fresh GPT through python-blivet (imported lazily, so the unit tests never
need it). commit_free_space adds partitions to free space with sgdisk and formats them with mkfs;
it never touches blivet and never writes a reused partition. shrink_partition resizes an existing
filesystem and its GPT entry in place (alongside).
"""

from __future__ import annotations

import os
import re
import tempfile

from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass

from . import layout
from .protocol import InstallerError
from .runner import CommandError, Runner

ROOT_BASE_SIZE = "1GiB"  # growable root starts at 1 GiB and fills the disk


@dataclass(frozen=True)
class StorageLayout:
    boot_path: str | None  # /boot partition: the ESP (UEFI) or an ext4 partition (BIOS, encrypted)
    boot_fstype: str | None  # "vfat" for the ESP, "ext4" for the BIOS /boot; None when no /boot
    luks_path: str | None  # partition carrying the LUKS header, when encrypted
    fs_path: str  # device the root filesystem lives on (mapper device or btrfs volume)
    luks_name: str | None  # open LUKS mapper after commit, e.g. luks-<uuid>; None when unencrypted
    swap_path: str | None  # swap partition, when one was created


def commit_layout(
    *,
    disk: str,
    size_bytes: int,
    uefi: bool,
    filesystem: str,
    encrypted: bool,
    passphrase: str,
    swap_partition_bytes: int = 0,
) -> StorageLayout:  # pragma: no cover - needs python-blivet and a real disk
    """Initialise the GPT, create partitions, LUKS and filesystems. Destructive."""
    try:
        from blivet import Blivet, udev
        from blivet.devices import LUKSDevice
        from blivet.formats import get_format
        from blivet.formats.disklabel import DiskLabel
        from blivet.partitioning import do_partitioning
        from blivet.size import Size
    except ImportError as exc:
        raise InstallerError("missing_tool", "python-blivet is not importable") from exc

    specs = {s.role: s for s in layout.partition_layout(
        uefi=uefi, size_bytes=size_bytes, encrypted=encrypted,
        swap_partition_bytes=swap_partition_bytes)}
    try:
        b = Blivet()
        b.reset()
        DiskLabel.set_default_label_type("gpt")
        dev = b.devicetree.get_device_by_path(disk)
        if dev is None:
            raise InstallerError("disk_not_found", f"blivet does not see {disk}")
        for child in list(dev.children):
            b.recursive_remove(child)
        b.initialize_disk(dev)

        parts: dict[str, object] = {}
        if uefi:
            boot = b.new_partition(
                fmt_type="efi", fmt_args={"label": layout.ESP_LABEL},
                size=Size(specs["esp"].size_bytes), parents=[dev])
            parts["esp"] = boot
        else:
            boot = b.new_partition(
                fmt_type="biosboot", size=Size(specs["bios_boot"].size_bytes), parents=[dev])
        b.create_device(boot)
        if specs.get("boot") is not None:
            ext_boot = b.new_partition(
                fmt_type="ext4", fmt_args={"label": layout.BOOT_LABEL},
                size=Size(specs["boot"].size_bytes), parents=[dev])
            b.create_device(ext_boot)
            parts["boot"] = ext_boot
        if specs.get("swap") is not None:
            swap = b.new_partition(fmt_type="swap", size=Size(specs["swap"].size_bytes),
                                   parents=[dev])
            b.create_device(swap)
            parts["swap"] = swap

        root_kwargs: dict = {}
        if encrypted:
            root_kwargs = {"fmt_type": "luks", "fmt_args": {"passphrase": passphrase}}
        elif filesystem == "btrfs":
            root_kwargs = {"fmt_type": "btrfs"}
        root = b.new_partition(size=Size(ROOT_BASE_SIZE), grow=True, parents=[dev],
                               **root_kwargs)
        b.create_device(root)
        parts["root"] = root

        # Growable partitions only receive their final size here; the LUKS mapping and
        # filesystems are created afterwards, sized from the allocated partition.
        do_partitioning(b)

        container = root
        if encrypted:
            # A btrfs volume needs a btrfs member format on its parent; ext4 is formatted below.
            member = get_format("btrfs") if filesystem == "btrfs" else None
            container = LUKSDevice(name=layout.LUKS_NAME, fmt=member, size=root.size, parents=[root])
            b.create_device(container)

        if filesystem == "btrfs":
            volume = b.new_btrfs(name=layout.ROOT_LABEL, parents=[container])
            b.create_device(volume)
            for name, mountpoint in layout.BTRFS_SUBVOLS:
                sub = b.new_btrfs_sub_volume(name=name, parents=[volume], mountpoint=mountpoint)
                b.create_device(sub)
            fs_device = volume
        else:
            fs_type = "xfs" if filesystem == "xfs" else "ext4"
            b.format_device(container, get_format(fs_type, device=container.path,
                                                  label=layout.ROOT_LABEL))
            fs_device = container

        b.do_it()  # ---- commit point: the disk is written from here on ----
        # dm-crypt mappings and filesystems appear through udev; the caller reads them next.
        udev.settle()
    except InstallerError:
        raise
    except Exception as exc:  # blivet raises many exception types; keep the message only
        raise InstallerError("storage_failed", f"partitioning failed: {exc}") from exc

    boot_path, boot_fstype = None, None
    if uefi:
        boot_path, boot_fstype = parts["esp"].path, "vfat"
    elif "boot" in parts:
        boot_path, boot_fstype = parts["boot"].path, "ext4"
    # Paths come from the devices blivet created, not from partition numbers: the kernel's
    # numbering follows the disk order, which blivet chooses (swap landed before the ESP once).
    return StorageLayout(
        boot_path=boot_path,
        boot_fstype=boot_fstype,
        luks_path=parts["root"].path if encrypted else None,
        fs_path=fs_device.path,
        luks_name=container.name if encrypted else None,
        swap_path=parts["swap"].path if "swap" in parts else None,
    )


def commit_free_space(
    *,
    runner: Runner,
    disk: str,
    partitions: tuple[layout.FreePartition, ...],
    filesystem: str,
    encrypted: bool,
    passphrase: str,
) -> StorageLayout:
    """Create the planned partitions in free space and format the new ones. Destructive only to
    the new partitions: a reused partition (existing ESP or BIOS boot) is never written.
    Commit point: the first sgdisk call. The passphrase goes on stdin only."""
    paths = {p.role: layout.partition_path(disk, p.number) for p in partitions}
    fresh = {p.role for p in partitions if not p.reuse}
    root = paths["root"]
    fs_path = f"/dev/mapper/{layout.LUKS_NAME}" if encrypted else root
    args: list[str] = []
    for p in partitions:
        if not p.reuse:
            args += [f"--new={p.number}:{p.start}:{p.end}", f"--typecode={p.number}:{p.code}",
                     f"--change-name={p.number}:{p.name}"]
    try:
        runner.run(["sgdisk", *args, disk])
    except CommandError as exc:
        raise InstallerError("storage_failed", "sgdisk could not write the new partitions") from exc
    _settle(runner)
    try:
        if "esp" in fresh:
            runner.run(["mkfs.fat", "-F", "32", "-n", layout.ESP_LABEL, paths["esp"]])
        if "boot" in fresh:
            runner.run(["mkfs.ext4", "-F", "-L", layout.BOOT_LABEL, paths["boot"]])
        if "swap" in fresh:
            runner.run(["mkswap", paths["swap"]])
        if encrypted:
            runner.run(["cryptsetup", "luksFormat", "--batch-mode", "--key-file=-", root],
                       stdin=passphrase)
            runner.run(["cryptsetup", "open", "--key-file=-", root, layout.LUKS_NAME],
                       stdin=passphrase)
            _settle(runner)
        if filesystem == "btrfs":
            runner.run(["mkfs.btrfs", "-f", "-L", layout.ROOT_LABEL, fs_path])
            _make_btrfs_subvolumes(runner, fs_path)
        elif filesystem == "xfs":
            runner.run(["mkfs.xfs", "-f", "-L", layout.ROOT_LABEL, fs_path])
        else:
            runner.run(["mkfs.ext4", "-F", "-L", layout.ROOT_LABEL, fs_path])
    except CommandError as exc:
        # Name the tool only: stderr can echo device details, and the passphrase must never leak.
        raise InstallerError("storage_failed", f"{exc.argv[0]} failed (exit {exc.returncode})") from exc
    _settle(runner)
    if "esp" in paths:
        boot_path, boot_fstype = paths["esp"], "vfat"
    elif "boot" in paths:
        boot_path, boot_fstype = paths["boot"], "ext4"
    else:
        boot_path, boot_fstype = None, None
    return StorageLayout(
        boot_path=boot_path,
        boot_fstype=boot_fstype,
        luks_path=root if encrypted else None,
        fs_path=fs_path,
        luks_name=layout.LUKS_NAME if encrypted else None,
        swap_path=paths.get("swap"),
    )


def _settle(runner: Runner) -> None:
    try:
        runner.run(["udevadm", "settle"])
    except CommandError as exc:
        raise InstallerError("storage_failed", "udev did not settle after partitioning") from exc


@contextmanager
def _mounted(runner: Runner, device: str) -> Iterator[str]:
    """Mount device on a temporary directory for the duration of the block, then unmount it."""
    mount_point = tempfile.mkdtemp(prefix="bingux-mount-")
    try:
        runner.run(["mount", device, mount_point])
        try:
            yield mount_point
        finally:
            runner.run(["umount", mount_point])
    finally:
        os.rmdir(mount_point)


def _make_btrfs_subvolumes(runner: Runner, device: str) -> None:
    """Create the top-level subvolumes through a temporary mount, then unmount it."""
    with _mounted(runner, device) as mount_point:
        for name, _ in layout.BTRFS_SUBVOLS:
            runner.run(["btrfs", "subvolume", "create", f"{mount_point}/{name}"])


def shrink_partition(
    *,
    runner: Runner,
    disk: str,
    part: layout.GptPartition,
    fstype: str,
    size_bytes: int,
    new_end: int,
) -> None:
    """Shrink one unmounted partition to size_bytes: its filesystem first, then its GPT entry.

    The filesystem is resized while the old partition still covers it, so a failure before the
    sgdisk call leaves the partition at its old size. The entry keeps its unique GUID, name and
    type. Destructive to the shrunk tail only; the caller has checked that size_bytes fits."""
    try:
        _shrink_filesystem(runner, fstype, layout.partition_path(disk, part.number), size_bytes)
    except CommandError as exc:
        raise InstallerError("storage_failed", f"{exc.argv[0]} failed (exit {exc.returncode})") from exc
    guid = _partition_guid(runner, disk, part.number)
    n = part.number
    try:
        runner.run(["sgdisk", f"--delete={n}", f"--new={n}:{part.start}:{new_end}",
                    f"--typecode={n}:{part.code}", f"--change-name={n}:{part.name}",
                    f"--partition-guid={n}:{guid}", disk])
    except CommandError as exc:
        raise InstallerError("storage_failed", "sgdisk could not shrink the partition") from exc
    _settle(runner)


def _shrink_filesystem(runner: Runner, fstype: str, path: str, size_bytes: int) -> None:
    if fstype == "ext4":
        # e2fsck exits 1 when it corrected errors, which is safe to continue from; 2 and up is not.
        fsck = runner.run(["e2fsck", "-f", "-y", path], check=False)
        if fsck.returncode not in (0, 1):
            raise CommandError(["e2fsck"], fsck.returncode)
        runner.run(["resize2fs", path, f"{size_bytes // 1024}K"])
    elif fstype == "ntfs":
        runner.run(["ntfsresize", "--force", "--size", str(size_bytes), path])
    elif fstype == "btrfs":
        with _mounted(runner, path) as mount_point:
            runner.run(["btrfs", "filesystem", "resize", str(size_bytes), mount_point])
    else:
        raise InstallerError("cannot_shrink", f"{fstype} cannot be shrunk")


def _partition_guid(runner: Runner, disk: str, number: int) -> str:
    try:
        result = runner.run(["sgdisk", "-i", str(number), disk])
    except CommandError as exc:
        raise InstallerError("storage_failed", f"sgdisk could not read partition {number}") from exc
    match = re.search(r"^Partition unique GUID:\s*(\S+)", result.stdout, re.MULTILINE)
    if match is None:
        raise InstallerError("storage_failed", f"partition {number} has no unique GUID")
    return match.group(1)



def blkid_value(runner: Runner, device: str, key: str) -> str:
    """Return one blkid field for a device, e.g. UUID. Raises storage_failed when empty."""
    try:
        result = runner.run(["blkid", "-s", key, "-o", "value", device])
    except CommandError as exc:
        raise InstallerError("storage_failed", f"blkid could not read {device}") from exc
    value = result.stdout.strip()
    if not value:
        raise InstallerError("storage_failed", f"{device} has no {key}")
    return value
