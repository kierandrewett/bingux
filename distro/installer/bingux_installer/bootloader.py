"""Install the bootloader into the target: systemd-boot (UEFI) or GRUB (BIOS)."""

from __future__ import annotations

from pathlib import Path

from . import layout
from .protocol import InstallerError
from .runner import CommandError, Runner

GRUB_DEFAULT = "etc/default/grub"


def _chroot(runner: Runner, target: Path, argv: list[str]) -> None:
    try:
        runner.run(["chroot", str(target), *argv])
    except CommandError as exc:
        raise InstallerError("bootloader_failed", f"{argv[0]} failed in the target") from exc


def install_uefi(runner: Runner, target: Path, cmdline_text: str) -> None:
    """systemd-boot on the ESP mounted at target/boot, with a single Bingux entry."""
    _chroot(runner, target, ["bootctl", "install", "--esp-path=/boot"])
    loader = target / "boot" / "loader"
    entries = loader / "entries"
    entries.mkdir(parents=True, exist_ok=True)
    (loader / "loader.conf").write_text(layout.loader_conf(), encoding="utf-8")
    (entries / "bingux.conf").write_text(layout.loader_entry(cmdline_text), encoding="utf-8")


def install_bios(runner: Runner, target: Path, disk: str, cmdline_text: str) -> None:
    """GRUB on the MBR/BIOS boot partition, with the Bingux distributor name."""
    default = target / GRUB_DEFAULT
    default.parent.mkdir(parents=True, exist_ok=True)
    default.write_text(layout.grub_defaults(cmdline_text=cmdline_text), encoding="utf-8")
    _chroot(runner, target, ["grub-install", "--target=i386-pc", "--recheck", disk])
    _chroot(runner, target, ["grub-mkconfig", "-o", "/boot/grub/grub.cfg"])
