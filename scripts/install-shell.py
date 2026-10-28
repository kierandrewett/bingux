#!/usr/bin/env python3
"""Install or stage the standalone shell and helpers."""

import argparse
import hashlib
import json
import re
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time


def install(source, destination, executable=False):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    # A script that names its interpreter is executable; packaging tools reject
    # a `#!` file without the bit.
    if not executable:
        with open(source, "rb") as handle:
            executable = handle.read(2) == b"#!"
    destination.chmod(0o755 if executable else 0o644)


def default_user_prefix():
    data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    return data_home / "bingux"


def validate_prefix(parser, prefix):
    if not prefix.is_absolute():
        parser.error("prefix must be absolute")
    if any(c.isspace() or c in '%"\\' for c in str(prefix)):
        parser.error("prefix must not contain whitespace, percent signs, quotes or backslashes")


def write_launcher(path, contents):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents)
    path.chmod(0o755)


def resolve_tool(name):
    """Absolute path of a desktop tool the shell calls, resolved at install time.

    Package builds run in a build root that lacks runtime tools, so a missing
    tool falls back to the distribution path the package depends on.
    """
    path = shutil.which(name)
    if path is None:
        path = f"/usr/bin/{name}"
        print(f"install-shell: {name} is not on PATH; the search config will use {path}", file=sys.stderr)
    return path


def resolve_quickshell(no_systemd):
    """Find a Quickshell binary compatible with the selected Gnoblin runtime.

    A source-installed Gnoblin can ship a matching Quickshell under its XDG
    data directory.  Prefer it to a distribution `qs`: it is built with the
    same Qt stack that Gnoblin starts for the user.  An explicit environment
    override always wins, so distributions and shell developers remain free to
    provide their own runtime.
    """
    configured = os.environ.get("BINGUX_QUICKSHELL")
    if configured:
        if shutil.which(configured):
            return configured
        if not no_systemd:
            raise ValueError(f"BINGUX_QUICKSHELL is not executable: {configured}")
        return configured
    data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    gnoblin_runtime = data_home / "gnoblin/quickshell-fedora/bin/quickshell"
    if gnoblin_runtime.is_file() and os.access(gnoblin_runtime, os.X_OK):
        return str(gnoblin_runtime)
    for candidate in ("gnoblin-quickshell", "qs", "quickshell"):
        resolved = shutil.which(candidate)
        if resolved:
            return resolved
    if not no_systemd:
        raise ValueError("Quickshell was not found; install it or set BINGUX_QUICKSHELL")
    return "qs"


def systemctl_user(arguments, check=True):
    command = ["systemctl", "--user", *arguments]
    try:
        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as error:
        if not check:
            return subprocess.CompletedProcess(command, 127, "", str(error))
        raise ValueError(f"could not run systemctl --user: {error}") from error
    if check and result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        command_text = "systemctl --user " + " ".join(arguments)
        raise ValueError(f"{command_text} failed ({result.returncode}): {detail or 'no diagnostic'}")
    return result


def release_gnome_input_source_bindings():
    """Release GNOME's bindings so Bingux can own layout switching."""
    for key in ("switch-input-source", "switch-input-source-backward"):
        try:
            subprocess.run(
                ["gsettings", "set", "org.gnome.desktop.wm.keybindings", key, "[]"],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except OSError:
            # Minimal sessions may not ship gsettings. The compositor-side
            # shortcut remains optional there, so installation can continue.
            pass


def configure_bingux_units():
    """Keep helper daemons in systemd and surfaces in Gnoblin autostart."""
    systemctl_user(
        [
            "disable",
            # Older source installs used this generic unit for every Bingux
            # surface. Keep it from starting a second shell after the Lua
            # autostart migration; the caller keeps an already-running shell
            # alive until its Gnoblin-owned replacement is ready.
            "quickshell.service",
            "bingux.service",
            "bingux-search-ui.service",
            "bingux-switcher-ui.service",
            "bingux-capture-ui.service",
            "bingux-emoji-ui.service",
        ],
        check=False,
    )
    systemctl_user(["enable", "bingux.target"])
    if systemctl_user(["is-active", "--quiet", "gnoblin-session.target"], check=False).returncode == 0:
        systemctl_user(["start", "bingux.target"])


def migrate_legacy_search_launcher(prefix):
    config_home = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    path = config_home / "bingux/search.json"
    if not path.exists():
        return
    subprocess.run(
        [
            sys.executable,
            str(prefix / "share/bingux/shell/migrate-search-config.py"),
            str(path),
            str(Path(sys.executable)),
            str(prefix / "share/bingux/shell/launch-application.py"),
            str(prefix / "share/bingux/shell/steam-games.py"),
        ],
        check=True,
    )


def build_payload(source, build, prefix, qml, target, quickshell="qs", managed=False, cargo=None, ladspa=None):
    """Install payload files and return their paths relative to prefix.

    `cargo` is the Cargo target directory that holds `release/` (default
    BUILD/cargo) and `ladspa` is where the LADSPA plugin library goes (default
    PREFIX/lib/ladspa; Fedora packages use lib64).
    """
    cargo = cargo or build / "cargo"
    ladspa = ladspa or prefix / "lib/ladspa"
    payload = {
        build / "effects/libbinguxeffects.so": qml / "Bingux/Effects/libbinguxeffects.so",
        source / "packages/bingux-effects/qmldir": qml / "Bingux/Effects/qmldir",
        build / "wayland-sync/libbinguxwaylandsync.so": qml / "Bingux/Wayland/libbinguxwaylandsync.so",
        source / "packages/bingux-wayland-sync/qmldir": qml / "Bingux/Wayland/qmldir",
        build / "text/libbinguxtext.so": qml / "Bingux/Text/libbinguxtext.so",
        source / "packages/bingux-text-layout/qmldir": qml / "Bingux/Text/qmldir",
        build / "bingux-audio-meter": prefix / "bin/bingux-audio-meter",
        build / "bingux-image-clipboard": prefix / "libexec/bingux/bingux-image-clipboard",
        build / "bingux-frame": prefix / "libexec/bingux/bingux-frame",
        cargo / "release/bingux-config": prefix / "bin/bingux-config",
        cargo / "release/bingux-searchd": prefix / "bin/bingux-searchd",
        cargo / "release/bingux-statusd": prefix / "bin/bingux-statusd",
        cargo / "release/bingux-settings": prefix / "bin/bingux-settings",
        # Sound effects: the engine Settings starts, and the LADSPA library
        # (bingux_dsp) with the compressor, limiter, reverb and the rest.
        cargo / "release/bingux-effects": prefix / "bin/bingux-effects",
        cargo / "release/libbingux_dsp.so": ladspa / "libbingux_dsp.so",
        source / "packages/bingux-settings/bingux-settings.desktop": prefix
        / "share/applications/bingux-settings.desktop",
        source / "packages/bingux-settings/bingux-settings.svg": prefix
        / "share/icons/hicolor/scalable/apps/bingux-settings.svg",
        cargo / "release/bingux-files": prefix / "bin/bingux-files",
        source / "packages/bingux-files-app/bingux-files.desktop": prefix
        / "share/applications/bingux-files.desktop",
        source / "packages/bingux-files-app/bingux-files.svg": prefix
        / "share/icons/hicolor/scalable/apps/bingux-files.svg",
        cargo / "release/bingux-taxonomy": prefix / "bin/bingux-taxonomy",
        source / "packages/bingux-taxonomy/bingux-taxonomy.desktop": prefix
        / "share/applications/bingux-taxonomy.desktop",
        source / "packages/bingux-taxonomy/bingux-taxonomy.svg": prefix
        / "share/icons/hicolor/scalable/apps/bingux-taxonomy.svg",
        source / "scripts/bingux-clipboard-paste": prefix / "bin/bingux-clipboard-paste",
        # Tells bingux-searchd that Settings exposes search: its program (opens a
        # hit's address) and its id. See design/search.md section 8.
        source / "packaging/search-apps/bingux-settings.json": prefix
        / "share/bingux/search-apps/bingux-settings.json",
        source / "packaging/gnoblin/bingux.lua": prefix / "share/gnoblin/conf.d/bingux.lua",
        # Enables bingux.target for every user; see packaging/systemd/90-bingux.preset.
        source / "packaging/systemd/90-bingux.preset": prefix / "lib/systemd/user-preset/90-bingux.preset",
    }
    for path in sorted((source / "packaging/metainfo").glob("*.metainfo.xml")):
        payload[path] = prefix / "share/metainfo" / path.name
    for path in sorted((source / "packaging/man").glob("*.1")):
        payload[path] = prefix / "share/man/man1" / path.name
    missing = [str(path) for path in payload if not path.is_file()]
    if missing:
        raise ValueError("run make first; missing: " + ", ".join(missing))

    installed = []

    def copy(source_path, destination, executable=False):
        install(source_path, target(destination), executable)
        try:
            installed.append(destination.relative_to(prefix))
        except ValueError:
            # QMLDIR is allowed to be outside PREFIX for package builders.
            installed.append(destination)

    adwaita_source = source / "subprojects/adwaita-icon-theme"
    adwaita_assets = adwaita_source / "Adwaita"
    adwaita_index = adwaita_source / "index.theme"
    if not adwaita_index.is_file() or not (adwaita_assets / "symbolic").is_dir():
        raise ValueError(
            "Adwaita icon theme submodule is missing; run git submodule update --init subprojects/adwaita-icon-theme"
        )
    # Private to Bingux (AdwaitaIcons.qml reads BINGUX_ADWAITA_ICON_DIR), so it
    # never collides with the distribution's adwaita-icon-theme under icons/.
    adwaita_install = prefix / "share/bingux/icons/Adwaita"
    copy(adwaita_index, adwaita_install / "index.theme")
    for path in adwaita_assets.rglob("*"):
        if path.is_file():
            copy(path, adwaita_install / path.relative_to(adwaita_assets))
    for license_name in ("COPYING", "COPYING_CCBYSA3", "COPYING_LGPL"):
        copy(
            adwaita_source / license_name,
            prefix / "share/licenses/bingux/adwaita-icon-theme" / license_name,
        )
    folder_open = adwaita_install / "scalable/status/folder-open.svg"
    if not folder_open.is_file():
        copy(
            adwaita_assets / "scalable/places/folder.svg",
            folder_open,
        )

    shell = prefix / "share/bingux/shell"
    for path in (source / "shell/bingux").rglob("*"):
        if not path.is_file() or "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        copy(path, shell / path.relative_to(source / "shell/bingux"))
    gnoblin_integration = prefix / "share/bingux/gnoblin"
    for path in (source / "shell/gnoblin").rglob("*"):
        if not path.is_file() or "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        copy(path, gnoblin_integration / path.relative_to(source / "shell/gnoblin"))
    for path, destination in payload.items():
        executable = (
            destination.parent == prefix / "bin"
            or destination.suffix == ".so"
            or destination
            in (
                prefix / "libexec/bingux/bingux-image-clipboard",
                prefix / "libexec/bingux/bingux-frame",
            )
        )
        if path == source / "packaging/search-apps/bingux-settings.json":
            destination_path = target(destination)
            destination_path.parent.mkdir(parents=True, exist_ok=True)
            destination_path.write_text(path.read_text().replace("@BINDIR@", str(prefix / "bin")))
            installed.append(destination.relative_to(prefix))
        elif path == source / "packaging/gnoblin/bingux.lua":
            # GDM does not necessarily include the user's command-link folder
            # in PATH. Use this install's wrappers and frame command directly.
            generated = path.read_text()
            for command in (
                "bingux",
                "bingux-search-ui",
                "bingux-switcher-ui",
                "bingux-capture-ui",
                "bingux-emoji-ui",
                "bingux-popouts",
                "bingux-wallpaper",
            ):
                # Match the formatter's spacing: `{"name"` and `{ "name"` both occur.
                generated = re.sub(
                    r"command = \{(\s*)" + re.escape(json.dumps(command)),
                    lambda match, command=command: (
                        "command = {" + match.group(1) + json.dumps(str(prefix / "bin" / command))
                    ),
                    generated,
                )
            generated = generated.replace(
                '"/usr/local/libexec/bingux/bingux-frame"',
                json.dumps(str(prefix / "libexec/bingux/bingux-frame")),
            )
            destination_path = target(destination)
            destination_path.parent.mkdir(parents=True, exist_ok=True)
            destination_path.write_text(generated)
            installed.append(destination.relative_to(prefix))
        else:
            copy(path, destination, executable)
    entry_path = prefix / "share/gnoblin/scripts/bingux-text-input.js"
    entry = target(entry_path)
    entry.parent.mkdir(parents=True, exist_ok=True)
    entry.write_text(
        "const integration = "
        + json.dumps((gnoblin_integration / "bingux-text-input.js").as_uri())
        + ";\n"
        + "export default async function (api) {\n"
        + '    const offset = import.meta.url.indexOf("?");\n'
        + '    const suffix = offset < 0 ? "" : import.meta.url.slice(offset);\n'
        + "    const script = await import(integration + suffix);\n"
        + "    return script.default(api);\n"
        + "}\n"
    )
    installed.append(entry_path.relative_to(prefix))
    copy(source / "packages/binguxctl/binguxctl.py", prefix / "libexec/bingux/binguxctl.py")
    if managed:
        copy(source / "scripts/install-shell.py", prefix / "libexec/bingux/install-shell.py", True)

    common = (
        "#!/bin/sh\nset -eu\nulimit -c 0\n# Optional: BINGUX_SOFTWARE_RENDERING=1|0 selects the shell rendering mode.\n"
    )
    common += "export BINGUX_QUICKSHELL=${BINGUX_QUICKSHELL:-" + shlex.quote(quickshell) + "}\n"
    common += "export QML_IMPORT_PATH=" + shlex.quote(str(qml)) + "${QML_IMPORT_PATH:+:$QML_IMPORT_PATH}\n"
    common += (
        "export XDG_DATA_DIRS="
        + shlex.quote(str(prefix / "share"))
        + ":${XDG_DATA_DIRS:-/usr/local/share:/usr/share}\n"
    )
    common += "export BINGUX_ADWAITA_ICON_DIR=" + shlex.quote(str(prefix / "share/bingux/icons/Adwaita")) + "\n"
    common += "export BINGUX_CONFIG_PATH=" + shlex.quote(str(shell)) + "\n"
    common += "export BINGUX_AUDIO_METER=" + shlex.quote(str(prefix / "bin/bingux-audio-meter")) + "\n"
    # Helpers run with the interpreter that installed Bingux, and the shell
    # finds its own binaries even when ~/.local/bin is not on PATH.
    common += "export BINGUX_PYTHON=${BINGUX_PYTHON:-" + shlex.quote(str(Path(sys.executable))) + "}\n"
    common += "export BINGUX_CONFIG_BIN=${BINGUX_CONFIG_BIN:-" + shlex.quote(str(prefix / "bin/bingux-config")) + "}\n"
    common += (
        "export BINGUX_SETTINGS_BIN=${BINGUX_SETTINGS_BIN:-" + shlex.quote(str(prefix / "bin/bingux-settings")) + "}\n"
    )
    common += "export PATH=" + shlex.quote(str(prefix / "bin")) + ':"$PATH"\n'
    wrappers = {
        "bingux": 'exec "$BINGUX_QUICKSHELL" -p "$BINGUX_CONFIG_PATH" "$@"\n',
        "bingux-search-ui": 'exec "$BINGUX_QUICKSHELL" -p ' + shlex.quote(str(shell / "SearchShell.qml")) + ' "$@"\n',
        "bingux-switcher-ui": 'exec "$BINGUX_QUICKSHELL" -p '
        + shlex.quote(str(shell / "SwitcherShell.qml"))
        + ' "$@"\n',
        "bingux-emoji-ui": 'exec "$BINGUX_QUICKSHELL" -p ' + shlex.quote(str(shell / "EmojiShell.qml")) + ' "$@"\n',
        "bingux-capture-ui": 'exec "$BINGUX_QUICKSHELL" -p ' + shlex.quote(str(shell / "CaptureShell.qml")) + ' "$@"\n',
        "bingux-popouts": 'exec "$BINGUX_QUICKSHELL" -p ' + shlex.quote(str(shell / "PopoutsShell.qml")) + ' "$@"\n',
        "bingux-wallpaper": 'exec "$BINGUX_QUICKSHELL" -p '
        + shlex.quote(str(shell / "WallpaperShell.qml"))
        + ' "$@"\n',
        "bingux-lock": 'exec "$BINGUX_QUICKSHELL" -p ' + shlex.quote(str(shell / "LockShell.qml")) + ' "$@"\n',
        "binguxctl": 'exec "$BINGUX_PYTHON" ' + shlex.quote(str(prefix / "libexec/bingux/binguxctl.py")) + ' "$@"\n',
        "bingux-frame": "exec " + shlex.quote(str(prefix / "libexec/bingux/bingux-frame")) + ' "$@"\n',
    }
    if managed:
        wrappers["bingux-uninstall"] = (
            "exec "
            + shlex.quote(sys.executable)
            + " "
            + shlex.quote(str(prefix / "libexec/bingux/install-shell.py"))
            + " --user --uninstall --prefix "
            + shlex.quote(str(prefix))
            + "\n"
        )
    for name, command in wrappers.items():
        copy_text = target(prefix / "bin" / name)
        write_launcher(copy_text, common + command)
        installed.append(Path("bin") / name)

    config = target(prefix / "share/bingux/search.json")
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text(
        json.dumps(
            {
                "protocolVersion": 1,
                "commands": {
                    # The daemon validates the executable path before it starts.
                    # Use the interpreter that ran this installer so the command
                    # remains valid when the user does not have python3 on PATH.
                    "applicationLauncher": [str(Path(sys.executable)), str(shell / "launch-application.py")],
                    "steamGameCatalog": [str(Path(sys.executable)), str(shell / "steam-games.py"), "--search"],
                    "fileOpener": [resolve_tool("xdg-open")],
                    "clipboard": [resolve_tool("wl-copy")],
                },
            },
            indent=2,
        )
        + "\n"
    )
    installed.append(Path("share/bingux/search.json"))

    search_service = prefix / "libexec/bingux/search-service"
    search_migrator = prefix / "share/bingux/shell/migrate-search-config.py"
    application_launcher = prefix / "share/bingux/shell/launch-application.py"
    steam_game_catalog = prefix / "share/bingux/shell/steam-games.py"
    write_launcher(
        target(search_service),
        "#!/bin/sh\nset -eu\n"
        + 'config="${XDG_CONFIG_HOME:-${HOME:?}/.config}/bingux/search.json"\n'
        + '[ -f "$config" ] || config='
        + shlex.quote(str(prefix / "share/bingux/search.json"))
        + "\n"
        + shlex.quote(str(Path(sys.executable)))
        + " "
        + shlex.quote(str(search_migrator))
        + ' "$config" '
        + shlex.quote(str(Path(sys.executable)))
        + " "
        + shlex.quote(str(application_launcher))
        + " "
        + shlex.quote(str(steam_game_catalog))
        + "\n"
        + "exec "
        + shlex.quote(str(prefix / "bin/bingux-searchd"))
        + ' --config "$config"\n',
    )
    search_command = target(prefix / "bin/bingux-search-service")
    write_launcher(
        search_command,
        "#!/bin/sh\nset -eu\nexec " + shlex.quote(str(search_service)) + ' "$@"\n',
    )
    installed.append(Path("libexec/bingux/search-service"))
    installed.append(Path("bin/bingux-search-service"))
    unit_sources = sorted((source / "packaging/systemd").glob("bingux*.service"))
    unit_sources.append(source / "packaging/systemd/bingux.target")
    for unit_source in unit_sources:
        unit = unit_source.read_text()
        unit = unit.replace("/usr/bin", str(prefix / "bin"))
        unit = unit.replace("/usr/libexec/bingux", str(prefix / "libexec/bingux"))
        unit = unit.replace("/usr/lib64/bingux/qml", str(qml))
        unit_path = target(prefix / "lib/systemd/user" / unit_source.name)
        unit_path.parent.mkdir(parents=True, exist_ok=True)
        unit_path.write_text(unit)
        installed.append(Path("lib/systemd/user") / unit_source.name)
    return installed


def integration_paths(prefix, managed=False):
    names = (
        "bingux",
        "bingux-search-service",
        "bingux-search-ui",
        "bingux-switcher-ui",
        "bingux-emoji-ui",
        "bingux-capture-ui",
        "bingux-popouts",
        "bingux-wallpaper",
        "bingux-lock",
        "binguxctl",
        "bingux-frame",
        "bingux-clipboard-paste",
    )
    if managed:
        names += ("bingux-uninstall",)
    units = (
        "bingux.target",
        "bingux.service",
        "bingux-searchd.service",
        "bingux-statusd.service",
        "bingux-search-ui.service",
        "bingux-switcher-ui.service",
        "bingux-capture-ui.service",
        "bingux-emoji-ui.service",
        "bingux-lock.service",
    )
    bin_dir = Path(os.environ.get("XDG_BIN_HOME") or Path.home() / ".local/bin")
    config_dir = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    data_dir = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    unit_dir = config_dir / "systemd/user"
    links = (
        [(bin_dir / name, prefix / "bin" / name) for name in names]
        + [(unit_dir / name, prefix / "lib/systemd/user" / name) for name in units]
        + [(config_dir / "gnoblin/conf.d/bingux.lua", prefix / "share/gnoblin/conf.d/bingux.lua")]
        + [(data_dir / "gnoblin/scripts/bingux-text-input.js", prefix / "share/gnoblin/scripts/bingux-text-input.js")]
    )
    return links


def ensure_integration_is_safe(paths):
    for link, target_path in paths:
        if link.exists() and (not link.is_symlink() or link.resolve() != target_path.resolve()):
            raise ValueError(f"refusing to replace existing file: {link}")


def migrate_legacy_bingux_paths(paths):
    """Back up only the obsolete pre-managed-install Bingux files.

    Earlier source installs wrote regular unit files which were tied to the
    removed org.gnoblin.Shell.target. The managed installer now owns symlinks
    for these same names. Preserve those exact legacy files beside the unit
    before checking whether the managed links are safe to create.
    """
    marker = "org.gnoblin.Shell.target"
    timestamp = time.strftime("%Y%m%dT%H%M%S", time.gmtime())
    for link, target_path in paths:
        if not link.name.startswith("bingux") or link.is_symlink() or not link.exists():
            continue
        if not link.is_file():
            continue
        if link.parent.name == "user":
            try:
                contents = link.read_text()
            except UnicodeDecodeError:
                continue
            if marker not in contents:
                continue
        elif link.parent.name != "bin":
            continue
        backup = link.with_name(link.name + f".legacy-{timestamp}.bak")
        if backup.exists():
            raise ValueError(f"legacy Bingux unit backup already exists: {backup}")
        link.rename(backup)
        print(f"Backed up legacy Bingux path: {backup}")


def migrate_legacy_quickshell_unit():
    """Retire the old all-in-one Bingux shell service without losing it.

    A disabled unit can still be pulled in by an old session target, which
    leaves two complete Bingux shells mapped at once. Only migrate the exact
    historical Bingux unit shape; an unrelated user Quickshell service stays
    untouched.
    """
    config_home = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    unit = config_home / "systemd/user/quickshell.service"
    if unit.is_symlink() or not unit.is_file():
        return
    try:
        contents = unit.read_text()
    except UnicodeDecodeError:
        return
    markers = (
        "Description=Bingux Quickshell desktop",
        "PartOf=org.gnoblin.Shell.target",
        "--path ",
        "/bingux",
    )
    if not all(marker in contents for marker in markers):
        return
    timestamp = time.strftime("%Y%m%dT%H%M%S", time.gmtime())
    backup = unit.with_name(unit.name + f".legacy-{timestamp}.bak")
    if backup.exists():
        raise ValueError(f"legacy Quickshell unit backup already exists: {backup}")
    unit.rename(backup)
    print(f"Backed up legacy Quickshell unit: {backup}")


def ensure_gnoblin_dropin_loader():
    """Create or extend init.lua so the managed drop-in is loaded last."""
    config_dir = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    init = config_dir / "gnoblin/init.lua"
    include = 'gnoblin.load("conf.d/**/*.lua")'
    if init.exists():
        if not init.is_file():
            raise ValueError(f"refusing to replace existing Gnoblin config: {init}")
        contents = init.read_text()
        # Repair the exact loader fragment appended by older installers. It
        # assumed a local `g` variable that many valid configs did not define.
        legacy = '\n-- Load installed and personal Gnoblin drop-ins.\ng.load("conf.d/**/*.lua")\n'
        if legacy in contents:
            contents = contents.replace(legacy, "\n")
        # This loader is installer-owned. Always place it after the user's
        # base rules: Bingux supplies the final frame and layer-shell rules.
        # Removing only this exact call leaves other explicit module ordering
        # untouched.
        contents = "\n".join(line for line in contents.split("\n") if line.strip() != include).rstrip()
        suffix = "" if contents.endswith("\n") else "\n"
        init.write_text(contents + suffix + "\n-- Load installed and personal Gnoblin drop-ins.\n" + include + "\n")
        return
    init.parent.mkdir(parents=True, exist_ok=True)
    init.write_text(
        "-- Generated by Bingux; keep machine-specific settings in conf.d/.\n"
        'local g = require("gnoblin")\n' + include + "\n"
    )


def install_user(source, build, prefix, qml, no_systemd, cargo=None, ladspa=None):
    manifest_path = prefix / ".bingux-install.json"
    if prefix.is_symlink():
        raise ValueError(f"refusing to install through symlink: {prefix}")
    preserved_entries = {"extensions"}
    if prefix.exists() and not manifest_path.is_file():
        unmanaged_entries = [entry.name for entry in prefix.iterdir() if entry.name not in preserved_entries]
        if unmanaged_entries:
            raise ValueError(f"refusing to use existing unmanaged directory: {prefix}")
    prefix.parent.mkdir(parents=True, exist_ok=True)
    links = integration_paths(prefix, managed=True)
    migrate_legacy_bingux_paths(links)
    migrate_legacy_quickshell_unit()
    ensure_integration_is_safe(links)
    quickshell = resolve_quickshell(no_systemd)
    staging = Path(tempfile.mkdtemp(prefix=f".{prefix.name}.install-", dir=prefix.parent))
    backup = None
    committed = False
    try:
        installed = build_payload(
            source,
            build,
            prefix,
            qml,
            lambda path: staging / path.relative_to(prefix),
            quickshell,
            managed=True,
            cargo=cargo,
            ladspa=ladspa,
        )
        manifest = {
            "format": 1,
            "prefix": str(prefix),
            "files": [str(path) for path in installed],
            "links": [{"path": str(link), "target": str(target_path)} for link, target_path in links],
        }
        (staging / ".bingux-install.json").write_text(json.dumps(manifest, indent=2) + "\n")
        if prefix.exists():
            backup = Path(tempfile.mkdtemp(prefix=f".{prefix.name}.old-", dir=prefix.parent))
            backup.rmdir()
            prefix.rename(backup)
        staging.rename(prefix)
        staging = None
        for link, target_path in links:
            link.parent.mkdir(parents=True, exist_ok=True)
            if link.is_symlink():
                link.unlink()
            link.symlink_to(target_path)
        ensure_gnoblin_dropin_loader()
        release_gnome_input_source_bindings()
        migrate_legacy_search_launcher(prefix)
        if not no_systemd:
            systemctl_user(["daemon-reload"])
            # The target must be visible through its freshly-created unit link
            # before systemd can enable it.
            configure_bingux_units()
        committed = True
    except Exception:
        if staging is not None and staging.exists():
            shutil.rmtree(staging)
        if not committed:
            if prefix.exists():
                shutil.rmtree(prefix)
            if backup is not None and backup.exists():
                backup.rename(prefix)
        elif not no_systemd:
            configure_bingux_units()
        raise
    finally:
        if backup is not None and backup.exists():
            shutil.rmtree(backup)
    print(f"Installed Bingux under {prefix}")
    print("  command links: " + str(Path(os.environ.get("XDG_BIN_HOME") or Path.home() / ".local/bin")))
    if no_systemd:
        print("  helper services: systemd setup skipped (--no-systemd)")
    else:
        print("  helper services: enabled through the systemd user target")
    print("  shell surfaces: launched by Gnoblin Lua autostart")
    print("  Gnoblin integration: config drop-in and compositor script installed under XDG data/config directories")
    print("  remove with: make uninstall-user USER_PREFIX=" + str(prefix))


def uninstall_user(prefix, no_systemd):
    manifest_path = prefix / ".bingux-install.json"
    if not manifest_path.is_file():
        raise ValueError(f"{prefix} is not a managed Bingux install (missing .bingux-install.json)")
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("format") != 1 or Path(manifest.get("prefix", "")).resolve() != prefix.resolve():
        raise ValueError(f"invalid Bingux install manifest: {manifest_path}")
    if not no_systemd:
        systemctl_user(["disable", "--now", "bingux.target"], check=False)
        systemctl_user(["daemon-reload"], check=False)
    target_path = prefix / "lib/systemd/user/bingux.target"
    unit_dirs = {
        Path(link["path"]).parent for link in manifest.get("links", []) if Path(link["path"]).name == "bingux.target"
    }
    for unit_dir in unit_dirs:
        for enabled in unit_dir.glob("*.wants/bingux.target"):
            if enabled.is_symlink() and enabled.resolve() == target_path.resolve():
                enabled.unlink()
    for link in manifest.get("links", []):
        path = Path(link["path"])
        target_path = Path(link["target"])
        if path.is_symlink() and path.resolve() == target_path.resolve():
            path.unlink()
    shutil.rmtree(prefix)
    print(f"Removed Bingux install: {prefix}")
    print("  user configuration was kept")


# The applications-only install: Settings, Files and Taxonomy with what Settings
# needs (the configuration command and the sound effects engine), put below one
# prefix such as ~/.local. It never installs the shell: no Quickshell files, no
# Gnoblin module, no session or user-service changes. A manifest records every
# file, its checksum, the file it displaced and the folders it created, so
# --uninstall removes exactly that and puts the displaced files back.
APPS_PROGRAMS = ("bingux-config", "bingux-settings", "bingux-files", "bingux-taxonomy", "bingux-effects")
APPS_MANIFEST = Path("share/bingux-apps/manifest.json")
APPS_HELPER = Path("share/bingux-apps/install-shell.py")
APPS_BACKUP_SUFFIX = ".pre-bingux-apps.bak"
# Written by the cache tools into folders the install may have created.
APPS_CACHE_FILES = ("mimeinfo.cache", "icon-theme.cache")


def default_apps_prefix():
    return Path.home() / ".local"


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def apps_payload(source, cargo, prefix):
    """Return (destination, content or source path, mode, strip) for every installed file."""
    bin_dir = str(prefix / "bin")
    entries = []
    for name in APPS_PROGRAMS:
        entries.append((prefix / "bin" / name, cargo / "release" / name, 0o755, True))
    entries.append((prefix / "lib/ladspa/libbingux_dsp.so", cargo / "release/libbingux_dsp.so", 0o755, True))
    for package, name in (
        ("bingux-settings", "bingux-settings"),
        ("bingux-files-app", "bingux-files"),
        ("bingux-taxonomy", "bingux-taxonomy"),
    ):
        directory = source / "packages" / package
        # Exec names the program by its bare name; point it at this install, so
        # the entry works when the launcher's PATH lacks the prefix.
        desktop = re.sub(
            r"^(Exec=)(bingux-[a-z]+)(?=[ \n]|$)",
            lambda match: match.group(1) + bin_dir + "/" + match.group(2),
            (directory / (name + ".desktop")).read_text(),
            flags=re.MULTILINE,
        )
        entries.append((prefix / "share/applications" / (name + ".desktop"), desktop.encode(), 0o644, False))
        entries.append(
            (prefix / "share/icons/hicolor/scalable/apps" / (name + ".svg"), directory / (name + ".svg"), 0o644, False)
        )
    # No AppStream metainfo: appstreamcli and software centres index only the
    # system metainfo folders, so a copy below ~/.local would never be read.
    for path in sorted((source / "packaging/man").glob("*.1")):
        entries.append((prefix / "share/man/man1" / path.name, path, 0o644, False))
    # bingux-searchd reads this manifest to know Settings exposes search.
    search_app = (source / "packaging/search-apps/bingux-settings.json").read_text().replace("@BINDIR@", bin_dir)
    entries.append((prefix / "share/bingux/search-apps/bingux-settings.json", search_app.encode(), 0o644, False))
    # Settings turns the sound effects on by enabling this template unit; the
    # user manager reads $XDG_DATA_HOME/systemd/user. Nothing is enabled here.
    unit = (source / "packaging/systemd/bingux-effects@.service").read_text().replace("/usr/bin", bin_dir)
    entries.append((prefix / "share/systemd/user/bingux-effects@.service", unit.encode(), 0o644, False))
    # The way out does not need the source checkout.
    entries.append((prefix / APPS_HELPER, Path(__file__).resolve(), 0o755, False))
    uninstall = (
        "#!/bin/sh\nset -eu\nexec "
        + shlex.quote(sys.executable)
        + " "
        + shlex.quote(str(prefix / APPS_HELPER))
        + " --apps --uninstall --prefix "
        + shlex.quote(str(prefix))
        + ' "$@"\n'
    )
    entries.append((prefix / "bin/bingux-apps-uninstall", uninstall.encode(), 0o755, False))
    missing = [str(entry[1]) for entry in entries if isinstance(entry[1], Path) and not entry[1].is_file()]
    if missing:
        raise ValueError("build the programs first (make apps); missing: " + ", ".join(missing))
    return entries


def load_apps_manifest(prefix):
    path = prefix / APPS_MANIFEST
    if not path.is_file():
        return None
    manifest = json.loads(path.read_text())
    if manifest.get("format") != 1 or manifest.get("kind") != "bingux-apps":
        raise ValueError(f"invalid Bingux apps manifest: {path}")
    if Path(manifest.get("prefix", "")).resolve() != prefix.resolve():
        raise ValueError(f"manifest {path} belongs to {manifest.get('prefix')}, not {prefix}")
    return manifest


def refresh_desktop_caches(prefix):
    """Refresh the user-level desktop, icon and MIME caches below prefix/share.

    A cache that does not exist is not created, except the desktop database,
    whose mimeinfo.cache is what makes `gio mime` list the new handlers.
    """
    share = prefix / "share"
    steps = []
    if (share / "applications").is_dir():
        steps.append(["update-desktop-database", "-q", str(share / "applications")])
    icons = share / "icons/hicolor"
    if (icons / "icon-theme.cache").is_file():
        # -t: the user's hicolor folder has no index.theme of its own.
        steps.append(["gtk-update-icon-cache", "-q", "-f", "-t", str(icons)])
    if (share / "mime").is_dir():
        steps.append(["update-mime-database", str(share / "mime")])
    for command in steps:
        if shutil.which(command[0]) is None:
            print(f"  cache: {command[0]} is not installed; skipped", file=sys.stderr)
            continue
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode != 0:
            print(f"  cache: {' '.join(command)} failed: {result.stderr.strip()}", file=sys.stderr)
        else:
            print("  cache: " + " ".join(command))


def install_apps(source, prefix, cargo, strip=True, update_caches=True):
    previous = load_apps_manifest(prefix)
    owned = {prefix / entry["path"] for entry in previous["files"]} if previous else set()
    # A file displaced by an earlier run stays displaced by it.
    former_backups = {entry["path"]: entry["backup"] for entry in previous["files"]} if previous else {}
    plan = apps_payload(source, cargo, prefix)
    strip_tool = shutil.which("strip") if strip else None
    if strip and strip_tool is None:
        raise ValueError("strip is not installed (binutils); pass --no-strip to install the programs as built")
    backups = {}
    for destination, _content, _mode, _strip in plan:
        if destination in owned or not os.path.lexists(destination):
            continue
        backup = destination.with_name(destination.name + APPS_BACKUP_SUFFIX)
        if os.path.lexists(backup):
            raise ValueError(f"{destination} exists and so does its backup {backup}; move one away first")
        backups[destination] = backup
    created_dirs = [Path(directory) for directory in previous["dirs"]] if previous else []
    written = []
    moved = []
    entries = []
    try:
        for destination, backup in backups.items():
            os.rename(destination, backup)
            moved.append((destination, backup))
            print(f"  kept the existing {destination} as {backup.name}")
        for destination, content, mode, strippable in plan:
            missing = []
            parent = destination.parent
            while not parent.exists():
                missing.append(parent)
                parent = parent.parent
            for directory in reversed(missing):
                directory.mkdir()
                created_dirs.append(directory)
            temporary = destination.with_name(f".{destination.name}.tmp-{os.getpid()}")
            if isinstance(content, Path):
                shutil.copyfile(content, temporary)
            else:
                temporary.write_bytes(content)
            temporary.chmod(mode)
            if strippable and strip_tool:
                subprocess.run([strip_tool, "--strip-unneeded", str(temporary)], check=True)
            if destination not in owned:
                written.append(destination)
            os.replace(temporary, destination)
            relative = str(destination.relative_to(prefix))
            entries.append(
                {
                    "path": relative,
                    "sha256": sha256_file(destination),
                    "backup": (
                        str(backups[destination].relative_to(prefix))
                        if destination in backups
                        else former_backups.get(relative)
                    ),
                }
            )
        # Files a former install owned that this one no longer ships.
        shipped = {destination for destination, *_ in plan}
        for entry in previous["files"] if previous else []:
            path = prefix / entry["path"]
            if path not in shipped and path.is_file() and sha256_file(path) == entry["sha256"]:
                path.unlink()
        manifest = {
            "format": 1,
            "kind": "bingux-apps",
            "prefix": str(prefix),
            "files": entries,
            "dirs": [str(directory) for directory in created_dirs],
        }
        (prefix / APPS_MANIFEST).write_text(json.dumps(manifest, indent=2) + "\n")
    except BaseException:
        for path in written:
            path.unlink(missing_ok=True)
        for destination, backup in reversed(moved):
            os.rename(backup, destination)
        for directory in reversed(created_dirs):
            try:
                directory.rmdir()
            except OSError:
                pass
        raise
    print(f"Installed the Bingux apps under {prefix}")
    for name in ("bingux-settings", "bingux-files", "bingux-taxonomy"):
        print(f"  {prefix / 'bin' / name}")
    if update_caches:
        refresh_desktop_caches(prefix)
    print("  no defaults were changed; to make Files the default folder handler run:")
    print("    xdg-mime default bingux-files.desktop inode/directory")
    print(f"  remove with: {prefix / 'bin/bingux-apps-uninstall'}")


def uninstall_apps(prefix, update_caches=True):
    manifest = load_apps_manifest(prefix)
    if manifest is None:
        raise ValueError(f"{prefix} has no Bingux apps install ({APPS_MANIFEST} is missing)")
    removed = 0
    for entry in reversed(manifest["files"]):
        path = prefix / entry["path"]
        if os.path.lexists(path):
            if path.is_symlink() or not path.is_file() or sha256_file(path) != entry["sha256"]:
                print(f"  kept {path}: it changed after the install", file=sys.stderr)
                continue
            path.unlink()
            removed += 1
        if entry["backup"]:
            backup = prefix / entry["backup"]
            if os.path.lexists(backup):
                if os.path.lexists(path):
                    print(f"  left {backup} in place: {path} exists", file=sys.stderr)
                else:
                    os.rename(backup, path)
                    print(f"  restored {path}")
    (prefix / APPS_MANIFEST).unlink()
    if update_caches:
        refresh_desktop_caches(prefix)
    for directory in map(Path, reversed(manifest["dirs"])):
        if directory.is_dir():
            for cache in APPS_CACHE_FILES:
                if [child.name for child in directory.iterdir()] == [cache]:
                    (directory / cache).unlink()
            try:
                directory.rmdir()
            except OSError:
                pass
    print(f"Removed the Bingux apps from {prefix} ({removed} files)")
    print("  user configuration, Files settings and sound effect presets were kept")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--user", action="store_true", help="manage a self-contained user install")
    parser.add_argument(
        "--apps",
        action="store_true",
        help="install only Settings, Files and Taxonomy (no shell) below --prefix (default ~/.local)",
    )
    parser.add_argument("--uninstall", action="store_true", help="remove a managed user or apps install")
    parser.add_argument("--no-strip", action="store_true", help="with --apps: install the programs unstripped")
    parser.add_argument("--prefix")
    parser.add_argument("--destdir", default="")
    parser.add_argument("--build-dir", default="build")
    parser.add_argument("--qml-dir")
    parser.add_argument("--cargo-dir", help="Cargo target directory that holds release/ (default: BUILD_DIR/cargo)")
    parser.add_argument("--ladspa-dir", help="where the LADSPA plugin library goes (default: PREFIX/lib/ladspa)")
    parser.add_argument("--no-systemd", action="store_true", help="do not reload or start user services")
    parser.add_argument(
        "--no-cache-update",
        action="store_true",
        help="with --apps: do not refresh the desktop, icon and MIME caches",
    )
    args = parser.parse_args()
    if args.apps:
        if args.user or args.destdir or args.qml_dir or args.ladspa_dir:
            parser.error("--apps cannot be combined with --user, --destdir, --qml-dir or --ladspa-dir")
        prefix = Path(args.prefix or default_apps_prefix()).expanduser()
        validate_prefix(parser, prefix)
        try:
            if args.uninstall:
                uninstall_apps(prefix, not args.no_cache_update)
            else:
                source = Path(__file__).resolve().parents[1]
                cargo = Path(args.cargo_dir).resolve() if args.cargo_dir else Path(args.build_dir).resolve() / "cargo"
                install_apps(source, prefix, cargo, not args.no_strip, not args.no_cache_update)
        except (KeyError, OSError, TypeError, ValueError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
            parser.error(str(error))
        return
    if args.no_strip or args.no_cache_update:
        parser.error("--no-strip and --no-cache-update are only valid with --apps")
    if args.uninstall and not args.user:
        parser.error("--uninstall is only valid with --user or --apps")
    if args.user and args.destdir:
        parser.error("--destdir cannot be used with --user")
    if args.user and args.prefix is None:
        args.prefix = str(default_user_prefix())
    if args.prefix is None:
        args.prefix = "/usr/local"
    prefix = Path(args.prefix).expanduser()
    validate_prefix(parser, prefix)
    qml = Path(args.qml_dir or prefix / "lib/bingux/qml")
    if not qml.is_absolute():
        parser.error("prefix and QML directory must be absolute")
    if args.user:
        try:
            qml.relative_to(prefix)
        except ValueError:
            parser.error("--qml-dir must be inside --prefix for a managed user install")
    ladspa = Path(args.ladspa_dir or prefix / "lib/ladspa")
    if not ladspa.is_absolute():
        parser.error("prefix and LADSPA directory must be absolute")
    if args.user:
        try:
            ladspa.relative_to(prefix)
        except ValueError:
            parser.error("--ladspa-dir must be inside --prefix for a managed user install")
    if args.uninstall:
        try:
            uninstall_user(prefix, args.no_systemd)
        except (KeyError, OSError, TypeError, ValueError, json.JSONDecodeError) as error:
            parser.error(str(error))
        return
    source = Path(__file__).resolve().parents[1]
    build = Path(args.build_dir).resolve()
    cargo = Path(args.cargo_dir).resolve() if args.cargo_dir else None
    try:
        if args.user:
            install_user(source, build, prefix, qml, args.no_systemd, cargo, ladspa)
            return
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.error(str(error))
    stage = Path(args.destdir or "/").resolve()

    def target(path):
        return stage / path.relative_to("/")

    try:
        build_payload(source, build, prefix, qml, target, cargo=cargo, ladspa=ladspa)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print("Installed Bingux under " + str(target(prefix)))


if __name__ == "__main__":
    main()
