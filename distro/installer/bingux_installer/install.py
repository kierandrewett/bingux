"""Install orchestrator: validate, preflight, commit, mount, copy, strip, configure, boot, finish.

Only one install may run at a time (InstallState). A cancel request is honoured only
until the first destructive write; begin_commit() and request_cancel() share one lock,
so the two outcomes cannot interleave.
"""

from __future__ import annotations

import threading
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from . import __version__, bootloader, configure, layout, payload, probe, storage
from .plan import Plan, validate_plan
from .protocol import PROTOCOL_VERSION, InstallerError, progress, redact
from .runner import Runner

ZONEINFO = Path("/usr/share/zoneinfo")
SUPPORTED_LOCALES = Path("/usr/share/i18n/SUPPORTED")
Emit = Callable[[dict[str, Any]], None]


def system_timezone_exists(tz: str) -> bool:
    return (ZONEINFO / tz).is_file()


def system_locale_supported(locale: str, path: Path = SUPPORTED_LOCALES) -> bool:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return False
    return any(parts and parts[0] == locale for parts in (line.split() for line in text.splitlines()))


class InstallState:
    """Install lock plus cancel/commit state. All transitions happen under one lock."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._running = False
        self._cancel = False
        self._committed = False

    @property
    def running(self) -> bool:
        with self._lock:
            return self._running

    def acquire(self) -> None:
        with self._lock:
            if self._running:
                raise InstallerError("busy", "another install is already running")
            self._running = True
            self._cancel = False
            self._committed = False

    def release(self) -> None:
        with self._lock:
            self._running = False

    def request_cancel(self) -> None:
        with self._lock:
            if not self._running:
                raise InstallerError("no_install", "no install is running")
            if self._committed:
                raise InstallerError(
                    "cancel_refused_after_commit",
                    "the disk is already being modified; cancel is refused",
                )
            self._cancel = True

    def check_cancelled(self) -> None:
        with self._lock:
            if self._cancel:
                raise InstallerError("cancelled", "install cancelled before the disk was modified")

    def begin_commit(self) -> None:
        """Mark the point of no return. Raises cancelled if a cancel already landed."""
        with self._lock:
            if self._cancel:
                raise InstallerError("cancelled", "install cancelled before the disk was modified")
            self._committed = True


@dataclass
class Env:
    runner: Runner
    state: InstallState = field(default_factory=InstallState)
    target: Path = payload.TARGET_MNT
    source: Path = payload.SOURCE_MNT
    sfs: str = payload.SOURCE_SFS
    scratch_root: Path = probe.PROBE_ROOT


def version_data() -> dict[str, Any]:
    return {"installer_version": __version__, "protocol": PROTOCOL_VERSION}


def run_probe(env: Env) -> dict[str, Any]:
    runner = env.runner
    if env.state.running:
        raise InstallerError("busy", "another installation is already running")
    firmware = probe.detect_firmware()
    doc = probe.list_block_devices(runner)
    source = probe.live_source_device(runner)
    disks = probe.probe_disks(runner, scratch_root=env.scratch_root, scan_os=True,
                              live_source=source, doc=doc)
    return {
        "firmware": firmware,
        "live_disk": probe.find_live_disk(doc, source),
        "disks": probe.disk_report(disks),
        "tools_missing": probe.missing_tools(firmware=firmware),
        "installer_version": __version__,
    }


def _guard(code: str, fn: Callable[..., Any], *args: Any, **kwargs: Any) -> Any:
    """Run one phase step; any failure becomes that phase's error code."""
    try:
        return fn(*args, **kwargs)
    except InstallerError:
        raise
    except Exception as exc:  # noqa: BLE001 - phase boundary converts every failure
        raise InstallerError(code, f"{code}: {type(exc).__name__}: {exc}") from exc


@dataclass
class _Mounts:
    target: bool = False
    source: bool = False
    opened: bool = False  # LUKS mapper open after commit


def _teardown(env: Env, mounts: _Mounts, *, strict: bool) -> None:
    runner = env.runner
    if mounts.source:
        payload.unmount_source(runner, env.source)
        mounts.source = False
    if mounts.target:
        runner.run(["sync"], check=False)
        result = runner.run(["umount", "-R", str(env.target)], check=False)
        if result.returncode != 0 and strict:
            raise InstallerError("storage_failed", f"could not unmount {env.target}")
        mounts.target = False
    if mounts.opened:
        result = runner.run(["cryptsetup", "close", layout.LUKS_NAME], check=False)
        if result.returncode != 0 and strict:
            raise InstallerError("storage_failed", "could not close the LUKS mapper")
        mounts.opened = False


def _install(raw: Any, emit: Emit, env: Env) -> dict[str, Any]:
    runner = env.runner
    mounts = _Mounts()
    secrets: tuple[str, ...] = ()

    def phase(name: str, percent: int | None = None, message: str = "") -> None:
        emit(progress(name, percent, message or name))

    try:
        phase("validate", None, "validating the plan")
        plan: Plan = validate_plan(
            raw,
            booted_firmware=probe.detect_firmware(),
            timezone_exists=system_timezone_exists,
            locale_supported=system_locale_supported,
        )
        secrets = plan.secrets()
        doc = probe.list_block_devices(runner)
        live = probe.live_source_device(runner)
        disks = probe.probe_disks(runner, scratch_root=env.scratch_root, scan_os=True,
                                  live_source=live, doc=doc)
        disk = probe.find_disk(disks, plan.disk)
        if disk.is_live_medium:
            raise InstallerError("disk_is_live_medium", f"{plan.disk} is the live medium")
        layout.check_disk_size(disk.size_bytes)
        if disk.other_os and not plan.other_os_allow:
            raise InstallerError(
                "other_os_present",
                f"{plan.disk} holds {', '.join(disk.other_os)}; set other_os to allow to erase it",
            )
        missing = probe.missing_tools(firmware=plan.firmware)
        if missing:
            raise InstallerError("missing_tool", "missing tools: " + ", ".join(missing))

        phase("preflight", None, "checking the live payload")
        env.state.check_cancelled()
        _guard("payload_failed", payload.preflight, env.sfs)
        env.state.check_cancelled()

        env.state.begin_commit()
        phase("partition", None, f"erasing and partitioning {plan.disk}")
        lay = _guard(
            "storage_failed", storage.commit_layout,
            disk=plan.disk, size_bytes=disk.size_bytes, uefi=plan.uefi,
            filesystem=plan.filesystem, encrypted=plan.encrypted,
            passphrase=plan.encryption.passphrase,
        )
        mounts.opened = plan.encrypted
        if plan.encrypted and lay.luks_path is None:
            raise InstallerError("storage_failed", "encrypted install has no LUKS partition")
        if plan.uefi and lay.esp_path is None:
            raise InstallerError("storage_failed", "UEFI install has no ESP partition")
        root_uuid = _guard("storage_failed", storage.blkid_value, runner, lay.fs_path, "UUID")
        luks_uuid = (_guard("storage_failed", storage.blkid_value, runner, lay.luks_path, "UUID")
                     if plan.encrypted else None)
        esp_uuid = (_guard("storage_failed", storage.blkid_value, runner, lay.esp_path, "UUID")
                    if plan.uefi else None)

        phase("mount", None, "mounting the target")
        _guard("payload_failed", payload.mount_target, runner, lay, env.target,
               uefi=plan.uefi, filesystem=plan.filesystem)
        mounts.target = True
        _guard("payload_failed", payload.mount_source, runner, env.sfs, env.source)
        mounts.source = True

        phase("copy", 0, "copying the live system")
        _guard("payload_failed", payload.copy_payload, runner, env.source, env.target,
               lambda pct: phase("copy", pct, f"copying the live system ({pct}%)"))
        _guard("payload_failed", payload.copy_kernel, env.target)
        payload.unmount_source(runner, env.source)
        mounts.source = False
        _guard("payload_failed", payload.mount_chroot_binds, runner, env.target)

        phase("strip", None, "removing live-only files and packages")
        _guard("config_failed", configure.strip_live_and_packages, runner, env.target)

        phase("configure", None, "configuring the system")
        fstab = layout.fstab(root_fs_uuid=root_uuid, filesystem=plan.filesystem, esp_uuid=esp_uuid)
        _guard("config_failed", configure.write_identity, env.target, plan, fstab_text=fstab)
        _guard("config_failed", configure.write_locale, runner, env.target, plan)
        _guard("config_failed", configure.create_users, runner, env.target, plan)
        _guard("config_failed", configure.write_sudoers, env.target, plan)
        _guard("config_failed", configure.write_initramfs, runner, env.target, plan)

        phase("bootloader", None, "installing the bootloader")
        cmdline = layout.cmdline(
            root_fs_uuid=root_uuid, luks_uuid=luks_uuid, filesystem=plan.filesystem,
            encrypted=plan.encrypted, kernel_args=plan.kernel_args,
        )
        if plan.uefi:
            _guard("bootloader_failed", bootloader.install_uefi, runner, env.target, cmdline)
        else:
            _guard("bootloader_failed", bootloader.install_bios, runner, env.target,
                   plan.disk, cmdline)

        phase("finish", None, "unmounting")
        _teardown(env, mounts, strict=True)
        return {"disk": plan.disk, "firmware": plan.firmware, "filesystem": plan.filesystem}
    except InstallerError as exc:
        _quiet_teardown(env, mounts)
        raise InstallerError(exc.code, redact(str(exc), secrets)) from exc
    except Exception as exc:  # noqa: BLE001 - never leak a traceback over the socket
        _quiet_teardown(env, mounts)
        raise InstallerError("internal", f"unexpected {type(exc).__name__} during install") from exc


def _quiet_teardown(env: Env, mounts: _Mounts) -> None:
    try:
        _teardown(env, mounts, strict=False)
    except Exception:  # noqa: BLE001 - cleanup must not mask the original failure
        pass


def run_install(raw: Any, *, emit: Emit, env: Env) -> dict[str, Any]:
    """Run one install. Raises InstallerError; the caller emits the terminal event."""
    env.state.acquire()
    try:
        return _install(raw, emit, env)
    finally:
        env.state.release()
