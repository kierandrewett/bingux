"""Plan schema v1: validation and normalisation. No I/O.

Zoneinfo and SUPPORTED-locale lookups are injected so tests need no system files.
Every rejection raises InstallerError with code invalid_plan (or unsupported).
"""

from __future__ import annotations

import re
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from typing import Any

from .protocol import InstallerError

SCHEMA_VERSION = 1

TOP_KEYS = frozenset({
    "version", "disk", "confirm_erase", "firmware", "filesystem", "encryption",
    "other_os", "hostname", "timezone", "locale", "keymap", "users", "kernel_args",
})
REQUIRED_KEYS = frozenset({"version", "disk", "confirm_erase", "timezone", "locale",
                           "keymap", "hostname", "users"})
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
FILESYSTEMS = ("ext4", "btrfs")


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
class Plan:
    disk: str
    firmware: str  # resolved: "uefi" or "bios"
    filesystem: str
    encryption: Encryption
    other_os_allow: bool
    hostname: str
    timezone: str
    locale: str  # normalised, always ends in .UTF-8
    keymap: str
    users: tuple[User, ...]
    kernel_args: tuple[str, ...]

    def secrets(self) -> tuple[str, ...]:
        values = [u.password for u in self.users]
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


def _fail(field_name: str, rule: str) -> InstallerError:
    return InstallerError("invalid_plan", f"invalid plan: {field_name}: {rule}")


def _has_control(text: str) -> bool:
    return any(ord(ch) < 32 or ord(ch) == 127 for ch in text)


def _require_str(value: Any, name: str) -> str:
    if not isinstance(value, str):
        raise _fail(name, "must be a string")
    return value


def _check_user(raw: Any, index: int) -> User:
    label = f"users[{index}]"
    if not isinstance(raw, dict):
        raise _fail(label, "must be an object")
    unknown = set(raw) - USER_KEYS
    if unknown:
        raise _fail(label, f"unknown key {sorted(unknown)[0]!r}")
    for key in ("name", "password"):
        if key not in raw:
            raise _fail(f"{label}.{key}", "is required")
    name = _require_str(raw["name"], f"{label}.name")
    if not USERNAME_RE.match(name):
        raise _fail(f"{label}.name", "must match ^[a-z_][a-z0-9_-]{0,31}$")
    if name == "root":
        raise _fail(f"{label}.name", "root cannot be created; the root account is locked")
    password = _require_str(raw["password"], f"{label}.password")
    if len(password) < MIN_PASSWORD:
        raise _fail(f"{label}.password", f"must be at least {MIN_PASSWORD} characters")
    if _has_control(password):
        raise _fail(f"{label}.password", "must not contain control characters")
    admin = raw.get("admin", False)
    if not isinstance(admin, bool):
        raise _fail(f"{label}.admin", "must be a boolean")
    full_name = raw.get("full_name", "")
    full_name = _require_str(full_name, f"{label}.full_name")
    if len(full_name) > FULL_NAME_MAX:
        raise _fail(f"{label}.full_name", f"must be at most {FULL_NAME_MAX} characters")
    if _has_control(full_name) or ":" in full_name or "," in full_name:
        raise _fail(f"{label}.full_name", "must not contain control characters, ':' or ','")
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
    passphrase = _require_str(raw.get("passphrase", ""), "encryption.passphrase")
    if len(passphrase) < MIN_PASSWORD:
        raise _fail("encryption.passphrase", f"must be at least {MIN_PASSWORD} characters")
    if _has_control(passphrase):
        raise _fail("encryption.passphrase", "must not contain control characters")
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
    confirm = _require_str(raw["confirm_erase"], "confirm_erase")
    if confirm != disk:
        raise _fail("confirm_erase", "must equal disk exactly")

    firmware = raw.get("firmware", "auto")
    if firmware not in FIRMWARES:
        raise _fail("firmware", "must be auto, uefi or bios")
    if firmware == "auto":
        firmware = booted_firmware
    elif firmware != booted_firmware:
        raise _fail("firmware", f"must match the booted firmware ({booted_firmware})")

    filesystem = raw.get("filesystem", "ext4")
    if filesystem not in FILESYSTEMS:
        raise _fail("filesystem", "must be ext4 or btrfs")

    encryption = _check_encryption(raw.get("encryption"))
    if encryption.enabled and firmware == "bios":
        raise InstallerError("unsupported", "encryption is not supported with BIOS firmware")

    other_os = raw.get("other_os", "refuse")
    if other_os not in ("refuse", "allow"):
        raise _fail("other_os", "must be refuse or allow")

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

    users_raw = raw["users"]
    if not isinstance(users_raw, list) or not users_raw:
        raise _fail("users", "must be a non-empty list")
    users = tuple(_check_user(u, i) for i, u in enumerate(users_raw))
    if len({u.name for u in users}) != len(users):
        raise _fail("users", "user names must be unique")

    return Plan(
        disk=disk,
        firmware=firmware,
        filesystem=filesystem,
        encryption=encryption,
        other_os_allow=(other_os == "allow"),
        hostname=hostname,
        timezone=timezone,
        locale=locale,
        keymap=keymap,
        users=users,
        kernel_args=_check_kernel_args(raw.get("kernel_args")),
    )
