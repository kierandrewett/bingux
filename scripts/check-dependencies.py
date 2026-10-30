#!/usr/bin/env python3
"""Check the prerequisites needed to build and install Bingux."""

import argparse
import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile


def available(command):
    return bool(command and shutil.which(command))


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
    parser.add_argument("--cmake", default="cmake")
    parser.add_argument("--pkg-config", default="pkg-config")
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[1]

    failures = []
    for label, command in (
        ("QMake 6", args.qmake),
        ("Cargo", args.cargo),
        ("C compiler", args.cc),
        ("CMake", args.cmake),
        ("Ninja", "ninja"),
        ("pkg-config", args.pkg_config),
    ):
        if not available(command):
            failures.append(f"{label} not found: {command}")

    if available(args.pkg_config):
        # The first group builds the QML plugins, daemons, helpers and the
        # applications (GStreamer and poppler are the preview host's); the second
        # builds bingux-shell (Quickshell) from packages/bingux-shell.
        packages = ("gio-2.0", "libpulse", "wayland-client", "Qt6WaylandClient", "gtk4", "libadwaita-1")
        packages += ("gstreamer-1.0", "gstreamer-app-1.0", "gstreamer-video-1.0", "poppler-glib")
        packages += ("libarchive", "CLI11", "jemalloc", "libpipewire-0.3", "libdrm", "xcb", "polkit-agent-1")
        packages += ("wayland-protocols", "Qt6ShaderTools", "Qt6Svg")
        for package in packages:
            result = subprocess.run([args.pkg_config, "--exists", package], check=False)
            if result.returncode != 0:
                failures.append(f"pkg-config dependency missing: {package}")

    if available(args.qmake):
        qt_failure = check_qt_modules(args.qmake, source)
        if qt_failure:
            failures.append(qt_failure)

    for submodule in ("packages/bingux-shell/quickshell", "subprojects/adwaita-icon-theme"):
        if not (source / submodule / "CMakeLists.txt").is_file() and not (source / submodule / "index.theme").is_file():
            failures.append(f"submodule missing: {submodule}; run git submodule update --init")

    if args.install_user and not available("systemctl"):
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
