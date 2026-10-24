"""The install matrix builds plans the installer accepts: confirm_erase only where the disk may be erased."""

from __future__ import annotations

import importlib.util
from pathlib import Path

MATRIX = Path(__file__).resolve().parents[1] / "matrix"


def _load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _run_module():
    import sys
    sys.path.insert(0, str(MATRIX))
    try:
        return _load("matrix_run", MATRIX / "run.py")
    finally:
        sys.path.remove(str(MATRIX))


def test_free_space_plan_has_no_confirm_erase() -> None:
    run = _run_module()
    plan = run.make_plan("uefi", "ext4", False, run.CASES["uefi-free-space"][3])
    assert "confirm_erase" not in plan
    assert "other_os" not in plan
    assert plan["storage"]["mode"] == "free_space"


def test_erase_plan_keeps_confirm_erase_and_other_os() -> None:
    run = _run_module()
    plan = run.make_plan("uefi", "ext4", False)
    assert plan["confirm_erase"] == run.DISK
    assert plan["other_os"] == "refuse"


def test_manual_plan_keeps_confirm_erase() -> None:
    run = _run_module()
    plan = run.make_plan("uefi", "ext4", False, run.CASES["uefi-manual"][3])
    assert plan["confirm_erase"] == run.DISK
    assert "other_os" not in plan


def test_fixture_entries_are_size_and_filesystem_pairs() -> None:
    run = _run_module()
    for name, (size, fs) in run.FIXTURES.items():
        assert name in run.CASES
        assert isinstance(size, int) and size > 0
        assert fs in ("ext4", "ntfs")
