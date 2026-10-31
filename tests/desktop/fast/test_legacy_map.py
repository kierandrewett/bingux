"""Every tracked script under tests/ and scripts/ has a place in tests/desktop/legacy.toml."""

import pytest

from bxdesktop import legacy

pytestmark = pytest.mark.fast


def test_the_map_parses_and_every_entry_has_a_note_and_patterns():
    for entry in legacy.load():
        assert entry["note"] and entry["patterns"], entry["area"]


def test_every_tracked_script_is_placed():
    entries = legacy.load()
    unplaced = [path for path in legacy.tracked_files() if legacy.classify(path, entries) is None]
    assert not unplaced, (
        "add these to an area in tests/desktop/legacy.toml (or write them as framework tests):\n  "
        + "\n  ".join(unplaced)
    )


def test_no_pattern_is_dead():
    entries = legacy.load()
    files = legacy.tracked_files()
    import fnmatch

    for entry in entries:
        for pattern in entry["patterns"]:
            assert any(fnmatch.fnmatch(path, pattern) for path in files), f"{entry['area']}: {pattern} matches nothing"
