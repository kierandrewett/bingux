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


def test_bios_encrypted_layout_has_unencrypted_boot_before_root() -> None:
    parts = layout.partition_layout(uefi=False, size_bytes=16 * GIB, encrypted=True)
    assert [(p.index, p.role) for p in parts] == [(1, "bios_boot"), (2, "boot"), (3, "root")]
    assert parts[1].size_bytes == layout.BOOT_BYTES
    assert parts[2].size_bytes is None


def test_disk_below_eight_gib_is_too_small() -> None:
    with pytest.raises(InstallerError) as exc:
        layout.partition_layout(uefi=True, size_bytes=8 * GIB - 1)
    assert exc.value.code == "disk_too_small"


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
    text = layout.fstab(root_fs_uuid=ROOT_UUID, filesystem="ext4", boot_uuid=ESP_UUID)
    assert f"UUID={ROOT_UUID}\t/\text4\tdefaults\t0\t1" in text
    assert f"UUID={ESP_UUID}\t/boot\tvfat\tumask=0077\t0\t2" in text
    assert "\tswap\t" not in text


def test_fstab_btrfs_has_root_and_home_subvolumes_and_no_esp_on_bios() -> None:
    text = layout.fstab(root_fs_uuid=ROOT_UUID, filesystem="btrfs", boot_uuid=None)
    assert f"UUID={ROOT_UUID}\t/\tbtrfs\tsubvol=@,defaults\t0\t0" in text
    assert f"UUID={ROOT_UUID}\t/home\tbtrfs\tsubvol=@home,defaults\t0\t0" in text
    assert "/boot" not in text


def test_fstab_ext4_boot_partition_on_bios_encrypted() -> None:
    text = layout.fstab(root_fs_uuid=ROOT_UUID, filesystem="ext4", boot_uuid=LUKS_UUID,
                        boot_fstype="ext4")
    assert f"UUID={LUKS_UUID}\t/boot\text4\tdefaults\t0\t2" in text
    assert "vfat" not in text


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


def test_swap_partition_sits_between_boot_and_root() -> None:
    parts = layout.partition_layout(uefi=True, size_bytes=16 * GIB, swap_partition_bytes=2 * GIB)
    assert [(p.index, p.role) for p in parts] == [(1, "esp"), (2, "swap"), (3, "root")]
    assert parts[1].size_bytes == 2 * GIB and parts[2].size_bytes is None
    bios = layout.partition_layout(uefi=False, size_bytes=16 * GIB, swap_partition_bytes=2 * GIB)
    assert [(p.index, p.role) for p in bios] == [(1, "bios_boot"), (2, "swap"), (3, "root")]


def test_no_swap_partition_by_default() -> None:
    parts = layout.partition_layout(uefi=True, size_bytes=16 * GIB)
    assert all(p.role != "swap" for p in parts)


def test_fstab_xfs_root() -> None:
    text = layout.fstab(root_fs_uuid=ROOT_UUID, filesystem="xfs", boot_uuid=ESP_UUID)
    assert f"UUID={ROOT_UUID}\t/\txfs\tdefaults\t0\t0" in text


def test_fstab_swapfile_and_encrypted_swap_entries() -> None:
    text = layout.fstab(root_fs_uuid=ROOT_UUID, filesystem="ext4", boot_uuid=ESP_UUID,
                        swap_source=layout.SWAPFILE)
    assert "/swapfile\tnone\tswap\tdefaults\t0\t0" in text
    text = layout.fstab(root_fs_uuid=ROOT_UUID, filesystem="ext4", boot_uuid=ESP_UUID,
                        swap_source=f"/dev/mapper/{layout.SWAP_MAPPER}")
    assert f"/dev/mapper/{layout.SWAP_MAPPER}\tnone\tswap\tdefaults\t0\t0" in text


def test_crypttab_random_key_swap() -> None:
    text = layout.crypttab(swap_partuuid="1234-abcd")
    assert text.splitlines()[1] == (
        f"{layout.SWAP_MAPPER}\tPARTUUID=1234-abcd\t/dev/urandom\t"
        "swap,cipher=aes-xts-plain64,size=256")


def test_zram_conf_is_half_ram_capped_at_4gib() -> None:
    text = layout.zram_conf()
    assert "[zram0]" in text
    assert "zram-size = min(ram / 2, 4096)" in text
    assert "compression-algorithm = zstd" in text


def test_modules_line_renders_mkinitcpio_modules() -> None:
    assert layout.modules_line(()) == "MODULES=()"
    assert layout.modules_line(("xfs",)) == "MODULES=(xfs)"


SGDISK_PRINT = """Disk /dev/vda: 20971520 sectors, 10.0 GiB
Sector size (logical/physical): 512/512 bytes
Disk identifier (GUID): 0F0F
Partition table holds up to 128 entries
Main partition table begins at sector 2 and ends at sector 33
First usable sector is 34, last usable sector is 20971486
Partitions will be aligned on 2048-sector boundaries
Number  Start (sector)    End (sector)  Size       Code  Name
   1            2048         1050623   512.0 MiB   EF00  EFI System
   2         1050624        11538431   5.0 GiB     8300  Linux root
"""


def test_parse_sgdisk_print_reads_partitions_and_usable_range() -> None:
    table = layout.parse_sgdisk_print(SGDISK_PRINT)
    assert table.sector_size == 512
    assert table.first_usable == 34 and table.last_usable == 20971486
    assert [(p.number, p.start, p.end, p.code) for p in table.partitions] == [
        (1, 2048, 1050623, "EF00"), (2, 1050624, 11538431, "8300")]
    assert table.partitions[0].name == "EFI System"


def test_free_regions_lie_between_partitions_and_after_the_last() -> None:
    table = layout.parse_sgdisk_print(SGDISK_PRINT)
    assert table.free_regions() == [(34, 2047), (11538432, 20971486)]
    assert table.free_bytes() == (2047 - 34 + 1 + 20971486 - 11538432 + 1) * 512


def test_parse_sgdisk_print_rejects_non_gpt_text() -> None:
    with pytest.raises(ValueError):
        layout.parse_sgdisk_print("Error: not a GPT\n")
