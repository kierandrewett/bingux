"""Live payload checks: the live kernel is present before commit and lands in the target /boot."""

from __future__ import annotations

import pytest

from bingux_installer import payload
from bingux_installer.protocol import InstallerError


def test_preflight_missing_kernel_is_payload_failed() -> None:
    with pytest.raises(InstallerError) as exc:
        payload.preflight("/x/airootfs.sfs", "/x/vmlinuz-linux",
                          exists=lambda path: path.endswith("airootfs.sfs"))
    assert exc.value.code == "payload_failed"
    assert "kernel" in str(exc.value)


def test_preflight_present_kernel_and_payload_passes() -> None:
    payload.preflight("/x/airootfs.sfs", "/x/vmlinuz-linux", exists=lambda path: True)


def test_copy_kernel_places_kernel_in_target_boot(tmp_path) -> None:  # noqa: ANN001
    kernel = tmp_path / "vmlinuz-linux"
    kernel.write_bytes(b"kernel-image")
    target = tmp_path / "target"
    target.mkdir()
    payload.copy_kernel(target, str(kernel))
    assert (target / "boot" / "vmlinuz-linux").read_bytes() == b"kernel-image"


def test_copy_kernel_missing_source_is_payload_failed(tmp_path) -> None:  # noqa: ANN001
    with pytest.raises(InstallerError) as exc:
        payload.copy_kernel(tmp_path, str(tmp_path / "absent"))
    assert exc.value.code == "payload_failed"
