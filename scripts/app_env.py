"""The environment every script runs a Bingux app in.

Not a script. Every check that starts bingux-settings, bingux-files or
bingux-taxonomy (a `--snapshot` capture, a headless sway session, a cargo test
run that opens one) builds the child's environment with `isolated()`:

  * `GSETTINGS_BACKEND=memory`: the Settings pages read and write desktop
    settings through the `gsettings` command. A sweep that presses sliders and
    switches would otherwise change the real dconf of whoever runs it (the
    mouse speed, the double-click time, the key repeat). The memory backend
    keeps every write in the process that made it.
  * no `WAYLAND_DISPLAY` and no `DISPLAY`: a capture renders in software and
    must never find the live desktop. A sway session sets its own display.

`scripts/probe_lib.py` re-exports it; `scripts/layout-lint.py`,
`scripts/frame-gate.py` and the rest import it directly. `tests/app-env.test.py`
holds the rule.
"""

from __future__ import annotations

import os
from collections.abc import Mapping

# What a child must have, and what it must not inherit from the person's session.
FORCED = {"GSETTINGS_BACKEND": "memory"}
DROPPED = ("WAYLAND_DISPLAY", "DISPLAY", "DCONF_PROFILE")


def isolated(env: Mapping[str, str] | None = None) -> dict[str, str]:
    """A copy of `env` (the process environment when None) that cannot reach the
    person's dconf or desktop."""
    out = dict(os.environ if env is None else env)
    for name in DROPPED:
        out.pop(name, None)
    out.update(FORCED)
    return out
