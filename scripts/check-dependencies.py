#!/usr/bin/env python3
"""Check the prerequisites needed to build and install Bingux."""

import argparse
import os
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
        for package in ("libpulse", "wayland-client", "Qt6WaylandClient"):
            result = subprocess.run([args.pkg_config, "--exists", package], check=False)
            if result.returncode != 0:
                failures.append(f"pkg-config dependency missing: {package}")

    if available(args.qmake):
        qt_failure = check_qt_modules(args.qmake, Path(__file__).resolve().parents[1])
        if qt_failure:
            failures.append(qt_failure)

    if args.install_user:
        quickshell = os.environ.get("BINGUX_QUICKSHELL")
        if quickshell:
            if not available(quickshell):
                failures.append(f"BINGUX_QUICKSHELL is not executable: {quickshell}")
        elif not any(available(candidate) for candidate in ("qs", "quickshell")):
            failures.append("Quickshell not found: install qs/quickshell or set BINGUX_QUICKSHELL")
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
