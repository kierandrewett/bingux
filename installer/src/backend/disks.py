import json
import subprocess


def list_disks():
    """Return list of disk dicts from lsblk."""
    try:
        out = subprocess.run(
            ["lsblk", "-Jbpo", "NAME,SIZE,TYPE,FSTYPE,LABEL,MOUNTPOINT,PARTTYPE,MODEL"],
            capture_output=True, text=True, check=True,
        )
        data = json.loads(out.stdout)
        return [d for d in data.get("blockdevices", []) if d.get("type") == "disk"]
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        return []


def list_partitions(disk_path):
    """Return list of partition dicts for a given disk."""
    try:
        out = subprocess.run(
            ["lsblk", "-Jbpo", "NAME,SIZE,TYPE,FSTYPE,LABEL,MOUNTPOINT,PARTTYPE", disk_path],
            capture_output=True, text=True, check=True,
        )
        data = json.loads(out.stdout)
        devs = data.get("blockdevices", [])
        if devs:
            return devs[0].get("children", [])
        return []
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        return []


def format_size(size_bytes):
    """Format bytes to human-readable string."""
    if size_bytes is None:
        return "?"
    size = int(size_bytes)
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} PB"


def detect_partitions(disk_path):
    """Auto-detect EFI, root, and swap partitions on a disk."""
    parts = list_partitions(disk_path)
    efi = root = swap = ""
    EFI_GUID = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"
    SWAP_GUID = "0657fd6d-a4ab-43c4-84e5-0933c84b4f4f"

    for p in parts:
        name = p.get("name", "")
        fstype = p.get("fstype") or ""
        parttype = (p.get("parttype") or "").lower()
        size = int(p.get("size") or 0)

        if not efi:
            if parttype == EFI_GUID or (fstype == "vfat" and size <= 2 * 1024**3):
                efi = name
                continue

        if fstype == "swap" or parttype == SWAP_GUID:
            swap = name
            continue

        if not root and fstype != "vfat":
            root = name

    return efi, root, swap
