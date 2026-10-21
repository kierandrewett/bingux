"""Live payload: mount the squashfs source, mount the target, copy with rsync.

Mount helpers only run commands through the Runner, so tests record argv.
Chroot bind mounts are created only after the copy, so rsync never writes into
host pseudo-filesystems.
"""

from __future__ import annotations

import os
import re
from collections.abc import Callable
from pathlib import Path

from .protocol import InstallerError
from .runner import Runner
from .storage import StorageLayout

SOURCE_SFS = "/run/archiso/bootmnt/arch/x86_64/airootfs.sfs"
SOURCE_MNT = Path("/run/bingux-installer/source")
TARGET_MNT = Path("/run/bingux-installer/target")
RSYNC_PERCENT = re.compile(r"(\d{1,3})%")
CHROOT_BINDS = (("/dev", "dev", "--rbind"), ("/proc", "proc", "--rbind"),
                ("/sys", "sys", "--rbind"), ("/run", "run", "--bind"))


def preflight(sfs: str = SOURCE_SFS, *, exists: Callable[[str], bool] = os.path.isfile) -> None:
    if not exists(sfs):
        raise InstallerError("payload_failed", f"live payload {sfs} is missing")


def mount_source(runner: Runner, sfs: str, mountpoint: Path) -> None:
    mountpoint.mkdir(parents=True, exist_ok=True)
    runner.run(["mount", "-t", "squashfs", "-o", "ro,loop", sfs, str(mountpoint)])


def unmount_source(runner: Runner, mountpoint: Path) -> None:
    runner.run(["umount", str(mountpoint)], check=False)


def copy_payload(runner: Runner, source: Path, target: Path,
                 on_percent: Callable[[int], None]) -> None:
    """rsync the live system into the target, reporting whole-percent progress."""

    def on_line(line: str) -> None:
        match = RSYNC_PERCENT.search(line)
        if match:
            on_percent(min(100, int(match.group(1))))

    runner.stream(
        ["rsync", "-aAXHS", "--numeric-ids", "--info=progress2", f"{source}/", f"{target}/"],
        on_line,
    )


def mount_target(runner: Runner, lay: StorageLayout, target: Path = TARGET_MNT, *,
                 uefi: bool, filesystem: str) -> None:
    """Mount the installed root (and btrfs @home, and the ESP at target/boot)."""
    target.mkdir(parents=True, exist_ok=True)
    if filesystem == "btrfs":
        runner.run(["mount", "-o", "subvol=@", lay.fs_path, str(target)])
        (target / "home").mkdir(exist_ok=True)
        runner.run(["mount", "-o", "subvol=@home", lay.fs_path, str(target / "home")])
    else:
        runner.run(["mount", lay.fs_path, str(target)])
    if uefi:
        if lay.esp_path is None:
            raise InstallerError("storage_failed", "UEFI install has no ESP device")
        boot = target / "boot"
        boot.mkdir(exist_ok=True)
        runner.run(["mount", "-o", "umask=0077", lay.esp_path, str(boot)])


def mount_chroot_binds(runner: Runner, target: Path = TARGET_MNT) -> None:
    """Bind the host API filesystems into the target. Call only after the copy."""
    for source, name, flag in CHROOT_BINDS:
        mountpoint = target / name
        mountpoint.mkdir(exist_ok=True)
        runner.run(["mount", flag, source, str(mountpoint)])
    efivars = target / "sys" / "firmware" / "efi" / "efivars"
    if os.path.isdir("/sys/firmware/efi/efivars"):
        efivars.mkdir(parents=True, exist_ok=True)
        runner.run(["mount", "-t", "efivarfs", "efivarfs", str(efivars)], check=False)
