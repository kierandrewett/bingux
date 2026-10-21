"""Plan schema v1 validation: positive and negative cases. Pure, no system files."""

from __future__ import annotations

import pytest

from bingux_installer.plan import validate_plan
from bingux_installer.protocol import InstallerError

DROP = object()
GOOD_TZ = "Europe/London"
GOOD_LOCALE = "en_GB.UTF-8"


def base_user(**changes: object) -> dict[str, object]:
    user: dict[str, object] = {"name": "kieran", "password": "correct horse",
                               "admin": True, "full_name": "Kieran"}
    for key, value in changes.items():
        if value is DROP:
            user.pop(key, None)
        else:
            user[key] = value
    return user


def base_raw() -> dict[str, object]:
    return {
        "version": 1,
        "disk": "/dev/vda",
        "confirm_erase": "/dev/vda",
        "timezone": GOOD_TZ,
        "locale": "en_GB",
        "keymap": "uk",
        "hostname": "bingux",
        "users": [base_user()],
    }


def make(booted: str = "uefi", **changes: object):
    raw = base_raw()
    for key, value in changes.items():
        if value is DROP:
            raw.pop(key, None)
        else:
            raw[key] = value
    return validate_plan(
        raw,
        booted_firmware=booted,
        timezone_exists=lambda tz: tz == GOOD_TZ,
        locale_supported=lambda loc: loc == GOOD_LOCALE,
    )


def rejects(code: str, booted: str = "uefi", **changes: object) -> None:
    with pytest.raises(InstallerError) as exc:
        make(booted, **changes)
    assert exc.value.code == code


def test_minimal_plan_gets_documented_defaults() -> None:
    plan = make()
    assert plan.firmware == "uefi"
    assert plan.filesystem == "ext4"
    assert plan.encryption.enabled is False
    assert plan.other_os_allow is False
    assert plan.locale == GOOD_LOCALE
    assert plan.kernel_args == ()
    assert plan.users[0].admin is True


def test_firmware_auto_follows_booted_firmware() -> None:
    assert make("bios").firmware == "bios"
    assert make("bios").uefi is False


def test_explicit_firmware_must_match_boot() -> None:
    assert make("uefi", firmware="uefi").firmware == "uefi"
    rejects("invalid_plan", booted="uefi", firmware="bios")
    rejects("invalid_plan", firmware="maybe")


@pytest.mark.parametrize("changes", [{"version": 2}, {"version": True}, {"version": DROP}])
def test_version_must_be_one(changes: dict[str, object]) -> None:
    rejects("invalid_plan", **changes)


@pytest.mark.parametrize("key", ["disk", "confirm_erase", "timezone", "locale", "keymap",
                                 "hostname", "users"])
def test_required_keys_are_enforced(key: str) -> None:
    rejects("invalid_plan", **{key: DROP})


def test_unknown_top_level_key_is_refused() -> None:
    rejects("invalid_plan", extra=1)


@pytest.mark.parametrize("disk", ["vda", "/tmp/disk", "/dev/", "/dev/../sda"])
def test_disk_must_be_a_dev_name(disk: str) -> None:
    rejects("invalid_plan", disk=disk, confirm_erase=disk)


def test_confirm_erase_must_equal_disk() -> None:
    rejects("invalid_plan", confirm_erase="/dev/vdb")


def test_encryption_is_refused_with_bios() -> None:
    with pytest.raises(InstallerError) as exc:
        make("bios", encryption={"enabled": True, "passphrase": "long enough"})
    assert exc.value.code == "unsupported"


def test_encryption_enabled_keeps_passphrase_for_secrets() -> None:
    plan = make(encryption={"enabled": True, "passphrase": "long enough"})
    assert plan.encrypted is True
    assert "long enough" in plan.secrets()


@pytest.mark.parametrize("encryption", [
    {"enabled": True, "passphrase": "short"},
    {"enabled": False, "passphrase": "not allowed here"},
    {"enabled": True, "passphrase": "long enough", "extra": 1},
    {"enabled": "yes"},
])
def test_encryption_rejects_bad_shapes(encryption: dict[str, object]) -> None:
    rejects("invalid_plan", encryption=encryption)


def test_filesystem_accepts_btrfs_and_rejects_others() -> None:
    assert make(filesystem="btrfs").filesystem == "btrfs"
    rejects("invalid_plan", filesystem="xfs")


def test_other_os_allow_sets_flag() -> None:
    assert make(other_os="allow").other_os_allow is True
    rejects("invalid_plan", other_os="maybe")


@pytest.mark.parametrize("hostname", ["", "-bingux", "bingux-", "under_score", "a" * 64])
def test_hostname_must_be_rfc1123_label(hostname: str) -> None:
    rejects("invalid_plan", hostname=hostname)


def test_hostname_accepts_63_characters() -> None:
    assert make(hostname="a" * 63).hostname == "a" * 63


@pytest.mark.parametrize("timezone", ["Mars/Olympus", "../etc/passwd", "UTC"])
def test_timezone_must_exist(timezone: str) -> None:
    rejects("invalid_plan", timezone=timezone)


@pytest.mark.parametrize("locale", ["english", "en_GB.latin1", "xx_YY"])
def test_locale_format_and_support(locale: str) -> None:
    rejects("invalid_plan", locale=locale)


def test_locale_gets_utf8_suffix() -> None:
    assert make(locale="en_GB").locale == "en_GB.UTF-8"


@pytest.mark.parametrize("keymap", ["", "uk layout", "x" * 65])
def test_keymap_format(keymap: str) -> None:
    rejects("invalid_plan", keymap=keymap)


@pytest.mark.parametrize("users", [
    [],
    "kieran",
    [base_user(name="kieran"), base_user(name="kieran")],
    [base_user(name="root")],
    [base_user(name="Kieran")],
    [base_user(password="short")],
    [base_user(password=DROP)],
    [base_user(extra="x")],
    [base_user(admin="yes")],
    [base_user(full_name="Kieran: admin")],
    [base_user(full_name="a" * 129)],
])
def test_users_rules(users: object) -> None:
    rejects("invalid_plan", users=users)


def test_user_admin_defaults_to_false() -> None:
    user = base_user(admin=DROP)
    plan = make(users=[user])
    assert plan.users[0].admin is False


@pytest.mark.parametrize("arg", ["root=/dev/sda2", "init=/bin/sh", "rd.luks.name=x=y",
                                 "rootflags=subvol=x", "bad arg"])
def test_kernel_args_refuse_managed_and_malformed(arg: str) -> None:
    rejects("invalid_plan", kernel_args=[arg])


def test_kernel_args_accept_safe_values() -> None:
    plan = make(kernel_args=["quiet", "mitigations=off"])
    assert plan.kernel_args == ("quiet", "mitigations=off")


def test_non_object_plan_is_refused() -> None:
    with pytest.raises(InstallerError) as exc:
        validate_plan(["not", "a", "dict"], booted_firmware="uefi",
                      timezone_exists=lambda _: True, locale_supported=lambda _: True)
    assert exc.value.code == "invalid_plan"


def test_booted_firmware_must_be_known() -> None:
    with pytest.raises(ValueError):
        make("efi")
