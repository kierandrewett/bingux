#!/usr/bin/env python3
"""bgx — Bingux package manager."""

import json
import os
import subprocess
import sys


VOLATILE_PROFILE = f"/nix/var/nix/profiles/per-user/{os.environ.get('USER', 'root')}/bgx-volatile"
PERMANENT_PROFILE = os.path.expanduser("~/.local/state/nix/profiles/profile")

GREEN = "\033[32m"
RED = "\033[31m"
YELLOW = "\033[33m"
BLUE = "\033[34m"
CYAN = "\033[36m"
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"

COL_NAME = 30
COL_VER = 16


def run(cmd, **kwargs):
    return subprocess.run(cmd, **kwargs)


def pkg_info(pkg):
    """Get package name, version, and description from nixpkgs."""
    info = {"name": pkg, "version": "", "description": ""}
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.version"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            info["version"] = r.stdout.strip()
    except subprocess.TimeoutExpired:
        pass
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.meta.description"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            info["description"] = r.stdout.strip()
    except subprocess.TimeoutExpired:
        pass
    return info


def confirm(prompt="Proceed? [y/N] "):
    try:
        return input(prompt).strip().lower() in ("y", "yes")
    except (EOFError, KeyboardInterrupt):
        print()
        return False


def pkg_row(name, version="", description=""):
    """Format a package as an aligned row."""
    n = name.ljust(COL_NAME)
    v = version.ljust(COL_VER) if version else " " * COL_VER
    return f"  {BOLD}{n}{RESET} {CYAN}{v}{RESET} {DIM}{description}{RESET}"


def show_transaction(installs, removes, save=False):
    if not installs and not removes:
        return True

    if installs:
        mode = "permanently" if save else "for this session"
        print(f"\n  {GREEN}Installing ({mode}):{RESET}")
        print(f"  {'Package'.ljust(COL_NAME)} {'Version'.ljust(COL_VER)} Description")
        for info in installs:
            print(pkg_row(info["name"], info["version"], info["description"]))

    if removes:
        print(f"\n  {RED}Removing:{RESET}")
        for pkg in removes:
            print(f"  {BOLD}{pkg}{RESET}")

    parts = []
    if installs:
        parts.append(f"{GREEN}{len(installs)} to install{RESET}")
    if removes:
        parts.append(f"{RED}{len(removes)} to remove{RESET}")
    print(f"\n  {', '.join(parts)}\n")

    return confirm(f"  Proceed? [{GREEN}y{RESET}/{RED}N{RESET}] ")


def do_install(pkgs, save=False, skip_confirm=False):
    profile = PERMANENT_PROFILE if save else VOLATILE_PROFILE

    print(f"  {BLUE}::{RESET} Resolving packages...")
    infos = [pkg_info(p) for p in pkgs]

    if not skip_confirm and not show_transaction(infos, [], save=save):
        print("  Aborted.")
        return False

    failed = 0
    for pkg in pkgs:
        nixpkg = f"nixpkgs#{pkg}"
        r = run(["nix", "profile", "install", "--profile", profile, nixpkg])
        if r.returncode == 0:
            print(f"  {GREEN}\u2713{RESET} {pkg}")
        else:
            print(f"  {RED}\u2717{RESET} {pkg}", file=sys.stderr)
            failed += 1

    return failed == 0


def do_remove(pkgs, skip_confirm=False):
    if not skip_confirm and not show_transaction([], pkgs):
        print("  Aborted.")
        return False

    failed = 0
    for pkg in pkgs:
        removed = False
        for profile, label in [
            (VOLATILE_PROFILE, "session"),
            (PERMANENT_PROFILE, "permanent"),
        ]:
            r = run(
                ["nix", "profile", "remove", "--profile", profile, f".*{pkg}.*"],
                capture_output=True,
            )
            if r.returncode == 0:
                print(f"  {GREEN}\u2713{RESET} {pkg} ({label})")
                removed = True

        if not removed:
            print(f"  {RED}\u2717{RESET} {pkg} not installed", file=sys.stderr)
            failed += 1

    return failed == 0


def do_search(query):
    run(["nix", "search", "nixpkgs", query])


def do_list():
    for profile, label, color in [
        (VOLATILE_PROFILE, "Session", YELLOW),
        (PERMANENT_PROFILE, "Permanent", GREEN),
    ]:
        print(f"\n  {color}\u25cf{RESET} {BOLD}{label}{RESET}")
        r = run(["nix", "profile", "list", "--profile", profile], capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip():
            for line in r.stdout.strip().split("\n"):
                print(f"    {line}")
        else:
            print(f"    {DIM}(none){RESET}")
    print()


def run_prefix_mode(args):
    installs = []
    saves = []
    removes = []

    for arg in args:
        if arg.startswith("++"):
            saves.append(arg[2:])
        elif arg.startswith("+"):
            installs.append(arg[1:])
        elif arg.startswith("-"):
            removes.append(arg[1:])
        elif arg.startswith("?"):
            do_search(arg[1:])
            return
        else:
            print(f"  {RED}\u2717{RESET} Unknown: {arg}", file=sys.stderr)
            sys.exit(1)

    ok = True
    if installs:
        ok = do_install(installs, save=False) and ok
    if saves:
        ok = do_install(saves, save=True) and ok
    if removes:
        ok = do_remove(removes) and ok

    if not ok:
        sys.exit(1)


def run_subcommand_mode(args):
    cmd = args[0]
    rest = args[1:]

    if cmd in ("install", "add", "a"):
        save = False
        yes = False
        pkgs = []
        for arg in rest:
            if arg in ("-s", "--save"):
                save = True
            elif arg in ("-y", "--yes"):
                yes = True
            else:
                pkgs.append(arg)
        if not pkgs:
            print("  Package name required.", file=sys.stderr)
            sys.exit(1)
        if not do_install(pkgs, save=save, skip_confirm=yes):
            sys.exit(1)

    elif cmd in ("remove", "uninstall", "rm", "r"):
        yes = "-y" in rest or "--yes" in rest
        pkgs = [a for a in rest if a not in ("-y", "--yes")]
        if not pkgs:
            print("  Package name required.", file=sys.stderr)
            sys.exit(1)
        if not do_remove(pkgs, skip_confirm=yes):
            sys.exit(1)

    elif cmd in ("search", "s", "q"):
        if not rest:
            print("  Search query required.", file=sys.stderr)
            sys.exit(1)
        do_search(" ".join(rest))

    elif cmd in ("list", "ls"):
        do_list()

    elif cmd in ("help", "--help", "-h"):
        print_usage()

    else:
        print_usage()
        sys.exit(1)


VERSION = "0.2.0"

C1 = 36  # command/example column
C2 = 20  # flags column


def print_usage():
    print(f"""
  {BOLD}bgx{RESET} {DIM}v{VERSION} — Bingux package manager{RESET}

  {BOLD}Quick syntax:{RESET}
    {"bgx +firefox".ljust(C1)}{DIM}Install for this session{RESET}
    {"bgx ++firefox".ljust(C1)}{DIM}Install permanently{RESET}
    {"bgx -firefox".ljust(C1)}{DIM}Remove{RESET}
    {"bgx +pkg1 +pkg2 -pkg3".ljust(C1)}{DIM}Batch operations{RESET}
    {"bgx ?query".ljust(C1)}{DIM}Search{RESET}

  {BOLD}Commands:{RESET}
    {"install, add, a".ljust(C1)}{DIM}Install packages{RESET}
    {"remove, rm, r".ljust(C1)}{DIM}Remove packages{RESET}
    {"search, s, q".ljust(C1)}{DIM}Search nixpkgs{RESET}
    {"list, ls".ljust(C1)}{DIM}List installed packages{RESET}
    {"help".ljust(C1)}{DIM}Show this help{RESET}

  {BOLD}Flags:{RESET}
    {"-s, --save".ljust(C1)}{DIM}Install permanently (persists after reboot){RESET}
    {"-y, --yes".ljust(C1)}{DIM}Skip confirmation prompt{RESET}
""")


def main():
    args = sys.argv[1:]

    if not args:
        print_usage()
        return

    if args[0][0] in ("+", "-", "?"):
        run_prefix_mode(args)
    else:
        run_subcommand_mode(args)


if __name__ == "__main__":
    main()
