"""Bingux Settings, headless: layout stability, page transitions and scroll continuity.

These run the existing probes in scripts/ against `bingux-settings --snapshot` (software rendering, no
compositor, GSettings on the memory backend). They are the app-level suites of the desktop test
framework; the pages they cover are a representative few so the suite stays short. The probes take
their own full sweeps (`make layout-check` and the others in CONTRIBUTING.md).
"""

import pytest

from conftest import assert_probe_passed, run_probe

pytestmark = pytest.mark.app


@pytest.mark.slow
def test_pages_do_not_move_while_their_data_loads(settings_binary, cargo_target_dir, backend_env, tmp_path):
    done = run_probe(
        "layout-stability.py",
        [
            *("--binary", str(settings_binary)),
            *("--out", str(tmp_path / "out")),
            *("--pages", "mouse"),
            *("--sizes", "default,390x800"),
        ],
        cargo_target_dir,
        env=backend_env,
    )
    # Deselected by default (`-m slow` runs it): on a machine at load average 45 the Settings `mouse` page was
    # reported shifting at 390x800 in 3 runs of 5, and not shifting in the others, with the same binary.
    assert_probe_passed(done)
    assert "0 shifts" in done.stdout


def test_the_scroll_position_survives_a_push_and_pop(settings_binary, cargo_target_dir, backend_env, tmp_path):
    done = run_probe(
        "scroll-continuity.py",
        ["--only", "settings", "--pages", "network", "--sizes", "default", "--keep", str(tmp_path / "kept")],
        cargo_target_dir,
        env=backend_env,
    )
    assert_probe_passed(done)


@pytest.mark.slow
def test_opening_and_closing_a_subpage_slides(settings_binary, cargo_target_dir, tmp_path):
    """Deselected by default (`-m slow` runs it). Its verdict depends on how many frames a loaded machine
    draws during a 250 ms slide: on a machine at load average 45 it failed three runs in three, each time on
    a different transition ("did not slide"), against a binary older than the checkout."""
    done = run_probe(
        "nav-slide.py",
        ["--binary", str(settings_binary), "--out", str(tmp_path / "out"), "--subpages", "Wi-Fi"],
        cargo_target_dir,
    )
    assert_probe_passed(done)
