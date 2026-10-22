"""Probe helpers: lsblk parsing, live-disk mapping, other-OS detection, tools, firmware."""

from __future__ import annotations

from pathlib import Path

import pytest

from bingux_installer import probe
from bingux_installer.protocol import InstallerError

LSBLK = {"blockdevices": [
    {"name": "sda", "path": "/dev/sda", "type": "disk", "size": 64000000000,
     "model": "VBOX HARDDISK ", "rm": False,
     "children": [{"name": "sda1", "path": "/dev/sda1", "type": "part",
                   "fstype": "vfat", "size": 536870912, "mountpoint": None}]},
    {"name": "loop0", "path": "/dev/loop0", "type": "disk", "size": 1},
    {"name": "sr0", "path": "/dev/sr0", "type": "rom", "size": 1},
    {"name": "sdb", "path": "/dev/sdb", "type": "disk", "size": 8000000000, "rm": True,
     "children": [{"name": "sdb1", "path": "/dev/sdb1", "type": "part", "fstype": "iso9660",
                   "size": 1000000}]},
]}


def test_parse_lsblk_keeps_real_disks_only() -> None:
    disks = probe.parse_lsblk(LSBLK)
    assert [d["name"] for d in disks] == ["sda", "sdb"]
    assert disks[0]["size_bytes"] == 64000000000
    assert disks[0]["model"] == "VBOX HARDDISK"
    assert disks[1]["removable"] is True
    assert disks[0]["partitions"][0]["fstype"] == "vfat"


def test_find_live_disk_maps_partition_to_parent_disk() -> None:
    assert probe.find_live_disk(LSBLK, "/dev/sdb1") == "/dev/sdb"
    assert probe.find_live_disk(LSBLK, "/dev/sdb") == "/dev/sdb"


def test_find_live_disk_is_none_when_unknown() -> None:
    assert probe.find_live_disk(LSBLK, None) is None
    assert probe.find_live_disk(LSBLK, "/dev/nvme9n9") is None


def test_detect_os_reads_os_release_read_only(tmp_path: Path) -> None:
    (tmp_path / "etc").mkdir()
    (tmp_path / "etc" / "os-release").write_text(
        'NAME="Fedora Linux"\nPRETTY_NAME="Fedora Linux 43 (Workstation)"\n', encoding="utf-8")
    assert probe.detect_os(tmp_path) == "Fedora Linux 43 (Workstation)"


def test_detect_os_falls_back_to_usr_lib_os_release(tmp_path: Path) -> None:
    (tmp_path / "usr" / "lib").mkdir(parents=True)
    (tmp_path / "usr" / "lib" / "os-release").write_text('NAME="Debian"\n', encoding="utf-8")
    assert probe.detect_os(tmp_path) == "Debian"


def test_detect_os_finds_windows_system32(tmp_path: Path) -> None:
    (tmp_path / "Windows" / "System32").mkdir(parents=True)
    assert probe.detect_os(tmp_path) == "Windows"


def test_detect_os_is_none_for_empty_root(tmp_path: Path) -> None:
    assert probe.detect_os(tmp_path) is None


def test_detect_firmware_checks_efi_directory(tmp_path: Path) -> None:
    assert probe.detect_firmware(str(tmp_path / "missing")) == "bios"
    (tmp_path / "efi").mkdir()
    assert probe.detect_firmware(str(tmp_path / "efi")) == "uefi"


def test_missing_tools_reports_absent_uefi_tools_and_blivet() -> None:
    present = {"sgdisk", "mkfs.fat", "mkfs.ext4", "mkfs.btrfs", "mkfs.xfs", "rsync", "openssl",
               "cryptsetup", "bootctl", "mkinitcpio"}
    missing = probe.missing_tools(firmware="uefi", which=lambda n: "/x" if n in present else None,
                                  blivet_available=False)
    assert "bootctl" not in missing
    assert "python-blivet" in missing
    assert "grub-install" not in missing  # BIOS tools are not required for UEFI


def test_missing_tools_checks_grub_for_bios() -> None:
    missing = probe.missing_tools(firmware="bios", which=lambda n: None, blivet_available=True)
    assert "grub-install" in missing and "grub-mkconfig" in missing
    assert "bootctl" not in missing and "python-blivet" not in missing


def test_disk_info_json_shape() -> None:
    info = probe.DiskInfo(path="/dev/vda", size_bytes=10, model="m", removable=False,
                          is_live_medium=False, other_os=["Windows"])
    data = info.as_json()
    assert set(data) == {"path", "size_bytes", "model", "removable", "is_live_medium", "other_os"}
    assert data["other_os"] == ["Windows"]


def test_find_disk_unknown_path_is_disk_not_found() -> None:
    with pytest.raises(InstallerError) as exc:
        probe.find_disk([], "/dev/vda")
    assert exc.value.code == "disk_not_found"
