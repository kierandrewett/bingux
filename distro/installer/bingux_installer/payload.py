"""Live payload: mount the squashfs source, mount the target, copy with rsync.

Mount helpers only run commands through the Runner, so tests record argv.
Chroot bind mounts are created only after the copy, so rsync never writes into
host pseudo-filesystems.
"""

from __future__ import annotations

import os
import re
import shutil
from collections.abc import Callable
from pathlib import Path

from .protocol import InstallerError
from .runner import CommandError, Runner
from .storage import StorageLayout

SOURCE_SFS = "/run/archiso/bootmnt/arch/x86_64/airootfs.sfs"
LIVE_KERNEL = "/run/archiso/bootmnt/arch/boot/x86_64/vmlinuz-linux"
SOURCE_MNT = Path("/run/bingux-installer/source")
TARGET_MNT = Path("/run/bingux-installer/target")
RSYNC_PERCENT = re.compile(r"(\d{1,3})%")
CHROOT_BINDS = (("/dev", "dev", "--rbind"), ("/proc", "proc", "--rbind"),
                ("/sys", "sys", "--rbind"), ("/run", "run", "--bind"))


def preflight(sfs: str = SOURCE_SFS, kernel: str = LIVE_KERNEL, *,
              exists: Callable[[str], bool] = os.path.isfile) -> None:
    if not exists(sfs):
        raise InstallerError("payload_failed", f"live payload {sfs} is missing")
    if not exists(kernel):
        raise InstallerError("payload_failed", f"live kernel {kernel} is missing")


def mount_source(runner: Runner, sfs: str, mountpoint: Path) -> None:
    mountpoint.mkdir(parents=True, exist_ok=True)
    runner.run(["mount", "-t", "squashfs", "-o", "ro,loop", sfs, str(mountpoint)])


def unmount_source(runner: Runner, mountpoint: Path) -> None:
    runner.run(["umount", str(mountpoint)], check=False)


def copy_kernel(target: Path, kernel: str = LIVE_KERNEL) -> None:
    """Place the live kernel at target/boot/vmlinuz-linux.

    archiso moves the kernel and initramfs out of the airootfs onto the ISO, so the
    copied /boot is empty and mkinitcpio has no kernel to build from. The ISO kernel
    is the same build as the squashfs modules. On UEFI target/boot is the ESP.
    """
    boot = target / "boot"
    try:
        boot.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(kernel, boot / "vmlinuz-linux")
    except OSError as exc:
        raise InstallerError("payload_failed", f"could not copy the kernel: {exc}") from exc


def copy_payload(runner: Runner, source: Path, target: Path,
                 on_percent: Callable[[int], None]) -> None:
    """rsync the live system into the target, reporting whole-percent progress.

    /boot is excluded: it is the vfat ESP mount point, which cannot hold the xattrs
    the live tree carries (rsync then fails at the end with exit 23). The live
    /boot is empty, so nothing is lost; the bootloader writes to the ESP.

    A failure carries rsync's own error lines: the exit status alone does not say
    which file or attribute could not be copied.
    """
    errors: list[str] = []

    def on_line(line: str) -> None:
        match = RSYNC_PERCENT.search(line)
        if match:
            on_percent(min(100, int(match.group(1))))
        elif "rsync" in line or "failed" in line or "denied" in line:
            errors.append(line.strip())

    try:
        runner.stream(
            ["rsync", "-aAXHS", "--numeric-ids", "--info=progress2", "--exclude=/boot",
             f"{source}/", f"{target}/"],
            on_line,
        )
    except CommandError as exc:
        detail = " | ".join(errors[-8:]) or str(exc)
        raise InstallerError("payload_failed", f"rsync failed ({exc}): {detail}") from exc


def _private_mountpoint(runner: Runner, target: Path) -> None:
    """Make target a private mountpoint so the installed root's mounts never propagate.

    /run is a shared mount, so a plain mount on target would be copied into every peer
    mount namespace; the final umount -R in the daemon could then never release the
    LUKS mapper. Binding target onto itself and marking it private stops that propagation.
    """
    target.mkdir(parents=True, exist_ok=True)
    runner.run(["mount", "--bind", str(target), str(target)])
    runner.run(["mount", "--make-private", str(target)])


def mount_target(runner: Runner, lay: StorageLayout, target: Path = TARGET_MNT, *,
                 filesystem: str) -> None:
    """Mount the installed root, /home (a btrfs @home subvolume or its own partition) and the
    /boot partition at target/boot."""
    _private_mountpoint(runner, target)
    if filesystem == "btrfs":
        runner.run(["mount", "-o", "subvol=@", lay.fs_path, str(target)])
    else:
        runner.run(["mount", lay.fs_path, str(target)])
    if lay.home_path is not None:
        (target / "home").mkdir(exist_ok=True)
        runner.run(["mount", lay.home_path, str(target / "home")])
    elif filesystem == "btrfs":
        (target / "home").mkdir(exist_ok=True)
        runner.run(["mount", "-o", "subvol=@home", lay.fs_path, str(target / "home")])
    if lay.boot_path is not None:
        boot = target / "boot"
        boot.mkdir(exist_ok=True)
        options = ["-o", "umask=0077"] if lay.boot_fstype == "vfat" else []
        runner.run(["mount", *options, lay.boot_path, str(boot)])


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
    # Binds inherit the host's shared propagation: keep unmounts inside the target from
    # propagating back to the host, so the final umount -R of the target is self-contained.
    runner.run(["mount", "--make-rslave", str(target)])
