"""System probe: firmware, disks, live medium, other OSes, tool presence.

Reads only. Other-OS detection mounts partitions read-only under a scratch
directory and unmounts them again; nothing is written to the disks.
"""

from __future__ import annotations

import importlib.util
import json
import os
import re
import shutil
from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .layout import GptTable, parse_sgdisk_print
from .protocol import InstallerError
from .runner import CommandError, Runner

LIVE_BOOTMNT = "/run/archiso/bootmnt"
PROBE_ROOT = Path("/run/bingux-installer/probe")
EFI_SYSFS = "/sys/firmware/efi"
BASE_TOOLS = ("sgdisk", "mkfs.fat", "mkfs.ext4", "mkfs.btrfs", "mkfs.xfs", "rsync", "openssl",
              "mkinitcpio", "cryptsetup", "mount", "umount", "chroot")
UEFI_TOOLS = ("bootctl",)
BIOS_TOOLS = ("grub-install", "grub-mkconfig")
SKIP_BLOCK_PREFIXES = ("loop", "sr", "zram", "ram", "fd")
OS_SCAN_FSTYPES = frozenset({"ext2", "ext3", "ext4", "btrfs", "xfs", "vfat", "ntfs",
                             "ntfs3", "exfat", "f2fs"})


@dataclass
class DiskInfo:
    path: str
    size_bytes: int
    model: str
    removable: bool
    is_live_medium: bool
    partition_table: str | None = None
    free_bytes: int | None = None
    other_os: list[str] = field(default_factory=list)
    partitions: list[dict[str, Any]] = field(default_factory=list)

    def as_json(self) -> dict[str, Any]:
        return {
            "path": self.path,
            "size_bytes": self.size_bytes,
            "model": self.model,
            "removable": self.removable,
            "is_live_medium": self.is_live_medium,
            "other_os": list(self.other_os),
            "partition_table": self.partition_table,
            "free_bytes": self.free_bytes,
            "partitions": [_partition_json(part) for part in self.partitions],
        }


PARTITION_JSON_KEYS = ("path", "fstype", "label", "size_bytes", "mountpoint", "used_bytes",
                       "free_bytes", "os", "shrinkable", "min_bytes")
SHRINKABLE_FSTYPES = frozenset({"ext4", "btrfs", "ntfs"})


def _partition_json(part: Mapping[str, Any]) -> dict[str, Any]:
    return {key: part.get(key) for key in PARTITION_JSON_KEYS}


def detect_firmware(efi_path: str = EFI_SYSFS) -> str:
    return "uefi" if os.path.isdir(efi_path) else "bios"


def missing_tools(
    *,
    firmware: str,
    which: Callable[[str], str | None] = shutil.which,
    blivet_available: bool | None = None,
) -> list[str]:
    names = list(BASE_TOOLS) + list(UEFI_TOOLS if firmware == "uefi" else BIOS_TOOLS)
    missing = [name for name in names if which(name) is None]
    if blivet_available is None:
        blivet_available = importlib.util.find_spec("blivet") is not None
    if not blivet_available:
        missing.append("python-blivet")
    return missing


def _to_int(value: Any) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def _to_bool(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return value != 0
    return str(value).strip().lower() in ("1", "true", "yes")


def parse_lsblk(doc: Mapping[str, Any]) -> list[dict[str, Any]]:
    """Flatten `lsblk -J` into disk dicts with nested partitions. Pure."""
    disks: list[dict[str, Any]] = []
    for dev in doc.get("blockdevices", []) or []:
        if not isinstance(dev, dict) or dev.get("type") != "disk":
            continue
        name = str(dev.get("name", ""))
        if name.startswith(SKIP_BLOCK_PREFIXES):
            continue
        partitions = []
        for child in dev.get("children", []) or []:
            if isinstance(child, dict) and child.get("type") in ("part", "crypt", "lvm"):
                partitions.append(_partition(child))
        disks.append({
            "name": name,
            "path": str(dev.get("path") or f"/dev/{name}"),
            "size_bytes": _to_int(dev.get("size")),
            "model": str(dev.get("model") or "").strip(),
            "removable": _to_bool(dev.get("rm")),
            "partitions": partitions,
            "partition_table": _partition_table(dev.get("pttype")),
        })
    return disks


def _partition_table(pttype: Any) -> str | None:
    """lsblk PTTYPE mapped to the README names; None when there is no table or it is unknown."""
    return {"gpt": "gpt", "dos": "msdos"}.get(str(pttype or "").lower())


def _partition(dev: Mapping[str, Any]) -> dict[str, Any]:
    label = dev.get("label")
    return {
        "name": str(dev.get("name", "")),
        "path": str(dev.get("path") or f"/dev/{dev.get('name', '')}"),
        "fstype": str(dev.get("fstype") or ""),
        "label": str(label) if label else None,
        "mountpoint": dev.get("mountpoint"),
        "size_bytes": _to_int(dev.get("size")),
        # Filled in by probe_disks. None means not read: mounted, unsupported or failed.
        "used_bytes": None,
        "free_bytes": None,
        "os": None,
        "shrinkable": None,
        "min_bytes": None,
    }


def find_live_disk(doc: Mapping[str, Any], source: str | None) -> str | None:
    """Map the live medium's backing device (e.g. /dev/sr0 or /dev/sdb1) to its disk path."""
    if not source:
        return None
    parent: dict[str, str | None] = {}

    def walk(nodes: Iterable[Any], up: str | None) -> None:
        for node in nodes or []:
            if not isinstance(node, dict):
                continue
            path = str(node.get("path") or f"/dev/{node.get('name', '')}")
            parent[path] = up
            walk(node.get("children", []), path)

    walk(doc.get("blockdevices", []), None)
    if source not in parent:
        return None
    node = source
    while parent.get(node) is not None:
        node = parent[node]  # type: ignore[assignment]
    return node


def _read_os_release(root: Path) -> str | None:
    for rel in ("etc/os-release", "usr/lib/os-release"):
        candidate = root / rel
        if candidate.is_file():
            values: dict[str, str] = {}
            for line in candidate.read_text(encoding="utf-8", errors="replace").splitlines():
                key, sep, value = line.partition("=")
                if sep:
                    values[key.strip()] = value.strip().strip('"').strip("'")
            name = values.get("PRETTY_NAME") or values.get("NAME")
            if name:
                return name
    return None


def _has_windows(root: Path) -> bool:
    for top in ("Windows", "WINDOWS", "windows"):
        system32 = root / top / "System32"
        if system32.is_dir():
            return True
    return False


def detect_os(root: Path) -> str | None:
    """Name the OS installed under a mounted filesystem root, or None. Read-only."""
    name = _read_os_release(root)
    if name:
        return name
    if _has_windows(root):
        return "Windows"
    return None


def mount_ro(runner: Runner, device: str, target: Path) -> bool:
    target.mkdir(parents=True, exist_ok=True)
    try:
        runner.run(["mount", "-o", "ro,nosuid,nodev,noexec,noload", device, str(target)])
    except CommandError:
        # noload is ext4/xfs-only; retry without it so ntfs/vfat/btrfs still work
        try:
            runner.run(["mount", "-o", "ro,nosuid,nodev,noexec", device, str(target)])
        except CommandError:
            return False
    return True


def umount(runner: Runner, target: Path) -> None:
    runner.run(["umount", str(target)], check=False)


def usage(path: Path) -> tuple[int, int]:
    """(used, free) bytes of the filesystem mounted at path. Free is what an unprivileged user may use."""
    st = os.statvfs(path)
    return (st.f_blocks - st.f_bfree) * st.f_frsize, st.f_bavail * st.f_frsize


def read_gpt(runner: Runner, path: str) -> GptTable | None:
    """The GPT of a disk from `sgdisk -p` (read-only), or None when it has none or cannot be read."""
    try:
        result = runner.run(["sgdisk", "-p", path], check=False)
    except OSError:
        return None
    if result.returncode != 0:
        return None
    try:
        return parse_sgdisk_print(result.stdout)
    except ValueError:
        return None


def unallocated_bytes(runner: Runner, path: str, partition_table: str | None) -> int | None:
    if partition_table != "gpt":
        return None
    table = read_gpt(runner, path)
    return table.free_bytes() if table is not None else None


def parse_tune2fs_block_size(text: str) -> int | None:
    match = re.search(r"^Block size:\s*(\d+)", text, re.MULTILINE)
    return int(match.group(1)) if match else None


def parse_resize2fs_min(text: str, block_size: int) -> int | None:
    match = re.search(r"Estimated minimum size of the filesystem:\s*(\d+)", text)
    return int(match.group(1)) * block_size if match else None


def parse_ntfsresize_min(text: str) -> int | None:
    match = re.search(r"You might resize at (\d+) bytes", text)
    return int(match.group(1)) if match else None


def parse_btrfs_min(text: str) -> int | None:
    match = re.search(r"(\d+)\s+bytes", text)
    return int(match.group(1)) if match else None


def ext4_min_bytes(runner: Runner, device: str) -> int | None:
    """Smallest size of an unmounted ext4 filesystem. Read-only: tune2fs -l, then resize2fs -P."""
    superblock = runner.run(["tune2fs", "-l", device], check=False)
    block_size = parse_tune2fs_block_size(superblock.stdout) if superblock.returncode == 0 else None
    if block_size is None:
        return None
    minimum = runner.run(["resize2fs", "-P", device], check=False)
    return parse_resize2fs_min(minimum.stdout, block_size) if minimum.returncode == 0 else None


def ntfs_min_bytes(runner: Runner, device: str) -> int | None:
    """Smallest size of an unmounted NTFS volume. Read-only: ntfsresize --info."""
    result = runner.run(["ntfsresize", "--info", device], check=False)
    return parse_ntfsresize_min(result.stdout) if result.returncode == 0 else None


def btrfs_min_bytes(runner: Runner, mount: Path) -> int | None:
    """Smallest device size of a mounted btrfs filesystem. Read-only: inspect-internal min-dev-size."""
    result = runner.run(["btrfs", "inspect-internal", "min-dev-size", str(mount)], check=False)
    return parse_btrfs_min(result.stdout) if result.returncode == 0 else None


def scan_partition(runner: Runner, part: Mapping[str, Any], scratch: Path) -> dict[str, Any]:
    """OS, usage and shrink limit of one filesystem not mounted by the system. Read-only.

    A value that cannot be read stays None. shrinkable is False for a filesystem the
    installer never shrinks, and None when it could shrink it but the limit is unknown.
    """
    found: dict[str, Any] = {"os": None, "used_bytes": None, "free_bytes": None,
                             "shrinkable": None, "min_bytes": None}
    fstype = str(part["fstype"])
    device = str(part["path"])
    if not mount_ro(runner, device, scratch):
        return found
    try:
        found["os"] = detect_os(scratch)
        found["used_bytes"], found["free_bytes"] = usage(scratch)
        if fstype == "btrfs":
            found["min_bytes"] = btrfs_min_bytes(runner, scratch)
    finally:
        umount(runner, scratch)
    if fstype == "ext4":
        found["min_bytes"] = ext4_min_bytes(runner, device)
    elif fstype == "ntfs":
        found["min_bytes"] = ntfs_min_bytes(runner, device)
    if found["min_bytes"] is not None:
        found["shrinkable"] = True
    elif fstype not in SHRINKABLE_FSTYPES:
        found["shrinkable"] = False
    return found


def list_block_devices(runner: Runner) -> dict[str, Any]:
    try:
        result = runner.run(["lsblk", "-J", "-b", "-o",
                             "NAME,PATH,TYPE,SIZE,MODEL,RM,PKNAME,MOUNTPOINT,FSTYPE,UUID,LABEL,PTTYPE"])
    except CommandError as exc:
        raise InstallerError("internal", f"lsblk failed: {exc.stderr.strip() or exc}") from exc
    try:
        doc = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise InstallerError("internal", "lsblk returned invalid JSON") from exc
    if not isinstance(doc, dict):
        raise InstallerError("internal", "lsblk returned an unexpected document")
    return doc


def live_source_device(runner: Runner) -> str | None:
    try:
        result = runner.run(["findmnt", "-no", "SOURCE", LIVE_BOOTMNT], check=False)
    except OSError:
        return None
    source = result.stdout.strip()
    return source or None


def probe_disks(
    runner: Runner,
    *,
    scratch_root: Path = PROBE_ROOT,
    scan_os: bool = True,
    live_source: str | None = None,
    doc: Mapping[str, Any] | None = None,
) -> list[DiskInfo]:
    """Return every whole disk with live-medium and other-OS annotations."""
    if doc is None:
        doc = list_block_devices(runner)
    if live_source is None:
        live_source = live_source_device(runner)
    live_disk = find_live_disk(doc, live_source)
    disks: list[DiskInfo] = []
    scratch_root.mkdir(parents=True, exist_ok=True)
    for raw in parse_lsblk(doc):
        is_live = raw["path"] == live_disk
        info = DiskInfo(
            path=raw["path"],
            size_bytes=raw["size_bytes"],
            model=raw["model"],
            removable=raw["removable"],
            is_live_medium=is_live,
            partition_table=raw["partition_table"],
            partitions=raw["partitions"],
        )
        if not is_live:
            info.free_bytes = unallocated_bytes(runner, info.path, info.partition_table)
        if scan_os and not is_live:
            found: list[str] = []
            for index, part in enumerate(raw["partitions"]):
                if part["mountpoint"] or part["fstype"] not in OS_SCAN_FSTYPES:
                    continue
                scratch = scratch_root / f"{Path(raw['name']).name}-{index}"
                result = scan_partition(runner, part, scratch)
                part.update(result)
                if result["os"] and result["os"] not in found:
                    found.append(result["os"])
            info.other_os = found
        disks.append(info)
    return disks


def find_disk(disks: Iterable[DiskInfo], path: str) -> DiskInfo:
    for disk in disks:
        if disk.path == path:
            return disk
    raise InstallerError("disk_not_found", f"no disk {path} was found")


def disk_report(disks: Iterable[DiskInfo]) -> list[dict[str, Any]]:
    return [d.as_json() for d in disks]
