import pytest

from bxdesktop.hostguard import HostProcess, HostSnapshot, compare

pytestmark = pytest.mark.fast


def snapshot(*processes: HostProcess, **variables: str) -> HostSnapshot:
    return HostSnapshot({p.pid: p for p in processes}, variables)


def test_an_unchanged_host_has_no_problems():
    before = snapshot(HostProcess(10, 500, "gnoblin"), WAYLAND_DISPLAY="wayland-0")
    assert (
        compare(
            before,
            snapshot(HostProcess(10, 500, "gnoblin"), HostProcess(11, 9, "quickshell"), WAYLAND_DISPLAY="wayland-0"),
        )
        == []
    )


def test_a_host_process_that_disappears_or_is_replaced_is_reported():
    before = snapshot(HostProcess(10, 500, "gnoblin"), HostProcess(12, 600, "quickshell"))
    after = snapshot(HostProcess(12, 777, "quickshell"))
    assert compare(before, after) == [
        "host process gnoblin (pid 10) is gone",
        "host pid 12 (quickshell) was replaced by a different process",
    ]


def test_changed_session_variables_are_reported():
    assert compare(snapshot(WAYLAND_DISPLAY="a"), snapshot(WAYLAND_DISPLAY="b"))
