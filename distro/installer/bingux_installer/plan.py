"""Plan schema v2: validation and normalisation. No I/O.

Zoneinfo and SUPPORTED-locale lookups are injected so tests need no system files.
Every rejection raises InstallerError with code invalid_plan (or unsupported).
Device checks (the partition exists, has room, can be shrunk) need the probe and run in
storage.py before anything is written.
"""

from __future__ import annotations

import re
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from typing import Any

from .protocol import InstallerError

SCHEMA_VERSION = 2

MIB = 1024 * 1024
GIB = 1024 * MIB

TOP_KEYS = frozenset({
    "version", "disk", "confirm_erase", "firmware", "storage", "filesystem", "encryption",
    "swap", "swap_bytes", "other_os", "hostname", "timezone", "locale", "keymap", "ntp",
    "users", "root_password", "autologin", "kernel_args",
})
REQUIRED_KEYS = frozenset({"version", "disk", "timezone", "locale", "keymap", "hostname", "users"})
STORAGE_KEYS = frozenset({"mode", "shrink", "partitions"})
SHRINK_KEYS = frozenset({"partition", "size_bytes"})
PARTITION_KEYS = frozenset({"device", "action", "role", "size_bytes", "fs"})
USER_KEYS = frozenset({"name", "password", "admin", "full_name"})

DISK_RE = re.compile(r"^/dev/[A-Za-z0-9][A-Za-z0-9._-]*$")
HOSTNAME_RE = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$")
TIMEZONE_RE = re.compile(r"^[A-Za-z][A-Za-z0-9_+-]*(?:/[A-Za-z0-9_+-]+)+$")
LOCALE_RE = re.compile(r"^[A-Za-z]{2,3}_[A-Za-z]{2}(\.UTF-8)?$")
KEYMAP_RE = re.compile(r"^[A-Za-z0-9_-]{1,64}$")
USERNAME_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
KERNEL_ARG_RE = re.compile(r"^[A-Za-z0-9_.=,:/+-]{1,200}$")
KERNEL_ARG_REFUSED_PREFIXES = ("root=", "init=", "rd.luks.", "rootflags=")

MIN_PASSWORD = 8
FULL_NAME_MAX = 128
FIRMWARES = ("auto", "uefi", "bios")
STORAGE_MODES = ("erase", "free_space", "alongside", "manual")
FILESYSTEMS = ("ext4", "btrfs", "xfs")
SWAP_KINDS = ("none", "zram", "file", "partition")
SWAP_MIN = 1 * GIB
SWAP_MAX = 64 * GIB
PARTITION_ROLES = ("esp", "bios_boot", "boot", "root", "home", "swap")
PARTITION_ACTIONS = ("reuse", "format", "create")
CREATE_MIN = 64 * MIB

# role -> actions allowed for that role. Filesystems are fixed per role except home.
ROLE_ACTIONS: dict[str, tuple[str, ...]] = {
    "esp": ("reuse", "format", "create"),
    "bios_boot": ("reuse", "create"),
    "boot": ("format", "create"),
    "root": ("format", "create"),
    "home": ("reuse", "format", "create"),
    "swap": ("format", "create"),
}
HOME_FILESYSTEMS = ("ext4", "btrfs", "xfs")


@dataclass(frozen=True)
class User:
    name: str
    password: str = field(repr=False)
    admin: bool
    full_name: str


@dataclass(frozen=True)
class Encryption:
    enabled: bool
    passphrase: str = field(default="", repr=False)


@dataclass(frozen=True)
class Shrink:
    partition: str
    size_bytes: int


@dataclass(frozen=True)
class ManualPartition:
    device: str
    action: str  # reuse | format | create
    role: str  # esp | bios_boot | boot | root | home | swap
    size_bytes: int  # create only; 0 otherwise
    fs: str  # home only: ext4 | btrfs | xfs; empty otherwise


@dataclass(frozen=True)
class Plan:
    disk: str
    firmware: str  # resolved: "uefi" or "bios"
    storage_mode: str  # erase | free_space | alongside | manual
    shrink: Shrink | None  # alongside only
    partitions: tuple[ManualPartition, ...]  # manual only
    filesystem: str
    encryption: Encryption
    swap: str  # none | zram | file | partition
    swap_bytes: int  # file and partition only; 0 otherwise
    other_os_allow: bool
    hostname: str
    timezone: str
    locale: str  # normalised, always ends in .UTF-8
    keymap: str
    ntp: bool
    users: tuple[User, ...]
    root_password: str | None = field(default=None, repr=False)
    autologin: str | None = None
    kernel_args: tuple[str, ...] = ()

    def secrets(self) -> tuple[str, ...]:
        values = [u.password for u in self.users]
        values.append(self.root_password or "")
        if self.encryption.enabled:
            values.append(self.encryption.passphrase)
        return tuple(v for v in values if v)

    @property
    def uefi(self) -> bool:
        return self.firmware == "uefi"

    @property
    def encrypted(self) -> bool:
        return self.encryption.enabled

    @property
    def has_admin(self) -> bool:
        return any(u.admin for u in self.users)

    @property
    def destructive(self) -> bool:
        """True when the plan may erase the whole disk (erase) or some partitions (manual)."""
        return self.storage_mode in ("erase", "manual")


def _fail(field_name: str, rule: str) -> InstallerError:
    return InstallerError("invalid_plan", f"invalid plan: {field_name}: {rule}")


def _has_control(text: str) -> bool:
    return any(ord(ch) < 32 or ord(ch) == 127 for ch in text)


def _require_str(value: Any, name: str) -> str:
    if not isinstance(value, str):
        raise _fail(name, "must be a string")
    return value


def _require_int(value: Any, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise _fail(name, "must be an integer number of bytes")
    return value


def _check_password(value: Any, name: str) -> str:
    password = _require_str(value, name)
    if len(password) < MIN_PASSWORD:
        raise _fail(name, f"must be at least {MIN_PASSWORD} characters")
    if _has_control(password):
        raise _fail(name, "must not contain control characters")
    return password


def _check_user(raw: Any, index: int) -> User:
    label = f"users[{index}]"
    if not isinstance(raw, dict):
        raise _fail(label, "must be an object")
    unknown = set(raw) - USER_KEYS
    if unknown:
        raise _fail(f"{label}.{sorted(unknown)[0]}", "unknown key")
    for key in ("name", "password"):
        if key not in raw:
            raise _fail(f"{label}.{key}", "is required")
    name = _require_str(raw["name"], f"{label}.name")
    if not USERNAME_RE.match(name) or name == "root":
        raise _fail(f"{label}.name", "must match ^[a-z_][a-z0-9_-]{0,31}$ and not be root")
    password = _check_password(raw["password"], f"{label}.password")
    admin = raw.get("admin", False)
    if not isinstance(admin, bool):
        raise _fail(f"{label}.admin", "must be a boolean")
    full_name = raw.get("full_name", "")
    full_name = _require_str(full_name, f"{label}.full_name")
    if len(full_name) > FULL_NAME_MAX or _has_control(full_name) or ":" in full_name or "," in full_name:
        raise _fail(f"{label}.full_name",
                    f"must be at most {FULL_NAME_MAX} characters with no control characters, ':' or ','")
    return User(name=name, password=password, admin=admin, full_name=full_name)


def _check_encryption(raw: Any) -> Encryption:
    if raw is None:
        return Encryption(enabled=False)
    if not isinstance(raw, dict):
        raise _fail("encryption", "must be an object")
    unknown = set(raw) - {"enabled", "passphrase"}
    if unknown:
        raise _fail("encryption", f"unknown key {sorted(unknown)[0]!r}")
    enabled = raw.get("enabled", False)
    if not isinstance(enabled, bool):
        raise _fail("encryption.enabled", "must be a boolean")
    if not enabled:
        if "passphrase" in raw:
            raise _fail("encryption.passphrase", "is only allowed when enabled is true")
        return Encryption(enabled=False)
    passphrase = _check_password(raw.get("passphrase", ""), "encryption.passphrase")
    return Encryption(enabled=True, passphrase=passphrase)


def _check_kernel_args(raw: Any) -> tuple[str, ...]:
    if raw is None:
        return ()
    if not isinstance(raw, list):
        raise _fail("kernel_args", "must be a list of strings")
    out: list[str] = []
    for index, arg in enumerate(raw):
        label = f"kernel_args[{index}]"
        if not isinstance(arg, str):
            raise _fail(label, "must be a string")
        if not KERNEL_ARG_RE.match(arg):
            raise _fail(label, "must match ^[A-Za-z0-9_.=,:/+-]{1,200}$")
        if arg.startswith(KERNEL_ARG_REFUSED_PREFIXES):
            raise _fail(label, "is managed by the installer and cannot be set")
        out.append(arg)
    return tuple(out)


def _check_shrink(raw: Any, disk: str) -> Shrink:
    if not isinstance(raw, dict):
        raise _fail("storage.shrink", "must be an object")
    unknown = set(raw) - SHRINK_KEYS
    if unknown:
        raise _fail(f"storage.shrink.{sorted(unknown)[0]}", "unknown key")
    partition = _require_str(raw.get("partition"), "storage.shrink.partition")
    if not _is_partition_of(partition, disk):
        raise _fail("storage.shrink.partition", f"must be a partition of {disk}")
    size = _require_int(raw.get("size_bytes"), "storage.shrink.size_bytes")
    if size <= 0:
        raise _fail("storage.shrink.size_bytes", "must be positive")
    return Shrink(partition=partition, size_bytes=size)


def _is_partition_of(device: str, disk: str) -> bool:
    return re.fullmatch(re.escape(disk) + r"p?[0-9]+", device) is not None


def _check_manual_partition(raw: Any, index: int, disk: str) -> ManualPartition:
    label = f"storage.partitions[{index}]"
    if not isinstance(raw, dict):
        raise _fail(label, "must be an object")
    unknown = set(raw) - PARTITION_KEYS
    if unknown:
        raise _fail(f"{label}.{sorted(unknown)[0]}", "unknown key")
    for key in ("device", "action", "role"):
        if key not in raw:
            raise _fail(f"{label}.{key}", "is required")
    device = _require_str(raw["device"], f"{label}.device")
    if not _is_partition_of(device, disk):
        raise _fail(f"{label}.device", f"must be a partition of {disk}")
    role = raw["role"]
    if role not in PARTITION_ROLES:
        raise _fail(f"{label}.role", "must be esp, bios_boot, boot, root, home or swap")
    action = raw["action"]
    if action not in PARTITION_ACTIONS:
        raise _fail(f"{label}.action", "must be reuse, format or create")
    if action not in ROLE_ACTIONS[role]:
        raise _fail(f"{label}.action", f"{action} is not allowed for role {role}")
    size = 0
    if action == "create":
        size = _require_int(raw.get("size_bytes"), f"{label}.size_bytes")
        if size < CREATE_MIN:
            raise _fail(f"{label}.size_bytes", "must be at least 64 MiB")
    elif "size_bytes" in raw:
        raise _fail(f"{label}.size_bytes", "is only allowed with action create")
    fs = ""
    if "fs" in raw:
        if role != "home":
            raise _fail(f"{label}.fs", "is only allowed for role home")
        if action == "reuse":
            raise _fail(f"{label}.fs", "cannot be set with action reuse")
        fs = _require_str(raw["fs"], f"{label}.fs")
        if fs not in HOME_FILESYSTEMS:
            raise _fail(f"{label}.fs", "must be ext4, btrfs or xfs")
    elif role == "home" and action != "reuse":
        fs = "ext4"
    return ManualPartition(device=device, action=action, role=role, size_bytes=size, fs=fs)


def _check_manual_layout(
    parts: tuple[ManualPartition, ...], *, uefi: bool, encrypted: bool, swap: str
) -> None:
    """Role counts and firmware rules for a manual layout. Pure: no devices involved."""
    devices = [p.device for p in parts]
    if len(set(devices)) != len(devices):
        raise _fail("storage.partitions", "each device may appear only once")
    counts = {role: sum(1 for p in parts if p.role == role) for role in PARTITION_ROLES}
    for role, count in counts.items():
        if count > 1:
            raise _fail("storage.partitions", f"at most one {role} partition is allowed")
    if counts["root"] != 1:
        raise _fail("storage.partitions", "exactly one root partition is required")
    if uefi:
        if counts["esp"] != 1:
            raise _fail("storage.partitions", "UEFI needs exactly one esp partition")
        if counts["bios_boot"] or counts["boot"]:
            raise _fail("storage.partitions", "bios_boot and boot are BIOS-only roles")
    else:
        if counts["bios_boot"] != 1:
            raise _fail("storage.partitions", "BIOS needs exactly one bios_boot partition")
        if counts["esp"]:
            raise _fail("storage.partitions", "esp is a UEFI-only role")
        if encrypted and not counts["boot"]:
            raise _fail("storage.partitions", "BIOS with encryption needs a boot partition")
    if counts["swap"] and swap != "partition":
        raise _fail("swap", "must be partition when the layout has a swap partition")
    if swap == "partition" and not counts["swap"]:
        raise _fail("storage.partitions", "swap partition needs a swap role entry")


def _check_storage(raw: Any, *, disk: str, uefi: bool, encrypted: bool, swap: str) -> tuple[
        str, Shrink | None, tuple[ManualPartition, ...]]:
    if raw is None:
        return "erase", None, ()
    if not isinstance(raw, dict):
        raise _fail("storage", "must be an object")
    unknown = set(raw) - STORAGE_KEYS
    if unknown:
        raise _fail(f"storage.{sorted(unknown)[0]}", "unknown key")
    mode = raw.get("mode", "erase")
    if mode not in STORAGE_MODES:
        raise _fail("storage.mode", "must be erase, free_space, alongside or manual")
    if mode == "alongside":
        if "shrink" not in raw:
            raise _fail("storage.shrink", "is required for mode alongside")
        if "partitions" in raw:
            raise _fail("storage.partitions", "is only allowed for mode manual")
        return mode, _check_shrink(raw["shrink"], disk), ()
    if mode == "manual":
        if "shrink" in raw:
            raise _fail("storage.shrink", "is only allowed for mode alongside")
        parts_raw = raw.get("partitions")
        if not isinstance(parts_raw, list) or not parts_raw:
            raise _fail("storage.partitions", "must be a non-empty list")
        parts = tuple(_check_manual_partition(p, i, disk) for i, p in enumerate(parts_raw))
        _check_manual_layout(parts, uefi=uefi, encrypted=encrypted, swap=swap)
        return mode, None, parts
    for key in ("shrink", "partitions"):
        if key in raw:
            raise _fail(f"storage.{key}", f"is only allowed for mode {'alongside' if key == 'shrink' else 'manual'}")
    return mode, None, ()


def validate_plan(
    raw: Any,
    *,
    booted_firmware: str,
    timezone_exists: Callable[[str], bool],
    locale_supported: Callable[[str], bool],
) -> Plan:
    """Validate a decoded plan object and return a normalised Plan.

    booted_firmware: "uefi" or "bios", as probed from the running system.
    timezone_exists: True when "Region/City" is a zoneinfo file.
    locale_supported: True when the normalised locale ("en_GB.UTF-8") is in SUPPORTED.
    """
    if booted_firmware not in ("uefi", "bios"):
        raise ValueError("booted_firmware must be 'uefi' or 'bios'")
    if not isinstance(raw, Mapping):
        raise _fail("plan", "must be an object")
    unknown = set(raw) - TOP_KEYS
    if unknown:
        raise _fail(sorted(unknown)[0], "unknown key")
    missing = REQUIRED_KEYS - set(raw)
    if missing:
        raise _fail(sorted(missing)[0], "is required")

    version = raw["version"]
    if isinstance(version, bool) or version != SCHEMA_VERSION:
        raise _fail("version", f"must be {SCHEMA_VERSION}")

    disk = _require_str(raw["disk"], "disk")
    if not DISK_RE.match(disk):
        raise _fail("disk", "must be a /dev/ block device name")

    firmware = raw.get("firmware", "auto")
    if firmware not in FIRMWARES:
        raise _fail("firmware", "must be auto, uefi or bios")
    if firmware == "auto":
        firmware = booted_firmware
    elif firmware != booted_firmware:
        raise _fail("firmware", f"must match the booted firmware ({booted_firmware})")

    filesystem = raw.get("filesystem", "ext4")
    if filesystem not in FILESYSTEMS:
        raise _fail("filesystem", "must be ext4, btrfs or xfs")

    encryption = _check_encryption(raw.get("encryption"))

    swap = raw.get("swap", "none")
    if swap not in SWAP_KINDS:
        raise _fail("swap", "must be none, zram, file or partition")

    mode, shrink, partitions = _check_storage(
        raw.get("storage"), disk=disk, uefi=(firmware == "uefi"),
        encrypted=encryption.enabled, swap=swap)

    confirm_required = mode in ("erase", "manual")
    if confirm_required:
        confirm = _require_str(raw.get("confirm_erase"), "confirm_erase")
        if confirm != disk:
            raise _fail("confirm_erase", "must equal disk exactly")
    elif "confirm_erase" in raw:
        raise _fail("confirm_erase", "is only allowed for storage modes erase and manual")

    if "other_os" in raw and mode != "erase":
        raise _fail("other_os", "is only allowed for storage mode erase")
    other_os = raw.get("other_os", "refuse")
    if other_os not in ("refuse", "allow"):
        raise _fail("other_os", "must be refuse or allow")

    swap_bytes = 0
    if swap in ("file", "partition") and mode == "manual" and swap == "partition":
        if "swap_bytes" in raw:
            raise _fail("swap_bytes", "is not allowed with a manual swap partition")
    elif swap in ("file", "partition"):
        swap_bytes = _require_int(raw.get("swap_bytes"), "swap_bytes")
        if not SWAP_MIN <= swap_bytes <= SWAP_MAX:
            raise _fail("swap_bytes", "must be between 1 GiB and 64 GiB")
    elif "swap_bytes" in raw:
        raise _fail("swap_bytes", "is only allowed with swap file or partition")

    hostname = _require_str(raw["hostname"], "hostname")
    if not HOSTNAME_RE.match(hostname):
        raise _fail("hostname", "must be an RFC 1123 label of 1-63 characters")

    timezone = _require_str(raw["timezone"], "timezone")
    if not TIMEZONE_RE.match(timezone) or ".." in timezone:
        raise _fail("timezone", "must be Region/City")
    if not timezone_exists(timezone):
        raise _fail("timezone", f"{timezone} is not in the zoneinfo database")

    locale = _require_str(raw["locale"], "locale")
    if not LOCALE_RE.match(locale):
        raise _fail("locale", "must match ^[A-Za-z]{2,3}_[A-Za-z]{2}(\\.UTF-8)?$")
    if not locale.endswith(".UTF-8"):
        locale += ".UTF-8"
    if not locale_supported(locale):
        raise _fail("locale", f"{locale} is not listed in SUPPORTED")

    keymap = _require_str(raw["keymap"], "keymap")
    if not KEYMAP_RE.match(keymap):
        raise _fail("keymap", "must match ^[A-Za-z0-9_-]{1,64}$")

    ntp = raw.get("ntp", True)
    if not isinstance(ntp, bool):
        raise _fail("ntp", "must be a boolean")

    users_raw = raw["users"]
    if not isinstance(users_raw, list) or not users_raw:
        raise _fail("users", "must be a non-empty list")
    users = tuple(_check_user(u, i) for i, u in enumerate(users_raw))
    if len({u.name for u in users}) != len(users):
        raise _fail("users", "user names must be unique")

    root_password = raw.get("root_password")
    if root_password is not None:
        root_password = _check_password(root_password, "root_password")

    autologin = raw.get("autologin")
    if autologin is not None:
        autologin = _require_str(autologin, "autologin")
        if autologin not in {u.name for u in users}:
            raise _fail("autologin", "must name a user in users")

    return Plan(
        disk=disk,
        firmware=firmware,
        storage_mode=mode,
        shrink=shrink,
        partitions=partitions,
        filesystem=filesystem,
        encryption=encryption,
        swap=swap,
        swap_bytes=swap_bytes,
        other_os_allow=(other_os == "allow"),
        hostname=hostname,
        timezone=timezone,
        locale=locale,
        keymap=keymap,
        ntp=ntp,
        users=users,
        root_password=root_password,
        autologin=autologin,
        kernel_args=_check_kernel_args(raw.get("kernel_args")),
    )
