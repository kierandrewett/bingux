#!/usr/bin/env python3
"""Install or stage the standalone shell and helpers."""

import argparse
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


def build_payload(source, build, prefix, qml, target, quickshell="qs", managed=False):
    """Install payload files and return their paths relative to prefix."""
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
        build / "cargo/release/bingux-config": prefix / "bin/bingux-config",
        build / "cargo/release/bingux-searchd": prefix / "bin/bingux-searchd",
        build / "cargo/release/bingux-statusd": prefix / "bin/bingux-statusd",
        build / "cargo/release/bingux-settings": prefix / "bin/bingux-settings",
        source / "packages/bingux-settings/bingux-settings.desktop": prefix
        / "share/applications/bingux-settings.desktop",
        source / "scripts/bingux-clipboard-paste": prefix / "bin/bingux-clipboard-paste",
        source / "packaging/gnoblin/bingux.lua": prefix / "share/gnoblin/conf.d/bingux.lua",
    }
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
    adwaita_install = prefix / "share/icons/Adwaita"
    copy(adwaita_index, adwaita_install / "index.theme")
    for path in adwaita_assets.rglob("*"):
        if path.is_file():
            copy(path, adwaita_install / path.relative_to(adwaita_assets))
    for license_name in ("COPYING", "COPYING_CCBYSA3", "COPYING_LGPL"):
        copy(
            adwaita_source / license_name,
            prefix / "share/licenses/adwaita-icon-theme" / license_name,
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
        executable = destination.parent == prefix / "bin" or destination in (
            prefix / "libexec/bingux/bingux-image-clipboard",
            prefix / "libexec/bingux/bingux-frame",
        )
        if path == source / "packaging/gnoblin/bingux.lua":
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
    common += "export BINGUX_ADWAITA_ICON_DIR=" + shlex.quote(str(prefix / "share/icons/Adwaita")) + "\n"
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

    search_service = target(prefix / "libexec/bingux/search-service")
    search_migrator = prefix / "share/bingux/shell/migrate-search-config.py"
    application_launcher = prefix / "share/bingux/shell/launch-application.py"
    steam_game_catalog = prefix / "share/bingux/shell/steam-games.py"
    write_launcher(
        search_service,
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


def install_user(source, build, prefix, qml, no_systemd):
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--user", action="store_true", help="manage a self-contained user install")
    parser.add_argument("--uninstall", action="store_true", help="remove a managed user install")
    parser.add_argument("--prefix")
    parser.add_argument("--destdir", default="")
    parser.add_argument("--build-dir", default="build")
    parser.add_argument("--qml-dir")
    parser.add_argument("--no-systemd", action="store_true", help="do not reload or start user services")
    args = parser.parse_args()
    if args.uninstall and not args.user:
        parser.error("--uninstall is only valid with --user")
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
    if args.uninstall:
        try:
            uninstall_user(prefix, args.no_systemd)
        except (KeyError, OSError, TypeError, ValueError, json.JSONDecodeError) as error:
            parser.error(str(error))
        return
    source = Path(__file__).resolve().parents[1]
    build = Path(args.build_dir).resolve()
    try:
        if args.user:
            install_user(source, build, prefix, qml, args.no_systemd)
            return
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.error(str(error))
    stage = Path(args.destdir or "/").resolve()

    def target(path):
        return stage / path.relative_to("/")

    try:
        build_payload(source, build, prefix, qml, target)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print("Installed Bingux under " + str(target(prefix)))


if __name__ == "__main__":
    main()
