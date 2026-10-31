"""pytest wiring for the desktop tests: options, the stack, and the shared desktop session."""

from __future__ import annotations

import os
import time
from pathlib import Path

import pytest

from bxdesktop import Session, Stack, StackUnavailable
from bxdesktop.hostguard import HostSnapshot

REPO = Path(__file__).resolve().parents[2]


def pytest_addoption(parser):
    group = parser.getgroup("desktop")
    group.addoption(
        "--artifacts", default=os.environ.get("BINGUX_TEST_ARTIFACTS"), help="where logs and screenshots go"
    )
    group.addoption("--gpu", action="store_true", help="give the session /dev/dri instead of software rendering")
    group.addoption("--hardware", action="store_true", help="run tests marked needs_hardware")
    group.addoption(
        "--require-desktop",
        action="store_true",
        default=os.environ.get("BINGUX_TEST_REQUIRE") == "1",
        help="fail, instead of skip, when the isolated desktop cannot start (CI sets this)",
    )


def pytest_collection_modifyitems(config, items):
    if config.getoption("--hardware"):
        return
    skip = pytest.mark.skip(reason="needs real hardware: run with --hardware on a machine that has it")
    for item in items:
        if "needs_hardware" in item.keywords:
            item.add_marker(skip)


@pytest.hookimpl(wrapper=True)
def pytest_runtest_makereport(item, call):
    report = yield
    setattr(item, f"report_{report.when}", report)
    return report


@pytest.fixture(scope="session")
def artifact_root(request) -> Path:
    configured = request.config.getoption("--artifacts")
    base = Path(configured) if configured else REPO / "build" / "desktop-test"
    root = base / f"{time.strftime('%Y%m%d-%H%M%S')}-{os.getpid()}"
    root.mkdir(parents=True, exist_ok=True)
    return root


@pytest.fixture(scope="session")
def stack(request) -> Stack:
    try:
        return Stack.discover()
    except StackUnavailable as error:
        if request.config.getoption("--require-desktop"):
            pytest.fail(str(error), pytrace=False)
        pytest.skip(str(error))


@pytest.fixture(scope="session")
def host_before() -> HostSnapshot:
    """The host as it was before any session started (compared at the end by test_isolation)."""
    return HostSnapshot.take()


@pytest.fixture(scope="session")
def desktop_session(request, stack, artifact_root, host_before):
    """One isolated desktop for the whole run. Starting it takes a few seconds; every test shares it."""
    session = Session(stack, artifact_root / "desktop", gpu=request.config.getoption("--gpu"))
    try:
        session.start()
    except Exception as error:
        if request.config.getoption("--require-desktop"):
            raise
        pytest.skip(f"the isolated desktop did not start: {error}")
    # What the desktop looked like the moment it was ready, before any test touched it.
    session.boot = {
        "log": session.log_report(),
        "introduction": session.shell_json("introduction", "status"),
        "layers": session.layers(),
        "screenshot": session.screenshot("boot"),
    }
    yield session
    session.stop()
    assert not session.leftover, f"processes left in the session's PID namespace: {session.leftover}"


@pytest.fixture
def desktop(desktop_session: Session, request):
    """The shared desktop, idle: no client windows, shell panels closed, first-run card dismissed.

    A test that leaves error lines in the session logs fails at teardown, with the lines.
    """
    session = desktop_session
    session.reset()
    if session.shell_json("introduction", "status").get("open"):
        session.shell_ipc("introduction", "dismiss")
    cursor = session.log_cursor()
    yield session
    failed = getattr(request.node, "report_call", None) is not None and request.node.report_call.failed
    if failed:
        try:
            session.screenshot(f"FAILED-{request.node.name}")
        except Exception:  # a screenshot of a failure must never hide the failure
            pass
    session.log_report(cursor).assert_clean()
