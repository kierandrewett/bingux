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
                          is_live_medium=False, other_os=["Windows"], partition_table="gpt",
                          free_bytes=4, partitions=[{"path": "/dev/vda1", "fstype": "ext4",
                                                     "name": "vda1", "label": None,
                                                     "size_bytes": 6, "mountpoint": None,
                                                     "used_bytes": 1, "free_bytes": 5, "os": None,
                                                     "shrinkable": True, "min_bytes": 2}])
    data = info.as_json()
    assert set(data) == {"path", "size_bytes", "model", "removable", "is_live_medium", "other_os",
                         "partition_table", "free_bytes", "partitions"}
    assert data["other_os"] == ["Windows"]
    assert set(data["partitions"][0]) == set(probe.PARTITION_JSON_KEYS)
    assert data["partitions"][0]["min_bytes"] == 2


def test_parse_lsblk_reads_partition_table_and_label() -> None:
    doc = {"blockdevices": [{"name": "sda", "path": "/dev/sda", "type": "disk", "size": 9,
                             "pttype": "gpt",
                             "children": [{"name": "sda1", "path": "/dev/sda1", "type": "part",
                                           "fstype": "ntfs", "label": "Windows", "size": 4}]},
                            {"name": "sdb", "path": "/dev/sdb", "type": "disk", "size": 9,
                             "pttype": "dos"}]}
    disks = probe.parse_lsblk(doc)
    assert disks[0]["partition_table"] == "gpt"
    assert disks[1]["partition_table"] == "msdos"
    assert disks[0]["partitions"][0]["label"] == "Windows"
    assert disks[0]["partitions"][0]["min_bytes"] is None


def test_parse_shrink_limits() -> None:
    assert probe.parse_tune2fs_block_size("Block count: 9\nBlock size:               4096\n") == 4096
    assert probe.parse_resize2fs_min("Estimated minimum size of the filesystem: 7538\n", 1024) == 7538 * 1024
    assert probe.parse_ntfsresize_min("You might resize at 2564096 bytes or 3 MB (freeing 39 MB).") == 2564096
    assert probe.parse_ntfsresize_min("no estimate") is None
    assert probe.parse_btrfs_min("4194304 bytes (4.00 MiB)\n") == 4194304


class _Result:
    def __init__(self, returncode: int = 0, stdout: str = "", stderr: str = "") -> None:
        self.returncode, self.stdout, self.stderr = returncode, stdout, stderr


class _FakeRunner:
    """Answers the read-only tools a probe calls; records argv."""

    def __init__(self, answers: dict[str, _Result]) -> None:
        self.answers = answers
        self.calls: list[list[str]] = []

    def run(self, argv, *, check: bool = True, stdin: str | None = None):  # noqa: ANN001, ANN201
        self.calls.append(list(argv))
        return self.answers.get(argv[0], _Result(1))


def test_ext4_min_bytes_combines_block_size_and_estimate() -> None:
    runner = _FakeRunner({
        "tune2fs": _Result(stdout="Block size:               1024\n"),
        "resize2fs": _Result(stdout="Estimated minimum size of the filesystem: 7538\n"),
    })
    assert probe.ext4_min_bytes(runner, "/dev/vda1") == 7538 * 1024


def test_unreadable_shrink_limit_is_none() -> None:
    assert probe.ntfs_min_bytes(_FakeRunner({"ntfsresize": _Result(1)}), "/dev/vda2") is None
    assert probe.ext4_min_bytes(_FakeRunner({}), "/dev/vda1") is None


def test_scan_partition_reports_usage_and_marks_unshrinkable_fs(tmp_path, monkeypatch) -> None:  # noqa: ANN001
    monkeypatch.setattr(probe, "mount_ro", lambda runner, device, target: True)
    monkeypatch.setattr(probe, "umount", lambda runner, target: None)
    monkeypatch.setattr(probe, "detect_os", lambda root: "Windows")
    runner = _FakeRunner({})
    found = probe.scan_partition(runner, {"path": "/dev/vda1", "fstype": "vfat"}, tmp_path)
    assert found["os"] == "Windows"
    assert found["used_bytes"] >= 0 and found["free_bytes"] >= 0
    assert found["shrinkable"] is False and found["min_bytes"] is None
    assert runner.calls == []


def test_scan_partition_unreadable_mount_is_all_unknown(tmp_path, monkeypatch) -> None:  # noqa: ANN001
    monkeypatch.setattr(probe, "mount_ro", lambda runner, device, target: False)
    found = probe.scan_partition(_FakeRunner({}), {"path": "/dev/vda1", "fstype": "ext4"}, tmp_path)
    assert found == {"os": None, "used_bytes": None, "free_bytes": None,
                     "shrinkable": None, "min_bytes": None}


def test_find_disk_unknown_path_is_disk_not_found() -> None:
    with pytest.raises(InstallerError) as exc:
        probe.find_disk([], "/dev/vda")
    assert exc.value.code == "disk_not_found"


def test_missing_shrink_tools_names_only_what_the_filesystem_needs() -> None:
    present = {"e2fsck", "resize2fs", "btrfs"}
    which = lambda n: "/x" if n in present else None  # noqa: E731
    assert probe.missing_shrink_tools("ext4", which=which) == []
    assert probe.missing_shrink_tools("btrfs", which=which) == []
    assert probe.missing_shrink_tools("ntfs", which=which) == ["ntfsresize"]
    assert probe.missing_shrink_tools("xfs", which=which) == []


def test_ntfs_min_bytes_is_none_when_ntfsresize_is_not_installed() -> None:
    class _NoTool(_FakeRunner):
        def run(self, argv, *, check: bool = True, stdin: str | None = None):  # noqa: ANN001, ANN201
            raise FileNotFoundError(argv[0])

    assert probe.ntfs_min_bytes(_NoTool({}), "/dev/vda2") is None
