#!/usr/bin/env python3
"""Check the prerequisites needed to build and install Bingux."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile


def available(command):
    return bool(command and shutil.which(command))


def find_quickshell():
    # Match install-shell.py's selection order, including explicit overrides.
    configured = os.environ.get("BINGUX_QUICKSHELL")
    if configured:
        return shutil.which(configured)
    data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    runtime = data_home / "gnoblin/quickshell-fedora/bin/quickshell"
    if runtime.is_file() and os.access(runtime, os.X_OK):
        return str(runtime)
    return next(
        (path for candidate in ("gnoblin-quickshell", "qs", "quickshell") if (path := shutil.which(candidate))), None
    )


def check_quickshell_qt(quickshell):
    # --version exits without loading QML or opening a desktop window. Its
    # verbose output reports Qt from this process, including wrapper env changes.
    try:
        result = subprocess.run(
            [quickshell, "--version", "-vv", "--no-color"],
            capture_output=True,
            text=True,
            check=False,
            timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return (
            f"Cannot check Quickshell Qt versions ({quickshell}): {error}; repair this runtime or set BINGUX_QUICKSHELL"
        )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip().splitlines()
        detail = detail[-1] if detail else f"exit status {result.returncode}"
        return f"Cannot check Quickshell Qt versions ({quickshell}): {detail}; repair this runtime or set BINGUX_QUICKSHELL"
    output = result.stdout + "\n" + result.stderr
    build = re.search(r"Buildtime Qt Version:\s*(\S+)", output)
    runtime = re.search(r"Runtime Qt Version:\s*(\S+)", output)
    if not build or not runtime:
        return f"Cannot read Quickshell Qt versions ({quickshell}); check --version -vv or set BINGUX_QUICKSHELL to a build reporting both Qt versions"
    # Quickshell uses private Qt APIs and warns on *any* version difference:
    # https://github.com/quickshell-mirror/quickshell/blob/v0.2.1/src/launch/command.cpp
    # This follows that contract; a mismatch alone does not prove a crash cause.
    if build[1] != runtime[1]:
        return (
            f"Quickshell Qt mismatch ({quickshell}): built against Qt {build[1]}, loads Qt {runtime[1]}; "
            "rebuild or reinstall this Quickshell for the loaded Qt, or set BINGUX_QUICKSHELL to a matching build"
        )
    return None


def check_quickshell_actions(quickshell):
    """Read the selected runtime's real desktop-action model, without a session."""
    remedy = (
        "use a Quickshell build with the desktop-action parser fix, or set BINGUX_QUICKSHELL to one; "
        "see https://github.com/kierandrewett/gnoblin/tree/main/patches/quickshell"
    )
    expected = {
        "id": "org.bingux.ActionIdentity",
        "actions": [
            {"id": "proof", "name": "Open proof"},
            {"id": "x", "name": "Single character"},
            {"id": "aproof", "name": "Same label"},
            {"id": "bproof", "name": "Same label"},
        ],
    }
    expected["actions"].sort(key=lambda action: action["id"])
    with tempfile.TemporaryDirectory(prefix="bingux-quickshell-actions-") as temporary:
        root = Path(temporary)
        for name in ("home", "data/applications", "empty", "config", "cache", "runtime"):
            (root / name).mkdir(parents=True, mode=0o700)
        desktop = (
            "[Desktop Entry]\nType=Application\nName=Bingux dependency probe\nExec=/bin/false\n"
            "Actions=proof;x;aproof;bproof;\n"
        )
        for action in expected["actions"]:
            desktop += f"\n[Desktop Action {action['id']}]\nName={action['name']}\nExec=/bin/false\n"
        (root / "data/applications/org.bingux.ActionIdentity.desktop").write_text(desktop)
        shutil.copyfile(Path(__file__).with_name("quickshell-actions.qml"), root / "shell.qml")
        # Retain Qt library/plugin selection from the version check. Replace all
        # session endpoints and config/data paths before loading our only QML file.
        environment = {
            key: value
            for key, value in os.environ.items()
            if key not in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "SESSION_MANAGER")
            and not key.startswith("QS_")
        }
        environment.update(
            HOME=str(root / "home"),
            XDG_DATA_HOME=str(root / "data"),
            XDG_DATA_DIRS=str(root / "empty"),
            XDG_CONFIG_HOME=str(root / "config"),
            XDG_CONFIG_DIRS=str(root / "empty"),
            XDG_CACHE_HOME=str(root / "cache"),
            XDG_STATE_HOME=str(root / "state"),
            XDG_RUNTIME_DIR=str(root / "runtime"),
            DBUS_SESSION_BUS_ADDRESS=f"unix:path={root}/no-session-bus",
            DBUS_SYSTEM_BUS_ADDRESS=f"unix:path={root}/no-system-bus",
            QT_QPA_PLATFORM="offscreen",
            QT_QUICK_BACKEND="software",
            QSG_RHI_BACKEND="software",
            LANG="C.UTF-8",
            LC_ALL="C.UTF-8",
        )
        try:
            with subprocess.Popen(
                [quickshell, "--no-color", "--log-rules", "qml.info=true", "--path", str(root / "shell.qml")],
                env=environment,
                # Keep relative executable/library paths identical to the Qt
                # version check; the QML config and all XDG paths are private.
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                start_new_session=True,
            ) as process:
                try:
                    stdout, stderr = process.communicate(timeout=10)
                finally:
                    # Also clean up wrapper/crash-handler children on timeout.
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
        except (OSError, subprocess.TimeoutExpired) as error:
            return f"Cannot verify Quickshell desktop-action IDs ({quickshell}): {error}; {remedy}"
        if process.returncode != 0:
            detail = (stderr or stdout).strip().splitlines()
            detail = detail[-1][:500] if detail else f"exit status {process.returncode}"
            return f"Cannot verify Quickshell desktop-action IDs ({quickshell}): {detail}; {remedy}"
        marker = "BINGUX_ACTION_IDENTITY="
        try:
            results = [
                json.loads(line.split(marker, 1)[1]) for line in (stdout + stderr).splitlines() if marker in line
            ]
        except json.JSONDecodeError:
            results = []
        if not results:
            return f"Cannot verify Quickshell desktop-action IDs ({quickshell}): no valid probe result; {remedy}"
        if results != [expected]:
            return f"Quickshell desktop-action IDs are incorrect ({quickshell}); {remedy}"
    return None


def check_qt_modules(qmake, source):
    with tempfile.TemporaryDirectory(prefix="bingux-qmake-check-") as directory:
        root = Path(directory)
        project = source / "packages/bingux-text-layout/text-layout.pro"
        result = subprocess.run(
            [qmake, str(project), "-o", str(root / "Makefile")],
            capture_output=True,
            text=True,
            check=False,
        )
    if result.returncode == 0:
        return None
    detail = (result.stderr or result.stdout).strip().splitlines()
    detail = detail[-1] if detail else "qmake could not load Qt Quick/QML"
    return f"Qt Quick/QML modules are unavailable ({detail})"


# Helpers that need these degrade with a message when they are missing, so a
# missing one never fails the check. Keep in step with PYTHON_MODULE_PACKAGES in
# shell/bingux/bingux_runtime.py.
OPTIONAL_PYTHON_FEATURES = (
    ("gi", "python3-gobject", "Control Centre, calendar and most helpers"),
    ("cairo", "python3-cairo", "document previews"),
    ("PIL", "python3-pillow", "document and image previews"),
    ("yaml", "python3-pyyaml", "YAML frontmatter in Markdown previews"),
)


def optional_notes():
    """Describe features that will be unavailable on this host, without failing."""
    notes = [
        f"{package} is not installed: {feature} will be unavailable"
        for module, package, feature in OPTIONAL_PYTHON_FEATURES
        if importlib.util.find_spec(module) is None
    ]
    if available("gsettings"):
        schemas = subprocess.run(
            ["gsettings", "list-schemas"], capture_output=True, text=True, check=False, timeout=10
        ).stdout.split()
        if "org.gnome.settings-daemon.plugins.color" not in schemas:
            notes.append("gnome-settings-daemon is not installed: the Night Light control will show Unavailable")
    return notes


def print_platform_hint():
    if Path("/etc/fedora-release").is_file() and available("dnf"):
        print("\nOn Fedora, install the project dependencies with:")
        print("  sudo dnf install rpmdevtools")
        print("  sudo dnf builddep packaging/rpm/bingux.spec")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install-user", action="store_true", help="also check user-service runtime requirements")
    parser.add_argument("--qmake", default="qmake6")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--cc", default="cc")
    parser.add_argument("--pkg-config", default="pkg-config")
    args = parser.parse_args()

    failures = []
    for label, command in (
        ("QMake 6", args.qmake),
        ("Cargo", args.cargo),
        ("C compiler", args.cc),
        ("pkg-config", args.pkg_config),
    ):
        if not available(command):
            failures.append(f"{label} not found: {command}")

    if available(args.pkg_config):
        for package in ("gio-2.0", "libpulse", "wayland-client", "Qt6WaylandClient", "gtk4", "libadwaita-1", "gstreamer-1.0", "gstreamer-app-1.0", "gstreamer-video-1.0", "poppler-glib"):
            result = subprocess.run([args.pkg_config, "--exists", package], check=False)
            if result.returncode != 0:
                failures.append(f"pkg-config dependency missing: {package}")

    if available(args.qmake):
        qt_failure = check_qt_modules(args.qmake, Path(__file__).resolve().parents[1])
        if qt_failure:
            failures.append(qt_failure)

    if args.install_user:
        quickshell = find_quickshell()
        if quickshell:
            qt_failure = check_quickshell_qt(quickshell)
            if qt_failure:
                failures.append(qt_failure)
            elif action_failure := check_quickshell_actions(quickshell):
                failures.append(action_failure)
        elif configured := os.environ.get("BINGUX_QUICKSHELL"):
            failures.append(f"BINGUX_QUICKSHELL is not executable: {configured}")
        else:
            failures.append(
                "Quickshell not found: install qs/quickshell, install Gnoblin's runtime, or set BINGUX_QUICKSHELL"
            )
        if not available("systemctl"):
            failures.append("systemctl not found: user services cannot be managed")

    if failures:
        print("Bingux prerequisites are not ready:")
        for failure in failures:
            print(f"  - {failure}")
        print_platform_hint()
        print("\nInstall the matching build dependencies, then rerun: make doctor")
        return 1
    print("Bingux prerequisites are ready.")
    if notes := optional_notes():
        print("Optional features not available:")
        for note in notes:
            print(f"  - {note}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
