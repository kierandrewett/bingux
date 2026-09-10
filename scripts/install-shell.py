#!/usr/bin/env python3
"""Stage the standalone shell and helpers without changing user configuration."""
import argparse
import json
from pathlib import Path
import shlex
import shutil


def install(source, destination, executable=False):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if executable else 0o644)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", default="/usr/local")
    parser.add_argument("--destdir", default="")
    parser.add_argument("--build-dir", default="build")
    parser.add_argument("--qml-dir")
    args = parser.parse_args()
    prefix = Path(args.prefix)
    qml = Path(args.qml_dir or prefix / "lib/bingux/qml")
    if not prefix.is_absolute() or not qml.is_absolute():
        parser.error("prefix and QML directory must be absolute")
    if any(c.isspace() or c in "%\"\\" for c in str(prefix)):
        parser.error("prefix must not contain whitespace, percent signs, quotes or backslashes")
    source = Path(__file__).resolve().parents[1]
    build = Path(args.build_dir).resolve()
    payload = {
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
        parser.error("run make first; missing: " + ", ".join(missing))
    stage = Path(args.destdir or "/").resolve()
    def target(path):
        return stage / path.relative_to("/")
    shell = prefix / "share/bingux/shell"
    for path in (source / "shell/bingux").rglob("*"):
        if not path.is_file() or "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        if path.name.endswith("Test.qml"):
            continue
        install(path, target(shell / path.relative_to(source / "shell/bingux")))
    for path, destination in payload.items():
        install(path, target(destination), destination.parent == prefix / "bin")
    install(source / "packages/binguxctl/binguxctl.py", target(prefix / "libexec/bingux/binguxctl.py"))
    install(source / "packages/bingux-settings/bingux-settings", target(prefix / "libexec/bingux/bingux-settings"), True)
    common = "#!/bin/sh\nset -eu\nulimit -c 0\n"
    common += "export QML_IMPORT_PATH=" + shlex.quote(str(qml)) + '${QML_IMPORT_PATH:+:$QML_IMPORT_PATH}\n'
    common += "export BINGUX_CONFIG_PATH=" + shlex.quote(str(shell)) + "\n"
    common += "export BINGUX_SETTINGS_QML=" + shlex.quote(str(shell / "settings.qml")) + "\n"
    wrappers = {
        "bingux": 'exec "${BINGUX_QUICKSHELL:-qs}" -p "$BINGUX_CONFIG_PATH" "$@"\n',
        "bingux-search-ui": 'exec "${BINGUX_QUICKSHELL:-qs}" -p ' + shlex.quote(str(shell / "SearchShell.qml")) + ' "$@"\n',
        "bingux-switcher-ui": 'exec "${BINGUX_QUICKSHELL:-qs}" -p ' + shlex.quote(str(shell / "SwitcherShell.qml")) + ' "$@"\n',
        "bingux-settings": "exec " + shlex.quote(str(prefix / "libexec/bingux/bingux-settings")) + ' "$@"\n',
        "binguxctl": "exec python3 " + shlex.quote(str(prefix / "libexec/bingux/binguxctl.py")) + ' "$@"\n',
    }
    for name, command in wrappers.items():
        path = target(prefix / "bin" / name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(common + command)
        path.chmod(0o755)
    config = target(prefix / "share/bingux/search.json")
    config.write_text(json.dumps({"protocolVersion": 1, "commands": {
        "applicationLauncher": ["python3", str(shell / "launch-application.py")],
        "fileOpener": ["xdg-open"], "clipboard": ["wl-copy"]}}, indent=2) + "\n")
    unit_dir = target(prefix / "lib/systemd/user")
    unit_dir.mkdir(parents=True, exist_ok=True)
    commands = {
        "bingux": str(prefix / "bin/bingux") + " --no-color",
        "bingux-search-ui": str(prefix / "bin/bingux-search-ui"),
        "bingux-switcher-ui": str(prefix / "bin/bingux-switcher-ui"),
        "bingux-statusd": str(prefix / "bin/bingux-statusd"),
        "bingux-searchd": str(prefix / "libexec/bingux/search-service"),
    }
    search_service = target(prefix / "libexec/bingux/search-service")
    search_service.write_text("#!/bin/sh\nset -eu\n" +
        'config="${XDG_CONFIG_HOME:-$HOME/.config}/bingux/search.json"\n' +
        '[ -f "$config" ] || config=' + shlex.quote(str(prefix / "share/bingux/search.json")) + "\n" +
        "exec " + shlex.quote(str(prefix / "bin/bingux-searchd")) + ' --config "$config"\n')
    search_service.chmod(0o755)
    for name, command in commands.items():
        unit = "[Unit]\nDescription=" + name + "\nPartOf=bingux.target\nAfter=graphical-session.target\n"
        unit += "\n[Service]\nExecStart=" + command + "\nRestart=on-failure\nRestartSec=3\nLimitCORE=0\n"
        unit += "Environment=QT_QPA_PLATFORM=wayland\nEnvironment=QML_IMPORT_PATH=" + str(qml) + "\n"
        unit += "Environment=BINGUX_CONFIG_PATH=" + str(shell) + "\n"
        if name == "bingux":
            unit += "Type=dbus\nBusName=org.freedesktop.Notifications\n"
        (unit_dir / (name + ".service")).write_text(unit)
    (unit_dir / "bingux.target").write_text(
        "[Unit]\nDescription=Bingux desktop shell\nPartOf=graphical-session.target\n" +
        "After=graphical-session.target\nWants=" + " ".join(name + ".service" for name in commands) + "\n" +
        "\n[Install]\nWantedBy=graphical-session.target\n")
    print("Installed Bingux under " + str(target(prefix)))


if __name__ == "__main__":
    main()
