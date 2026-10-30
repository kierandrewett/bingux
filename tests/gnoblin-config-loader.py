#!/usr/bin/env python3
"""Exercise the packaged module through Gnoblin's production C config loader."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import gnoblin_loader  # noqa: E402

SETUP = r"""
local g = require("gnoblin")
g.configure {
    window_management = {edge_tiling = false},
    autostart = {
        {name = "existing", command = {"true"}},
        {name = "bingux-background", command = {"false"}},
    },
    window_rules = {{match = {app_id = "^existing$"}, opacity = 0.7}},
    keybindings = {
        keyboard = {
            switch_input_source = {binding = "<Super>space", callback = function() end},
            switch_input_source_backward = {binding = "<Shift><Super>space", callback = function() end},
            retained = {binding = "F12", callback = function() end},
        },
        wm = {switch_applications = {"<Alt>Tab"}},
    },
}
"""


def check(document, loads, disable):
    assert document["window-management"]["edge-tiling"] is True, "edge snapping stayed disabled"
    assert "window_management" not in document, "unconverted public section"
    assert [entry["name"] for entry in document["autostart"]] == [
        "existing",
        "bingux-background",
        "bingux",
        "bingux-popouts",
    ], "autostart lost entries, changed order or duplicated names"
    assert document["autostart"][1]["command"] == ["bingux-wallpaper"]
    assert document["autostart"][2]["command"] == ["bingux", "--no-color"]
    assert document["autostart"][3]["command"] == ["bingux-popouts"]
    rules = document["window-rules"]
    assert len(rules) == 1 + 9 * loads, "ordered rules were replaced or lost"
    assert rules[0]["match"]["app-id"] == "^existing$"
    for offset in range(loads):
        rule = rules[1 + 9 * offset]
        assert rule["frame"]["renderer"] == "bingux"
        corners = rule["corners"]
        assert corners["border-width"] == 1
        assert corners["border-color"] == "#505050bf"
        assert corners["remove-csd"] is True
        assert corners["keep-fullscreen"] is False
        assert corners["keep-tiled"] is True
        assert corners["shadow-animation"]["duration"] == 180
        assert "border_width" not in corners
    assert document["protocols"]["wlr-layer-shell"] is True
    assert document["frame-renderers"]["bingux"][1] == "--compact"
    lock = [entry for entry in document["shortcuts"] if entry["name"] == "bingux-lock"]
    assert len(lock) == 1, "Super+L lock shortcut missing or duplicated"
    assert lock[0]["binding"] == "<Super>l"
    assert lock[0]["command"][:2] == ["sh", "-c"] and "bsmctl lock" in lock[0]["command"][2]
    for name in ("switch_applications", "switch_applications_backward", "switch_windows", "switch_windows_backward"):
        assert document["keybindings"]["wm"][name] == [], name
    if disable:
        for name in ("switch_input_source", "switch_input_source_backward"):
            assert document["keybindings"]["keyboard"][name] == [], name
        handlers = json.dumps(document["input-handlers"])
        assert "space" not in handlers, "disabled keyboard callbacks still registered"
        assert "F12" in handlers, "unrelated callback lost"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    gnoblin_loader.add_arguments(parser)
    parser.add_argument(
        "--module", type=Path, default=Path(__file__).resolve().parents[1] / "packaging/gnoblin/bingux.lua"
    )
    parser.add_argument("--require-callback-disable", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="bingux-config-loader-") as directory:
        root = Path(directory)
        probe = gnoblin_loader.build_probe(args, root)
        conf = root / "conf.d"
        conf.mkdir()
        (conf / "10-bingux.lua").write_text(args.module.read_text())
        for loads in (1, 2):
            source = SETUP + 'g.load("conf.d/*.lua")\n' * loads
            source += """g.on("test.settings", function()
                assert(g.settings.window_management.edge_tiling == true, "active settings: edge snapping stayed disabled")
            end)\n"""
            (root / "init.lua").write_text(source)
            result = subprocess.run(
                [str(probe), str(root / "init.lua")],
                env={**os.environ, "XDG_STATE_HOME": str(root / "state"), "GNOBLIN_PREFIX": str(args.gnoblin_prefix)},
                text=True,
                capture_output=True,
            )
            assert result.returncode == 0, result.stderr
            assert "unknown section" not in result.stderr, result.stderr
            snapshots = [json.loads(line) for line in result.stdout.splitlines()]
            assert len(snapshots) == 3
            for document in snapshots:
                check(document, loads, args.require_callback_disable)
            assert snapshots[0] == snapshots[1] == snapshots[2], "reload accumulated configuration"
        print("PASS: real loader, active settings, aliases, shortcuts, ordered declarations and three reloads")


if __name__ == "__main__":
    main()
