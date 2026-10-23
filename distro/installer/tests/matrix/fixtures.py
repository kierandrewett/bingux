"""Host-side builders for matrix disks that already hold another OS.

A fixture is a GPT disk image written without root: sfdisk edits the raw image, mkfs.ext4 -d
fills a partition file from a directory, and dd splices it in. The other OS data carries a
sha256 manifest so the booted system can prove the install left it intact.
"""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path

SECTOR = 512
MIB = 1024**2
GIB = 1024**3
LINUX_DATA = "0FC63DAF-8483-4772-8E79-3D69D8477DE4"
OTHER_LABEL = "OTHEROS"
MANIFEST = "MANIFEST.sha256"
OTHER_OS_NAME = "ExampleOS"
FIRST_SECTOR = 2048  # sfdisk's default first usable sector: 1 MiB


def _run(args: list[str], **kwargs) -> None:
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, **kwargs)


def _other_os_tree(root: Path) -> None:
    """Write a small other-OS payload with a manifest of every file it holds."""
    (root / "etc").mkdir(parents=True)
    (root / "etc" / "os-release").write_text(f'NAME="{OTHER_OS_NAME}"\nID=exampleos\n')
    (root / "home").mkdir()
    payload = {"home/keep.txt": "keep me\n" * 4096, "home/notes/a.txt": "alpha\n" * 2048}
    for rel, text in payload.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    lines = []
    for rel in sorted(payload):
        digest = subprocess.run(["sha256sum", str(root / rel)], check=True, capture_output=True,
                                text=True).stdout.split()[0]
        lines.append(f"{digest}  {rel}")
    (root / MANIFEST).write_text("\n".join(lines) + "\n")


def build_other_os_disk(image: Path, disk_bytes: int, other_bytes: int) -> None:
    """Make `image` a raw GPT disk whose first partition is an ext4 other-OS filesystem.

    The partition is other_bytes long; everything after it is unallocated.
    """
    image.parent.mkdir(parents=True, exist_ok=True)
    with image.open("wb") as handle:
        handle.truncate(disk_bytes)
    other_sectors = other_bytes // SECTOR
    script = (f"label: gpt\nstart={FIRST_SECTOR}, size={other_sectors}, type={LINUX_DATA}, "
              f"name=\"{OTHER_LABEL}\"\n")
    subprocess.run(["sfdisk", "--no-reread", str(image)], input=script, text=True, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    with tempfile.TemporaryDirectory(dir=image.parent) as scratch:
        scratch_path = Path(scratch)
        tree = scratch_path / "tree"
        _other_os_tree(tree)
        part = scratch_path / "other.img"
        with part.open("wb") as handle:
            handle.truncate(other_bytes)
        _run(["mkfs.ext4", "-q", "-F", "-L", OTHER_LABEL, "-d", str(tree), str(part)])
        _run(["dd", f"if={part}", f"of={image}", "bs=1M", f"seek={FIRST_SECTOR * SECTOR // MIB}",
              "conv=notrunc,fsync", "status=none"])


def build_qcow2(raw: Path, qcow2: Path) -> None:
    qcow2.unlink(missing_ok=True)
    _run(["qemu-img", "convert", "-f", "raw", "-O", "qcow2", str(raw), str(qcow2)])
    raw.unlink(missing_ok=True)


# Guest-side check: the other OS manifest still verifies after the install.
CHECK_MANIFEST = (f"mkdir -p /mnt/other && mount LABEL={OTHER_LABEL} /mnt/other && "
                  f"cd /mnt/other && sha256sum -c {MANIFEST} && cd / && umount /mnt/other")
