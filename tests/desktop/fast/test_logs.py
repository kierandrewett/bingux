"""The log policy: what fails a run, what is allowed and why, what is a known defect."""

import re
from pathlib import Path

import pytest

from bxdesktop import logs

pytestmark = pytest.mark.fast


@pytest.mark.parametrize(
    "line",
    [
        "(gnoblin:18): libmutter-CRITICAL **: 00:20:46.1: meta_foo: assertion 'x' failed",
        " ERROR quickshell.something: it broke",
        "  WARN scene: file:///x/Y.qml[3:1]: ReferenceError: nope is not defined",
        "  WARN scene: file:///x/Y.qml[3:1]: TypeError: undefined is not a function",
        "Traceback (most recent call last):",
        "thread 'main' panicked at src/main.rs:3:1",
        "Segmentation fault (core dumped)",
    ],
)
def test_error_lines_fail(line):
    assert logs.classify(line, "x.log").kind == "error"


@pytest.mark.parametrize(
    "line",
    [
        "libmutter-Message: Running Mutter (using mutter 51.0) as a Wayland display server",
        "  INFO: Configuration Loaded",
        "  WARN qml: [bingux-metrics] 2",
        "> Warning:          Could not resolve keysym XF86OK",
        "",
    ],
)
def test_ordinary_lines_are_ignored(line):
    assert logs.classify(line, "x.log") is None


def test_allowed_lines_are_kept_with_their_reason():
    finding = logs.classify("(mutter-devkit:71): Gtk-WARNING **: 00:20:46.714: Failed to open display", "g.log")
    assert finding is None  # a plain warning is not a candidate at all
    finding = logs.classify(
        " ERROR quickshell.service.pipewire.loop: Failed to connect pipewire context. Errno: 112", "g"
    )
    assert finding.kind == "allowed" and "PipeWire" in finding.reason


def test_every_rule_has_a_reason_and_compiles():
    for pattern, reason in [*logs.ALLOWED, *logs.KNOWN_DEFECTS]:
        assert isinstance(pattern, re.Pattern)
        assert len(reason) > 20, f"{pattern.pattern}: say why this line is expected"


def test_a_known_defect_does_not_fail_and_a_stale_one_is_reported(tmp_path: Path, monkeypatch):
    monkeypatch.setattr(logs, "KNOWN_DEFECTS", [(re.compile("sidebarCustomise"), "tracked in the issue list: sidebar")])
    log = tmp_path / "gnoblin.log"
    log.write_text("  WARN scene: TerminalSidebar.qml[973:-1]: ReferenceError: sidebarCustomise is not defined\n")
    report = logs.scan_logs({"gnoblin.log": log})
    report.assert_clean()
    assert [f.kind for f in report.findings] == ["known"]
    assert report.stale == []
    log.write_text("nothing wrong\n")
    assert logs.scan_logs({"gnoblin.log": log}).stale == ["tracked in the issue list: sidebar"]


def test_assert_clean_names_the_lines(tmp_path: Path):
    log = tmp_path / "s.log"
    log.write_text(" ERROR a: first\nfine\n ERROR b: second\n")
    with pytest.raises(AssertionError, match=r"2 error line\(s\)[\s\S]*first[\s\S]*second"):
        logs.scan_logs({"s.log": log}).assert_clean()


def test_only_lines_after_the_cursor_are_read(tmp_path: Path):
    log = tmp_path / "s.log"
    log.write_text(" ERROR old\n")
    cursor = {"s.log": log.stat().st_size}
    with log.open("a") as handle:
        handle.write("fine\n")
    assert not logs.scan_logs({"s.log": log}, cursor).errors
    with log.open("a") as handle:
        handle.write(" ERROR new\n")
    assert [f.line for f in logs.scan_logs({"s.log": log}, cursor).errors] == [" ERROR new"]
