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
BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"


def run(cmd, **kwargs):
    return subprocess.run(cmd, **kwargs)


def pkg_info(pkg):
    """Get package name, version, and description from nixpkgs."""
    info = {"name": pkg, "version": "?", "description": ""}
    try:
        r = run(["nix", "eval", "--raw", f"nixpkgs#{pkg}.version"],
                capture_output=True, text=True, timeout=15)
        if r.returncode == 0:
            info["version"] = r.stdout.strip() or "?"
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


def confirm(prompt="Is this ok [y/N]: "):
    try:
        return input(prompt).strip().lower() in ("y", "yes")
    except (EOFError, KeyboardInterrupt):
        print()
        return False


def show_transaction(installs, removes, save=False):
    """Show a dnf-style transaction summary and prompt for confirmation."""
    if not installs and not removes:
        return True

    print(f"\n{BOLD}Transaction Summary:{RESET}")

    if installs:
        mode = "permanently" if save else "until reboot"
        print(f"  Installing ({mode}):")
        for info in installs:
            ver = info["version"]
            desc = info["description"]
            line = f"    {BOLD}{info['name']}{RESET}"
            if ver != "?":
                line += f"  {DIM}{ver}{RESET}"
            if desc:
                line += f"  {DIM}— {desc}{RESET}"
            print(line)

    if removes:
        print(f"  Removing:")
        for pkg in removes:
            print(f"    {BOLD}{pkg}{RESET}")

    total = len(installs) + len(removes)
    parts = []
    if installs:
        parts.append(f"{len(installs)} to install")
    if removes:
        parts.append(f"{len(removes)} to remove")
    print(f"\n  {', '.join(parts)}")
    print()

    return confirm(f"{BOLD}Proceed? [y/N]:{RESET} ")


def do_install(pkgs, save=False, skip_confirm=False):
    profile = PERMANENT_PROFILE if save else VOLATILE_PROFILE

    # Gather info
    print(f"{DIM}Resolving packages...{RESET}")
    infos = [pkg_info(p) for p in pkgs]

    if not skip_confirm and not show_transaction(infos, [], save=save):
        print("Aborted.")
        return False

    failed = 0
    for pkg in pkgs:
        nixpkg = f"nixpkgs#{pkg}"
        r = run(["nix", "profile", "install", "--profile", profile, nixpkg])
        if r.returncode == 0:
            print(f"{GREEN}\u2713{RESET} {pkg}")
        else:
            print(f"{RED}\u2717{RESET} {pkg}", file=sys.stderr)
            failed += 1

    return failed == 0


def do_remove(pkgs, skip_confirm=False):
    if not skip_confirm and not show_transaction([], pkgs):
        print("Aborted.")
        return False

    failed = 0
    for pkg in pkgs:
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
                print(f"{GREEN}\u2713{RESET} Removed {pkg} ({label})")
                removed = True

        if not removed:
            print(f"{RED}\u2717{RESET} {pkg} is not installed.", file=sys.stderr)
            failed += 1

    return failed == 0


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
            print(f"{RED}\u2717{RESET} Unknown: {arg}", file=sys.stderr)
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
    """Handle subcommands: install, remove, search, list"""
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
            print("Package name required.", file=sys.stderr)
            sys.exit(1)
        if not do_install(pkgs, save=save, skip_confirm=yes):
            sys.exit(1)

    elif cmd in ("remove", "uninstall", "rm", "r"):
        yes = "-y" in rest or "--yes" in rest
        pkgs = [a for a in rest if a not in ("-y", "--yes")]
        if not pkgs:
            print("Package name required.", file=sys.stderr)
            sys.exit(1)
        if not do_remove(pkgs, skip_confirm=yes):
            sys.exit(1)

    elif cmd in ("search", "s", "q"):
        if not rest:
            print("Search query required.", file=sys.stderr)
            sys.exit(1)
        do_search(" ".join(rest))

    elif cmd in ("list", "ls"):
        do_list()

    else:
        print_usage()
        sys.exit(1)


def print_usage():
    print(f"""
{BOLD}bgx{RESET} — Bingux package manager

{BOLD}Quick syntax:{RESET}
  bgx +firefox                    Install (until reboot)
  bgx ++firefox                   Install permanently
  bgx -firefox                    Remove
  bgx +firefox +htop -chromium    Batch operations
  bgx ?browser                    Search
  bgx                             List installed

{BOLD}Subcommands:{RESET}
  bgx install [-s] [-y] <pkg...>  Install (-s save, -y skip prompt)
  bgx remove [-y] <pkg...>        Remove
  bgx search <query>              Search nixpkgs
  bgx list                        List installed packages

{BOLD}Aliases:{RESET}
  install: add, a     remove: uninstall, rm, r
  search: s, q        list: ls
""".strip())


def main():
    args = sys.argv[1:]

    if not args:
        do_list()
        return

    if args[0][0] in ("+", "-", "?"):
        run_prefix_mode(args)
    else:
        run_subcommand_mode(args)


if __name__ == "__main__":
    main()
