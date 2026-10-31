"""Fixtures for the app-level suites: the built Bingux apps in their headless `--snapshot` mode."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[3]
SCRIPTS = REPO / "scripts"


def has_snapshot_mode(binary: Path, workdir: Path) -> bool:
    env = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "DISPLAY")}
    env.update(
        GSETTINGS_BACKEND="memory", BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS="300", XDG_CACHE_HOME=str(workdir / "cache")
    )
    done = subprocess.run(
        [str(binary), "--snapshot", str(workdir), "mouse"], env=env, capture_output=True, text=True, timeout=120
    )
    return done.returncode == 0 and bool(list(workdir.glob("*.png")))


@pytest.fixture(scope="session")
def settings_binary(stack, tmp_path_factory) -> Path:
    """A bingux-settings built with `--features snapshot`, or a skip that says how to build it."""
    builds = stack.binaries("bingux-settings")
    if not builds:
        pytest.skip("bingux-settings is not built: `cargo build --features snapshot` in apps/settings")
    for index, binary in enumerate(builds):
        if has_snapshot_mode(binary, tmp_path_factory.mktemp(f"snapshot-probe-{index}")):
            return binary
    pytest.skip(
        f"none of {[str(b) for b in builds]} has a --snapshot mode: build apps/settings with --features snapshot"
    )


@pytest.fixture(scope="session")
def cargo_target_dir(settings_binary) -> Path:
    """The CARGO_TARGET_DIR the probe scripts expect: `<dir>/debug/bingux-settings`."""
    if settings_binary.parent.name not in ("debug", "release"):
        pytest.skip(f"{settings_binary} is not in a Cargo target directory")
    return settings_binary.parent.parent


def run_probe(
    script: str, args: list[str], cargo_target: Path | None, timeout: float = 900, env: dict[str, str] | None = None
) -> subprocess.CompletedProcess:
    """Run a probe from scripts/ the way a developer would, and return the finished process."""
    extra = env
    env = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "DISPLAY")}
    if cargo_target is not None:
        env["CARGO_TARGET_DIR"] = str(cargo_target)
    env.update(extra or {})
    return subprocess.run(
        [sys.executable, str(SCRIPTS / script), *args],
        cwd=REPO,
        env=env,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )


def assert_probe_passed(done: subprocess.CompletedProcess) -> None:
    assert done.returncode == 0, (
        f"{' '.join(done.args[1:])} exited {done.returncode}\n{done.stdout[-3000:]}\n{done.stderr[-1500:]}"
    )


@pytest.fixture(scope="session")
def backend_env(stack) -> dict[str, str]:
    """Where Settings finds `bingux-config`, the backend of its Appearance, Desktop, Search, Sound and Users pages."""
    config = stack.binary("bingux-config")
    return {"BINGUX_CONFIG_BIN": str(config)} if config else {}
