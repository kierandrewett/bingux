#!/usr/bin/env python3
"""Check the prerequisites needed to build and install Bingux."""

import argparse
import os
from pathlib import Path
import re
import shutil
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
        for package in ("gio-2.0", "libpulse", "wayland-client", "Qt6WaylandClient", "gtk4", "libadwaita-1"):
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
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
