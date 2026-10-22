"""commit_free_space against a recording runner: no disk is touched."""

from __future__ import annotations

import pytest

from bingux_installer import layout, storage
from bingux_installer.protocol import InstallerError
from bingux_installer.runner import CommandError

PASSPHRASE = "hunter22-secret"


class RecordingRunner:
    def __init__(self, fail_on: str | None = None) -> None:
        self.calls: list[tuple[list[str], str | None]] = []
        self.fail_on = fail_on

    def run(self, argv, *, check=True, stdin=None):  # noqa: ANN001, ANN201
        self.calls.append((list(argv), stdin))
        if self.fail_on is not None and argv[0] == self.fail_on:
            raise CommandError(argv, 1, "device detail /dev/vda2")
        return None

    def stream(self, argv, on_line, **kwargs):  # noqa: ANN001, ANN003, ANN201
        raise AssertionError(f"unexpected stream: {argv}")

    def argvs(self) -> list[list[str]]:
        return [argv for argv, _ in self.calls]


def _fresh_esp_and_root() -> tuple[layout.FreePartition, ...]:
    return (
        layout.FreePartition("esp", 2, 100, 200, layout.GPT_ESP, layout.ESP_LABEL, reuse=False),
        layout.FreePartition("root", 3, 201, 400, layout.GPT_LINUX, layout.ROOT_LABEL, reuse=False),
    )


def test_fresh_partitions_are_created_then_formatted_on_ext4() -> None:
    runner = RecordingRunner()
    out = storage.commit_free_space(
        runner=runner, disk="/dev/vda", partitions=_fresh_esp_and_root(),
        filesystem="ext4", encrypted=False, passphrase="")
    argvs = runner.argvs()
    assert argvs[0] == [
        "sgdisk", "--new=2:100:200", "--typecode=2:EF00", f"--change-name=2:{layout.ESP_LABEL}",
        "--new=3:201:400", "--typecode=3:8300", f"--change-name=3:{layout.ROOT_LABEL}",
        "/dev/vda",
    ]
    assert ["mkfs.fat", "-F", "32", "-n", layout.ESP_LABEL, "/dev/vda2"] in argvs
    assert ["mkfs.ext4", "-F", "-L", layout.ROOT_LABEL, "/dev/vda3"] in argvs
    assert out == storage.StorageLayout(
        boot_path="/dev/vda2", boot_fstype="vfat", luks_path=None,
        fs_path="/dev/vda3", luks_name=None, swap_path=None)


def test_reused_esp_is_never_written_or_formatted() -> None:
    parts = (
        layout.FreePartition("esp", 1, 2048, 4095, layout.GPT_ESP, "EFI", reuse=True),
        layout.FreePartition("root", 3, 201, 400, layout.GPT_LINUX, layout.ROOT_LABEL, reuse=False),
    )
    runner = RecordingRunner()
    storage.commit_free_space(runner=runner, disk="/dev/vda", partitions=parts,
                              filesystem="ext4", encrypted=False, passphrase="")
    argvs = runner.argvs()
    assert not any("--new=1:" in part for argv in argvs for part in argv)
    assert not any("/dev/vda1" in argv for argv in argvs)
    assert not any(argv[0] == "mkfs.fat" for argv in argvs)


def test_encrypted_root_takes_passphrase_on_stdin_only() -> None:
    parts = (layout.FreePartition("root", 3, 201, 400, layout.GPT_LINUX_LUKS,
                                  layout.ROOT_LABEL, reuse=False),)
    runner = RecordingRunner()
    out = storage.commit_free_space(runner=runner, disk="/dev/vda", partitions=parts,
                                    filesystem="ext4", encrypted=True, passphrase=PASSPHRASE)
    assert all(PASSPHRASE not in arg for argv in runner.argvs() for arg in argv)
    secret_calls = [(argv, stdin) for argv, stdin in runner.calls if stdin == PASSPHRASE]
    assert [argv[1] for argv, _ in secret_calls] == ["luksFormat", "open"]
    assert out.fs_path == f"/dev/mapper/{layout.LUKS_NAME}"
    assert out.luks_path == "/dev/vda3" and out.luks_name == layout.LUKS_NAME


def test_tool_failure_names_the_tool_and_hides_stderr() -> None:
    runner = RecordingRunner(fail_on="sgdisk")
    with pytest.raises(InstallerError) as exc:
        storage.commit_free_space(runner=runner, disk="/dev/vda",
                                  partitions=_fresh_esp_and_root(), filesystem="ext4",
                                  encrypted=False, passphrase="")
    assert exc.value.code == "storage_failed"
    assert "/dev/vda2" not in exc.value.message
