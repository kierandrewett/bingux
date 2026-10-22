"""Install orchestrator pieces that run without a disk: version, payload preflight, lock release."""

from __future__ import annotations

import pytest

from bingux_installer import layout, payload
from bingux_installer.install import Env, run_install, run_probe, version_data
from bingux_installer.protocol import InstallerError, redact


class NoRunner:
    def run(self, argv, **kwargs):  # noqa: ANN001, ANN003
        raise AssertionError(f"unexpected command: {argv}")

    def stream(self, argv, on_line, **kwargs):  # noqa: ANN001, ANN003
        raise AssertionError(f"unexpected command: {argv}")


def test_version_data_reports_version_and_protocol() -> None:
    data = version_data()
    assert data == {"installer_version": "0.1.0", "protocol": 1}


def test_preflight_missing_payload_is_payload_failed(tmp_path) -> None:  # noqa: ANN001
    with pytest.raises(InstallerError) as exc:
        payload.preflight(str(tmp_path / "airootfs.sfs"), exists=lambda path: False)
    assert exc.value.code == "payload_failed"


def test_preflight_present_payload_passes() -> None:
    payload.preflight("/x/airootfs.sfs", exists=lambda path: True)


def test_run_install_rejects_bad_version_and_releases_lock() -> None:
    env = Env(runner=NoRunner())
    events: list[dict] = []
    with pytest.raises(InstallerError) as exc:
        run_install({"version": 2}, emit=events.append, env=env)
    assert exc.value.code == "invalid_plan"
    assert env.state.running is False


def test_run_install_emits_validate_phase_first() -> None:
    env = Env(runner=NoRunner())
    events: list[dict] = []
    with pytest.raises(InstallerError):
        run_install("not an object", emit=events.append, env=env)
    assert events[0] == {"event": "progress", "phase": "validate", "percent": None,
                         "message": "validating the plan"}


def test_redact_hides_passwords() -> None:
    assert "hunter22" not in redact("password hunter22 leaked", ["hunter22"])


def test_probe_is_refused_while_install_runs() -> None:
    env = Env(runner=NoRunner())
    env.state.acquire()
    with pytest.raises(InstallerError) as exc:
        run_probe(env)
    assert exc.value.code == "busy"


def _base_plan():  # noqa: ANN202
    from bingux_installer.plan import validate_plan

    return validate_plan(
        {"version": 2, "disk": "/dev/vda", "confirm_erase": "/dev/vda",
         "timezone": "Europe/London",
         "locale": "en_GB", "keymap": "uk", "hostname": "bingux",
         "users": [{"name": "kieran", "password": "correct horse", "admin": True}]},
        booted_firmware="uefi", timezone_exists=lambda _: True, locale_supported=lambda _: True)


def test_manual_encrypted_home_is_refused() -> None:
    from dataclasses import replace

    from bingux_installer.install import _refuse_unimplemented
    from bingux_installer.plan import Encryption, ManualPartition

    manual = replace(_base_plan(), storage_mode="manual",
                     encryption=Encryption(enabled=True, passphrase="hunter22-secret"),
                     partitions=(ManualPartition("/dev/vda3", "reuse", "home", 0, "ext4"),))
    with pytest.raises(InstallerError) as exc:
        _refuse_unimplemented(manual)
    assert exc.value.code == "unsupported"


@pytest.mark.parametrize("changes", [
    {"filesystem": "xfs"},
    {"storage_mode": "alongside"},
    {"storage_mode": "free_space"},
    {"storage_mode": "manual"},
    {"swap": "zram"},
    {"swap": "file", "swap_bytes": 2 * 1024 ** 3},
    {"swap": "partition", "swap_bytes": 2 * 1024 ** 3},
    {"ntp": False},
    {"root_password": "root pass word"},
    {"autologin": "kieran"},
])
def test_implemented_plan_features_are_accepted(changes: dict[str, object]) -> None:
    from dataclasses import replace

    from bingux_installer.install import _refuse_unimplemented

    _refuse_unimplemented(replace(_base_plan(), **changes))


def _manual_table() -> layout.GptTable:
    esp_end = 2048 + layout.ESP_BYTES // 512 - 1
    home_start = esp_end + 1
    home_end = home_start + 10 * 1024 ** 3 // 512 - 1
    return layout.GptTable(
        sector_size=512, first_usable=34, last_usable=home_end + 60 * 1024 ** 3 // 512,
        partitions=(layout.GptPartition(1, 2048, esp_end, layout.GPT_ESP, "EFI"),
                    layout.GptPartition(3, home_start, home_end, layout.GPT_LINUX, "home")))


def _manual_disk(fs3: str):  # noqa: ANN202
    from types import SimpleNamespace

    return SimpleNamespace(partitions=[
        {"path": "/dev/vda1", "fstype": "vfat", "mountpoint": None},
        {"path": "/dev/vda3", "fstype": fs3, "mountpoint": None},
    ])


def test_plan_manual_maps_the_plan_onto_the_probed_disk() -> None:
    from dataclasses import replace

    from bingux_installer.install import _plan_manual
    from bingux_installer.plan import ManualPartition

    plan = replace(_base_plan(), storage_mode="manual", partitions=(
        ManualPartition("/dev/vda1", "reuse", "esp", 0, ""),
        ManualPartition("/dev/vda3", "reuse", "home", 0, "btrfs"),
        ManualPartition("/dev/vda4", "create", "root", 20 * 1024 ** 3, ""),
    ))
    out = _plan_manual(plan, _manual_disk("ext4"), _manual_table())
    assert [(p.role, p.number, p.action) for p in out] == [
        ("esp", 1, "reuse"), ("home", 3, "reuse"), ("root", 4, "create")]
    assert out[1].fs == "ext4"  # the filesystem on disk wins over the plan's


def test_plan_manual_refuses_a_reused_home_that_is_not_a_linux_filesystem() -> None:
    from dataclasses import replace

    from bingux_installer.install import _plan_manual
    from bingux_installer.plan import ManualPartition

    plan = replace(_base_plan(), storage_mode="manual", partitions=(
        ManualPartition("/dev/vda3", "reuse", "home", 0, ""),
    ))
    with pytest.raises(InstallerError) as exc:
        _plan_manual(plan, _manual_disk("vfat"), _manual_table())
    assert exc.value.code == "invalid_plan"
