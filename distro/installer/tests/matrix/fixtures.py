"""Host-side builders for matrix disks that already hold another OS.

A fixture is a GPT disk image written without root: sfdisk edits the raw image, mkfs.ext4 -d
fills a partition file from a directory, and dd splices it in. The other OS data carries a
sha256 manifest so the booted system can prove the install left it intact.
"""

from __future__ import annotations

import hashlib
import subprocess
import tempfile
from pathlib import Path

SECTOR = 512
MIB = 1024**2
GIB = 1024**3
LINUX_DATA = "0FC63DAF-8483-4772-8E79-3D69D8477DE4"
MS_BASIC_DATA = "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7"
OTHER_LABEL = "OTHEROS"
MANIFEST = "MANIFEST.sha256"
OTHER_OS_NAME = "ExampleOS"
FIRST_SECTOR = 2048  # sfdisk's default first usable sector: 1 MiB


def _run(args: list[str], **kwargs) -> None:
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, **kwargs)


EXT4_PAYLOAD = {"home/keep.txt": "keep me\n" * 4096, "home/notes/a.txt": "alpha\n" * 2048}
NTFS_PAYLOAD = {"keep.txt": "keep me\n" * 4096, "notes-a.txt": "alpha\n" * 2048}


def _write_payload(root: Path, payload: dict[str, str]) -> None:
    """Write the payload files under root, then the sha256 manifest of every one of them."""
    lines = []
    for rel, text in sorted(payload.items()):
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        digest = subprocess.run(["sha256sum", str(path)], check=True, capture_output=True,
                                text=True).stdout.split()[0]
        lines.append(f"{digest}  {rel}")
    (root / MANIFEST).write_text("\n".join(lines) + "\n")


def _fill_ntfs(part: Path, tree: Path, payload: dict[str, str]) -> None:
    """Copy the flat payload and manifest into a freshly made NTFS image (ntfscp, no mount)."""
    for rel in [*payload, MANIFEST]:
        _run(["ntfscp", "-q", str(part), str(tree / rel), rel])


def build_other_os_disk(image: Path, disk_bytes: int, other_bytes: int, fs: str = "ext4") -> None:
    """Make `image` a raw GPT disk whose first partition is an other-OS filesystem.

    fs is ext4 (Linux data partition, nested payload) or ntfs (Microsoft basic data, flat payload).
    The partition is other_bytes long; everything after it is unallocated.
    """
    if fs not in ("ext4", "ntfs"):
        raise ValueError(f"unsupported other-OS filesystem: {fs}")
    image.parent.mkdir(parents=True, exist_ok=True)
    with image.open("wb") as handle:
        handle.truncate(disk_bytes)
    other_sectors = other_bytes // SECTOR
    type_code = LINUX_DATA if fs == "ext4" else MS_BASIC_DATA
    script = (f"label: gpt\nstart={FIRST_SECTOR}, size={other_sectors}, type={type_code}, "
              f"name=\"{OTHER_LABEL}\"\n")
    subprocess.run(["sfdisk", "--no-reread", str(image)], input=script, text=True, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    with tempfile.TemporaryDirectory(dir=image.parent) as scratch:
        scratch_path = Path(scratch)
        tree = scratch_path / "tree"
        tree.mkdir()
        part = scratch_path / "other.img"
        with part.open("wb") as handle:
            handle.truncate(other_bytes)
        if fs == "ext4":
            (tree / "etc").mkdir()
            (tree / "etc" / "os-release").write_text(f'NAME="{OTHER_OS_NAME}"\nID=exampleos\n')
            _write_payload(tree, EXT4_PAYLOAD)
            _run(["mkfs.ext4", "-q", "-F", "-L", OTHER_LABEL, "-d", str(tree), str(part)])
        else:
            _write_payload(tree, NTFS_PAYLOAD)
            _run(["mkntfs", "-F", "-Q", "-q", "-L", OTHER_LABEL, str(part)])
            _fill_ntfs(part, tree, NTFS_PAYLOAD)
        _run(["dd", f"if={part}", f"of={image}", "bs=1M", f"seek={FIRST_SECTOR * SECTOR // MIB}",
              "conv=notrunc,fsync", "status=none"])


def build_qcow2(raw: Path, qcow2: Path) -> None:
    qcow2.unlink(missing_ok=True)
    _run(["qemu-img", "convert", "-f", "raw", "-O", "qcow2", str(raw), str(qcow2)])
    raw.unlink(missing_ok=True)


def _extract_sectors(fmt: str, src: Path, dst: Path, skip: int, count: int) -> None:
    """Copy `count` 512-byte sectors starting at sector `skip` of a qcow2/raw image into a raw file."""
    flat = dst.with_suffix(".full")
    _run(["qemu-img", "convert", "-f", fmt, "-O", "raw", str(src), str(flat)])
    try:
        with flat.open("rb") as whole, dst.open("wb") as out:
            whole.seek(skip * 512)
            out.write(whole.read(count * 512))
    finally:
        flat.unlink(missing_ok=True)


def verify_ntfs_manifest(image: Path, fmt: str, scratch: Path) -> tuple[bool, str]:
    """Check the NTFS other-OS partition against its manifest from the host, after the guest is off.

    The installed guest has no NTFS driver, so the partition is read with ntfscat. The size comes
    from the NTFS boot sector (total sectors at offset 0x28), so a shrunk volume is read in full.
    """
    boot = scratch / "boot.bin"
    _extract_sectors(fmt, image, boot, FIRST_SECTOR, 1)
    total = int.from_bytes(boot.read_bytes()[0x28:0x30], "little")
    part = scratch / "other.img"
    _extract_sectors(fmt, image, part, FIRST_SECTOR, total + 1)
    listing = subprocess.run(["ntfscat", str(part), MANIFEST], capture_output=True, check=False)
    lines = listing.stdout.decode(errors="replace").splitlines()
    if listing.returncode != 0 or not lines:
        return False, f"manifest unreadable: rc={listing.returncode} {listing.stderr.decode(errors='replace')[-200:]!r}"
    bad = []
    for line in lines:
        digest, rel = line.split("  ", 1)
        data = subprocess.run(["ntfscat", str(part), rel], capture_output=True, check=False).stdout
        if hashlib.sha256(data).hexdigest() != digest:
            bad.append(rel)
    return (not bad, f"{len(lines)} files, mismatched: {bad}")


# Guest-side check: the other OS manifest still verifies after the install.
CHECK_MANIFEST = (f"d=$(mktemp -d) && mount LABEL={OTHER_LABEL} \"$d\" && "
                  f"cd \"$d\" && sha256sum -c {MANIFEST}; rc=$?; cd / && umount \"$d\" && rmdir \"$d\"; exit $rc")
