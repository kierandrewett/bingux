"""Pure partition layout, cmdline, fstab and boot text rendering."""

from __future__ import annotations

import pytest

from bingux_installer import layout
from bingux_installer.protocol import InstallerError

GIB = 1 << 30
ROOT_UUID = "11111111-2222-3333-4444-555555555555"
LUKS_UUID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
ESP_UUID = "ABCD-1234"
DISPLAY = "splash quiet loglevel=3 rd.udev.log_level=3 vt.global_cursor_default=0"


def test_uefi_layout_has_esp_then_growing_root() -> None:
    parts = layout.partition_layout(uefi=True, size_bytes=16 * GIB)
    assert [(p.index, p.role) for p in parts] == [(1, "esp"), (2, "root")]
    assert parts[0].size_bytes == layout.ESP_BYTES
    assert parts[1].size_bytes is None


def test_bios_layout_has_bios_boot_then_root() -> None:
    parts = layout.partition_layout(uefi=False, size_bytes=16 * GIB)
    assert [p.role for p in parts] == ["bios_boot", "root"]
    assert parts[0].size_bytes == layout.BIOS_BOOT_BYTES


def test_disk_below_eight_gib_is_too_small() -> None:
    with pytest.raises(InstallerError) as exc:
        layout.partition_layout(uefi=True, size_bytes=8 * GIB - 1)
    assert exc.value.code == "disk_too_small"


@pytest.mark.parametrize("disk,index,expected", [
    ("/dev/sda", 2, "/dev/sda2"),
    ("/dev/nvme0n1", 1, "/dev/nvme0n1p1"),
    ("/dev/mmcblk0", 2, "/dev/mmcblk0p2"),
])
def test_partition_path_naming(disk: str, index: int, expected: str) -> None:
    assert layout.partition_path(disk, index) == expected


def test_cmdline_ext4_plain_root() -> None:
    text = layout.cmdline(root_fs_uuid=ROOT_UUID, luks_uuid=None, filesystem="ext4", encrypted=False)
    assert text == f"rw root=UUID={ROOT_UUID} {DISPLAY}"


def test_cmdline_btrfs_adds_rootflags_before_display_args() -> None:
    text = layout.cmdline(root_fs_uuid=ROOT_UUID, luks_uuid=None, filesystem="btrfs", encrypted=False)
    assert text == f"rw root=UUID={ROOT_UUID} rootflags=subvol=@ {DISPLAY}"


def test_cmdline_luks_uses_mapper_and_luks_name() -> None:
    text = layout.cmdline(root_fs_uuid=ROOT_UUID, luks_uuid=LUKS_UUID, filesystem="ext4", encrypted=True)
    assert text == (f"rw rd.luks.name={LUKS_UUID}=bingux-root root=/dev/mapper/bingux-root {DISPLAY}")
    assert "root=UUID" not in text


def test_cmdline_luks_btrfs_and_kernel_args_are_appended_last() -> None:
    text = layout.cmdline(root_fs_uuid=ROOT_UUID, luks_uuid=LUKS_UUID, filesystem="btrfs",
                          encrypted=True, kernel_args=("mitigations=off",))
    assert text.startswith(f"rw rd.luks.name={LUKS_UUID}=bingux-root root=/dev/mapper/bingux-root "
                           "rootflags=subvol=@ ")
    assert text.endswith(" mitigations=off")


@pytest.mark.parametrize("kwargs", [
    dict(root_fs_uuid=None, luks_uuid=None, filesystem="ext4", encrypted=False),
    dict(root_fs_uuid=ROOT_UUID, luks_uuid=None, filesystem="ext4", encrypted=True),
])
def test_cmdline_requires_matching_uuid(kwargs: dict[str, object]) -> None:
    with pytest.raises(ValueError):
        layout.cmdline(**kwargs)  # type: ignore[arg-type]


def test_fstab_ext4_with_esp() -> None:
    text = layout.fstab(root_fs_uuid=ROOT_UUID, filesystem="ext4", esp_uuid=ESP_UUID)
    assert f"UUID={ROOT_UUID}\t/\text4\tdefaults\t0\t1" in text
    assert f"UUID={ESP_UUID}\t/boot\tvfat\tumask=0077\t0\t2" in text
    assert "\tswap\t" not in text


def test_fstab_btrfs_has_root_and_home_subvolumes_and_no_esp_on_bios() -> None:
    text = layout.fstab(root_fs_uuid=ROOT_UUID, filesystem="btrfs", esp_uuid=None)
    assert f"UUID={ROOT_UUID}\t/\tbtrfs\tsubvol=@,defaults\t0\t0" in text
    assert f"UUID={ROOT_UUID}\t/home\tbtrfs\tsubvol=@home,defaults\t0\t0" in text
    assert "/boot" not in text


def test_mkinitcpio_hooks_order_with_encryption_and_plymouth() -> None:
    hooks = layout.mkinitcpio_hooks(encrypted=True, plymouth=True)
    assert hooks.index("plymouth") == hooks.index("kms") + 1
    assert hooks.index("sd-encrypt") < hooks.index("filesystems")
    assert hooks[-2:] == ("filesystems", "fsck")
    assert hooks[0] == "base" and hooks[1] == "systemd"


def test_mkinitcpio_hooks_plain_without_plymouth() -> None:
    hooks = layout.mkinitcpio_hooks(encrypted=False, plymouth=False)
    assert "plymouth" not in hooks and "sd-encrypt" not in hooks
    assert layout.hooks_line(hooks).startswith("HOOKS=(base systemd autodetect")


def test_boot_text_renderers() -> None:
    cmd = "rw root=UUID=x quiet"
    assert layout.loader_conf() == "default bingux.conf\ntimeout 3\n"
    assert layout.loader_entry(cmd).endswith(f"options {cmd}\n")
    assert 'GRUB_DISTRIBUTOR="Bingux OS"' in layout.grub_defaults(cmdline_text=cmd)
    assert f'GRUB_CMDLINE_LINUX_DEFAULT="{cmd}"' in layout.grub_defaults(cmdline_text=cmd)
