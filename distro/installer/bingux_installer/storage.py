"""python-blivet adapter: partition, encrypt and format the target disk.

This is the only module that touches blivet, and blivet is imported lazily so the
unit tests never need it. Commit point: the call to Blivet.do_it().
"""

from __future__ import annotations

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

        if uefi:
            boot = b.new_partition(
                fmt_type="efi", fmt_args={"label": layout.ESP_LABEL},
                size=Size(specs["esp"].size_bytes), parents=[dev])
        else:
            boot = b.new_partition(
                fmt_type="biosboot", size=Size(specs["bios_boot"].size_bytes), parents=[dev])
        b.create_device(boot)
        if specs.get("boot") is not None:
            ext_boot = b.new_partition(
                fmt_type="ext4", fmt_args={"label": layout.BOOT_LABEL},
                size=Size(specs["boot"].size_bytes), parents=[dev])
            b.create_device(ext_boot)
        if specs.get("swap") is not None:
            swap = b.new_partition(fmt_type="swap", size=Size(specs["swap"].size_bytes),
                                   parents=[dev])
            b.create_device(swap)

        root_kwargs: dict = {}
        if encrypted:
            root_kwargs = {"fmt_type": "luks", "fmt_args": {"passphrase": passphrase}}
        elif filesystem == "btrfs":
            root_kwargs = {"fmt_type": "btrfs"}
        root = b.new_partition(size=Size(ROOT_BASE_SIZE), grow=True, parents=[dev],
                               **root_kwargs)
        b.create_device(root)

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
        boot_path, boot_fstype = layout.partition_path(disk, specs["esp"].index), "vfat"
    elif specs.get("boot") is not None:
        boot_path, boot_fstype = layout.partition_path(disk, specs["boot"].index), "ext4"
    swap_path = None
    if specs.get("swap") is not None:
        swap_path = layout.partition_path(disk, specs["swap"].index)
    return StorageLayout(
        boot_path=boot_path,
        boot_fstype=boot_fstype,
        luks_path=layout.partition_path(disk, specs["root"].index) if encrypted else None,
        fs_path=fs_device.path,
        luks_name=container.name if encrypted else None,
        swap_path=swap_path,
    )


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
