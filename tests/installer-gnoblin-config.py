#!/usr/bin/env python3
"""Load the live installer's Gnoblin config through Gnoblin's production loader.

usage: tests/installer-gnoblin-config.py --gnoblin-source DIR --gnoblin-build DIR --gnoblin-prefix DIR

bingux-live-session starts the installer compositor with
distro/installer/gnoblin/installer.lua. If that file does not load, Gnoblin
runs its built-in defaults: a black background, an error panel, no installer
UI, and the desktop's XDG autostart programs. The c0c2c3a live image did this
on every boot ("installer/gnoblin.lua:12: attempt to index a nil value
(global 'os')"): Gnoblin's Lua has no os or io library.

The files are laid out as on the live image: the packaged greeter config, the
installer config with its greeter path pointed at that copy (the installer
packaging rewrites the path the same way for other prefixes), and a home
directory with the bingux-os skel drop-in. The checks:

- the config loads, with no ignored item;
- bingux-installer-ui is the only autostart entry. bingux-greeter is off, and
  so is bingux-statusd: the live account's user manager already runs it, and a
  second copy exits on the busy socket, which Gnoblin shows as an error panel;
- the compositor background comes from the account's drop-in, so the first
  frame is the wallpaper and not black.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import gnoblin_loader  # noqa: E402

REPO = HERE.parent
INSTALLER_LUA = REPO / "distro/installer/gnoblin/installer.lua"
GREETER_LUA = REPO / "packaging/gnoblin/greeter.lua"
BACKGROUND_DROPIN = REPO / "distro/packages/bingux-os/bingux-background.lua"
GREETER_PATH = "/usr/share/bingux/greeter/gnoblin.lua"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    gnoblin_loader.add_arguments(parser)
    args = parser.parse_args()
    installer = INSTALLER_LUA.read_text()
    if GREETER_PATH not in installer:
        print(f"FAIL: {INSTALLER_LUA.name} no longer loads {GREETER_PATH}")
        return 1
    background = re.search(r'image = "([^"]+)"', BACKGROUND_DROPIN.read_text()).group(1)
    with tempfile.TemporaryDirectory(prefix="bingux-installer-config-") as directory:
        root = Path(directory)
        probe = gnoblin_loader.build_probe(args, root)
        greeter = root / "share/bingux/greeter/gnoblin.lua"
        greeter.parent.mkdir(parents=True)
        greeter.write_text(GREETER_LUA.read_text())
        config = root / "share/bingux/installer/gnoblin.lua"
        config.parent.mkdir(parents=True)
        config.write_text(installer.replace(GREETER_PATH, str(greeter)))
        home = root / "home"
        dropin = home / ".config/gnoblin/conf.d/bingux-background.lua"
        dropin.parent.mkdir(parents=True)
        dropin.write_text(BACKGROUND_DROPIN.read_text())
        env = {name: value for name, value in os.environ.items() if not name.startswith("XDG_")}
        env.update(HOME=str(home), XDG_STATE_HOME=str(root / "state"), GNOBLIN_PREFIX=str(args.gnoblin_prefix))
        result = subprocess.run([str(probe), str(config)], env=env, text=True, capture_output=True)
    failures = []
    if result.returncode != 0:
        failures.append(f"the config does not load: {result.stderr.strip()}")
    elif "ignored" in result.stderr:
        failures.append(f"the loader ignored part of the config: {result.stderr.strip()}")
    else:
        document = json.loads(result.stdout.splitlines()[0])
        autostart = [(entry["name"], entry["command"]) for entry in document.get("autostart", [])]
        if autostart != [("bingux-installer-ui", ["/usr/bin/bingux-installer-ui"])]:
            failures.append(f"autostart is {autostart}, want only bingux-installer-ui")
        image = document.get("compositor", {}).get("background", {}).get("image")
        if image != background:
            failures.append(f"compositor background is {image!r}, want the account drop-in's {background!r}")
    if failures:
        print("installer config: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("installer config: ok (real loader, installer UI only, wallpaper drop-in)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
