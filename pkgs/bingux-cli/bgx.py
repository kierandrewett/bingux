#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import argparse
import os
import subprocess
import sys


VOLATILE_PROFILE = f"/nix/var/nix/profiles/per-user/{os.environ.get('USER', 'root')}/bgx-volatile"
PERMANENT_PROFILE = os.path.expanduser("~/.local/state/nix/profiles/profile")


def run(cmd, **kwargs):
    return subprocess.run(cmd, **kwargs)


def cmd_install(args):
    pkg = args.package
    nixpkg = f"nixpkgs#{pkg}"

    if args.save:
        print(f"Installing {pkg} permanently...")
        r = run(["nix", "profile", "install", "--profile", PERMANENT_PROFILE, nixpkg])
        if r.returncode == 0:
            print(f"\033[32m✓\033[0m {pkg} installed permanently.")
        else:
            print(f"\033[31m✗\033[0m Failed to install {pkg}.", file=sys.stderr)
            sys.exit(1)
    else:
        print(f"Installing {pkg} (until reboot)...")
        r = run(["nix", "profile", "install", "--profile", VOLATILE_PROFILE, nixpkg])
        if r.returncode == 0:
            print(f"\033[32m✓\033[0m {pkg} installed. Available system-wide until reboot.")
        else:
            print(f"\033[31m✗\033[0m Failed to install {pkg}.", file=sys.stderr)
            sys.exit(1)


def cmd_remove(args):
    pkg = args.package
    removed = False

    for profile, label in [
        (VOLATILE_PROFILE, "temporary"),
        (PERMANENT_PROFILE, "permanent"),
    ]:
        r = run(
            ["nix", "profile", "remove", "--profile", profile, f".*{pkg}.*"],
            capture_output=True,
        )
        if r.returncode == 0:
            print(f"\033[32m✓\033[0m Removed {pkg} from {label} packages.")
            removed = True

    if not removed:
        print(f"{pkg} is not installed.", file=sys.stderr)
        sys.exit(1)


def cmd_search(args):
    run(["nix", "search", "nixpkgs", args.query])


def cmd_list(args):
    print("\033[1mTemporary (until reboot):\033[0m")
    r = run(["nix", "profile", "list", "--profile", VOLATILE_PROFILE], capture_output=True, text=True)
    if r.returncode == 0 and r.stdout.strip():
        print(r.stdout)
    else:
        print("  (none)\n")

    print("\033[1mPermanent:\033[0m")
    r = run(["nix", "profile", "list", "--profile", PERMANENT_PROFILE], capture_output=True, text=True)
    if r.returncode == 0 and r.stdout.strip():
        print(r.stdout)
    else:
        print("  (none)")


def main():
    parser = argparse.ArgumentParser(
        prog="bgx",
        description="Bingux package manager",
    )
    sub = parser.add_subparsers(dest="command")

    # install
    p_install = sub.add_parser("install", help="Install a package")
    p_install.add_argument("--save", action="store_true", help="Persist after reboot")
    p_install.add_argument("package", help="Package name")
    p_install.set_defaults(func=cmd_install)

    # remove
    p_remove = sub.add_parser("remove", help="Remove a package")
    p_remove.add_argument("package", help="Package name")
    p_remove.set_defaults(func=cmd_remove)

    # search
    p_search = sub.add_parser("search", help="Search for packages")
    p_search.add_argument("query", help="Search query")
    p_search.set_defaults(func=cmd_search)

    # list
    p_list = sub.add_parser("list", help="List installed packages")
    p_list.set_defaults(func=cmd_list)

    args = parser.parse_args()
    if not args.command:
        parser.print_help()
        sys.exit(1)

    args.func(args)


if __name__ == "__main__":
    main()
