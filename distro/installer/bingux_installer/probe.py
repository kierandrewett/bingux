"""System probe: firmware, disks, live medium, other OSes, tool presence.

Reads only. Other-OS detection mounts partitions read-only under a scratch
directory and unmounts them again; nothing is written to the disks.
"""

from __future__ import annotations

import importlib.util
import json
import os
import shutil
from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

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
        }


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
        })
    return disks


def _partition(dev: Mapping[str, Any]) -> dict[str, Any]:
    return {
        "name": str(dev.get("name", "")),
        "path": str(dev.get("path") or f"/dev/{dev.get('name', '')}"),
        "fstype": str(dev.get("fstype") or ""),
        "mountpoint": dev.get("mountpoint"),
        "size_bytes": _to_int(dev.get("size")),
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


def scan_partition_for_os(runner: Runner, device: str, scratch: Path) -> str | None:
    if not mount_ro(runner, device, scratch):
        return None
    try:
        return detect_os(scratch)
    finally:
        umount(runner, scratch)


def list_block_devices(runner: Runner) -> dict[str, Any]:
    try:
        result = runner.run(["lsblk", "-J", "-b", "-o",
                             "NAME,PATH,TYPE,SIZE,MODEL,RM,PKNAME,MOUNTPOINT,FSTYPE,UUID,LABEL"])
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
            partitions=raw["partitions"],
        )
        if scan_os and not is_live:
            found: list[str] = []
            for index, part in enumerate(raw["partitions"]):
                if part["mountpoint"] or part["fstype"] not in OS_SCAN_FSTYPES:
                    continue
                scratch = scratch_root / f"{Path(raw['name']).name}-{index}"
                name = scan_partition_for_os(runner, part["path"], scratch)
                if name and name not in found:
                    found.append(name)
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
