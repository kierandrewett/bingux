#!/usr/bin/env python3
"""Install or stage the standalone shell and helpers."""
import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile


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
    if any(c.isspace() or c in "%\"\\" for c in str(prefix)):
        parser.error("prefix must not contain whitespace, percent signs, quotes or backslashes")


def write_launcher(path, contents):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents)
    path.chmod(0o755)


def resolve_quickshell(no_systemd):
    configured = os.environ.get("BINGUX_QUICKSHELL")
    if configured:
        if shutil.which(configured):
            return configured
        if not no_systemd:
            raise ValueError(f"BINGUX_QUICKSHELL is not executable: {configured}")
        return configured
    for candidate in ("qs", "quickshell"):
        resolved = shutil.which(candidate)
        if resolved:
            return resolved
    if not no_systemd:
        raise ValueError("Quickshell was not found; install it or set BINGUX_QUICKSHELL")
    return "qs"


def systemctl_user(arguments, check=True):
    try:
        result = subprocess.run(
            ["systemctl", "--user", *arguments],
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as error:
        raise ValueError(f"could not run systemctl --user: {error}") from error
    if check and result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        command = "systemctl --user " + " ".join(arguments)
        raise ValueError(f"{command} failed ({result.returncode}): {detail or 'no diagnostic'}")
    return result


def build_payload(source, build, prefix, qml, target, quickshell="qs"):
    """Install payload files and return their paths relative to prefix."""
    payload = {
        build / "effects/libbinguxeffects.so": qml / "Bingux/Effects/libbinguxeffects.so",
        source / "packages/bingux-effects/qmldir": qml / "Bingux/Effects/qmldir",
        build / "text/libbinguxtext.so": qml / "Bingux/Text/libbinguxtext.so",
        source / "packages/bingux-text-layout/qmldir": qml / "Bingux/Text/qmldir",
        build / "settings/libbinguxsettings.so": qml / "Bingux/Settings/libbinguxsettings.so",
        source / "packages/bingux-settings/platform/qmldir": qml / "Bingux/Settings/qmldir",
        build / "bingux-audio-meter": prefix / "bin/bingux-audio-meter",
        build / "cargo/release/bingux-searchd": prefix / "bin/bingux-searchd",
        build / "cargo/release/bingux-statusd": prefix / "bin/bingux-statusd",
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

    shell = prefix / "share/bingux/shell"
    for path in (source / "shell/bingux").rglob("*"):
        if not path.is_file() or "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        if path.name.endswith("Test.qml"):
            continue
        copy(path, shell / path.relative_to(source / "shell/bingux"))
    for path, destination in payload.items():
        copy(path, destination, destination.parent == prefix / "bin")
    copy(source / "packages/binguxctl/binguxctl.py", prefix / "libexec/bingux/binguxctl.py")
    copy(source / "packages/bingux-settings/bingux-settings", prefix / "libexec/bingux/bingux-settings", True)

    common = "#!/bin/sh\nset -eu\nulimit -c 0\n"
    common += "export BINGUX_QUICKSHELL=${BINGUX_QUICKSHELL:-" + shlex.quote(quickshell) + "}\n"
    common += "export QML_IMPORT_PATH=" + shlex.quote(str(qml)) + '${QML_IMPORT_PATH:+:$QML_IMPORT_PATH}\n'
    common += "export BINGUX_CONFIG_PATH=" + shlex.quote(str(shell)) + "\n"
    common += "export BINGUX_SETTINGS_QML=" + shlex.quote(str(shell / "settings.qml")) + "\n"
    wrappers = {
        "bingux": 'exec "${BINGUX_QUICKSHELL:-qs}" -p "$BINGUX_CONFIG_PATH" "$@"\n',
        "bingux-search-ui": 'exec "${BINGUX_QUICKSHELL:-qs}" -p ' + shlex.quote(str(shell / "SearchShell.qml")) + ' "$@"\n',
        "bingux-switcher-ui": 'exec "${BINGUX_QUICKSHELL:-qs}" -p ' + shlex.quote(str(shell / "SwitcherShell.qml")) + ' "$@"\n',
        "bingux-emoji-ui": 'exec "${BINGUX_QUICKSHELL:-qs}" -p ' + shlex.quote(str(shell / "EmojiShell.qml")) + ' "$@"\n',
        "bingux-capture-ui": 'exec "${BINGUX_QUICKSHELL:-qs}" -p ' + shlex.quote(str(shell / "CaptureShell.qml")) + ' "$@"\n',
        "bingux-settings": "exec " + shlex.quote(str(prefix / "libexec/bingux/bingux-settings")) + ' "$@"\n',
        "binguxctl": "exec python3 " + shlex.quote(str(prefix / "libexec/bingux/binguxctl.py")) + ' "$@"\n',
    }
    for name, command in wrappers.items():
        copy_text = target(prefix / "bin" / name)
        write_launcher(copy_text, common + command)
        installed.append(Path("bin") / name)

    config = target(prefix / "share/bingux/search.json")
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text(json.dumps({"protocolVersion": 1, "commands": {
        "applicationLauncher": ["python3", str(shell / "launch-application.py")],
        "fileOpener": ["xdg-open"], "clipboard": ["wl-copy"]}}, indent=2) + "\n")
    installed.append(Path("share/bingux/search.json"))

    search_service = target(prefix / "libexec/bingux/search-service")
    write_launcher(search_service, "#!/bin/sh\nset -eu\n" +
        'config="${XDG_CONFIG_HOME:-$HOME/.config}/bingux/search.json"\n' +
        '[ -f "$config" ] || config=' + shlex.quote(str(prefix / "share/bingux/search.json")) + "\n" +
        "exec " + shlex.quote(str(prefix / "bin/bingux-searchd")) + ' --config "$config"\n')
    installed.append(Path("libexec/bingux/search-service"))
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


def integration_paths(prefix):
    names = ("bingux", "bingux-search-ui", "bingux-switcher-ui", "bingux-emoji-ui",
             "bingux-capture-ui", "bingux-settings", "binguxctl")
    units = ("bingux.target", "bingux.service", "bingux-searchd.service", "bingux-statusd.service",
             "bingux-search-ui.service", "bingux-switcher-ui.service", "bingux-capture-ui.service",
             "bingux-emoji-ui.service")
    bin_dir = Path(os.environ.get("XDG_BIN_HOME") or Path.home() / ".local/bin")
    unit_dir = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "systemd/user"
    return [(bin_dir / name, prefix / "bin" / name) for name in names] + \
        [(unit_dir / name, prefix / "lib/systemd/user" / name) for name in units]


def ensure_integration_is_safe(paths):
    for link, target_path in paths:
        if link.exists() and (not link.is_symlink() or link.resolve() != target_path.resolve()):
            raise ValueError(f"refusing to replace existing file: {link}")


def install_user(source, build, prefix, qml, no_systemd):
    manifest_path = prefix / ".bingux-install.json"
    if prefix.is_symlink():
        raise ValueError(f"refusing to install through symlink: {prefix}")
    if prefix.exists() and any(prefix.iterdir()) and not manifest_path.is_file():
        raise ValueError(f"refusing to use existing unmanaged directory: {prefix}")
    prefix.parent.mkdir(parents=True, exist_ok=True)
    links = integration_paths(prefix)
    ensure_integration_is_safe(links)
    quickshell = resolve_quickshell(no_systemd)
    staging = Path(tempfile.mkdtemp(prefix=f".{prefix.name}.install-", dir=prefix.parent))
    backup = None
    committed = False
    was_active = False
    try:
        installed = build_payload(
            source, build, prefix, qml,
            lambda path: staging / path.relative_to(prefix), quickshell,
        )
        manifest = {
            "format": 1,
            "prefix": str(prefix),
            "files": [str(path) for path in installed],
            "links": [{"path": str(link), "target": str(target_path)} for link, target_path in links],
        }
        (staging / ".bingux-install.json").write_text(json.dumps(manifest, indent=2) + "\n")
        if prefix.exists():
            if not no_systemd:
                was_active = systemctl_user(["is-active", "--quiet", "bingux.target"], check=False).returncode == 0
                systemctl_user(["stop", "bingux.target"], check=False)
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
        committed = True
        if not no_systemd:
            systemctl_user(["daemon-reload"])
            systemctl_user(["enable", "--now", "bingux.target"])
    except Exception:
        if staging is not None and staging.exists():
            shutil.rmtree(staging)
        if not committed:
            if prefix.exists():
                shutil.rmtree(prefix)
            if backup is not None and backup.exists():
                backup.rename(prefix)
            if was_active and not no_systemd:
                systemctl_user(["enable", "--now", "bingux.target"], check=False)
        elif not no_systemd:
            # The payload is complete, but recover the desktop if systemd
            # rejected the reload/start operation after the swap.
            systemctl_user(["enable", "--now", "bingux.target"], check=False)
        raise
    finally:
        if backup is not None and backup.exists():
            shutil.rmtree(backup)
    print(f"Installed Bingux under {prefix}")
    print("  command links: " + str(Path(os.environ.get("XDG_BIN_HOME") or Path.home() / ".local/bin")))
    print("  service links: " + str(Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "systemd/user"))
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
