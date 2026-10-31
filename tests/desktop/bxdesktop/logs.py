"""What counts as an error in the logs of an isolated session.

The compositor, the shell, the helper scripts and the services all log to
files. `scan_logs` reads them and sorts every line that looks wrong into one of
three classes:

* `error`: a line that matches a FAILING pattern and no rule below. A test that
  calls `LogReport.assert_clean()` fails on it.
* `allowed`: matches a FAILING pattern and an ALLOWED rule. The rule states why
  the line is expected in this sandbox (no GPU, no PipeWire, no parent
  display). Allowed lines are counted and listed in the report, never dropped.
* `known`: matches a KNOWN_DEFECTS rule: a real defect in the code under test
  that is already understood. It does not fail the run, so the suite can be
  green while the defect is open, but every run lists it. Remove the entry
  when the defect is fixed: an entry that stops matching is reported as stale.

A rule is a regular expression and a reason. There is no blanket "ignore
warnings" and no rule without a reason; add the narrowest pattern that works.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

# A line is a candidate when it matches one of these.
FAILING = [
    re.compile(p)
    for p in (
        r"-CRITICAL\b",
        r"-ERROR\b",
        r"^\s*(FATAL|ERROR)\b",  # Quickshell's own log levels
        r"\bSIGSEGV\b|Segmentation fault|core dumped|Aborted \(core",
        r"\bassertion .* failed\b|\bAssertion `",
        r"ReferenceError|TypeError|SyntaxError|is not a function|is not defined",
        r"Unable to assign|Cannot assign|is not installed|Failed to load|Type .* unavailable",
        r"Lua error|lua:.*error|stack traceback",
        r"Traceback \(most recent call last\)",
        r"panicked at",
        r"\bWARN scene:.*(Error|failed)",
    )
]

ALLOWED = [
    (
        re.compile(r"Gtk-WARNING.*Failed to open display"),
        "mutter-devkit, Mutter's preview window, finds no parent Wayland display; the session runs without it",
    ),
    (
        re.compile(r"Devkit viewer exited; keeping the compositor session alive"),
        "GNOBLIN_DEVKIT_KEEP_SESSION=1 is how the session runs without the preview window",
    ),
    (
        re.compile(r"Failed to connect to colord daemon"),
        "no colord in the sandbox",
    ),
    (
        re.compile(r"quickshell\.service\.pipewire.*Failed to connect pipewire context"),
        "no PipeWire in the sandbox; audio is not covered (docs/desktop-testing.md)",
    ),
    (
        re.compile(r"Read of .*(extensions|switcher)\.json failed: File does not exist"),
        "a new profile has no saved extensions or switcher settings",
    ),
    (
        re.compile(r"xdg-desktop-portal.*(RealtimeKit|PipeWire|fuse|document portal)", re.I),
        "no RealtimeKit, PipeWire or FUSE in the sandbox",
    ),
]

KNOWN_DEFECTS: list[tuple[re.Pattern, str]] = [
    (
        re.compile(r"TerminalSidebar\.qml\[973:-1\]: ReferenceError: sidebarCustomise is not defined"),
        "shell/bingux/TerminalSidebar.qml: contentMenu.popupWidth reads `sidebarCustomise`, an id that no longer exists "
        "(found by the first isolated boot, 2026-10-31); the menu falls back to its 272 px minimum width",
    ),
    (
        re.compile(
            r"libmutter-CRITICAL.*meta_gnoblin_window_effects_set_window_outline: "
            r"assertion 'CLUTTER_IS_ACTOR\(parent\)' failed"
        ),
        "Gnoblin (not Bingux): closing a window with `gnoblinctl window close` logs this critical once "
        "(session/test_dock.py::test_the_icon_goes_when_the_last_window_closes, 2026-10-31)",
    ),
]


@dataclass(frozen=True)
class Finding:
    kind: str  # "error", "allowed" or "known"
    source: str
    line: str
    reason: str = ""

    def __str__(self) -> str:
        suffix = f"  [{self.kind}: {self.reason}]" if self.reason else ""
        return f"{self.source}: {self.line.strip()}{suffix}"


@dataclass
class LogReport:
    findings: list[Finding] = field(default_factory=list)
    stale: list[str] = field(default_factory=list)  # KNOWN_DEFECTS entries that matched nothing
    lines: int = 0

    @property
    def errors(self) -> list[Finding]:
        return [f for f in self.findings if f.kind == "error"]

    def of_kind(self, kind: str) -> list[Finding]:
        return [f for f in self.findings if f.kind == kind]

    def assert_clean(self) -> None:
        if self.errors:
            shown = "\n".join(f"  {finding}" for finding in self.errors[:25])
            more = f"\n  ... and {len(self.errors) - 25} more" if len(self.errors) > 25 else ""
            raise AssertionError(f"{len(self.errors)} error line(s) in the session logs:\n{shown}{more}")

    def summary(self) -> str:
        return (
            f"{self.lines} log lines: {len(self.errors)} errors, "
            f"{len(self.of_kind('allowed'))} allowed, {len(self.of_kind('known'))} known defects"
        )


def classify(line: str, source: str) -> Finding | None:
    """The finding for one line, or None when the line is not a candidate."""
    if not any(pattern.search(line) for pattern in FAILING):
        # Allowed and known rules may name lines the failing patterns miss (a plain warning).
        for pattern, reason in KNOWN_DEFECTS:
            if pattern.search(line):
                return Finding("known", source, line, reason)
        return None
    for pattern, reason in KNOWN_DEFECTS:
        if pattern.search(line):
            return Finding("known", source, line, reason)
    for pattern, reason in ALLOWED:
        if pattern.search(line):
            return Finding("allowed", source, line, reason)
    return Finding("error", source, line)


def scan_logs(sources: dict[str, Path], since: dict[str, int] | None = None) -> LogReport:
    """Classify the lines of each log. `since` maps a log name to the byte offset to start from."""
    report = LogReport()
    matched: set[str] = set()
    for name, path in sorted(sources.items()):
        try:
            with path.open("rb") as handle:
                handle.seek((since or {}).get(name, 0))
                text = handle.read().decode(errors="replace")
        except OSError:
            continue
        for line in text.splitlines():
            report.lines += 1
            finding = classify(line, name)
            if finding:
                report.findings.append(finding)
                if finding.kind == "known":
                    matched.add(finding.reason)
    if not since:  # a partial read cannot tell that an entry matched nothing
        report.stale = [reason for _, reason in KNOWN_DEFECTS if reason not in matched]
    return report
