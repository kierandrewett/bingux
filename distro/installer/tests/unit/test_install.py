"""Install orchestrator pieces that run without a disk: version, payload preflight, lock release."""

from __future__ import annotations

import pytest

from bingux_installer import payload
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
