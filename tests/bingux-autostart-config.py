#!/usr/bin/env python3
"""Load Bingux's real Gnoblin integration and check ordered autostart overrides."""

import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
GNOBLIN_SOURCE = Path(os.environ.get("GNOBLIN_SOURCE", ROOT.parent / "gnoblin")).resolve()


def lua_package():
    for package in ("lua5.4", "lua-5.4", "lua54", "lua"):
        if subprocess.run(["pkg-config", "--exists", package]).returncode == 0:
            return package
    raise RuntimeError("Lua development pkg-config metadata is required")


def main():
    assert (GNOBLIN_SOURCE / "CMakeLists.txt").is_file(), GNOBLIN_SOURCE
    integration = ROOT / "packaging/gnoblin/bingux.lua"
    fixture = ROOT / "tests/bingux-autostart-config.c"
    with tempfile.TemporaryDirectory(prefix="bingux-autostart-config-") as temporary:
        temporary = Path(temporary)
        build = temporary / "gnoblin-build"
        prefix = temporary / "gnoblin-prefix"
        binary = temporary / "bingux-autostart-config"
        loaded_integration = temporary / "bingux.lua"
        integration_text = integration.read_text()
        start_marker = "    autostart = {\n"
        end_marker = "    -- Bingux owns Alt+Tab and Super+Tab in its switcher. Clear Mutter's\n"
        start = integration_text.index(start_marker)
        end = integration_text.index(end_marker, start)
        # Load the packaged declaration verbatim without unrelated integration
        # sections that need a particular Mutter keybinding catalogue.
        loaded_integration.write_text("return {\n" + integration_text[start:end] + "}\n")
        subprocess.run(
            [
                "cmake",
                "-S",
                str(GNOBLIN_SOURCE),
                "-B",
                str(build),
                "-G",
                "Ninja",
                f"-DGNOBLIN_PREFIX={prefix}",
                "-DGNOBLIN_BUILD_TYPE=debugoptimized",
            ],
            check=True,
        )
        subprocess.run(
            ["cmake", "--build", str(build), "--target", "gnoblin-config-core", "--parallel", "4"], check=True
        )
        packages = ["gio-2.0", "gio-unix-2.0", "json-glib-1.0", "xkbcommon", lua_package()]
        flags = subprocess.check_output(["pkg-config", "--cflags", "--libs", *packages], text=True).split()
        subprocess.run(
            [
                os.environ.get("CC", "cc"),
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-I",
                str(GNOBLIN_SOURCE / "src/config"),
                "-I",
                str(GNOBLIN_SOURCE / "src/config/lua"),
                "-I",
                str(GNOBLIN_SOURCE / "src/control"),
                "-I",
                str(GNOBLIN_SOURCE / "src/input"),
                "-I",
                str(GNOBLIN_SOURCE / "src/session"),
                "-I",
                str(GNOBLIN_SOURCE / "src/portal/backend"),
                str(fixture),
                str(build / "libgnoblin-config-core.a"),
                *flags,
                "-lm",
                "-o",
                str(binary),
            ],
            check=True,
        )
        subprocess.run(
            [str(binary), str(loaded_integration)],
            env={**os.environ, "GNOBLIN_PREFIX": str(build / "gnoblin-config-test-prefix")},
            check=True,
        )
    print("PASS: Bingux autostart order is wallpaper, overridden desktop, popouts")


if __name__ == "__main__":
    main()
