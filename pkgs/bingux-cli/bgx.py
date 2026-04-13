#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import os
import subprocess
import sys


VOLATILE_PROFILE = f"/nix/var/nix/profiles/per-user/{os.environ.get('USER', 'root')}/bgx-volatile"
PERMANENT_PROFILE = os.path.expanduser("~/.local/state/nix/profiles/profile")

GREEN = "\033[32m"
RED = "\033[31m"
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"


def run(cmd, **kwargs):
    return subprocess.run(cmd, **kwargs)


def do_install(pkg, save=False):
    nixpkg = f"nixpkgs#{pkg}"
    profile = PERMANENT_PROFILE if save else VOLATILE_PROFILE
    label = "permanently" if save else "until reboot"

    r = run(["nix", "profile", "install", "--profile", profile, nixpkg])
    if r.returncode == 0:
        print(f"{GREEN}\u2713{RESET} {pkg} installed {label}.")
        return True
    else:
        print(f"{RED}\u2717{RESET} Failed to install {pkg}.", file=sys.stderr)
        return False


def do_remove(pkg):
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
        print(f"{RED}\u2717{RESET} {pkg} is not installed.", file=sys.stderr)
    return removed


def do_search(query):
    run(["nix", "search", "nixpkgs", query])


def do_list():
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


def run_prefix_mode(args):
    """Handle prefix operators: +pkg ++pkg -pkg ?query"""
    installed = 0
    removed = 0
    failed = 0

    for arg in args:
        if arg.startswith("++"):
            pkg = arg[2:]
            if do_install(pkg, save=True):
                installed += 1
            else:
                failed += 1
        elif arg.startswith("+"):
            pkg = arg[1:]
            if do_install(pkg, save=False):
                installed += 1
            else:
                failed += 1
        elif arg.startswith("-"):
            pkg = arg[1:]
            if do_remove(pkg):
                removed += 1
            else:
                failed += 1
        elif arg.startswith("?"):
            do_search(arg[1:])
        else:
            print(f"{RED}\u2717{RESET} Unknown: {arg}", file=sys.stderr)
            failed += 1

    # Summary for batch operations
    total = installed + removed + failed
    if total > 1:
        parts = []
        if installed:
            parts.append(f"{installed} installed")
        if removed:
            parts.append(f"{removed} removed")
        if failed:
            parts.append(f"{RED}{failed} failed{RESET}")
        print(f"\n{DIM}{', '.join(parts)}{RESET}")

    if failed:
        sys.exit(1)


def run_subcommand_mode(args):
    """Handle subcommands: install, remove, search, list"""
    cmd = args[0]
    rest = args[1:]

    # Install aliases
    if cmd in ("install", "add", "a"):
        save = False
        pkgs = []
        for arg in rest:
            if arg in ("-s", "--save"):
                save = True
            else:
                pkgs.append(arg)
        if not pkgs:
            print("Package name required.", file=sys.stderr)
            sys.exit(1)
        failed = 0
        for pkg in pkgs:
            if not do_install(pkg, save=save):
                failed += 1
        if failed:
            sys.exit(1)

    # Remove aliases
    elif cmd in ("remove", "uninstall", "rm", "r"):
        if not rest:
            print("Package name required.", file=sys.stderr)
            sys.exit(1)
        failed = 0
        for pkg in rest:
            if not do_remove(pkg):
                failed += 1
        if failed:
            sys.exit(1)

    # Search aliases
    elif cmd in ("search", "s", "q"):
        if not rest:
            print("Search query required.", file=sys.stderr)
            sys.exit(1)
        do_search(" ".join(rest))

    # List aliases
    elif cmd in ("list", "ls"):
        do_list()

    else:
        print_usage()
        sys.exit(1)


def print_usage():
    print(f"""
{BOLD}bgx{RESET} — Bingux package manager

{BOLD}Quick syntax:{RESET}
  bgx +firefox                  Install (until reboot)
  bgx ++firefox                 Install permanently
  bgx -firefox                  Remove
  bgx +firefox +htop -chromium  Batch operations
  bgx ?browser                  Search
  bgx                           List installed

{BOLD}Subcommands:{RESET}
  bgx install [-s] <pkg...>     Install (-s to save permanently)
  bgx remove <pkg...>           Remove
  bgx search <query>            Search nixpkgs
  bgx list                      List installed packages

{BOLD}Aliases:{RESET}
  install: add, a               remove: uninstall, rm, r
  search: s, q                  list: ls
""".strip())


def main():
    args = sys.argv[1:]

    # No args = list
    if not args:
        do_list()
        return

    # Check if first arg is a prefix operator
    if args[0][0] in ("+", "-", "?"):
        run_prefix_mode(args)
    else:
        run_subcommand_mode(args)


if __name__ == "__main__":
    main()
