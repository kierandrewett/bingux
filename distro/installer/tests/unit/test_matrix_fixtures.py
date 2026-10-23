"""Host-side matrix fixtures: the NTFS other-OS partition verifies, and a changed file is caught."""

from __future__ import annotations

import importlib.util
import shutil
import sys
from pathlib import Path

import pytest

MATRIX = Path(__file__).resolve().parents[1] / "matrix"
PART_BYTES = 32 * 1024**2

needs_ntfs = pytest.mark.skipif(
    not all(shutil.which(tool) for tool in ("mkntfs", "ntfscp", "ntfscat", "qemu-img", "sfdisk", "dd")),
    reason="needs ntfsprogs, qemu-img, sfdisk and dd on the host")


def _fixtures():
    sys.path.insert(0, str(MATRIX))
    try:
        spec = importlib.util.spec_from_file_location("matrix_fixtures", MATRIX / "fixtures.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        sys.path.remove(str(MATRIX))


@needs_ntfs
def test_ntfs_manifest_verifies_then_catches_a_changed_file(tmp_path: Path) -> None:
    fixtures = _fixtures()
    image = tmp_path / "disk.raw"
    fixtures.build_other_os_disk(image, 64 * 1024**2, PART_BYTES, "ntfs")
    scratch = tmp_path / "scratch"
    scratch.mkdir()

    ok, detail = fixtures.verify_ntfs_manifest(image, "raw", scratch)
    assert ok, detail

    # change one payload byte in place (same length, manifest untouched) and splice it back
    part = tmp_path / "part.img"
    fixtures._extract_sectors("raw", image, part, fixtures.FIRST_SECTOR, PART_BYTES // 512)
    data = bytearray(part.read_bytes())
    at = data.find(b"keep me\n")
    assert at > 0, "payload text not found in the NTFS partition"
    data[at:at + 4] = b"KEEP"
    with image.open("r+b") as handle:
        handle.seek(fixtures.FIRST_SECTOR * 512)
        handle.write(bytes(data))

    ok, detail = fixtures.verify_ntfs_manifest(image, "raw", scratch)
    assert not ok
    assert "keep.txt" in detail
