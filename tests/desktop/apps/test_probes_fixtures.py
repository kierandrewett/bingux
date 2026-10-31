"""The probes in scripts/ judge their own known-bad and clean fixtures correctly (no app, no compositor)."""

import pytest

from conftest import assert_probe_passed, run_probe

pytestmark = pytest.mark.fast


@pytest.mark.parametrize("script", ["nav-slide.py", "scroll-continuity.py"])
def test_probe_fixtures_are_judged_as_they_should_be(script):
    assert_probe_passed(run_probe(script, ["--fixtures"], None, timeout=300))
