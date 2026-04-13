#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import argparse
import os
import subprocess
import sys


VOLATILE_PROFILE = f"/nix/var/nix/profiles/per-user/{os.environ.get('USER', 'root')}/bgx-volatile"
PERMANENT_PROFILE = os.path.expanduser("~/.local/state/nix/profiles/profile")

GREEN = "\033[32m"
RED = "\033[31m"
BOLD = "\033[1m"
RESET = "\033[0m"


def run(cmd, **kwargs):
    return subprocess.run(cmd, **kwargs)


def cmd_install(args):
    pkg = args.package
    nixpkg = f"nixpkgs#{pkg}"

    if args.save:
        print(f"Installing {pkg} permanently...")
        r = run(["nix", "profile", "install", "--profile", PERMANENT_PROFILE, nixpkg])
        if r.returncode == 0:
            print(f"{GREEN}\u2713{RESET} {pkg} installed permanently.")
        else:
            print(f"{RED}\u2717{RESET} Failed to install {pkg}.", file=sys.stderr)
            sys.exit(1)
    else:
        print(f"Installing {pkg} (until reboot)...")
        r = run(["nix", "profile", "install", "--profile", VOLATILE_PROFILE, nixpkg])
        if r.returncode == 0:
            print(f"{GREEN}\u2713{RESET} {pkg} installed. Available system-wide until reboot.")
        else:
            print(f"{RED}\u2717{RESET} Failed to install {pkg}.", file=sys.stderr)
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
            print(f"{GREEN}\u2713{RESET} Removed {pkg} from {label} packages.")
            removed = True

    if not removed:
        print(f"{pkg} is not installed.", file=sys.stderr)
        sys.exit(1)


def cmd_search(args):
    run(["nix", "search", "nixpkgs", args.query])


def cmd_list(args):
    print(f"{BOLD}Temporary (until reboot):{RESET}")
    r = run(["nix", "profile", "list", "--profile", VOLATILE_PROFILE], capture_output=True, text=True)
    if r.returncode == 0 and r.stdout.strip():
        print(r.stdout)
    else:
        print("  (none)\n")

    print(f"{BOLD}Permanent:{RESET}")
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

    # install / add / +
    for name in ["install", "add", "+"]:
        p = sub.add_parser(name, help="Install a package")
        p.add_argument("-s", "--save", action="store_true", help="Persist after reboot")
        p.add_argument("package", help="Package name")
        p.set_defaults(func=cmd_install)

    # remove / rm / -
    for name in ["remove", "rm", "-"]:
        p = sub.add_parser(name, help="Remove a package")
        p.add_argument("package", help="Package name")
        p.set_defaults(func=cmd_remove)

    # search / s
    for name in ["search", "s"]:
        p = sub.add_parser(name, help="Search for packages")
        p.add_argument("query", help="Search query")
        p.set_defaults(func=cmd_search)

    # list / ls
    for name in ["list", "ls"]:
        p = sub.add_parser(name, help="List installed packages")
        p.set_defaults(func=cmd_list)

    args = parser.parse_args()
    if not args.command:
        parser.print_help()
        sys.exit(1)

    args.func(args)


if __name__ == "__main__":
    main()
